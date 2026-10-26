//! Reducer tests against SpacetimeDB's in-memory test datastore.

use spacetimedb::test_utils::{TestAuth, TestContext};
use spacetimedb::{ConnectionId, Identity, ReducerContext, Table, TimeDuration, Timestamp};

use crate::movement::{move_event, moved, suspicion};
use crate::session::*;
use sim::content::CONTENT_HASH;

mod labor;
#[cfg(feature = "dev")]
mod ships;

const T0: i64 = 1_800_000_000_000_000; // an arbitrary start time, in µs

fn world() -> TestContext {
    let t = TestContext::new().expect("test context");
    t.clock.set(Timestamp::from_micros_since_unix_epoch(T0));
    t
}

fn auth(player: u128) -> TestAuth {
    auth_on(player, player)
}

/// `player` on connection `conn`: a reconnecting client keeps its identity but gets a new connection.
fn auth_on(player: u128, conn: u128) -> TestAuth {
    let payload = format!(r#"{{"iss":"test","sub":"player{player}","iat":0}}"#);
    TestAuth::from_jwt_payload(payload, ConnectionId::from_u128(conn)).unwrap()
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
    call(t, player, |ctx| enter_world(ctx, char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    read(t, |ctx| ctx.db.presence().identity().find(identity(player)).unwrap())
}

/// The server's terrain hash, which a client on the same world would send.
fn server_terrain(ctx: &ReducerContext) -> u64 {
    crate::terrain::terrain_info(ctx).hash
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
    call(t, player, |ctx| move_event(ctx, seq, 0, x, 0.0, z, 0, 540, 1)).unwrap();
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
    assert!(call(&t, 2, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH, server_terrain(ctx))).is_err());
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
    let err = call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH ^ 1, server_terrain(ctx))).unwrap_err();
    assert!(err.contains("does not match"), "{err}");
    assert_eq!(read(&t, |ctx| ctx.db.presence().count()), 0);
    assert_eq!(read(&t, |ctx| ctx.db.entity().count()), 0);
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
}

#[test]
fn re_entering_replaces_the_old_entity() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
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
    assert!(call(&t, 1, |ctx| move_event(ctx, 1, 0, 0.0, 0.0, 0.0, 0, 0, 1)).is_err());
}

#[test]
fn unknown_modes_and_nan_positions_are_rejected() {
    let t = world();
    arrive(&t, 1, "Mira");
    assert!(call(&t, 1, |ctx| move_event(ctx, 1, 0, 0.0, 0.0, 0.0, 0, 0, 99)).is_err());
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
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    let tr = transform_of(&t, presence_of(&t, 1).entity_id);
    assert_eq!((tr.x, tr.z), (2.0, -1.0));
}

