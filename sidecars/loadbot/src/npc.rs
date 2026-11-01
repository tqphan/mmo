//! The `npc` scenario (docs/design/npcs.md): bot A (level 5, close enough to a wolf's level to earn
//! experience from it) and bot B (level 1) on the trade
//! island. A Greymane Wolf A summons notices it, closes in and bites; A kills it (summoning more
//! until one drops loot), earns the experience and owns the loot, which B is refused and A takes.
//! By the dev clock the corpse goes and the wolf respawns; A runs off and the wolf, leashed, goes
//! home at full HP. A Shore Skitter ignores A until hit, then pinches back. B kills one for
//! experience, then dies to a Wreck Captain and loses some. Then it cleans up.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sim::combat::{COMBAT, kill_xp, skill_by_id};
use sim::npc::{NPC, npc_kind_by_id};

use crate::gather::{advance, call, subscribe};
use crate::land::refused;
use crate::module_bindings::*;
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, EventTable as _, Table};

/// On the trade island south-west of spawn, away from the world's camps and the other scenarios.
const ARENA: (f32, f32) = (-150.0, -150.0);
const NPC_KIND: u8 = 4;
const A_LEVEL: u8 = 5;
const HIT: u16 = 1;
const XP: u16 = 32;

fn vitals(bot: &Bot, id: u64) -> Option<Vitals> {
    bot.conn.db.vitals().entity_id().find(&id)
}

/// Living or dead NPCs of `kind` that `bot` sees.
fn npcs(bot: &Bot, kind: &str) -> Vec<u64> {
    let wire = npc_kind_by_id(kind).map_or(u64::MAX, |k| u64::from(k.wire));
    bot.conn.db.entity().iter().filter(|e| e.kind == NPC_KIND && e.ref_id == wire).map(|e| e.id).collect()
}

pub(crate) fn alive(bot: &Bot, id: u64) -> bool {
    vitals(bot, id).is_some_and(|v| v.dead_at.is_none())
}

/// Summon `kind` at (x, z) and return the new NPC once we see it.
pub(crate) fn summon(bot: &Bot, kind: &str, x: f32, z: f32) -> Result<u64, String> {
    let before = npcs(bot, kind);
    call(bot, |tx| bot.conn.reducers.dev_spawn_npc_then(kind.into(), x, z, tx))?;
    let fresh = || npcs(bot, kind).into_iter().find(|id| !before.contains(id) && vitals(bot, *id).is_some());
    wait_for(kind, || fresh().is_some())?;
    fresh().ok_or_else(|| format!("the {kind} vanished"))
}

pub(crate) fn xp_of(bot: &Bot) -> (u8, u32) {
    bot.conn.db.my_fighter().iter().next().map_or((0, 0), |f| (f.level, f.xp))
}

/// Hit `target` with A's blades until it dies, using whichever is ready.
pub(crate) fn kill(bot: &Bot, target: u64) -> Check {
    let started = Instant::now();
    while alive(bot, target) {
        if started.elapsed() > Duration::from_secs(40) {
            return Err(format!("{target} is still alive after 40 s"));
        }
        for skill in ["rending_blow", "shield_bash", "threefold_cut"] {
            let w = skill_by_id(skill).ok_or("no skill")?.wire;
            if call(bot, |tx| bot.conn.reducers.use_skill_then(w, target, tx)).is_ok() {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 30));
    }
    Ok(())
}

