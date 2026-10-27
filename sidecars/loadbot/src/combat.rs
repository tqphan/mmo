//! The `combat` scenario (docs/design/combat.md): bots A and B and a Straw Dummy on the trade
//! island. A is refused out of range and with B behind it, then hits B instantly, is held by the
//! global cooldown, hits the dummy, lands a timed cast and a projectile on B, and has a cast
//! interrupted by stepping. B heals itself with a heal aimed at the dummy. Then B, carrying a pack, is killed: its pack falls, it
//! can't move or fight, respawning is refused until the wait has passed, and it gets up at the
//! Wayshrine the scenario placed, with half its HP, which regenerates by the dev clock. Then it
//! cleans up.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sim::combat::{COMBAT, respawn_share, shrine_by_id, skill_by_id};

use crate::gather::{advance, call, game_now, subscribe};
use crate::land::refused;
use crate::module_bindings::*;
use crate::trade::{Ground, walk};
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, EventTable as _, Table};

/// On the trade island south-east of spawn, away from the other scenarios' spots.
const ARENA: (f32, f32) = (200.0, -150.0);
/// Where B stands: 10 m in front of A, who faces north (-z).
const APART_M: f32 = 10.0;
/// Our Wayshrine, nearer than the world's.
const SHRINE_OFF: f32 = 60.0;
/// The server stands the dead up this far in front of a shrine (`combat::SHRINE_STEP_M`).
const SHRINE_STEP_M: f32 = 3.0;

fn wire(id: &str) -> Result<u16, String> {
    skill_by_id(id).map(|s| s.wire).ok_or_else(|| format!("no skill {id}"))
}

fn vitals(bot: &Bot, entity_id: u64) -> Option<Vitals> {
    bot.conn.db.vitals().entity_id().find(&entity_id)
}

/// HP and mana as `bot` sees them now.
fn hp_mp(bot: &Bot, entity_id: u64) -> (i32, i32) {
    vitals(bot, entity_id).map_or((-1, -1), |v| {
        let (at, until, now, dead) = (v.at.to_micros_since_unix_epoch(), v.combat_until.to_micros_since_unix_epoch(), game_now(bot), v.dead_at.is_some());
        (sim::combat::hp_now(v.hp, v.hp_max, at, until, now, dead), sim::combat::mp_now(v.mp, v.mp_max, at, until, now, dead))
    })
}

fn use_skill(bot: &Bot, skill: &str, target: u64) -> Check {
    let w = wire(skill)?;
    call(bot, |tx| bot.conn.reducers.use_skill_then(w, target, tx))
}

fn refused_skill(bot: &Bot, skill: &str, target: u64, want: &str) -> Check {
    let w = wire(skill)?;
    refused(bot, &format!("{skill} on {target}"), want, |tx| bot.conn.reducers.use_skill_then(w, target, tx))
}