#[test]
fn a_late_disconnect_spares_a_reconnected_character() {
    let t = world();
    let p = arrive(&t, 1, "Mira");
    // The client reconnects on connection 101 and enters again before the server notices that
    // connection 1 has gone.
    let enter = |ctx: &ReducerContext| enter_world(ctx, p.char_id, CONTENT_HASH, server_terrain(ctx));
    t.with_reducer_tx(auth_on(1, 101), |ctx| {
        on_connect(ctx);
        enter(ctx)
    })
    .unwrap();
    let again = presence_of(&t, 1);
    assert_ne!(again.entity_id, p.entity_id);
    assert_eq!(again.connection_id, ConnectionId::from_u128(101));
    t.with_reducer_tx::<_, ()>(auth_on(1, 1), |ctx| {
        on_disconnect(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(presence_of(&t, 1).entity_id, again.entity_id, "the old connection's disconnect leaves it be");
    assert!(read(&t, |ctx| ctx.db.transform().entity_id().find(again.entity_id)).is_some());
    t.with_reducer_tx::<_, ()>(auth_on(1, 101), |ctx| {
        on_disconnect(ctx);
        Ok(())
    })
    .unwrap();
    assert!(read(&t, |ctx| ctx.db.presence().identity().find(identity(1))).is_none(), "its own disconnect despawns it");
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
    // Clients read the offset to compute values from timestamps (docs/design/labor.md).
    use crate::time::game_clock;
    assert_eq!(read(&t, |ctx| ctx.db.game_clock().id().find(0).unwrap().offset_micros), 3_600_000_000);
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
    use crate::combat::combat_event;
    use crate::dev::dev_swing;
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

// Terrain (docs/design/terrain.md) -----------------------------------------------------------

/// A 3 m platform west of x = 0, a 0.5 slope up to 18 m at x = 30, a cliff (5 m per metre) to a
/// plateau at 38 m from x = 34. The same along z.
fn cliff_height(x: f32, _z: f32) -> f32 {
    if x < 0.0 {
        3.0
    } else if x < 30.0 {
        3.0 + 0.5 * x
    } else if x < 34.0 {
        18.0 + 5.0 * (x - 30.0)
    } else {
        38.0
    }
}

const ADMIN: u128 = 99;

fn upload_cliff(t: &TestContext) {
    use crate::terrain::{TerrainChunk, admin_put_terrain};
    call(t, ADMIN, |ctx| {
        on_connect(ctx);
        record_owner(ctx);
        Ok(())
    })
    .unwrap();
    let file = sim::terrain::HeightFile::generate(124, 124, 3, 2, cliff_height);
    let chunks = file.chunks().into_iter().map(|(cell, samples)| TerrainChunk { cell, samples }).collect();
    call(t, ADMIN, |ctx| admin_put_terrain(ctx, chunks)).unwrap();
}

fn step_y(t: &TestContext, player: u128, seq: u32, x: f32, y: f32, z: f32) {
    call(t, player, |ctx| move_event(ctx, seq, 0, x, y, z, 0, 540, 1)).unwrap();
}

fn suspicions(t: &TestContext) -> Vec<String> {
    read(t, |ctx| ctx.db.suspicion().iter().map(|s| s.reason).collect())
}

#[test]
fn only_admins_upload_terrain_and_chunks_are_checked() {
    use crate::terrain::{TerrainChunk, admin_clear_terrain, admin_put_terrain, terrain_chunk};
    let t = world();
    arrive(&t, 1, "Mira");
    let chunk = || vec![TerrainChunk { cell: sim::cell::pack(0, 3, 4), samples: vec![0; sim::terrain::CHUNK_LEN] }];
    assert_eq!(call(&t, 1, |ctx| admin_put_terrain(ctx, chunk())).unwrap_err(), "admins only");
    assert!(call(&t, 1, admin_clear_terrain).is_err());

    upload_cliff(&t);
    assert_eq!(read(&t, |ctx| ctx.db.terrain_chunk().count()), 6);
    call(&t, ADMIN, |ctx| admin_put_terrain(ctx, chunk())).unwrap();
    call(&t, ADMIN, |ctx| admin_put_terrain(ctx, chunk())).unwrap(); // replaces, doesn't duplicate
    assert_eq!(read(&t, |ctx| ctx.db.terrain_chunk().count()), 7);

    let short = vec![TerrainChunk { cell: 0, samples: vec![0; 10] }];
    assert!(call(&t, ADMIN, |ctx| admin_put_terrain(ctx, short)).is_err());
    let other_instance = vec![TerrainChunk { cell: sim::cell::pack(1, 0, 0), samples: vec![0; sim::terrain::CHUNK_LEN] }];
    assert!(call(&t, ADMIN, |ctx| admin_put_terrain(ctx, other_instance)).is_err());
    assert_eq!(read(&t, |ctx| ctx.db.terrain_chunk().count()), 7);

    call(&t, ADMIN, admin_clear_terrain).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.terrain_chunk().count()), 0);
}

/// The terrain hash of the cliff's chunks, worked out the way a client does.
fn cliff_hash() -> u64 {
    let chunks = sim::terrain::HeightFile::generate(124, 124, 3, 2, cliff_height).chunks();
    sim::terrain::terrain_hash(chunks.iter().map(|(c, s)| (*c, s.as_slice())))
}

#[test]
fn terrain_info_follows_uploads_and_clears() {
    use crate::terrain::{TerrainChunk, admin_clear_terrain, admin_put_terrain, terrain_info};
    let t = world();
    assert_eq!(read(&t, |ctx| (terrain_info(ctx).chunks, terrain_info(ctx).hash)), (0, 0));
    upload_cliff(&t);
    assert_eq!(read(&t, |ctx| (terrain_info(ctx).chunks, terrain_info(ctx).hash)), (6, cliff_hash()));
    // Uploading the same again changes nothing; replacing one chunk matches recomputing.
    upload_cliff(&t);
    assert_eq!(read(&t, |ctx| terrain_info(ctx).hash), cliff_hash());
    let mut chunks = sim::terrain::HeightFile::generate(124, 124, 3, 2, cliff_height).chunks();
    chunks[4].1 = vec![1234; sim::terrain::CHUNK_LEN];
    let want = sim::terrain::terrain_hash(chunks.iter().map(|(c, s)| (*c, s.as_slice())));
    let replaced = vec![TerrainChunk { cell: chunks[4].0, samples: chunks[4].1.clone() }];
    call(&t, ADMIN, |ctx| admin_put_terrain(ctx, replaced)).unwrap();
    assert_eq!(read(&t, |ctx| (terrain_info(ctx).chunks, terrain_info(ctx).hash)), (6, want));
    call(&t, ADMIN, admin_clear_terrain).unwrap();
    assert_eq!(read(&t, |ctx| (terrain_info(ctx).chunks, terrain_info(ctx).hash)), (0, 0));
}

#[test]
fn a_missing_terrain_info_row_is_rebuilt_from_the_chunks() {
    use crate::terrain::terrain_info;
    let t = world();
    upload_cliff(&t);
    call(&t, ADMIN, |ctx| {
        ctx.db.terrain_info().id().delete(0);
        Ok(())
    })
    .unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.terrain_info().count()), 0);
    assert_eq!(call(&t, ADMIN, |ctx| Ok(terrain_info(ctx).hash)).unwrap(), cliff_hash());
    assert_eq!(read(&t, |ctx| ctx.db.terrain_info().count()), 1);
}

