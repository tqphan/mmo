//! The `crime` scenario (docs/design/crime.md): bots A, B and a witness W on the trade island. A
//! hits B: B sees a Bloodstain and A Bloodmarked. B hits back and leaves nothing. A kills B: a
//! Blood Pool. A can't report its own crimes and the dead B can't report at all, so W reports both,
//! and A's record shows their points. W steals A's dropped pack and leaves Footprints, which A, the
//! victim, reports. `dev_set_crime` makes A Sought, which W sees in A's vitals. Last, a fresh
//! Bloodstain fades by the dev clock.

use std::collections::HashSet;
use std::time::Duration;

use sim::combat::{COMBAT, level_max, skill_by_id};
use sim::crime::{CRIME, Evidence as Kind};

use crate::gather::{advance, call, game_now, subscribe};
use crate::land::refused;
use crate::module_bindings::*;
use crate::vehicle::face;
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// On the trade island south-east of spawn, where the combat scenario fights.
const SCENE: (f32, f32) = (200.0, -150.0);
/// Where B stands: in front of A, who faces north (-z), within a blade's reach.
const APART_M: f32 = 3.0;

fn use_skill(bot: &Bot, skill: &str, target: u64) -> Check {
    let w = skill_by_id(skill).map(|s| s.wire).ok_or_else(|| format!("no skill {skill}"))?;
    call(bot, |tx| bot.conn.reducers.use_skill_then(w, target, tx))
}

fn report(bot: &Bot, id: u64) -> Check {
    call(bot, |tx| bot.conn.reducers.report_evidence_then(id, tx))
}

/// Whether `bot` sees `entity_id` Bloodmarked now.
fn marked(bot: &Bot, entity_id: u64) -> bool {
    let now = game_now(bot);
    bot.conn.db.vitals().entity_id().find(&entity_id).is_some_and(|v| v.statuses.iter().any(|s| s.status == CRIME.mark && s.until.to_micros_since_unix_epoch() > now))
}

fn gcd() {
    std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 50));
}

