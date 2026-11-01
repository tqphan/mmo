//! NPCs (docs/design/npcs.md).

use super::combat::{ADMIN, hp_mp, put, use_on, veteran, vitals_of};
use super::*;
use crate::combat::{CombatEvent, combat_event, dev_set_level, dev_set_vitals, fighter, flags, pick_skillset, vitals};
use crate::items::item;
use crate::npc::{
    Npc, SpawnPlacement, admin_clear_spawns, admin_put_spawns, dev_remove_npc, dev_spawn_npc, loot, loot_corpse, npc, spawn, state, threat,
};
use sim::npc::{NPC, npc_kind_by_id};

fn kind(id: &str) -> &'static sim::npc::NpcKindData {
    npc_kind_by_id(id).unwrap()
}

/// Put a `kind` NPC at (x, z) and return its entity.
pub(super) fn summon(t: &TestContext, player: u128, kind: &str, x: f32, z: f32) -> u64 {
    call(t, player, |ctx| dev_spawn_npc(ctx, kind.into(), x, z)).unwrap();
    read(t, |ctx| ctx.db.npc().iter().map(|n| n.entity_id).max().unwrap())
}

fn npc_of(t: &TestContext, id: u64) -> Option<Npc> {
    read(t, |ctx| ctx.db.npc().entity_id().find(id))
}

/// One brain step, returning the events it made.
fn think(t: &TestContext) -> Vec<CombatEvent> {
    read(t, |ctx| {
        crate::npc::think(ctx);
        ctx.db.combat_event().iter().collect()
    })
}

/// Brain steps every `brain_ms` for `ms`, with every event they made.
fn think_for(t: &TestContext, ms: i64) -> Vec<CombatEvent> {
    let mut events = Vec::new();
    for _ in 0..ms / i64::from(NPC.brain_ms) {
        advance_ms(t, i64::from(NPC.brain_ms));
        events.extend(think(t));
    }
    events
}

fn threat_on(t: &TestContext, npc_id: u64) -> Vec<(u64, i64, i64)> {
    let mut rows: Vec<_> = read(t, |ctx| ctx.db.threat().npc().filter(npc_id).map(|r| (r.id, r.entity, r.amount, r.damage)).collect::<Vec<_>>());
    rows.sort();
    rows.into_iter().map(|(_, e, a, d)| (e, a, d)).collect()
}

pub(super) fn set_hp(t: &TestContext, id: u64, hp: i32) {
    read(t, |ctx| {
        let v = ctx.db.vitals().entity_id().find(id).unwrap();
        ctx.db.vitals().entity_id().update(crate::combat::Vitals { hp, at: crate::time::now(ctx), ..v });
    });
}

fn bag(t: &TestContext, char_id: u64, def: u16) -> u32 {
    read(t, |ctx| ctx.db.item().iter().filter(|i| i.holder == char_id && i.def == def).map(|i| i.count).sum())
}