pub fn scenario(args: &Args) -> Check {
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    let land = Ground::of(&a)?;
    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    let (x, z) = ARENA;
    let cells = sim::cell::window_cells(sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z), 1);
    let seen: Arc<Mutex<Vec<CombatEvent>>> = Arc::default();
    for bot in [&a, &b] {
        subscribe(bot, &["SELECT * FROM my_fighter", "SELECT * FROM my_pack", "SELECT * FROM game_clock"])?;
        for c in &cells {
            subscribe(
                bot,
                &[
                    &format!("SELECT * FROM vitals WHERE cell = {c}"),
                    &format!("SELECT * FROM entity WHERE cell = {c}"),
                    &format!("SELECT * FROM combat_event WHERE cell = {c}"),
                    &format!("SELECT * FROM dropped_pack WHERE cell = {c}"),
                    &format!("SELECT * FROM shrine WHERE cell = {c}"),
                ],
            )?;
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
    }
    let log = seen.clone();
    b.conn.db.combat_event().on_insert(move |_, e| log.lock().unwrap().push(e.clone()));
    let events = |skill: &str| -> Vec<CombatEvent> {
        let w = wire(skill).map(u32::from).unwrap_or_default();
        seen.lock().unwrap().iter().filter(|e| e.skill_id == w).cloned().collect()
    };
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);
    let (ea, eb) = (pa.entity_id, pb.entity_id);

    // The arena: A, B in front of it, a dummy between them, and our Wayshrine off to the side.
    a.teleport(x, z)?;
    b.teleport(x, z - APART_M)?;
    call(&a, |tx| a.conn.reducers.dev_spawn_dummy_then(x + 1.0, z - 3.0, tx))?;
    let dummy_of = || a.conn.db.entity().iter().find(|e| e.kind == 3 && vitals(&a, e.id).is_some()).map(|e| e.id);
    wait_for("the dummy", || dummy_of().is_some())?;
    let dummy = dummy_of().ok_or("no dummy")?;
    let kind = shrine_by_id("gullhaven_wayshrine").ok_or("no gullhaven_wayshrine")?.wire;
    let (sx, sz) = (x + SHRINE_OFF, z);
    let row = ShrinePlacement { kind, x: sx, y: land.at(sx, sz), z: sz, yaw: 0 };
    call(&a, |tx| a.conn.reducers.admin_put_shrines_then(vec![row], tx))?;
    let ours = || a.conn.db.shrine().iter().find(|s| s.x == sx && s.z == sz).map(|s| s.id);
    wait_for("our Wayshrine", || ours().is_some())?;
    let shrine = ours().ok_or("no shrine")?;
    wait_for("both vitals", || vitals(&a, eb).is_some() && vitals(&b, ea).is_some())?;
    println!("arena at ({x:.0}, {z:.0}): B {APART_M} m in front of A, dummy {dummy} between them, Wayshrine {shrine} {SHRINE_OFF} m east");

    // Refusals: too far for a blade, then B behind A.
    refused_skill(&a, "threefold_cut", eb, "out of range")?;
    b.teleport(x, z + 3.0)?;
    refused_skill(&a, "frostspike", eb, "isn't in front")?;
    b.teleport(x, z - APART_M)?;

    // An instant hit, seen by B; the global cooldown holds the next.
    let spike = skill_by_id("frostspike").ok_or("no frostspike")?;
    use_skill(&a, "frostspike", eb)?;
    wait_for("B sees the Frostspike", || events("frostspike").len() == 1)?;
    wait_for("B's HP", || hp_mp(&b, eb).0 == COMBAT.hp_max - spike.effects[0].amount)?;
    refused_skill(&a, "frostspike", eb, "not ready")?;
    println!("A hit B with Frostspike for {}; the cooldowns held the next", spike.effects[0].amount);
    std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 50));
    use_skill(&a, "threefold_cut", dummy)?;
    let cut = skill_by_id("threefold_cut").ok_or("no threefold_cut")?.effects[0].amount;
    wait_for("the dummy's HP", || hp_mp(&a, dummy).0 == COMBAT.dummy_hp - cut)?;
    println!("A cut the dummy for {cut}");

    // A timed cast, seen in A's vitals by B, lands when it ends.
    std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 50));
    let flare = skill_by_id("flare_burst").ok_or("no flare_burst")?;
    let hp_before = hp_mp(&b, eb).0;
    let started = Instant::now();
    use_skill(&a, "flare_burst", eb)?;
    wait_for("B sees A casting", || vitals(&b, ea).is_some_and(|v| v.cast_skill == flare.wire))?;
    wait_for("the Flare Burst", || !events("flare_burst").is_empty())?;
    let took = started.elapsed();
    if took < Duration::from_millis(u64::from(flare.cast_ms) - 100) {
        return Err(format!("Flare Burst landed after {took:?}, before its {} ms cast", flare.cast_ms));
    }
    // B regenerates at the in-combat rate while the cast runs.
    let regen = COMBAT.hp_regen_in * (flare.cast_ms as i32 / 1000 + 2);
    if hp_mp(&b, eb).0 > hp_before - flare.effects[0].amount + regen {
        return Err(format!("B has {} HP after Flare Burst from {hp_before}", hp_mp(&b, eb).0));
    }
    println!("A's {} ms Flare Burst landed after {took:.2?}", flare.cast_ms);

    // A cast interrupted by a step lands nothing.
    std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 50));
    use_skill(&a, "brine_lash", eb)?;
    wait_for("A casting Brine Lash", || vitals(&a, ea).is_some_and(|v| v.cast_skill != 0))?;
    std::thread::sleep(Duration::from_millis(300));
    walk(&a, &land, x + 1.0, z)?;
    wait_for("the interrupted cast", || vitals(&a, ea).is_some_and(|v| v.cast_skill == 0))?;
    std::thread::sleep(Duration::from_millis(1500));
    if !events("brine_lash").is_empty() {
        return Err("an interrupted Brine Lash landed".into());
    }
    println!("A stepped during Brine Lash and it was interrupted");

    // A projectile lands a flight time after its cast ends.
    let dart = skill_by_id("cinder_dart").ok_or("no cinder_dart")?;
    let started = Instant::now();
    use_skill(&a, "cinder_dart", eb)?;
    wait_for("the Cinder Dart", || !events("cinder_dart").is_empty())?;
    let took = started.elapsed();
    let flight = Duration::from_micros(sim::combat::impact_micros(APART_M, dart.speed_mps) as u64);
    if took < Duration::from_millis(u64::from(dart.cast_ms)) + flight - Duration::from_millis(100) {
        return Err(format!("Cinder Dart landed after {took:?}, before its cast and {flight:?} flight"));
    }
    println!("A's Cinder Dart landed after {took:.2?} ({flight:.2?} of it in flight)");

    // B heals itself with an ally skill on the dummy, which isn't a character.
    let hurt = hp_mp(&b, eb).0;
    use_skill(&b, "quick_balm", dummy)?;
    wait_for("B's heal", || events("quick_balm").iter().any(|e| e.target == eb && e.source == eb && e.flags & 2 != 0))?;
    wait_for("B healed", || hp_mp(&b, eb).0 > hurt)?;
    println!("B's Quick Balm on the dummy healed B instead: {hurt} → {} HP", hp_mp(&b, eb).0);

    // B dies carrying a pack: the pack falls, and the dead wait.
    call(&b, |tx| b.conn.reducers.dev_give_pack_then("grain_bale".into(), tx))?;
    wait_for("B's pack", || b.conn.db.my_pack().count() == 1)?;
    call(&b, |tx| b.conn.reducers.dev_set_vitals_then(30, 400, tx))?;
    wait_for("B at 30 HP", || hp_mp(&b, eb).0 <= 40)?;
    a.teleport(x, z - APART_M + 2.0)?;
    std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 50));
    use_skill(&a, "threefold_cut", eb)?;
    wait_for("the killing blow", || events("threefold_cut").iter().any(|e| e.target == eb && e.flags & 4 != 0))?;
    wait_for("B dead", || vitals(&b, eb).is_some_and(|v| v.dead_at.is_some()))?;
    wait_for("the fallen pack", || b.conn.db.my_pack().count() == 0 && a.conn.db.dropped_pack().iter().any(|d| d.owner == pb.char_id))?;
    let p = b.presence().ok_or("B left")?;
    refused(&b, "a dead bot's move", "you are dead", |tx| b.conn.reducers.move_event_then(p.last_seq + 1, 0, p.x, p.y, p.z + 1.0, 0, 540, 1, tx))?;
    refused_skill(&b, "frostspike", ea, "you are dead")?;
    refused(&b, "respawning at once", "you can respawn in", |tx| b.conn.reducers.respawn_then(tx))?;
    let d = a.conn.db.dropped_pack().iter().find(|d| d.owner == pb.char_id).ok_or("no fallen pack")?;
    a.teleport(d.x + 1.0, d.z)?;
    call(&a, |tx| a.conn.reducers.pick_up_pack_then(d.id, tx))?;
    wait_for("the pack on A's back", || a.conn.db.my_pack().count() == 1)?;
    println!("A killed B; B's pack fell and A took it; B can't move, fight or respawn yet");

    // After the wait, B gets up at our Wayshrine with half its HP, which regenerates.
    advance(&b, i64::from(COMBAT.respawn_first_s) * 1_000_000)?;
    let corrections = p.corrections;
    call(&b, |tx| b.conn.reducers.respawn_then(tx))?;
    wait_for("B on its feet", || vitals(&b, eb).is_some_and(|v| v.dead_at.is_none()))?;
    let p = b.presence().ok_or("B left")?;
    // A few steps in front of it: our shrine faces north (-z).
    let (ux, uz) = (sx, sz - SHRINE_STEP_M);
    if (p.x, p.z, p.corrections) != (ux, uz, corrections + 1) {
        return Err(format!("B respawned at ({}, {}) with {} corrections, expected ({ux}, {uz}) and {}", p.x, p.z, p.corrections, corrections + 1));
    }
    let hp = hp_mp(&b, eb).0;
    if !(respawn_share(COMBAT.hp_max)..respawn_share(COMBAT.hp_max) + 3 * COMBAT.hp_regen_out).contains(&hp) {
        return Err(format!("B respawned with {hp} HP"));
    }
    advance(&b, 60_000_000)?;
    wait_for("B's HP to regenerate", || hp_mp(&b, eb).0 == COMBAT.hp_max)?;
    println!("B respawned at the Wayshrine with {hp} HP and was full a minute later by the dev clock");

    // Clean up.
    call(&a, |tx| a.conn.reducers.dev_discard_pack_then(tx))?;
    call(&a, |tx| a.conn.reducers.dev_remove_dummy_then(dummy, tx))?;
    call(&a, |tx| a.conn.reducers.admin_remove_shrines_then(vec![shrine], tx))?;
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}