pub fn scenario(args: &Args) -> Check {
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    let (x, z) = ARENA;
    let cells = sim::cell::window_cells(sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z), 2);
    let seen: Arc<Mutex<Vec<CombatEvent>>> = Arc::default();
    for bot in [&a, &b] {
        subscribe(bot, &["SELECT * FROM my_fighter", "SELECT * FROM my_inventory", "SELECT * FROM game_clock"])?;
        for c in &cells {
            subscribe(
                bot,
                &[
                    &format!("SELECT * FROM vitals WHERE cell = {c}"),
                    &format!("SELECT * FROM entity WHERE cell = {c}"),
                    &format!("SELECT * FROM transform WHERE cell = {c}"),
                    &format!("SELECT * FROM combat_event WHERE cell = {c}"),
                    &format!("SELECT * FROM loot WHERE cell = {c}"),
                ],
            )?;
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
    }
    call(&a, |tx| a.conn.reducers.dev_set_level_then(A_LEVEL, tx))?;
    for set in [1, 0] {
        call(&a, |tx| a.conn.reducers.pick_skillset_then(set, tx))?;
    }
    call(&b, |tx| b.conn.reducers.pick_skillset_then(1, tx))?;
    let log = seen.clone();
    a.conn.db.combat_event().on_insert(move |_, e| log.lock().unwrap().push(e.clone()));
    let events = |pred: &dyn Fn(&CombatEvent) -> bool| seen.lock().unwrap().iter().filter(|e| pred(e)).count();
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);
    let (ea, eb) = (pa.entity_id, pb.entity_id);
    a.teleport(x, z)?;
    b.teleport(x + 30.0, z)?;
    let mut summoned: Vec<u64> = Vec::new();

    // A wolf 12 m north notices A, closes in and bites.
    let wolf_kind = npc_kind_by_id("greymane_wolf").ok_or("no greymane_wolf")?;
    let wolf = summon(&a, "greymane_wolf", x, z - 12.0)?;
    summoned.push(wolf);
    wait_for("the wolf to notice A", || vitals(&a, wolf).is_some_and(|v| v.target == ea))?;
    let bit = || events(&|e| e.source == wolf && e.target == ea && e.flags & HIT != 0);
    wait_for("a bite", || bit() > 0)?;
    let t = a.conn.db.transform().entity_id().find(&wolf).ok_or("no wolf transform")?;
    let off = (t.x - x).hypot(t.z - z);
    if off > 4.0 {
        return Err(format!("the wolf bit from {off:.1} m away"));
    }
    println!("the wolf noticed A from 12 m, closed to {off:.1} m and bit {} times", bit());

    // A kills wolves until one drops loot: it's A's, B is refused, A takes it.
    let mut corpse = wolf;
    for round in 0..6 {
        let (_, xp_before) = xp_of(&a);
        kill(&a, corpse)?;
        wait_for("A's experience", || events(&|e| e.source == ea && e.flags & XP != 0) > round)?;
        let gained = kill_xp(wolf_kind.level, A_LEVEL);
        println!("A killed wolf {corpse} (experience {xp_before} -> {}, {gained} for the kill)", xp_of(&a).1);
        std::thread::sleep(Duration::from_millis(300));
        if a.conn.db.loot().iter().any(|l| l.corpse == corpse) {
            break;
        }
        corpse = summon(&a, "greymane_wolf", x, z - 3.0)?;
        summoned.push(corpse);
    }
    let loot: Vec<Loot> = a.conn.db.loot().iter().filter(|l| l.corpse == corpse).collect();
    if loot.is_empty() {
        return Err("six wolves and no loot".into());
    }
    let char_a = pa.char_id;
    if loot.iter().any(|l| l.owner != char_a) {
        return Err(format!("the loot isn't A's: {loot:?}"));
    }
    b.teleport(x, z - 2.0)?;
    refused(&b, "B looting A's wolf", "isn't yours", |tx| b.conn.reducers.loot_corpse_then(corpse, tx))?;
    let held = |def: u16| a.conn.db.my_inventory().iter().filter(|i| i.def == def).map(|i| i.count).sum::<u32>();
    let before: Vec<u32> = loot.iter().map(|l| held(l.def)).collect();
    call(&a, |tx| a.conn.reducers.loot_corpse_then(corpse, tx))?;
    wait_for("A's bag", || loot.iter().zip(&before).all(|(l, &n)| held(l.def) == n + l.count))?;
    wait_for("an empty corpse", || !a.conn.db.loot().iter().any(|l| l.corpse == corpse))?;
    println!("A looted {} stack(s) from wolf {corpse}; B was refused", loot.len());

    // The corpse goes after corpse_s, and a wolf is back after respawn_s.
    b.teleport(x + 30.0, z)?;
    advance(&a, i64::from(NPC.corpse_s) * 1_000_000 + 500_000)?;
    wait_for("the corpse to go", || vitals(&a, corpse).is_none())?;
    advance(&a, i64::from(wolf_kind.respawn_s) * 1_000_000)?;
    wait_for("a wolf to respawn", || npcs(&a, "greymane_wolf").iter().any(|&id| alive(&a, id)))?;
    let wolf = npcs(&a, "greymane_wolf").into_iter().find(|&id| alive(&a, id)).ok_or("no wolf")?;
    summoned.push(wolf);
    println!("the corpse went after {} s and a wolf ({wolf}) respawned by the dev clock", NPC.corpse_s);

    // Leash: A hurts it and runs 90 m south; it chases past the leash, gives up and goes home healed.
    wait_for("the new wolf to notice A", || vitals(&a, wolf).is_some_and(|v| v.target == ea))?;
    wait_for("the wolf in reach", || {
        a.conn.db.transform().entity_id().find(&wolf).is_some_and(|t| (t.x - x).hypot(t.z - z) < 3.5)
    })?;
    kill_once(&a, wolf)?;
    a.teleport(x, z + 90.0)?;
    wait_for("the wolf to give up", || vitals(&a, wolf).is_some_and(|v| v.target == 0))?;
    let home = |t: &Transform| (t.x - x).hypot(t.z - (z - 3.0)) < 20.0 && t.speed == 0;
    a.teleport(x, z + 40.0)?;
    wait_for("the wolf home at full HP", || {
        a.conn.db.transform().entity_id().find(&wolf).is_some_and(|t| home(&t)) && vitals(&a, wolf).is_some_and(|v| v.hp == v.hp_max)
    })?;
    println!("leashed, the wolf went home and healed to {}", wolf_kind.hp);

    // A passive Skitter ignores A until hit, then pinches back.
    a.teleport(x + 60.0, z)?;
    let skitter = summon(&a, "shore_skitter", x + 60.0, z - 3.0)?;
    summoned.push(skitter);
    std::thread::sleep(Duration::from_secs(2));
    if vitals(&a, skitter).is_some_and(|v| v.target != 0) || events(&|e| e.source == skitter) > 0 {
        return Err("the passive Skitter attacked first".into());
    }
    kill_once(&a, skitter)?;
    wait_for("the Skitter's pinch", || events(&|e| e.source == skitter && e.target == ea && e.flags & HIT != 0) > 0)?;
    println!("the Skitter ignored A for 2 s, then pinched back when hit");

    // B (level 1) kills a Skitter of its own, then dies to a Wreck Captain and loses experience.
    b.teleport(x + 90.0, z)?;
    let mine = summon(&b, "shore_skitter", x + 90.0, z - 3.0)?;
    summoned.push(mine);
    let cut = skill_by_id("threefold_cut").ok_or("no threefold_cut")?.wire;
    let started = Instant::now();
    while alive(&b, mine) {
        if started.elapsed() > Duration::from_secs(30) {
            return Err("B can't kill its Skitter".into());
        }
        let _ = call(&b, |tx| b.conn.reducers.use_skill_then(cut, mine, tx));
        std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 30));
    }
    wait_for("B's experience", || xp_of(&b).1 > 0)?;
    let (level, xp) = xp_of(&b);
    call(&b, |tx| b.conn.reducers.dev_set_vitals_then(50, 100, tx))?;
    let captain = summon(&b, "wreck_captain", x + 90.0, z - 6.0)?;
    summoned.push(captain);
    wait_for("B's death", || vitals(&b, eb).is_some_and(|v| v.dead_at.is_some()))?;
    wait_for("B's loss", || xp_of(&b).1 < xp)?;
    println!("B (level {level}) had {xp} experience from its Skitter and kept {} after the Captain killed it", xp_of(&b).1);
    call(&b, |tx| b.conn.reducers.dev_set_vitals_then(500, 100, tx))?;

    // Clean up.
    for id in summoned {
        let _ = call(&a, |tx| a.conn.reducers.dev_remove_npc_then(id, tx));
    }
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One Threefold Cut on `target`, after the global cooldown.
fn kill_once(bot: &Bot, target: u64) -> Check {
    std::thread::sleep(Duration::from_millis(COMBAT.gcd_ms as u64 + 30));
    let w = skill_by_id("threefold_cut").ok_or("no threefold_cut")?.wire;
    call(bot, |tx| bot.conn.reducers.use_skill_then(w, target, tx))
}
