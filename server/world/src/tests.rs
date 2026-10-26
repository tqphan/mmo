//! Reducer tests against SpacetimeDB's in-memory test datastore.

use spacetimedb::test_utils::{TestAuth, TestContext};
use spacetimedb::{ConnectionId, Identity, ReducerContext, Table, TimeDuration, Timestamp};

use crate::movement::{move_event, moved, suspicion};
use crate::session::*;
use sim::content::CONTENT_HASH;

const T0: i64 = 1_800_000_000_000_000; // an arbitrary start time, in µs

fn world() -> TestContext {
    let t = TestContext::new().expect("test context");
    t.clock.set(Timestamp::from_micros_since_unix_epoch(T0));
    t
}

fn auth(player: u128) -> TestAuth {
    let payload = format!(r#"{{"iss":"test","sub":"player{player}","iat":0}}"#);
    TestAuth::from_jwt_payload(payload, ConnectionId::from_u128(player)).unwrap()
}

fn identity(player: u128) -> Identity {
    Identity::from_claims("test", &format!("player{player}"))
}

/// Run `f` as `player`, committing on `Ok`.
fn call<T>(t: &TestContext, player: u128, f: impl FnOnce(&ReducerContext) -> Result<T, String>) -> Result<T, String> {
    t.with_reducer_tx(auth(player), f)
}

fn read<T>(t: &TestContext, f: impl FnOnce(&ReducerContext) -> T) -> T {
    t.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| Ok(f(ctx))).unwrap()
}

/// Connect `player`, create a character called `name` and enter the world with it.
fn arrive(t: &TestContext, player: u128, name: &str) -> Presence {
    call(t, player, |ctx| {
        on_connect(ctx);
        Ok(())
    })
    .unwrap();
    call(t, player, |ctx| create_character(ctx, name.into())).unwrap();
    let char_id = read(t, |ctx| ctx.db.character_private().owner().filter(identity(player)).last().unwrap().char_id);
    call(t, player, |ctx| enter_world(ctx, char_id, CONTENT_HASH)).unwrap();
    read(t, |ctx| ctx.db.presence().identity().find(identity(player)).unwrap())
}

fn transform_of(t: &TestContext, entity_id: u64) -> Transform {
    read(t, |ctx| ctx.db.transform().entity_id().find(entity_id).unwrap())
}

fn presence_of(t: &TestContext, player: u128) -> Presence {
    read(t, |ctx| ctx.db.presence().identity().find(identity(player)).unwrap())
}

fn advance_ms(t: &TestContext, ms: i64) {
    t.clock.advance(TimeDuration::from_micros(ms * 1000));
}

/// Run the tick's publish step: what observers see catches up with `presence`.
fn publish(t: &TestContext) -> usize {
    read(t, crate::movement::publish_moves)
}

fn step(t: &TestContext, player: u128, seq: u32, x: f32, z: f32) {
    call(t, player, |ctx| move_event(ctx, seq, x, 0.0, z, 0, 540, 1)).unwrap();
}

#[test]
fn connecting_creates_one_account() {
    let t = world();
    for _ in 0..2 {
        call(&t, 1, |ctx| {
            on_connect(ctx);
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(read(&t, |ctx| ctx.db.account().count()), 1);
}

#[test]
fn names_are_validated_and_unique_ignoring_case() {
    let t = world();
    for p in [1, 2] {
        call(&t, p, |ctx| {
            on_connect(ctx);
            Ok(())
        })
        .unwrap();
    }
    assert!(call(&t, 1, |ctx| create_character(ctx, "ab".into())).is_err());
    assert!(call(&t, 1, |ctx| create_character(ctx, "Bad Name".into())).is_err());
    assert!(call(&t, 1, |ctx| create_character(ctx, "Ñame".into())).is_err());
    call(&t, 1, |ctx| create_character(ctx, "mira".into())).unwrap();
    assert!(call(&t, 2, |ctx| create_character(ctx, "MIRA".into())).is_err());
    let names: Vec<String> = read(&t, |ctx| ctx.db.character_public().iter().map(|c| c.name).collect());
    assert_eq!(names, vec!["Mira".to_string()]);
}

#[test]
fn character_limit_per_account() {
    let t = world();
    call(&t, 1, |ctx| {
        on_connect(ctx);
        Ok(())
    })
    .unwrap();
    for name in ["Alda", "Brin", "Cass", "Dorn"] {
        call(&t, 1, |ctx| create_character(ctx, name.into())).unwrap();
    }
    assert!(call(&t, 1, |ctx| create_character(ctx, "Esk".into())).is_err());
}

#[test]
fn creating_a_character_needs_an_account() {
    let t = world();
    assert!(call(&t, 1, |ctx| create_character(ctx, "Mira".into())).is_err());
}

#[test]
fn entering_spawns_entity_and_transform_in_the_spawn_cell() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    let tr = transform_of(&t, p.entity_id);
    assert_eq!((tr.x, tr.z), (SPAWN[0], SPAWN[2]));
    assert_eq!(tr.cell, sim::cell::cell_of(0, SPAWN[0], SPAWN[2]));
    let e = read(&t, |ctx| ctx.db.entity().id().find(p.entity_id).unwrap());
    assert_eq!((e.kind, e.ref_id, e.cell), (entity_kind::CHARACTER, p.char_id, tr.cell));
}

#[test]
fn cannot_enter_with_someone_elses_character() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    call(&t, 2, |ctx| {
        on_connect(ctx);
        Ok(())
    })
    .unwrap();
    assert!(call(&t, 2, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH)).is_err());
}