fn admin(t: &TestContext) {
    call(t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::session::record_owner(ctx);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_spawn_point_wakes_near_a_character_and_sleeps_when_all_are_gone() {
    let t = world();
    admin(&t);
    let p = arrive(&t, 1, "Mira");
    let skitter = kind("shore_skitter").wire;
    let near = SpawnPlacement { kind: skitter, x: 60.0, y: 0.0, z: 0.0, yaw: 0 };
    let far = SpawnPlacement { kind: skitter, x: NPC.wake_m + 50.0, y: 0.0, z: 0.0, yaw: 0 };
    assert_eq!(call(&t, 1, |ctx| admin_put_spawns(ctx, vec![near.clone()])).unwrap_err(), "admins only");
    call(&t, ADMIN, |ctx| admin_put_spawns(ctx, vec![near, far])).unwrap();
    think(&t);
    let npcs: Vec<Npc> = read(&t, |ctx| ctx.db.npc().iter().collect());
    assert_eq!(npcs.len(), 1);
    let id = npcs[0].entity_id;
    let e = read(&t, |ctx| ctx.db.entity().id().find(id).unwrap());
    assert_eq!((e.kind, e.ref_id), (entity_kind::NPC, u64::from(skitter)));
    let v = vitals_of(&t, id);
    assert_eq!((v.hp, v.level, v.mp_max), (kind("shore_skitter").hp, 1, 0));
    assert_eq!((transform_of(&t, id).x, transform_of(&t, id).z), (60.0, 0.0));
    // Walking off past `sleep_m` puts it to sleep once it isn't strolling.
    put(&t, 1, -NPC.sleep_m - 70.0, 0.0);
    think_for(&t, i64::from(NPC.wander_max_s) * 1000 + 5000);
    assert_eq!(read(&t, |ctx| ctx.db.npc().count()), 0);
    assert_eq!(read(&t, |ctx| ctx.db.entity().count()), 1, "only Mira is left");
    assert!(read(&t, |ctx| ctx.db.spawn().iter().all(|s| s.npc == 0)));
    // Coming back wakes it again.
    put(&t, 1, 0.0, 0.0);
    think(&t);
    assert_eq!(read(&t, |ctx| ctx.db.npc().count()), 1);
    call(&t, ADMIN, admin_clear_spawns).unwrap();
    assert_eq!(read(&t, |ctx| (ctx.db.npc().count(), ctx.db.spawn().count(), ctx.db.vitals().count())), (0, 0, 1));
    let _ = p;
}

#[test]
fn an_idle_npc_strolls_near_home_and_publishes_like_a_client() {
    let t = world();
    arrive(&t, 1, "Mira");
    let id = summon(&t, 1, "bristleback_boar", 30.0, 0.0);
    let mut moved = false;
    for _ in 0..(NPC.wander_max_s * 5 + 20) {
        advance_ms(&t, i64::from(NPC.brain_ms));
        think(&t);
        let tr = transform_of(&t, id);
        moved |= tr.speed != 0;
        assert!(sim::npc::distance2d([tr.x, tr.z], [30.0, 0.0]) <= kind("bristleback_boar").wander_m + 0.01, "{tr:?}");
    }
    assert!(moved, "it strolled within wander_max_s");
    assert_eq!(npc_of(&t, id).unwrap().state, state::IDLE, "a passive kind never noticed Mira");
}

#[test]
fn an_aggressive_npc_notices_chases_and_bites() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    let wolf = kind("greymane_wolf");
    let id = summon(&t, 1, "greymane_wolf", 0.0, -(wolf.aggro_m - 2.0));
    think(&t);
    assert_eq!(threat_on(&t, id), [(p.entity_id, 1, 0)]);
    let events = think_for(&t, 4000);
    assert_eq!(vitals_of(&t, id).target, p.entity_id);
    let bites: Vec<_> = events.iter().filter(|e| e.source == id && e.target == p.entity_id && e.flags & flags::HIT != 0).collect();
    assert!(!bites.is_empty(), "{events:?}");
    let hamstring = sim::combat::skill_by_id("hamstring_bite").unwrap().wire;
    assert_eq!(u32::from(hamstring), bites[0].skill_id, "its first skill first");
    assert!(events.iter().any(|e| e.status == sim::combat::status_by_id("slowed").unwrap().wire && e.flags & flags::STATUS != 0));
    let n = npc_of(&t, id).unwrap();
    assert_eq!(n.state, state::FIGHT);
    assert!(n.cooldowns.iter().any(|c| c.skill == hamstring));
    let tr = transform_of(&t, id);
    assert!(sim::npc::distance2d([tr.x, tr.z], [0.0, 0.0]) < 3.0, "it closed in: {tr:?}");
    assert!(hp_mp(&t, p.entity_id).0 < COMBAT_HP, "Mira was bitten");
}

const COMBAT_HP: i32 = sim::combat::COMBAT.hp_max;

#[test]
fn a_passive_npc_fights_back_only_when_hit_and_takes_the_most_threatening() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    veteran(&t, 1);
    veteran(&t, 2);
    put(&t, 2, 1.0, 0.0);
    let id = summon(&t, 1, "mossback_tortoise", 0.0, -3.0);
    think_for(&t, 1000);
    assert!(threat_on(&t, id).is_empty());
    use_on(&t, 1, "threefold_cut", id).unwrap();
    let cut = sim::combat::skill_by_id("threefold_cut").unwrap().effects[0].amount;
    assert_eq!(threat_on(&t, id), [(a.entity_id, i64::from(cut), i64::from(cut))]);
    assert_eq!((npc_of(&t, id).unwrap().state, npc_of(&t, id).unwrap().tagger), (state::FIGHT, a.entity_id));
    think(&t);
    assert_eq!(vitals_of(&t, id).target, a.entity_id);
    // Tobin heals Mira: half the HP healed is threat on him; then he hits harder and takes it.
    call(&t, 1, |ctx| dev_set_vitals(ctx, 500, 500)).unwrap();
    use_on(&t, 2, "quick_balm", a.entity_id).unwrap();
    let balm = sim::combat::skill_by_id("quick_balm").unwrap().effects[0].amount;
    assert_eq!(threat_on(&t, id)[1], (b.entity_id, sim::npc::heal_threat(balm), 0));
    advance_ms(&t, 1000);
    use_on(&t, 2, "rending_blow", id).unwrap();
    think(&t);
    assert_eq!(vitals_of(&t, id).target, b.entity_id);
    assert_eq!(npc_of(&t, id).unwrap().tagger, a.entity_id, "the first hitter keeps the tag");
}