pub fn scenario(args: &Args) -> Check {
    let (x, z) = SCENE;
    let cells = sim::cell::window_cells(sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z), 1);
    let mut bots = Vec::new();
    for name in ["A", "B", "W"] {
        let bot = Bot::connect(args, name)?;
        bot.subscribe_self()?;
        subscribe(&bot, &["SELECT * FROM my_crime", "SELECT * FROM my_pack", "SELECT * FROM game_clock"])?;
        for c in &cells {
            subscribe(
                &bot,
                &[
                    &format!("SELECT * FROM vitals WHERE cell = {c}"),
                    &format!("SELECT * FROM entity WHERE cell = {c}"),
                    &format!("SELECT * FROM evidence WHERE cell = {c}"),
                    &format!("SELECT * FROM dropped_pack WHERE cell = {c}"),
                ],
            )?;
        }
        bot.arrive()?;
        call(&bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
        call(&bot, |tx| bot.conn.reducers.dev_set_level_then(level_max(), tx))?;
        call(&bot, |tx| bot.conn.reducers.pick_skillset_then(1, tx))?;
        bots.push(bot);
    }
    let [a, b, w] = <[Bot; 3]>::try_from(bots).map_err(|_| "three bots")?;
    let (pa, pb, pw) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?, w.presence().ok_or("W left")?);
    let (ea, eb, ew) = (pa.entity_id, pb.entity_id, pw.entity_id);
    // Earlier runs' evidence may still lie here; only this run's counts.
    let old: HashSet<u64> = w.conn.db.evidence().iter().map(|e| e.id).collect();
    let fresh = |bot: &Bot, kind: Kind| -> Vec<Evidence> { bot.conn.db.evidence().iter().filter(|e| !old.contains(&e.id) && e.kind == kind as u8).collect() };

    a.teleport(x, z)?;
    b.teleport(x, z - APART_M)?;
    w.teleport(x + 2.0, z - APART_M)?;
    wait_for("everyone's vitals", || [ea, eb, ew].iter().all(|e| w.conn.db.vitals().entity_id().find(e).is_some()))?;

    // Assault: one stain a bout, and the attacker marked for all to see.
    use_skill(&a, "threefold_cut", eb)?;
    wait_for("B sees a Bloodstain", || fresh(&b, Kind::Bloodstain).len() == 1)?;
    wait_for("B sees A Bloodmarked", || marked(&b, ea))?;
    let stain = fresh(&b, Kind::Bloodstain)[0].clone();
    gcd();
    use_skill(&a, "threefold_cut", eb)?;
    println!("A hit B twice: one Bloodstain at ({:.1}, {:.1}), and A is Bloodmarked", stain.x, stain.z);
    // B fights back: A is fair game.
    face(&b, std::f32::consts::PI)?;
    use_skill(&b, "threefold_cut", ea)?;
    std::thread::sleep(Duration::from_millis(500));
    if fresh(&w, Kind::Bloodstain).len() != 1 || marked(&w, eb) {
        return Err("B's blow back at the Bloodmarked A left evidence or marked B".into());
    }
    println!("B hit back: no evidence, and B isn't marked");

    // Murder.
    call(&b, |tx| b.conn.reducers.dev_set_vitals_then(30, 400, tx))?;
    gcd();
    use_skill(&a, "threefold_cut", eb)?;
    wait_for("B dead", || w.conn.db.vitals().entity_id().find(&eb).is_some_and(|v| v.dead_at.is_some()))?;
    wait_for("a Blood Pool", || fresh(&w, Kind::BloodPool).len() == 1)?;
    let pool = fresh(&w, Kind::BloodPool)[0].clone();
    println!("A killed B: a Blood Pool");

    // Reports: never the offender's, never the dead's; the witness's add up on A's record.
    refused(&a, "A's report of its own stain", "can't report your own crime", |tx| a.conn.reducers.report_evidence_then(stain.id, tx))?;
    refused(&b, "the dead B's report", "you are dead", |tx| b.conn.reducers.report_evidence_then(pool.id, tx))?;
    report(&w, stain.id)?;
    report(&w, pool.id)?;
    let points = (CRIME.bloodstain_points + CRIME.blood_pool_points).unsigned_abs();
    wait_for("A's record", || a.conn.db.my_crime().iter().any(|r| r.crime_points == points && r.notoriety == points && r.reports == 2))?;
    wait_for("the evidence gone", || fresh(&w, Kind::Bloodstain).is_empty() && fresh(&w, Kind::BloodPool).is_empty())?;
    if w.conn.db.my_crime().count() != 0 {
        return Err("W has a record for reporting".into());
    }
    println!("W reported both: A has {points} crime points and Notoriety; A and the dead B were refused");

    // Theft: W takes A's pack; A, the victim, reports the Footprints.
    call(&a, |tx| a.conn.reducers.dev_give_pack_then("grain_bale".into(), tx))?;
    wait_for("A's pack", || a.conn.db.my_pack().count() == 1)?;
    call(&a, |tx| a.conn.reducers.drop_pack_then(tx))?;
    let mine = || w.conn.db.dropped_pack().iter().find(|d| d.owner == pa.char_id);
    wait_for("A's pack on the ground", || mine().is_some())?;
    let d = mine().ok_or("no pack")?;
    w.teleport(d.x + 1.0, d.z)?;
    call(&w, |tx| w.conn.reducers.pick_up_pack_then(d.id, tx))?;
    wait_for("Footprints", || fresh(&a, Kind::Footprints).len() == 1)?;
    let prints = fresh(&a, Kind::Footprints)[0].clone();
    report(&a, prints.id)?;
    let theft = CRIME.footprint_points.unsigned_abs();
    wait_for("W's record", || w.conn.db.my_crime().iter().any(|r| r.crime_points == theft))?;
    call(&w, |tx| w.conn.reducers.dev_discard_pack_then(tx))?;
    println!("W stole A's pack and left Footprints; A reported them and W has {theft} crime points");

    // Sought, for everyone to see.
    if w.conn.db.vitals().entity_id().find(&ea).is_none_or(|v| v.sought) {
        return Err("A is Sought too soon".into());
    }
    call(&a, |tx| a.conn.reducers.dev_set_crime_then(CRIME.sought_points.unsigned_abs(), points, tx))?;
    wait_for("W sees A Sought", || w.conn.db.vitals().entity_id().find(&ea).is_some_and(|v| v.sought))?;
    println!("at {} crime points A is Sought", CRIME.sought_points);

    // A new bout once the mark runs out, and its stain fades by the dev clock.
    advance(&a, (i64::from(CRIME.mark_s) + 1) * 1_000_000)?;
    w.teleport(x, z - APART_M)?;
    use_skill(&a, "rending_blow", ew)?;
    wait_for("a second Bloodstain", || fresh(&w, Kind::Bloodstain).len() == 1)?;
    advance(&a, i64::from(CRIME.evidence_s) * 1_000_000)?;
    wait_for("the stain to fade", || fresh(&w, Kind::Bloodstain).is_empty())?;
    println!("a new bout left a new Bloodstain, which faded a day later by the dev clock");

    for bot in [a, b, w] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}
