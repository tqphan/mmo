//! Combat (docs/design/combat.md).

use super::*;
use crate::trade::dropped_pack;
use crate::combat::{
    CastJob, CombatEvent, ShrinePlacement, SkillImpact, Vitals, admin_clear_shrines, admin_put_shrines, cancel_cast, cast_done, cast_job, combat_event, dev_remove_dummy,
    dev_set_level, dev_set_vitals, dev_spawn_dummy, fighter, flags, my_fighter, periodic, pick_skillset, respawn, set_target, shrine, skill_impact,
    skill_lands, use_skill, vitals,
};
use crate::dev::dev_teleport;
use sim::combat::{COMBAT, respawn_share, shrine_by_id, skill_by_id};

pub(super) const ADMIN: u128 = 99;

fn wire(id: &str) -> u16 {
    skill_by_id(id).unwrap().wire
}

pub(super) fn vitals_of(t: &TestContext, entity_id: u64) -> Vitals {
    read(t, |ctx| ctx.db.vitals().entity_id().find(entity_id).unwrap())
}

pub(super) fn use_on(t: &TestContext, player: u128, skill: &str, target: u64) -> Result<(), String> {
    call(t, player, |ctx| use_skill(ctx, wire(skill), target))
}

/// Use a skill and return the events it made: event rows only exist inside their transaction.
fn use_seen(t: &TestContext, player: u128, skill: &str, target: u64) -> Vec<CombatEvent> {
    call(t, player, |ctx| {
        use_skill(ctx, wire(skill), target)?;
        Ok(ctx.db.combat_event().iter().collect())
    })
    .unwrap()
}

pub(super) fn put(t: &TestContext, player: u128, x: f32, z: f32) {
    call(t, player, |ctx| dev_teleport(ctx, x, z)).unwrap();
}

fn cast_of(t: &TestContext, entity_id: u64) -> Option<CastJob> {
    read(t, |ctx| ctx.db.cast_job().entity_id().find(entity_id))
}

/// Run a cast's scheduled finish, as the scheduler would.
fn finish_cast(t: &TestContext, entity_id: u64) {
    let job = cast_of(t, entity_id).expect("a cast under way");
    let id = job.scheduled_id;
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| cast_done(ctx, job)).unwrap();
    read(t, |ctx| ctx.db.cast_job().scheduled_id().delete(id));
}

fn impacts(t: &TestContext) -> Vec<SkillImpact> {
    read(t, |ctx| ctx.db.skill_impact().iter().collect())
}

/// Land every projectile in flight, returning the events they made.
fn land_all(t: &TestContext) -> Vec<CombatEvent> {
    let mut events = Vec::new();
    for i in impacts(t) {
        let id = i.scheduled_id;
        events.extend(
            t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| {
                skill_lands(ctx, i)?;
                Ok(ctx.db.combat_event().iter().collect::<Vec<_>>())
            })
            .unwrap(),
        );
        read(t, |ctx| ctx.db.skill_impact().scheduled_id().delete(id));
    }
    events
}

/// HP and mana settled to now.
pub(super) fn hp_mp(t: &TestContext, entity_id: u64) -> (i32, i32) {
    let v = vitals_of(t, entity_id);
    let now = read(t, |ctx| crate::time::now(ctx).to_micros_since_unix_epoch());
    let (at, until, dead) = (v.at.to_micros_since_unix_epoch(), v.combat_until.to_micros_since_unix_epoch(), v.dead_at.is_some());
    (sim::combat::hp_now(v.hp, v.hp_max, at, until, now, dead), sim::combat::mp_now(v.mp, v.mp_max, at, until, now, dead))
}

pub(super) fn shrines(t: &TestContext, at: &[(&str, f32, f32)]) {
    call(t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::session::record_owner(ctx);
        Ok(())
    })
    .unwrap();
    let rows = at.iter().map(|&(id, x, z)| ShrinePlacement { kind: shrine_by_id(id).unwrap().wire, x, y: 0.0, z, yaw: 0 }).collect();
    call(t, ADMIN, |ctx| admin_put_shrines(ctx, rows)).unwrap();
}

/// The HP and mana maximums at the top level.
const HP: i32 = 1000 + 14 * 100;
const MP: i32 = 800 + 14 * 40;

/// Make `player` top level with every skillset picked.
pub(super) fn veteran(t: &TestContext, player: u128) {
    call(t, player, |ctx| dev_set_level(ctx, sim::combat::level_max())).unwrap();
    for set in 0..3 {
        call(t, player, |ctx| pick_skillset(ctx, set)).unwrap();
    }
}

/// Two top-level characters with every skill: Mira at the origin facing north (-z), Tobin `d` m in
/// front of her.
fn duel(d: f32) -> (TestContext, Presence, Presence) {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    veteran(&t, 1);
    veteran(&t, 2);
    put(&t, 2, 0.0, -d);
    (t, a, b)
}

#[test]
fn entering_gives_full_vitals_and_a_fighter_seen_only_by_its_owner() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    let v = vitals_of(&t, a.entity_id);
    assert_eq!((v.hp, v.hp_max, v.mp, v.mp_max, v.dead_at), (COMBAT.hp_max, COMBAT.hp_max, COMBAT.mp_max, COMBAT.mp_max, None));
    assert_eq!(v.cell, transform_of(&t, a.entity_id).cell);
    let mine: Vec<u64> = my_fighter(&t.view_context(auth(1))).into_iter().map(|f| f.char_id).collect();
    assert_eq!(mine, [a.char_id]);
}