#[test]
fn a_timed_npc_cast_finishes_or_is_stunned_out() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    veteran(&t, 1);
    let id = summon(&t, 1, "mossback_tortoise", 0.0, -3.0);
    let slam = sim::combat::skill_by_id("shell_slam").unwrap();
    use_on(&t, 1, "threefold_cut", id).unwrap();
    think(&t);
    let v = vitals_of(&t, id);
    assert_eq!((v.cast_skill, v.cast_target), (slam.wire, p.entity_id));
    let hp = hp_mp(&t, p.entity_id).0;
    let events = think_for(&t, i64::from(slam.cast_ms) + 200);
    assert!(events.iter().any(|e| e.source == id && e.skill_id == u32::from(slam.wire) && e.amount == slam.effects[0].amount), "{events:?}");
    assert!(hp_mp(&t, p.entity_id).0 <= hp - slam.effects[0].amount + 10);
    // Next time a Shield Bash stuns it out of the cast.
    call(&t, 1, |ctx| dev_set_vitals(ctx, 2400, 1000)).unwrap();
    advance_ms(&t, i64::from(slam.cooldown_ms));
    think(&t);
    assert_eq!(vitals_of(&t, id).cast_skill, slam.wire);
    advance_ms(&t, 1000);
    // Stunned by Shield Bash before it lands.
    use_on(&t, 1, "shield_bash", id).unwrap();
    assert_eq!(vitals_of(&t, id).cast_skill, 0);
    let events = think_for(&t, 1400);
    assert!(!events.iter().any(|e| e.source == id), "stunned, it does nothing: {events:?}");
}

#[test]
fn leashed_it_goes_home_and_heals() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    veteran(&t, 1);
    let id = summon(&t, 1, "greymane_wolf", 0.0, -3.0);
    use_on(&t, 1, "threefold_cut", id).unwrap();
    think(&t);
    // The wolf follows Mira 32 m out, within the leash; then she runs past it and it gives up.
    put(&t, 1, 0.0, 32.0);
    think_for(&t, 6000);
    assert_eq!(npc_of(&t, id).unwrap().state, state::FIGHT);
    assert!(transform_of(&t, id).z > 25.0, "it followed her: {:?}", transform_of(&t, id));
    put(&t, 1, 0.0, NPC.leash_m + 60.0);
    think(&t);
    assert_eq!(npc_of(&t, id).unwrap().state, state::RETURN);
    assert!(threat_on(&t, id).is_empty());
    assert_eq!(vitals_of(&t, id).target, 0);
    // A hit on the way home does damage but adds no threat.
    let at = transform_of(&t, id);
    put(&t, 1, at.x, at.z + 2.0);
    let _ = p;
    let hp = vitals_of(&t, id).hp;
    use_on(&t, 1, "frostspike", id).unwrap();
    assert!(vitals_of(&t, id).hp < hp);
    assert!(threat_on(&t, id).is_empty());
    think_for(&t, 15_000);
    let n = npc_of(&t, id).unwrap();
    assert_eq!(n.state, state::IDLE);
    let tr = transform_of(&t, id);
    assert!(sim::npc::distance2d([tr.x, tr.z], [0.0, -3.0]) < 0.5 && tr.speed == 0, "{tr:?}");
    assert_eq!(vitals_of(&t, id).hp, kind("greymane_wolf").hp);
    assert_eq!(n.tagger, 0);
}