#[test]
fn a_client_on_other_terrain_is_refused() {
    let t = world();
    upload_cliff(&t);
    let p = arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    for wrong in [cliff_hash() ^ 1, 0] {
        let err = call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH, wrong)).unwrap_err();
        assert!(err.contains("client terrain") && err.contains("cargo xtask world"), "{err}");
    }
    assert_eq!(read(&t, |ctx| ctx.db.presence().count()), 0);
    call(&t, 1, |ctx| enter_world(ctx, p.char_id, CONTENT_HASH, cliff_hash())).unwrap();
}

#[cfg(feature = "dev")]
#[test]
fn dev_builds_let_anyone_become_admin() {
    use crate::terrain::admin_clear_terrain;
    let t = world();
    arrive(&t, 1, "Mira");
    assert!(call(&t, 1, admin_clear_terrain).is_err());
    call(&t, 1, |ctx| crate::dev::dev_set_gm(ctx, gm::ADMIN)).unwrap();
    call(&t, 1, admin_clear_terrain).unwrap();
}

#[test]
fn entering_settles_on_the_ground() {
    let t = world();
    upload_cliff(&t);
    // Saved at the origin at y = 0, now 3 m under the platform.
    let p = arrive(&t, 1, "Mira");
    assert!((p.y - 3.0).abs() < 0.05, "{}", p.y);
    assert!((transform_of(&t, p.entity_id).y - 3.0).abs() < 0.05);
    assert_eq!((p.anchor_x, p.anchor_y, p.anchor_z), (p.x, p.y, p.z));
}

#[test]
fn walking_up_a_slope_is_accepted() {
    let t = world();
    upload_cliff(&t);
    arrive(&t, 1, "Mira");
    for i in 1..=10u32 {
        advance_ms(&t, 500);
        let x = 2.7 * i as f32;
        step_y(&t, 1, i, x, cliff_height(x, 0.0), 0.0);
    }
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.x), (0, 27.0));
    assert_eq!(p.anchor_x, 27.0, "standing moves the anchor");
    assert!(suspicions(&t).is_empty());
}

