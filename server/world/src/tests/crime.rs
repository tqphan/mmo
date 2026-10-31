//! Crime (docs/design/crime.md).

use super::combat::{hp_mp, put, use_on, veteran, vitals_of};
use super::*;
use crate::combat::{dev_set_vitals, dev_spawn_dummy};
use crate::crime::{CrimeRecord, Evidence, EvidenceSecret, crime_record, evidence, evidence_fade, evidence_secret, fade_evidence, my_crime, report_evidence};
use crate::dev::dev_set_crime;
use crate::time::dev_advance_clock;
use crate::trade::{dev_give_pack, drop_pack, dropped_pack, pick_up_pack};
use sim::combat::{COMBAT, skill_by_id};
use sim::crime::{CRIME, Evidence as Kind};

/// Mira at the origin facing north (-z), Tobin `d` m in front of her, both top level.
fn duel(d: f32) -> (TestContext, Presence, Presence) {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    veteran(&t, 1);
    veteran(&t, 2);
    put(&t, 2, 0.0, -d);
    (t, a, b)
}

fn found(t: &TestContext) -> Vec<(Evidence, EvidenceSecret)> {
    read(t, |ctx| {
        let mut all: Vec<_> = ctx.db.evidence().iter().map(|e| (e.clone(), ctx.db.evidence_secret().evidence_id().find(e.id).unwrap())).collect();
        all.sort_by_key(|(e, _)| e.id);
        all
    })
}

/// The kinds, offenders and victims of the evidence lying about, oldest first.
fn kinds(t: &TestContext) -> Vec<(Kind, u64, u64)> {
    found(t).into_iter().map(|(e, s)| (Kind::from_u8(e.kind).unwrap(), s.offender, s.victim)).collect()
}

fn marked(t: &TestContext, entity_id: u64) -> bool {
    let v = vitals_of(t, entity_id);
    read(t, |ctx| crate::crime::marked(&v, crate::time::now(ctx)))
}

fn record(t: &TestContext, char_id: u64) -> Option<CrimeRecord> {
    read(t, |ctx| ctx.db.crime_record().char_id().find(char_id))
}

fn report(t: &TestContext, player: u128, id: u64) -> Result<(), String> {
    call(t, player, |ctx| report_evidence(ctx, id))
}

/// Turn `player` to `yaw` on the wire: 0 faces north (-z), 32768 south.
fn face(t: &TestContext, player: u128, yaw: u16) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        ctx.db.presence().identity().update(Presence { yaw, ..p });
    });
}

/// Wait out the global cooldown.
fn gcd(t: &TestContext) {
    advance_ms(t, i64::from(COMBAT.gcd_ms));
}

#[test]
fn assault_marks_the_attacker_and_leaves_one_stain_a_bout() {
    let (t, a, b) = duel(3.0);
    use_on(&t, 1, "rending_blow", b.entity_id).unwrap();
    assert!(marked(&t, a.entity_id));
    assert!(!marked(&t, b.entity_id));
    assert_eq!(kinds(&t), [(Kind::Bloodstain, a.char_id, b.char_id)]);
    let (e, s) = &found(&t)[0];
    let at = presence_of(&t, 2);
    assert_eq!((e.x, e.z, e.cell, s.points), (at.x, at.z, sim::cell::cell_of(sim::cell::OPEN_WORLD, at.x, at.z), CRIME.bloodstain_points.unsigned_abs()));
    // More hits in the same bout leave no more.
    gcd(&t);
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    assert_eq!(found(&t).len(), 1);
    // Tobin fights back: Mira is fair game.
    face(&t, 2, 32768);
    use_on(&t, 2, "threefold_cut", a.entity_id).unwrap();
    assert!(!marked(&t, b.entity_id));
    assert_eq!(found(&t).len(), 1);
    // A bout begins again once the mark has run out.
    advance_ms(&t, i64::from(CRIME.mark_s) * 1000);
    assert!(!marked(&t, a.entity_id));
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    assert_eq!(kinds(&t), [(Kind::Bloodstain, a.char_id, b.char_id); 2]);
}