#[test]
fn the_tagger_with_enough_damage_owns_the_loot_and_the_experience() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    call(&t, 1, |ctx| pick_skillset(ctx, 1)).unwrap();
    veteran(&t, 2);
    put(&t, 2, 1.0, 0.0);
    let id = summon(&t, 1, "wreck_captain", 0.0, -3.0);
    set_hp(&t, id, 60);
    let events = call(&t, 1, |ctx| {
        crate::combat::use_skill(ctx, sim::combat::skill_by_id("threefold_cut").unwrap().wire, id)?;
        Ok(ctx.db.combat_event().iter().collect::<Vec<_>>())
    })
    .unwrap();
    assert!(events.iter().any(|e| e.target == id && e.flags & flags::KILL != 0));
    let gained = sim::combat::kill_xp(15, 1);
    assert!(events.iter().any(|e| e.source == a.entity_id && e.flags & flags::XP != 0 && e.amount == i32::try_from(gained).unwrap()), "{events:?}");
    assert!(vitals_of(&t, id).dead_at.is_some());
    let token = sim::content::item_by_id("brigands_token").unwrap().wire;
    let rows: Vec<_> = read(&t, |ctx| ctx.db.loot().corpse().filter(id).collect::<Vec<_>>());
    assert!(rows.iter().all(|r| r.owner == a.char_id));
    assert!(rows.iter().any(|r| r.def == token && (2..=4).contains(&r.count)));
    // Tobin can't take it yet; Mira must stand close.
    assert_eq!(call(&t, 2, |ctx| loot_corpse(ctx, id)).unwrap_err(), "that isn't yours to loot yet");
    put(&t, 1, 0.0, 5.0);
    assert_eq!(call(&t, 1, |ctx| loot_corpse(ctx, id)).unwrap_err(), "too far away");
    put(&t, 1, 0.0, 0.0);
    assert_eq!(call(&t, 1, |ctx| loot_corpse(ctx, 424_242)).unwrap_err(), "nothing to loot");
    call(&t, 1, |ctx| loot_corpse(ctx, id)).unwrap();
    assert!((2..=4).contains(&bag(&t, a.char_id, token)));
    assert_eq!(read(&t, |ctx| ctx.db.loot().count()), 0);
    assert_eq!(call(&t, 1, |ctx| loot_corpse(ctx, id)).unwrap_err(), "nothing to loot");
    let _ = b;
}

#[test]
fn a_weak_tagger_owns_nothing_and_loot_frees_up() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    veteran(&t, 1);
    veteran(&t, 2);
    put(&t, 2, 1.0, 0.0);
    let id = summon(&t, 1, "wreck_captain", 0.0, -3.0);
    set_hp(&t, id, 200);
    use_on(&t, 1, "threefold_cut", id).unwrap();
    // Tobin finishes it with far more damage: nobody owns the loot, and the killer gets the experience.
    use_on(&t, 2, "rending_blow", id).unwrap();
    assert!(vitals_of(&t, id).dead_at.is_some());
    assert!(read(&t, |ctx| ctx.db.loot().iter().all(|r| r.owner == 0 && r.corpse == id)));
    assert_eq!(call(&t, 1, |ctx| loot_corpse(ctx, id)).unwrap_err(), "that isn't yours to loot yet");
    advance_ms(&t, i64::from(NPC.free_s) * 1000);
    call(&t, 2, |ctx| loot_corpse(ctx, id)).unwrap();
    let token = sim::content::item_by_id("brigands_token").unwrap().wire;
    assert!(bag(&t, b.char_id, token) >= 2);
    assert_eq!(bag(&t, a.char_id, token), 0);
}