#[test]
fn targets_are_set_and_cleared() {
    let (t, a, b) = duel(10.0);
    call(&t, 1, |ctx| set_target(ctx, b.entity_id)).unwrap();
    assert_eq!(vitals_of(&t, a.entity_id).target, b.entity_id);
    assert_eq!(call(&t, 1, |ctx| set_target(ctx, 12345)).unwrap_err(), "no such target");
    call(&t, 1, |ctx| set_target(ctx, 0)).unwrap();
    assert_eq!(vitals_of(&t, a.entity_id).target, 0);
}

#[test]
fn an_instant_hit_spends_mana_and_starts_the_cooldowns() {
    let (t, a, b) = duel(3.0);
    let events: Vec<_> = use_seen(&t, 1, "rending_blow", b.entity_id).into_iter().map(|e| (e.source, e.target, e.skill_id, e.amount, e.flags)).collect();
    let rend = skill_by_id("rending_blow").unwrap();
    assert_eq!(hp_mp(&t, b.entity_id).0, HP - rend.effects[0].amount);
    assert_eq!(hp_mp(&t, a.entity_id).1, MP - rend.cost_mp);
    assert_eq!(events, [(a.entity_id, b.entity_id, u32::from(rend.wire), rend.effects[0].amount, flags::HIT)]);
    // The global cooldown, then the skill's own.
    assert_eq!(use_on(&t, 1, "threefold_cut", b.entity_id).unwrap_err(), "not ready yet");
    advance_ms(&t, i64::from(COMBAT.gcd_ms));
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    advance_ms(&t, i64::from(COMBAT.gcd_ms));
    assert_eq!(use_on(&t, 1, "rending_blow", b.entity_id).unwrap_err(), "not ready yet");
    advance_ms(&t, i64::from(rend.cooldown_ms));
    use_on(&t, 1, "rending_blow", b.entity_id).unwrap();
    let f = read(&t, |ctx| ctx.db.fighter().char_id().find(a.char_id).unwrap());
    assert_eq!(f.cooldowns.iter().map(|c| c.skill).collect::<Vec<_>>(), [rend.wire]);
}