#[test]
fn a_client_with_other_content_is_refused() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    let err = call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH ^ 1)).unwrap_err();
    assert!(err.contains("does not match"), "{err}");
    assert_eq!(read(&t, |ctx| ctx.db.presence().count()), 0);
    assert_eq!(read(&t, |ctx| ctx.db.entity().count()), 0);
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH)).unwrap();
}

#[test]
fn re_entering_replaces_the_old_entity() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH)).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.entity().count()), 1);
    assert_eq!(read(&t, |ctx| ctx.db.transform().count()), 1);
    assert_ne!(presence_of(&t, 1).entity_id, p.entity_id);
}

#[test]
fn a_legal_run_is_accepted_and_changes_cell() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    // Run east at 5.4 m/s with 0.5 s heartbeats for 14 s: 75.6 m, past the 64 m cell boundary.
    for i in 1..=28u32 {
        advance_ms(&t, 500);
        step(&t, 1, i, 2.7 * i as f32, 0.0);
    }
    assert!((presence_of(&t, 1).x - 75.6).abs() < 1e-3);
    assert_eq!(publish(&t), 1);
    let tr = transform_of(&t, p.entity_id);
    assert!((tr.x - 75.6).abs() < 1e-3);
    assert_eq!(tr.cell, sim::cell::cell_of(0, 75.6, 0.0));
    let e = read(&t, |ctx| ctx.db.entity().id().find(p.entity_id).unwrap());
    assert_eq!(e.cell, tr.cell);
    assert_eq!(presence_of(&t, 1).corrections, 0);
    assert_eq!(read(&t, |ctx| ctx.db.suspicion().count()), 0);
}

#[test]
fn a_speed_hack_is_corrected_and_logged() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    advance_ms(&t, 500);
    step(&t, 1, 1, 20.0, 0.0); // 40 m/s
    let pr = presence_of(&t, 1);
    assert_eq!((pr.x, pr.z, pr.speed), (0.0, 0.0, 0));
    assert_eq!((pr.corrections, pr.last_seq), (1, 1));
    publish(&t);
    let tr = transform_of(&t, p.entity_id);
    assert_eq!((tr.x, tr.z, tr.speed), (0.0, 0.0, 0));
    assert_eq!(read(&t, |ctx| ctx.db.suspicion().count()), 1);

    // The next legal step from the server's position is accepted again.
    advance_ms(&t, 500);
    step(&t, 1, 2, 2.0, 0.0);
    assert_eq!(presence_of(&t, 1).x, 2.0);
}

#[test]
fn going_quiet_does_not_allow_a_teleport() {
    let t = world();
    arrive(&t, 1, "Mira");
    advance_ms(&t, 60_000);
    step(&t, 1, 1, 300.0, 0.0);
    assert_eq!(presence_of(&t, 1).x, 0.0);
    assert_eq!(presence_of(&t, 1).corrections, 1);
}

#[test]
fn stale_and_duplicate_events_are_ignored() {
    let t = world();
    arrive(&t, 1, "Mira");
    advance_ms(&t, 500);
    step(&t, 1, 5, 1.0, 0.0);
    advance_ms(&t, 500);
    step(&t, 1, 5, 2.0, 0.0);
    step(&t, 1, 3, 3.0, 0.0);
    assert_eq!(presence_of(&t, 1).x, 1.0);
    assert_eq!(presence_of(&t, 1).last_seq, 5);
}

#[test]
fn moving_requires_being_in_the_world() {
    let t = world();
    assert!(call(&t, 1, |ctx| move_event(ctx, 1, 0.0, 0.0, 0.0, 0, 0, 1)).is_err());
}