#[test]
fn a_kill_leaves_a_blood_pool_unless_the_dead_was_fair_game() {
    let (t, a, b) = duel(3.0);
    call(&t, 2, |ctx| dev_set_vitals(ctx, 50, 300)).unwrap();
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    assert!(vitals_of(&t, b.entity_id).dead_at.is_some());
    assert_eq!(kinds(&t), [(Kind::Bloodstain, a.char_id, b.char_id), (Kind::BloodPool, a.char_id, b.char_id)]);
    // Mira is marked: killing her is no crime.
    let c = arrive(&t, 3, "Wren");
    veteran(&t, 3);
    put(&t, 3, 0.0, 3.0);
    call(&t, 1, |ctx| dev_set_vitals(ctx, 50, 300)).unwrap();
    use_on(&t, 3, "threefold_cut", a.entity_id).unwrap();
    assert!(vitals_of(&t, a.entity_id).dead_at.is_some());
    assert!(!marked(&t, c.entity_id));
    assert_eq!(found(&t).len(), 2);
}

#[test]
fn debuffs_and_their_ticks_are_assault_but_dummies_and_heals_are_not() {
    let (t, a, b) = duel(3.0);
    // A dummy, struck and burned: nothing.
    call(&t, 1, |ctx| dev_spawn_dummy(ctx, 1.5, -2.0)).unwrap();
    let dummy = read(&t, |ctx| ctx.db.entity().iter().find(|e| e.kind == entity_kind::DUMMY).unwrap().id);
    use_on(&t, 1, "kindle", dummy).unwrap();
    gcd(&t);
    use_on(&t, 1, "threefold_cut", dummy).unwrap();
    advance_ms(&t, 2000);
    read(&t, crate::combat::run_periodic);
    assert!(found(&t).is_empty());
    assert!(!marked(&t, a.entity_id));
    // A heal on Tobin: nothing.
    call(&t, 2, |ctx| dev_set_vitals(ctx, 500, 800)).unwrap();
    gcd(&t);
    use_on(&t, 1, "quick_balm", b.entity_id).unwrap();
    assert!(found(&t).is_empty());
    // Kindle deals nothing at once, only a debuff: that's assault, and its ticks are the same bout.
    advance_ms(&t, i64::from(skill_by_id("kindle").unwrap().cooldown_ms));
    use_on(&t, 1, "kindle", b.entity_id).unwrap();
    assert_eq!(kinds(&t), [(Kind::Bloodstain, a.char_id, b.char_id)]);
    let hp = hp_mp(&t, b.entity_id).0;
    advance_ms(&t, 1000);
    read(&t, crate::combat::run_periodic);
    assert!(hp_mp(&t, b.entity_id).0 < hp + 20, "it burned");
    assert_eq!(found(&t).len(), 1);
}

#[test]
fn taking_anothers_pack_leaves_footprints_and_ones_own_does_not() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let thief = arrive(&t, 2, "Tobin");
    call(&t, 1, |ctx| dev_give_pack(ctx, "grain_bale".into())).unwrap();
    call(&t, 1, drop_pack).unwrap();
    let pack = read(&t, |ctx| ctx.db.dropped_pack().iter().next().unwrap());
    call(&t, 1, |ctx| pick_up_pack(ctx, pack.id)).unwrap();
    assert!(found(&t).is_empty(), "taking one's own is no crime");
    call(&t, 1, drop_pack).unwrap();
    let pack = read(&t, |ctx| ctx.db.dropped_pack().iter().next().unwrap());
    call(&t, 2, |ctx| pick_up_pack(ctx, pack.id)).unwrap();
    assert_eq!(kinds(&t), [(Kind::Footprints, thief.char_id, me.char_id)]);
    let (e, s) = &found(&t)[0];
    assert_eq!((e.x, e.z, s.points), (pack.x, pack.z, CRIME.footprint_points.unsigned_abs()));
}

#[test]
fn stealing_a_crop_leaves_footprints_at_it() {
    use super::farming::{crops, farmer, ripen, sow};
    use super::land::{ground, set_position};
    let t = world();
    ground(&t);
    let me = farmer(&t, 1, "Mira");
    let thief = farmer(&t, 2, "Tobin");
    set_position(&t, 1, 0.0, 0.0);
    set_position(&t, 2, 0.0, 1.0);
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    sow(&t, 1, "hearthgrain_seed", -1.0, 0.0).unwrap();
    let (a, b) = (crops(&t)[0].clone(), crops(&t)[1].clone());
    ripen(&t, &a);
    call(&t, 2, |ctx| crate::farming::harvest(ctx, a.id)).unwrap();
    call(&t, 2, |ctx| crate::farming::uproot(ctx, b.id)).unwrap();
    assert_eq!(kinds(&t), [(Kind::Footprints, thief.char_id, me.char_id); 2]);
    let at: Vec<_> = found(&t).iter().map(|(e, _)| (e.x, e.z)).collect();
    assert_eq!(at, [(a.x, a.z), (b.x, b.z)]);
}