#[test]
fn skills_are_refused_by_the_rules() {
    let (t, a, b) = duel(10.0);
    assert_eq!(use_on(&t, 1, "threefold_cut", a.entity_id).unwrap_err(), "can't target yourself");
    assert_eq!(use_on(&t, 1, "threefold_cut", b.entity_id).unwrap_err(), "out of range");
    assert_eq!(use_on(&t, 1, "threefold_cut", 0).unwrap_err(), "no target");
    assert_eq!(call(&t, 1, |ctx| use_skill(ctx, 7, b.entity_id)).unwrap_err(), "no such skill");
    put(&t, 2, 0.0, 10.0);
    assert_eq!(use_on(&t, 1, "frostspike", b.entity_id).unwrap_err(), "the target isn't in front of you");
    put(&t, 2, 0.0, -10.0);
    call(&t, 1, |ctx| dev_set_vitals(ctx, HP, 10)).unwrap();
    assert_eq!(use_on(&t, 1, "frostspike", b.entity_id).unwrap_err(), "not enough mana");
    call(&t, 2, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    call(&t, 1, |ctx| dev_set_vitals(ctx, HP, MP)).unwrap();
    assert_eq!(use_on(&t, 1, "frostspike", b.entity_id).unwrap_err(), "the target is dead");
    assert_eq!(use_on(&t, 2, "frostspike", a.entity_id).unwrap_err(), "you are dead");
    // Nothing was spent by any of it.
    assert_eq!(hp_mp(&t, a.entity_id), (HP, MP));
}

#[test]
fn a_timed_cast_lands_when_it_finishes_and_spends_then() {
    let (t, a, b) = duel(10.0);
    let flare = skill_by_id("flare_burst").unwrap();
    use_on(&t, 1, "flare_burst", b.entity_id).unwrap();
    let v = vitals_of(&t, a.entity_id);
    assert_eq!((v.cast_skill, v.cast_target), (flare.wire, b.entity_id));
    assert_eq!(v.cast_end.to_micros_since_unix_epoch() - v.cast_start.to_micros_since_unix_epoch(), i64::from(flare.cast_ms) * 1000);
    assert_eq!(hp_mp(&t, a.entity_id).1, MP, "mana is spent when it finishes");
    assert_eq!(use_on(&t, 1, "frostspike", b.entity_id).unwrap_err(), "already casting");
    advance_ms(&t, i64::from(flare.cast_ms));
    finish_cast(&t, a.entity_id);
    assert_eq!(vitals_of(&t, a.entity_id).cast_skill, 0);
    assert_eq!(hp_mp(&t, b.entity_id).0, HP - flare.effects[0].amount);
    assert!(hp_mp(&t, a.entity_id).1 < MP);
}

#[test]
fn moving_interrupts_a_cast_but_turning_does_not() {
    let (t, a, b) = duel(10.0);
    use_on(&t, 1, "flare_burst", b.entity_id).unwrap();
    advance_ms(&t, 300);
    call(&t, 1, |ctx| move_event(ctx, 1, 0, 0.0, a.y, 0.0, 9000, 0, 1)).unwrap();
    assert!(cast_of(&t, a.entity_id).is_some(), "turning on the spot keeps the cast");
    advance_ms(&t, 300);
    step(&t, 1, 2, 1.0, 0.0);
    assert!(cast_of(&t, a.entity_id).is_none());
    assert_eq!(vitals_of(&t, a.entity_id).cast_skill, 0);
    // And cancelling does too, with nothing spent either time.
    advance_ms(&t, 1000);
    use_on(&t, 1, "brine_lash", b.entity_id).unwrap();
    call(&t, 1, cancel_cast).unwrap();
    assert!(cast_of(&t, a.entity_id).is_none());
    assert_eq!(hp_mp(&t, a.entity_id).1, MP);
    assert_eq!(hp_mp(&t, b.entity_id).0, HP);
}

#[test]
fn a_cast_fails_without_spending_when_the_target_got_away() {
    let (t, a, b) = duel(10.0);
    use_on(&t, 1, "flare_burst", b.entity_id).unwrap();
    put(&t, 2, 0.0, -40.0);
    finish_cast(&t, a.entity_id);
    assert_eq!(hp_mp(&t, a.entity_id).1, MP);
    assert_eq!(hp_mp(&t, b.entity_id).0, HP);
    // Just past the range is within the leeway.
    advance_ms(&t, 2000);
    put(&t, 2, 0.0, -10.0);
    use_on(&t, 1, "flare_burst", b.entity_id).unwrap();
    put(&t, 2, 0.0, -26.0);
    finish_cast(&t, a.entity_id);
    assert!(hp_mp(&t, b.entity_id).0 < HP);
}

#[test]
fn leaving_ends_a_cast() {
    let (t, a, b) = duel(10.0);
    use_on(&t, 1, "flare_burst", b.entity_id).unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert!(cast_of(&t, a.entity_id).is_none());
    assert!(read(&t, |ctx| ctx.db.vitals().entity_id().find(a.entity_id).is_none()));
}

#[test]
fn a_projectile_lands_later_and_misses_a_target_that_left() {
    let (t, a, b) = duel(15.0);
    let dart = skill_by_id("cinder_dart").unwrap();
    use_on(&t, 1, "cinder_dart", b.entity_id).unwrap();
    finish_cast(&t, a.entity_id);
    let flying = impacts(&t);
    assert_eq!(flying.len(), 1);
    assert_eq!(hp_mp(&t, b.entity_id).0, HP, "not landed yet");
    assert_eq!(land_all(&t).len(), 1);
    assert_eq!(hp_mp(&t, b.entity_id).0, HP - dart.effects[0].amount);
    advance_ms(&t, 2000);
    use_on(&t, 1, "cinder_dart", b.entity_id).unwrap();
    finish_cast(&t, a.entity_id);
    call(&t, 2, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert!(land_all(&t).is_empty());
}

#[test]
fn heals_cap_and_an_ally_skill_on_an_enemy_heals_the_caster() {
    let (t, a, b) = duel(10.0);
    call(&t, 2, |ctx| dev_set_vitals(ctx, HP - 100, MP)).unwrap();
    let healed: Vec<_> = use_seen(&t, 1, "quick_balm", b.entity_id).into_iter().map(|e| (e.target, e.amount, e.flags)).collect();
    assert_eq!(hp_mp(&t, b.entity_id).0, HP);
    assert_eq!(healed, [(b.entity_id, 100, flags::HEAL)]);
    // On a dummy, it lands on the caster.
    call(&t, 1, |ctx| dev_spawn_dummy(ctx, 0.0, -5.0)).unwrap();
    let dummy = read(&t, |ctx| ctx.db.entity().iter().find(|e| e.kind == entity_kind::DUMMY).unwrap().id);
    advance_ms(&t, i64::from(COMBAT.gcd_ms));
    call(&t, 1, |ctx| dev_set_vitals(ctx, 500, MP)).unwrap();
    use_on(&t, 1, "mending_word", dummy).unwrap();
    let job = cast_of(&t, a.entity_id).unwrap();
    assert_eq!(job.target, a.entity_id);
    finish_cast(&t, a.entity_id);
    assert_eq!(hp_mp(&t, a.entity_id).0, 500 + skill_by_id("mending_word").unwrap().effects[0].amount);
}

#[test]
fn regeneration_is_slower_in_combat() {
    let (t, a, b) = duel(3.0);
    call(&t, 2, |ctx| dev_set_vitals(ctx, 500, MP)).unwrap();
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    let hit = skill_by_id("threefold_cut").unwrap().effects[0].amount;
    assert!(vitals_of(&t, b.entity_id).combat_until > vitals_of(&t, b.entity_id).at);
    assert!(vitals_of(&t, a.entity_id).combat_until > vitals_of(&t, a.entity_id).at, "the attacker is in combat too");
    advance_ms(&t, 3000);
    assert_eq!(hp_mp(&t, b.entity_id).0, 500 - hit + 3 * COMBAT.hp_regen_in);
    advance_ms(&t, i64::from(COMBAT.combat_s) * 1000 - 3000 + 10_000);
    assert_eq!(hp_mp(&t, b.entity_id).0, 500 - hit + COMBAT.combat_s * COMBAT.hp_regen_in + 10 * COMBAT.hp_regen_out);
    // The dev clock counts too.
    call(&t, 2, |ctx| crate::time::dev_advance_clock(ctx, 3_600_000_000)).unwrap();
    assert_eq!(hp_mp(&t, b.entity_id).0, HP);
}

#[test]
fn a_kill_drops_the_pack_and_the_dead_wait_to_respawn_at_the_nearest_shrine() {
    let (t, a, b) = duel(3.0);
    shrines(&t, &[("gullhaven_wayshrine", 100.0, 0.0), ("redfen_wayshrine", -500.0, 0.0)]);
    call(&t, 2, |ctx| crate::trade::dev_give_pack(ctx, "grain_bale".into())).unwrap();
    call(&t, 2, |ctx| dev_set_vitals(ctx, 50, 300)).unwrap();
    let flags_seen: Vec<u16> = use_seen(&t, 1, "threefold_cut", b.entity_id).iter().map(|e| e.flags).collect();
    let v = vitals_of(&t, b.entity_id);
    assert!(v.dead_at.is_some());
    assert_eq!(v.hp, 0);
    assert_eq!(flags_seen, [flags::HIT | flags::KILL]);
    assert!(read(&t, |ctx| crate::trade::carried(ctx, b.char_id).is_none()));
    assert_eq!(read(&t, |ctx| ctx.db.dropped_pack().iter().count()), 1);
    // The dead can't move, fight or pick up.
    advance_ms(&t, 500);
    assert_eq!(call(&t, 2, |ctx| move_event(ctx, 1, 0, 0.0, 0.0, -2.0, 0, 540, 1)).unwrap_err(), "you are dead");
    assert_eq!(use_on(&t, 2, "frostspike", a.entity_id).unwrap_err(), "you are dead");
    let pack_id = read(&t, |ctx| ctx.db.dropped_pack().iter().next().unwrap().id);
    assert_eq!(call(&t, 2, |ctx| crate::trade::pick_up_pack(ctx, pack_id)).unwrap_err(), "you are dead");
    // Too early, then at the nearer shrine with part of the HP and mana.
    assert_eq!(call(&t, 2, respawn).unwrap_err(), format!("you can respawn in {} s", COMBAT.respawn_first_s));
    assert_eq!(call(&t, 1, respawn).unwrap_err(), "you aren't dead");
    advance_ms(&t, i64::from(COMBAT.respawn_first_s) * 1000);
    let before = presence_of(&t, 2);
    call(&t, 2, respawn).unwrap();
    let p = presence_of(&t, 2);
    // A few steps in front of it: shrines face north (-z) at yaw 0.
    assert_eq!((p.x, p.z, p.parent, p.corrections), (100.0, -crate::combat::SHRINE_STEP_M, 0, before.corrections + 1));
    let v = vitals_of(&t, b.entity_id);
    assert_eq!((v.hp, v.mp, v.dead_at), (respawn_share(HP), respawn_share(MP), None));
    assert_eq!(v.cell, transform_of(&t, b.entity_id).cell);
    assert_eq!(read(&t, |ctx| ctx.db.dropped_pack().iter().count()), 1, "the pack stays where it fell");
}

#[test]
fn deaths_in_quick_succession_lengthen_the_wait_until_they_expire() {
    let t = world();
    arrive(&t, 1, "Mira");
    let die_and_wait = |t: &TestContext| {
        call(t, 1, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
        call(t, 1, respawn).unwrap_err()
    };
    assert_eq!(die_and_wait(&t), format!("you can respawn in {} s", COMBAT.respawn_first_s));
    advance_ms(&t, i64::from(COMBAT.respawn_first_s) * 1000);
    call(&t, 1, respawn).unwrap();
    assert_eq!(die_and_wait(&t), format!("you can respawn in {} s", COMBAT.respawn_first_s + COMBAT.respawn_step_s));
    advance_ms(&t, i64::from(COMBAT.death_stack_s) * 1000);
    call(&t, 1, respawn).unwrap();
    assert_eq!(die_and_wait(&t), format!("you can respawn in {} s", COMBAT.respawn_first_s));
}

#[test]
fn vitals_and_death_survive_leaving_and_entering() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| dev_set_vitals(ctx, 300, 200)).unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    advance_ms(&t, 10_000);
    call(&t, 1, |ctx| enter_world(ctx, a.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    let me = presence_of(&t, 1);
    assert_eq!(hp_mp(&t, me.entity_id), (300 + 10 * COMBAT.hp_regen_out, 200 + 10 * COMBAT.mp_regen_out));
    call(&t, 1, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    call(&t, 1, |ctx| enter_world(ctx, a.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    assert!(vitals_of(&t, presence_of(&t, 1).entity_id).dead_at.is_some());
}

#[test]
fn a_dummy_is_back_to_full_when_killed_and_can_be_removed() {
    let t = world();
    arrive(&t, 1, "Mira");
    veteran(&t, 1);
    call(&t, 1, |ctx| dev_spawn_dummy(ctx, 0.0, -3.0)).unwrap();
    let dummy = read(&t, |ctx| ctx.db.entity().iter().find(|e| e.kind == entity_kind::DUMMY).unwrap().id);
    read(&t, |ctx| {
        let v = ctx.db.vitals().entity_id().find(dummy).unwrap();
        ctx.db.vitals().entity_id().update(Vitals { hp: 50, ..v });
    });
    let seen: Vec<u16> = use_seen(&t, 1, "threefold_cut", dummy).iter().map(|e| e.flags).collect();
    let v = vitals_of(&t, dummy);
    assert_eq!((v.hp, v.dead_at), (COMBAT.dummy_hp, None));
    assert_eq!(seen, [flags::HIT | flags::KILL]);
    call(&t, 1, |ctx| dev_remove_dummy(ctx, dummy)).unwrap();
    assert!(read(&t, |ctx| ctx.db.vitals().entity_id().find(dummy).is_none()));
    assert!(call(&t, 1, |ctx| dev_remove_dummy(ctx, dummy)).is_err());
}

#[test]
fn only_admins_place_shrines_and_without_any_the_dead_respawn_at_spawn() {
    let t = world();
    arrive(&t, 1, "Mira");
    assert_eq!(call(&t, 1, admin_clear_shrines).unwrap_err(), "admins only");
    shrines(&t, &[("cairnwick_wayshrine", 50.0, 50.0)]);
    let bad = ShrinePlacement { kind: 200, x: 0.0, y: 0.0, z: 0.0, yaw: 0 };
    assert!(call(&t, ADMIN, |ctx| admin_put_shrines(ctx, vec![bad])).is_err());
    assert_eq!(read(&t, |ctx| ctx.db.shrine().count()), 1);
    call(&t, ADMIN, admin_clear_shrines).unwrap();
    put(&t, 1, 30.0, 30.0);
    call(&t, 1, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    advance_ms(&t, i64::from(COMBAT.respawn_first_s) * 1000);
    call(&t, 1, respawn).unwrap();
    let p = presence_of(&t, 1);
    assert_eq!((p.x, p.z), (SPAWN[0], SPAWN[2]));
}

fn status_id(id: &str) -> u8 {
    sim::combat::status_by_id(id).unwrap().wire
}

/// The statuses active on `entity_id` now, by content id.
fn statuses_on(t: &TestContext, entity_id: u64) -> Vec<&'static str> {
    let now = read(t, crate::time::now);
    let mut ids: Vec<_> = vitals_of(t, entity_id)
        .statuses
        .iter()
        .filter(|s| s.start <= now && now < s.until)
        .map(|s| sim::combat::status(s.status).unwrap().id)
        .collect();
    ids.sort();
    ids
}

/// The status events in `events`: (status id, ms, flags).
fn status_events(events: &[CombatEvent]) -> Vec<(&'static str, i32, u16)> {
    events
        .iter()
        .filter(|e| e.flags & (flags::STATUS | flags::IMMUNE) != 0)
        .map(|e| (sim::combat::status(e.status).unwrap().id, e.amount, e.flags))
        .collect()
}

/// Run the tick's over-time step, returning the events it made.
fn tick(t: &TestContext) -> Vec<CombatEvent> {
    read(t, |ctx| {
        crate::combat::run_periodic(ctx);
        ctx.db.combat_event().iter().collect()
    })
}

#[test]
fn skills_need_a_picked_skillset_and_the_level() {
    let t = world();
    arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    put(&t, 2, 0.0, -3.0);
    assert_eq!(use_on(&t, 1, "threefold_cut", b.entity_id).unwrap_err(), "you haven't learned that skill");
    assert_eq!(call(&t, 1, |ctx| pick_skillset(ctx, 9)).unwrap_err(), "no such skillset");
    call(&t, 1, |ctx| pick_skillset(ctx, 1)).unwrap();
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    advance_ms(&t, 2000);
    assert_eq!(use_on(&t, 1, "shield_bash", b.entity_id).unwrap_err(), "you haven't learned that skill", "level 5");
    assert_eq!(use_on(&t, 1, "frostspike", b.entity_id).unwrap_err(), "you haven't learned that skill", "not picked");
    assert_eq!(call(&t, 1, |ctx| pick_skillset(ctx, 1)).unwrap_err(), "already picked");
    assert_eq!(call(&t, 1, |ctx| pick_skillset(ctx, 0)).unwrap_err(), "you can pick another skillset at level 5");
    call(&t, 1, |ctx| dev_set_level(ctx, 5)).unwrap();
    use_on(&t, 1, "shield_bash", b.entity_id).unwrap();
    call(&t, 1, |ctx| pick_skillset(ctx, 0)).unwrap();
    assert_eq!(call(&t, 1, |ctx| pick_skillset(ctx, 2)).unwrap_err(), "you can pick another skillset at level 10");
    call(&t, 1, |ctx| dev_set_level(ctx, 15)).unwrap();
    call(&t, 1, |ctx| pick_skillset(ctx, 2)).unwrap();
    let f = read(&t, |ctx| ctx.db.fighter().iter().find(|f| f.owner == identity(1)).unwrap());
    assert_eq!((f.level, f.skillsets), (15, vec![1, 0, 2]));
    assert_eq!(call(&t, 1, |ctx| dev_set_level(ctx, 16)).unwrap_err(), "levels are 1 to 15");
}

#[test]
fn a_status_lands_lasts_its_time_and_refreshes_rather_than_stacking() {
    let (t, _a, b) = duel(3.0);
    let seen = use_seen(&t, 1, "sundering_cleave", b.entity_id);
    assert!(status_events(&seen).is_empty(), "a timed cast lands when it finishes");
    let cleave = skill_by_id("sundering_cleave").unwrap();
    advance_ms(&t, i64::from(cleave.cast_ms));
    finish_cast(&t, _a.entity_id);
    assert_eq!(statuses_on(&t, b.entity_id), ["sundered"]);
    advance_ms(&t, 7_900);
    assert_eq!(statuses_on(&t, b.entity_id), ["sundered"]);
    advance_ms(&t, 100);
    assert!(statuses_on(&t, b.entity_id).is_empty(), "8 s and gone, with nothing ticked");
    // Hamstring twice: one Slowed slot, refreshed.
    let first = use_seen(&t, 1, "hamstring", b.entity_id);
    assert_eq!(status_events(&first), [("slowed", 6000, flags::STATUS)]);
    advance_ms(&t, 12_000);
    use_on(&t, 1, "hamstring", b.entity_id).unwrap();
    let v = vitals_of(&t, b.entity_id);
    assert_eq!(v.statuses.iter().filter(|s| s.status == status_id("slowed")).count(), 1);
}

#[test]
fn a_snare_slows_moves_and_a_stun_stops_moves_skills_and_casts() {
    let (t, a, b) = duel(3.0);
    use_on(&t, 1, "hamstring", b.entity_id).unwrap();
    // The first move after it landed is judged at full speed; the client hadn't heard yet.
    advance_ms(&t, 500);
    step(&t, 2, 1, 0.0, -3.0 - 2.7);
    advance_ms(&t, 1000);
    let before = presence_of(&t, 2).corrections;
    step(&t, 2, 2, 0.0, -5.7 - 5.4);
    assert_eq!(presence_of(&t, 2).corrections, before + 1, "full speed while Slowed is corrected");
    advance_ms(&t, 1000);
    step(&t, 2, 3, 0.0, -5.7 - 3.0);
    assert_eq!(presence_of(&t, 2).corrections, before + 1, "60% is allowed");
    // Tobin starts a cast; Shield Bash stuns him, which ends it.
    put(&t, 2, 0.0, -3.0);
    call(&t, 2, |ctx| move_event(ctx, 10, 0, 0.0, 0.0, -3.0, 32768, 0, 1)).unwrap();
    call(&t, 2, |ctx| dev_set_level(ctx, 15)).unwrap();
    use_on(&t, 2, "cinder_dart", a.entity_id).unwrap();
    assert!(cast_of(&t, b.entity_id).is_some());
    advance_ms(&t, i64::from(COMBAT.gcd_ms));
    let seen = use_seen(&t, 1, "shield_bash", b.entity_id);
    assert_eq!(status_events(&seen), [("stunned", 2000, flags::STATUS)]);
    assert!(cast_of(&t, b.entity_id).is_none());
    assert_eq!(vitals_of(&t, b.entity_id).cast_skill, 0);
    advance_ms(&t, 1000);
    assert_eq!(use_on(&t, 2, "frostspike", a.entity_id).unwrap_err(), "you can't act right now");
    assert_eq!(call(&t, 2, crate::trade::drop_pack).unwrap_err(), "you can't act right now");
    advance_ms(&t, 300);
    step(&t, 2, 11, 0.0, -3.5);
    advance_ms(&t, 300);
    let before = presence_of(&t, 2).corrections;
    step(&t, 2, 12, 0.0, -4.5);
    assert_eq!(presence_of(&t, 2).corrections, before + 1, "stunned, moving is corrected");
    advance_ms(&t, 1000);
    call(&t, 2, |ctx| move_event(ctx, 13, 0, 0.0, 0.0, -3.5, 32768, 0, 1)).unwrap();
    use_on(&t, 2, "frostspike", a.entity_id).unwrap();
}

#[test]
fn controls_diminish_in_a_row_and_the_window_resets() {
    let (t, _a, b) = duel(3.0);
    let bash = |t: &TestContext| status_events(&use_seen(t, 1, "shield_bash", b.entity_id));
    assert_eq!(bash(&t), [("stunned", 2000, flags::STATUS)]);
    advance_ms(&t, 15_000);
    assert_eq!(bash(&t), [("stunned", 1000, flags::STATUS)]);
    // A snare is in its own group; a knockdown is in the stun group, and third in a row.
    advance_ms(&t, 1000);
    assert_eq!(status_events(&use_seen(&t, 1, "hamstring", b.entity_id)), [("slowed", 6000, flags::STATUS)]);
    advance_ms(&t, 1000);
    assert_eq!(status_events(&use_seen(&t, 1, "threefold_cut", b.entity_id)), [("felled", 375, flags::STATUS)]);
    advance_ms(&t, 13_000);
    assert_eq!(bash(&t), [("stunned", 0, flags::IMMUNE)]);
    assert!(statuses_on(&t, b.entity_id).is_empty());
    // The window ran 15 s past the end of the knockdown.
    advance_ms(&t, 15_000);
    assert_eq!(bash(&t), [("stunned", 2000, flags::STATUS)]);
}

#[test]
fn combos_land_off_statuses_from_other_skillsets_but_never_off_their_own() {
    let (t, a, b) = duel(10.0);
    let lash = skill_by_id("brine_lash").unwrap();
    // Frostspike without Soaked: damage and Slowed, no stun.
    let plain = use_seen(&t, 1, "frostspike", b.entity_id);
    assert_eq!(status_events(&plain), [("slowed", 4000, flags::STATUS)]);
    advance_ms(&t, 1000);
    use_on(&t, 1, "brine_lash", b.entity_id).unwrap();
    advance_ms(&t, i64::from(lash.cast_ms));
    finish_cast(&t, a.entity_id);
    assert_eq!(statuses_on(&t, b.entity_id), ["slowed", "soaked"]);
    advance_ms(&t, 8_000 - i64::from(lash.cast_ms) - 1000);
    let combo = use_seen(&t, 1, "frostspike", b.entity_id);
    assert_eq!(status_events(&combo), [("slowed", 2000, flags::STATUS), ("stunned", 2000, flags::STATUS)], "the second snare in a row, at 50%");
    // Slowed lets Threefold Cut fell, and Rending Blow hits the Felled harder.
    let (t, _a, b) = duel(3.0);
    use_on(&t, 1, "hamstring", b.entity_id).unwrap();
    advance_ms(&t, 1000);
    assert_eq!(status_events(&use_seen(&t, 1, "threefold_cut", b.entity_id)), [("felled", 1500, flags::STATUS)]);
    advance_ms(&t, 1000);
    let (hp, _) = hp_mp(&t, b.entity_id);
    let rend = skill_by_id("rending_blow").unwrap();
    let hits: Vec<i32> = use_seen(&t, 1, "rending_blow", b.entity_id).iter().filter(|e| e.flags == flags::HIT).map(|e| e.amount).collect();
    assert_eq!(hits, [rend.effects[0].amount, rend.effects[1].amount]);
    assert_eq!(hp_mp(&t, b.entity_id).0, hp - hits.iter().sum::<i32>());
    // Felled, then stunned (at 50%): another knockdown is the third in their group, at 25%.
    advance_ms(&t, 7000);
    use_on(&t, 1, "shield_bash", b.entity_id).unwrap();
    advance_ms(&t, 3000);
    use_on(&t, 1, "hamstring", b.entity_id).unwrap();
    advance_ms(&t, 1000);
    assert_eq!(status_events(&use_seen(&t, 1, "threefold_cut", b.entity_id)), [("felled", 375, flags::STATUS)]);
    // Undertow puts Slowed on and fells the Soaked, but not off the Slowed it brings itself.
    let (t, a, b) = duel(10.0);
    let undertow = skill_by_id("undertow").unwrap();
    use_on(&t, 1, "undertow", b.entity_id).unwrap();
    advance_ms(&t, i64::from(undertow.cast_ms));
    let events = {
        let job = cast_of(&t, a.entity_id).unwrap();
        t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| {
            cast_done(ctx, job)?;
            Ok(ctx.db.combat_event().iter().collect::<Vec<_>>())
        })
        .unwrap()
    };
    assert_eq!(status_events(&events), [("slowed", 5000, flags::STATUS)]);
}

#[test]
fn damage_is_changed_by_both_sides_statuses() {
    let (t, _a, b) = duel(3.0);
    let rend = skill_by_id("rending_blow").unwrap().effects[0].amount;
    use_on(&t, 1, "war_cry", 0).unwrap();
    assert_eq!(statuses_on(&t, _a.entity_id), ["fervor"]);
    advance_ms(&t, 1000);
    call(&t, 2, |ctx| dev_set_level(ctx, 15)).unwrap();
    use_on(&t, 2, "iron_hide", 0).unwrap();
    use_on(&t, 1, "rending_blow", b.entity_id).unwrap();
    assert_eq!(hp_mp(&t, b.entity_id).0, HP - sim::combat::scaled_damage(rend, 20, -30));
    assert_eq!(HP - hp_mp(&t, b.entity_id).0, rend * 120 / 100 * 70 / 100);
}

#[test]
fn damage_and_healing_over_time_tick_from_the_due_index_and_end() {
    let (t, _, b) = duel(10.0);
    let kindle = skill_by_id("kindle").unwrap();
    let per = kindle.effects[0].amount;
    use_on(&t, 1, "kindle", b.entity_id).unwrap();
    let rows: Vec<_> = read(&t, |ctx| ctx.db.periodic().iter().collect());
    assert_eq!(rows.len(), 1);
    assert!(tick(&t).is_empty(), "nothing due yet");
    advance_ms(&t, 1000);
    let first: Vec<_> = tick(&t).iter().map(|e| (e.target, e.amount, e.flags)).collect();
    assert_eq!(first, [(b.entity_id, per, flags::HIT | flags::TICK)]);
    // A dev clock jump catches up on every tick, exactly once each, and the row goes. (Identical
    // events in one transaction are one row: tables are sets.)
    call(&t, 1, |ctx| crate::time::dev_advance_clock(ctx, 60_000_000)).unwrap();
    assert_eq!(tick(&t).len(), 1);
    assert!(read(&t, |ctx| ctx.db.periodic().iter().next().is_none()));
    assert_eq!(hp_mp(&t, b.entity_id).0, HP - 7 * per, "regeneration made up the first before the jump");
    // Renewal heals over time; death ends what's ticking.
    call(&t, 2, |ctx| dev_set_vitals(ctx, 500, MP)).unwrap();
    let tide = skill_by_id("renewing_tide").unwrap().effects[0].amount;
    use_on(&t, 1, "renewing_tide", b.entity_id).unwrap();
    advance_ms(&t, 2000);
    let healed: Vec<_> = tick(&t).iter().map(|e| (e.amount, e.flags)).collect();
    assert_eq!(healed, [(tide, flags::HEAL | flags::TICK)]);
    let row = read(&t, |ctx| ctx.db.periodic().iter().next().unwrap());
    assert_eq!(row.due - row.start.to_micros_since_unix_epoch(), 3_000_000, "two ticks done, the third next");
    advance_ms(&t, 1000);
    use_on(&t, 1, "kindle", b.entity_id).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.periodic().count()), 2);
    call(&t, 2, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    assert!(read(&t, |ctx| ctx.db.periodic().iter().next().is_none()));
    assert!(vitals_of(&t, b.entity_id).statuses.is_empty());
}

#[test]
fn a_cleanse_ends_controls_and_leaves_other_debuffs() {
    let (t, a, b) = duel(3.0);
    let cleave = skill_by_id("sundering_cleave").unwrap();
    use_on(&t, 1, "sundering_cleave", b.entity_id).unwrap();
    advance_ms(&t, i64::from(cleave.cast_ms));
    finish_cast(&t, a.entity_id);
    use_on(&t, 1, "hamstring", b.entity_id).unwrap();
    advance_ms(&t, 1000);
    use_on(&t, 1, "shield_bash", b.entity_id).unwrap();
    call(&t, 2, |ctx| move_event(ctx, 1, 0, 0.0, 0.0, -3.0, 32768, 0, 1)).unwrap();
    assert_eq!(use_on(&t, 2, "frostspike", a.entity_id).unwrap_err(), "you can't act right now");
    assert_eq!(statuses_on(&t, b.entity_id), ["slowed", "stunned", "sundered"]);
    advance_ms(&t, 1000);
    use_on(&t, 1, "cleansing_spring", b.entity_id).unwrap();
    assert_eq!(statuses_on(&t, b.entity_id), ["sundered"]);
    use_on(&t, 2, "frostspike", a.entity_id).unwrap();
}

#[test]
fn kills_of_non_characters_give_experience_and_levels_and_pve_deaths_cost_some() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    put(&t, 2, 0.0, -3.0);
    call(&t, 1, |ctx| pick_skillset(ctx, 1)).unwrap();
    call(&t, 1, |ctx| dev_spawn_dummy(ctx, 0.0, -3.0)).unwrap();
    let dummy = read(&t, |ctx| ctx.db.entity().iter().find(|e| e.kind == entity_kind::DUMMY).unwrap().id);
    let fighter = |t: &TestContext| read(t, |ctx| ctx.db.fighter().char_id().find(a.char_id).unwrap());
    let kill = |t: &TestContext| {
        read(t, |ctx| {
            let v = ctx.db.vitals().entity_id().find(dummy).unwrap();
            ctx.db.vitals().entity_id().update(Vitals { hp: 10, ..v });
        });
        advance_ms(t, 1000);
        use_seen(t, 1, "threefold_cut", dummy)
    };
    let seen = kill(&t);
    let xp = sim::combat::PROGRESS.kill_xp;
    assert!(seen.iter().any(|e| e.flags == flags::XP && e.amount == xp as i32 && e.target == a.entity_id));
    assert_eq!((fighter(&t).level, fighter(&t).xp), (1, xp));
    kill(&t);
    let seen = kill(&t);
    assert!(seen.iter().any(|e| e.flags == flags::LEVEL && e.amount == 2));
    let need = sim::combat::xp_to_next(1);
    assert_eq!((fighter(&t).level, fighter(&t).xp), (2, 3 * xp - need));
    let v = vitals_of(&t, a.entity_id);
    assert_eq!((v.level, v.hp_max, v.mp_max), (2, sim::combat::hp_max_at(2), sim::combat::mp_max_at(2)));
    assert_eq!(hp_mp(&t, a.entity_id).0, v.hp_max, "a level's extra HP comes with it");
    // A level-2 kill of a level-1 dummy is worth less.
    kill(&t);
    assert_eq!(fighter(&t).xp, 3 * xp - need + sim::combat::kill_xp(1, 2));
    // Killing a character gives nothing, and dying to one costs nothing; dying otherwise does.
    let before = fighter(&t).xp;
    call(&t, 2, |ctx| dev_set_level(ctx, 15)).unwrap();
    call(&t, 2, |ctx| pick_skillset(ctx, 1)).unwrap();
    call(&t, 2, |ctx| move_event(ctx, 1, 0, 0.0, 0.0, -3.0, 32768, 0, 1)).unwrap();
    call(&t, 1, |ctx| dev_set_vitals(ctx, 10, 100)).unwrap();
    use_on(&t, 2, "threefold_cut", a.entity_id).unwrap();
    assert!(vitals_of(&t, a.entity_id).dead_at.is_some());
    assert_eq!(fighter(&t).xp, before);
    advance_ms(&t, i64::from(COMBAT.respawn_first_s) * 1000);
    call(&t, 1, respawn).unwrap();
    call(&t, 1, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    assert_eq!(fighter(&t).xp, sim::combat::xp_after_death(2, before));
    assert!(fighter(&t).xp < before);
}