#[test]
fn unknown_modes_and_nan_positions_are_rejected() {
    let t = world();
    arrive(&t, 1, "Mira");
    assert!(call(&t, 1, |ctx| move_event(ctx, 1, 0.0, 0.0, 0.0, 0, 0, 99)).is_err());
    step(&t, 1, 2, f32::NAN, 0.0);
    assert_eq!(presence_of(&t, 1).x, 0.0);
    assert_eq!(presence_of(&t, 1).corrections, 1);
}

#[test]
fn leaving_saves_position_and_despawns() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    advance_ms(&t, 500);
    step(&t, 1, 1, 2.0, -1.0);
    call(&t, 1, |ctx| {
        on_disconnect(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.entity().count()), 0);
    assert_eq!(read(&t, |ctx| ctx.db.transform().count()), 0);
    assert_eq!(read(&t, |ctx| ctx.db.moved().count()), 0, "the unpublished move goes too");
    assert!(read(&t, |ctx| ctx.db.presence().identity().find(identity(1))).is_none());
    let c = read(&t, |ctx| ctx.db.character_private().char_id().find(p.char_id).unwrap());
    assert_eq!((c.x, c.z), (2.0, -1.0));

    // Coming back puts the character where it left.
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH)).unwrap();
    let tr = transform_of(&t, presence_of(&t, 1).entity_id);
    assert_eq!((tr.x, tr.z), (2.0, -1.0));
}

#[test]
fn two_players_see_each_other_in_public_tables() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    let cell = sim::cell::cell_of(0, 0.0, 0.0);
    let ids: Vec<u64> = read(&t, |ctx| ctx.db.transform().cell().filter(cell).map(|r| r.entity_id).collect());
    assert!(ids.contains(&a.entity_id) && ids.contains(&b.entity_id));
}

#[test]
fn views_show_only_your_own_rows() {
    let t = world();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    let ctx = t.view_context(auth(1));
    let mine = super::session::my_characters(&ctx);
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].owner, identity(1));
    assert_eq!(super::session::my_presence(&ctx).unwrap().identity, identity(1));
}

#[cfg(feature = "dev")]
#[test]
fn dev_clock_moves_game_time_forward_only() {
    use crate::time::{dev_advance_clock, now};
    let t = world();
    let before = read(&t, now);
    call(&t, 1, |ctx| dev_advance_clock(ctx, 3_600_000_000)).unwrap();
    let after = read(&t, now);
    assert_eq!(after.to_micros_since_unix_epoch() - before.to_micros_since_unix_epoch(), 3_600_000_000);
    assert!(call(&t, 1, |ctx| dev_advance_clock(ctx, -1)).is_err());
}

fn tick(t: &TestContext) -> crate::tick::TickState {
    use crate::tick::{tick_state, world_tick, world_tick_timer};
    t.with_reducer_tx(TestAuth::internal(), |ctx| {
        let timer = ctx.db.world_tick_timer().iter().next().expect("tick timer");
        world_tick(ctx, timer)
    })
    .unwrap();
    read(t, |ctx| ctx.db.tick_state().id().find(0).unwrap())
}

fn started() -> TestContext {
    let t = world();
    read(&t, crate::init);
    t
}

#[test]
fn init_starts_one_10hz_tick() {
    use crate::tick::world_tick_timer;
    let t = started();
    arrive(&t, 1, "Mira"); // connecting must not start a second one
    let timers: Vec<_> = read(&t, |ctx| ctx.db.world_tick_timer().iter().map(|r| r.scheduled_at).collect());
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0], spacetimedb::ScheduleAt::Interval(TimeDuration::from_micros(sim::tick::TICK_MICROS)));
}

#[test]
fn connecting_starts_the_tick_on_an_older_database() {
    use crate::tick::world_tick_timer;
    let t = world(); // no init: as if published before the tick existed
    arrive(&t, 1, "Mira");
    assert_eq!(read(&t, |ctx| ctx.db.world_tick_timer().count()), 1);
}

#[test]
fn the_tick_steps_by_measured_time() {
    let t = started();
    let first = tick(&t);
    assert_eq!((first.ticks, first.last_dt), (1, 0.0));
    advance_ms(&t, 100);
    assert_eq!(tick(&t).last_dt, 0.1);
    advance_ms(&t, 137); // a late tick steps further
    let s = tick(&t);
    assert_eq!((s.ticks, s.last_dt), (3, 0.137));
    assert_eq!(s.last_at, Timestamp::from_micros_since_unix_epoch(T0 + 237_000));
}