#[test]
fn a_party_tags_together_and_shares_the_loot() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    veteran(&t, 1);
    veteran(&t, 2);
    put(&t, 2, 1.0, 0.0);
    super::party::group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
    let id = summon(&t, 1, "wreck_captain", 0.0, -3.0);
    set_hp(&t, id, 200);
    use_on(&t, 1, "threefold_cut", id).unwrap();
    // As in the weak tagger's case, but Tobin is in Mira's party, so their damage is hers.
    use_on(&t, 2, "rending_blow", id).unwrap();
    assert!(vitals_of(&t, id).dead_at.is_some());
    assert!(read(&t, |ctx| ctx.db.loot().iter().all(|r| r.owner == a.char_id)));
    call(&t, 2, |ctx| loot_corpse(ctx, id)).unwrap();
    let token = sim::content::item_by_id("brigands_token").unwrap().wire;
    assert!(bag(&t, b.char_id, token) >= 2);
}

#[test]
fn a_corpse_lies_then_goes_and_the_npc_respawns_later() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    veteran(&t, 1);
    let id = summon(&t, 1, "shore_skitter", 0.0, -3.0);
    set_hp(&t, id, 10);
    use_on(&t, 1, "threefold_cut", id).unwrap();
    think_for(&t, i64::from(NPC.corpse_s) * 1000 - 1000);
    assert!(npc_of(&t, id).is_some(), "the corpse still lies there");
    think_for(&t, 2000);
    assert!(npc_of(&t, id).is_none());
    assert_eq!(read(&t, |ctx| ctx.db.loot().count()), 0);
    assert_eq!(read(&t, |ctx| ctx.db.npc().count()), 0, "not before respawn_s");
    let wait = i64::from(kind("shore_skitter").respawn_s - u32::try_from(NPC.corpse_s).unwrap()) * 1000;
    think_for(&t, wait);
    let back: Vec<Npc> = read(&t, |ctx| ctx.db.npc().iter().collect());
    assert_eq!(back.len(), 1);
    assert_eq!(vitals_of(&t, back[0].entity_id).hp, kind("shore_skitter").hp);
    let _ = p;
}

#[test]
fn a_pve_death_costs_experience_and_ends_the_threat() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| dev_set_level(ctx, 3)).unwrap();
    read(&t, |ctx| {
        let f = ctx.db.fighter().char_id().find(p.char_id).unwrap();
        ctx.db.fighter().char_id().update(crate::combat::Fighter { xp: 100, ..f });
    });
    let id = summon(&t, 1, "greymane_wolf", 0.0, -3.0);
    call(&t, 1, |ctx| dev_set_vitals(ctx, 30, 100)).unwrap();
    think_for(&t, 2000);
    assert!(vitals_of(&t, p.entity_id).dead_at.is_some(), "the wolf killed Mira");
    let f = read(&t, |ctx| ctx.db.fighter().char_id().find(p.char_id).unwrap());
    assert_eq!(f.xp, sim::combat::xp_after_death(3, 100));
    assert!(f.xp < 100);
    assert!(threat_on(&t, id).is_empty());
    think_for(&t, 1000);
    assert_ne!(npc_of(&t, id).unwrap().state, state::FIGHT, "with no one left it went home");
    assert_eq!(vitals_of(&t, id).target, 0);
}

#[test]
fn npcs_are_not_pickable_and_dev_removal_takes_the_spawn() {
    let t = world();
    arrive(&t, 1, "Mira");
    let wild = sim::combat::SKILLSETS.iter().find(|s| s.npc).unwrap().wire;
    assert_eq!(call(&t, 1, |ctx| pick_skillset(ctx, wild)).unwrap_err(), "no such skillset");
    let id = summon(&t, 1, "fen_croaker", 20.0, 0.0);
    assert_eq!(call(&t, 1, |ctx| dev_spawn_npc(ctx, "dragon".into(), 0.0, 0.0)).unwrap_err(), "no NPC kind \"dragon\"");
    call(&t, 1, |ctx| dev_remove_npc(ctx, id)).unwrap();
    assert_eq!(read(&t, |ctx| (ctx.db.npc().count(), ctx.db.spawn().count(), ctx.db.transform().count())), (0, 0, 1));
}