#[test]
fn jumping_up_the_cliff_is_corrected_and_settled() {
    let t = world();
    upload_cliff(&t);
    arrive(&t, 1, "Mira");
    let route = [2.7, 5.4, 8.1, 10.8, 13.5, 16.2, 18.9, 21.6, 24.3, 27.0, 29.0];
    for (i, &x) in route.iter().enumerate() {
        advance_ms(&t, 500);
        step_y(&t, 1, i as u32 + 1, x, cliff_height(x, 0.0), 0.0);
    }
    // 2.5 m on, 7.5 m up the face: more than a jump plus the slope credit.
    advance_ms(&t, 500);
    step_y(&t, 1, 20, 31.5, 25.5, 0.0);
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.x), (1, 29.0));
    assert!((p.y - 17.5).abs() < 0.05, "settled on the ground: {}", p.y);
    assert!(suspicions(&t)[0].contains("above the ceiling"), "{:?}", suspicions(&t));
}

#[test]
fn a_fly_hack_is_corrected_after_the_grace() {
    let t = world();
    upload_cliff(&t);
    arrive(&t, 1, "Mira");
    // Stay 1.5 m up while running west: fine for a jump's worth of time, not for 3.5 s.
    for i in 1..=7u32 {
        advance_ms(&t, 500);
        step_y(&t, 1, i, -2.7 * i as f32, 4.5, 0.0);
        assert_eq!(presence_of(&t, 1).corrections, u32::from(i == 7), "after {} s", i as f32 / 2.0);
    }
    let p = presence_of(&t, 1);
    assert!((p.y - 3.0).abs() < 0.05, "settled back on the platform: {}", p.y);
}

#[test]
fn going_underground_is_corrected() {
    let t = world();
    upload_cliff(&t);
    arrive(&t, 1, "Mira");
    advance_ms(&t, 500);
    step_y(&t, 1, 1, -2.0, 1.0, 0.0);
    assert_eq!(presence_of(&t, 1).corrections, 1);
    assert!(suspicions(&t)[0].contains("below the ground"));
}

#[cfg(feature = "dev")]
#[test]
fn running_off_the_cliff_and_falling_is_accepted() {
    let t = world();
    upload_cliff(&t);
    arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| crate::dev::dev_teleport(ctx, 40.0, 0.0)).unwrap();
    assert!((presence_of(&t, 1).y - 38.0).abs() < 0.05, "teleport lands on the plateau");
    // West at run speed, leaving the edge at x = 34 and falling at 9.8 m/s² until landing.
    for k in 1..=8u32 {
        advance_ms(&t, 500);
        let x = 40.0 - 2.7 * k as f32;
        let air = ((34.0 - x) / 5.4).max(0.0);
        let y = (38.0 - 4.9 * air * air).max(cliff_height(x, 0.0));
        step_y(&t, 1, k, x, y, 0.0);
    }
    let p = presence_of(&t, 1);
    assert_eq!(p.corrections, 0, "{:?}", suspicions(&t));
    assert!(p.y < 15.0 && p.anchor_y == p.y, "landed and anchored at {}", p.y);
}

#[cfg(feature = "dev")]
#[test]
fn dev_swing_needs_line_of_sight() {
    use crate::dev::{dev_swing, dev_teleport};
    let t = world();
    upload_cliff(&t);
    arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    // Below the cliff looking up at the plateau, 20 m apart: the face is in the way.
    call(&t, 1, |ctx| dev_teleport(ctx, 20.0, 0.0)).unwrap();
    call(&t, 2, |ctx| dev_teleport(ctx, 40.0, 0.0)).unwrap();
    assert_eq!(call(&t, 1, |ctx| dev_swing(ctx, b.entity_id)).unwrap_err(), "no line of sight");
    // Out on the open platform it lands.
    call(&t, 1, |ctx| dev_teleport(ctx, -25.0, 0.0)).unwrap();
    call(&t, 2, |ctx| dev_teleport(ctx, -10.0, 5.0)).unwrap();
    call(&t, 1, |ctx| dev_swing(ctx, b.entity_id)).unwrap();
}