#[test]
fn reports_add_crime_points_and_notoriety_and_take_the_evidence() {
    let (t, a, b) = duel(3.0);
    use_on(&t, 1, "rending_blow", b.entity_id).unwrap();
    let id = found(&t)[0].0.id;
    // Mira can't; Tobin from afar can't; Tobin beside it can, once.
    assert_eq!(report(&t, 1, id).unwrap_err(), "you can't report your own crime");
    put(&t, 2, 0.0, -3.0 - CRIME.reach_m - 0.5);
    assert_eq!(report(&t, 2, id).unwrap_err(), "out of reach");
    put(&t, 2, 0.0, -3.0);
    report(&t, 2, id).unwrap();
    assert!(found(&t).is_empty());
    assert!(read(&t, |ctx| ctx.db.evidence_fade().iter().next().is_none()));
    assert_eq!(report(&t, 2, id).unwrap_err(), "no such evidence");
    let r = record(&t, a.char_id).unwrap();
    let points = CRIME.bloodstain_points.unsigned_abs();
    assert_eq!((r.crime_points, r.notoriety, r.reports, r.owner), (points, points, 1, identity(1)));
    // Only the owner sees it.
    let mine: Vec<u64> = my_crime(&t.view_context(auth(1))).into_iter().map(|r| r.char_id).collect();
    assert_eq!(mine, [a.char_id]);
    assert!(my_crime(&t.view_context(auth(2))).is_empty());
    // The dead can't report.
    advance_ms(&t, i64::from(CRIME.mark_s) * 1000);
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    let id = found(&t)[0].0.id;
    call(&t, 2, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    assert_eq!(report(&t, 2, id).unwrap_err(), "you are dead");
}

#[test]
fn evidence_fades_by_the_dev_clock() {
    let (t, _, b) = duel(3.0);
    use_on(&t, 1, "rending_blow", b.entity_id).unwrap();
    let id = found(&t)[0].0.id;
    call(&t, 1, |ctx| dev_advance_clock(ctx, i64::from(CRIME.evidence_s) * 1_000_000)).unwrap();
    assert_eq!(report(&t, 2, id).unwrap_err(), "this evidence has faded");
    // The dev clock rescheduled the fade for now; the scheduler runs it.
    let job = read(&t, |ctx| ctx.db.evidence_fade().evidence_id().find(id).unwrap());
    assert!(job.scheduled_at == spacetimedb::ScheduleAt::Time(read(&t, |ctx| ctx.timestamp)));
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| fade_evidence(ctx, job)).unwrap();
    assert!(found(&t).is_empty());
    assert!(read(&t, |ctx| ctx.db.evidence_fade().iter().next().is_none()));
}

#[test]
fn an_early_fade_is_rescheduled() {
    let (t, _, b) = duel(3.0);
    use_on(&t, 1, "rending_blow", b.entity_id).unwrap();
    let id = found(&t)[0].0.id;
    let job = read(&t, |ctx| ctx.db.evidence_fade().evidence_id().find(id).unwrap());
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| fade_evidence(ctx, job)).unwrap();
    assert_eq!(found(&t).len(), 1);
    assert!(read(&t, |ctx| ctx.db.evidence_fade().evidence_id().find(id).is_some()));
}

#[test]
fn enough_crime_points_make_a_character_sought_for_all_to_see() {
    let (t, a, b) = duel(3.0);
    call(&t, 1, |ctx| dev_set_crime(ctx, CRIME.sought_points.unsigned_abs() - 1, 0)).unwrap();
    assert!(!vitals_of(&t, a.entity_id).sought);
    call(&t, 2, |ctx| dev_set_vitals(ctx, 50, 300)).unwrap();
    use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
    let pool = found(&t).iter().find(|(e, _)| e.kind == Kind::BloodPool as u8).unwrap().0.id;
    // The dead Tobin can't report it, so a passer-by does.
    arrive(&t, 3, "Wren");
    put(&t, 3, 0.0, -3.0);
    report(&t, 3, pool).unwrap();
    assert!(vitals_of(&t, a.entity_id).sought);
    // It lasts across leaving and entering.
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    call(&t, 1, |ctx| enter_world(ctx, a.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    assert!(vitals_of(&t, presence_of(&t, 1).entity_id).sought);
    call(&t, 1, |ctx| dev_set_crime(ctx, 0, 11)).unwrap();
    assert!(!vitals_of(&t, presence_of(&t, 1).entity_id).sought);
    assert_eq!(record(&t, a.char_id).unwrap().reports, 1);
}