#[test]
fn a_stalled_tick_steps_at_most_max_dt() {
    let t = started();
    tick(&t);
    advance_ms(&t, 5_000);
    assert_eq!(tick(&t).last_dt, sim::tick::MAX_DT);
}

#[test]
fn clients_cannot_call_the_tick() {
    use crate::tick::{tick_state, world_tick, world_tick_timer};
    let t = started();
    let r = call(&t, 1, |ctx| {
        let timer = ctx.db.world_tick_timer().iter().next().unwrap();
        world_tick(ctx, timer)
    });
    assert!(r.is_err());
    assert_eq!(read(&t, |ctx| ctx.db.tick_state().count()), 0);
}

#[cfg(feature = "dev")]
#[test]
fn a_dev_clock_jump_does_not_jump_the_tick() {
    use crate::time::dev_advance_clock;
    let t = started();
    tick(&t);
    call(&t, 1, |ctx| dev_advance_clock(ctx, 86_400_000_000)).unwrap();
    advance_ms(&t, 100);
    assert_eq!(tick(&t).last_dt, sim::tick::MAX_DT);
    advance_ms(&t, 100);
    assert_eq!(tick(&t).last_dt, 0.1);
}

#[cfg(feature = "dev")]
#[test]
fn dev_view_shows_the_tick_state() {
    let t = started();
    tick(&t);
    let s = crate::tick::dev_tick(&t.view_context(auth(1))).unwrap();
    assert_eq!(s.ticks, 1);
}

#[cfg(feature = "dev")]
#[test]
fn dev_teleport_moves_and_restarts_the_move_clock() {
    use crate::dev::dev_teleport;
    let t = world();
    let p = arrive(&t, 1, "Mira");
    advance_ms(&t, 10_000);
    call(&t, 1, |ctx| dev_teleport(ctx, 300.0, -200.0)).unwrap();
    let tr = transform_of(&t, p.entity_id);
    assert_eq!((tr.x, tr.z, tr.cell), (300.0, -200.0, sim::cell::cell_of(0, 300.0, -200.0)));
    let e = read(&t, |ctx| ctx.db.entity().id().find(p.entity_id).unwrap());
    assert_eq!(e.cell, tr.cell);
    // The next legal step counts from the new place and time, not the 10 s gap.
    advance_ms(&t, 500);
    step(&t, 1, 1, 302.0, -200.0);
    assert_eq!(presence_of(&t, 1).corrections, 0);
    assert!(call(&t, 1, |ctx| dev_teleport(ctx, f32::NAN, 0.0)).is_err());
}

#[cfg(feature = "dev")]
#[test]
fn dev_swing_needs_a_target_in_range() {
    use crate::dev::{combat_event, dev_swing};
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    call(&t, 1, |ctx| dev_swing(ctx, b.entity_id)).unwrap();
    assert!(call(&t, 1, |ctx| dev_swing(ctx, a.entity_id)).is_err(), "not yourself");
    assert!(call(&t, 1, |ctx| dev_swing(ctx, 999)).is_err(), "no such target");
    call(&t, 2, |ctx| crate::dev::dev_teleport(ctx, 30.0, 0.0)).unwrap();
    assert!(call(&t, 1, |ctx| dev_swing(ctx, b.entity_id)).is_err(), "out of range");
    // Event rows are only visible inside the transaction that inserts them.
    let seen = call(&t, 2, |ctx| {
        crate::dev::dev_teleport(ctx, 10.0, 0.0)?;
        dev_swing(ctx, a.entity_id)?;
        Ok(ctx.db.combat_event().count())
    })
    .unwrap();
    assert_eq!(seen, 1);
}

#[test]
fn others_see_moves_only_when_the_tick_publishes_them() {
    let t = started();
    let p = arrive(&t, 1, "Mira");
    for (seq, x) in [(1, 2.0), (2, 4.0)] {
        advance_ms(&t, 500);
        step(&t, 1, seq, x, 0.0);
    }
    // Two moves, one pending publish, and the public row hasn't moved yet.
    assert_eq!(read(&t, |ctx| ctx.db.moved().count()), 1);
    assert_eq!(transform_of(&t, p.entity_id).x, 0.0);
    tick(&t);
    let tr = transform_of(&t, p.entity_id);
    assert_eq!((tr.x, tr.speed), (4.0, 540));
    assert_eq!(read(&t, |ctx| ctx.db.moved().count()), 0);
    // Nothing moved since, so the next tick publishes nothing.
    assert_eq!(publish(&t), 0);
}
