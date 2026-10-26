//! Ships and moving decks (docs/design/ships.md). Ships only come from `dev_spawn_ship` until
//! shipyards, so these need a dev build.

use super::*;
use crate::combat::combat_event;
use crate::tick::tick_state;
use crate::dev::{dev_remove_ship, dev_spawn_ship, dev_swing, dev_teleport};
use crate::ship::{CannonShot, cannon_impact, cannon_shot, deck, fire, helm, leave_helm, ship_motion, take_helm, vehicle};
use sim::ship::{Hull, HullData, Pose};

const RUN: u8 = 1;
const SWIM: u8 = 2;
/// Comfortably under the run limit per half-second heartbeat.
const STRIDE: f32 = 5.4 * 0.9 * 0.5;

fn clipper() -> &'static HullData {
    Hull::Clipper.data()
}

/// Spawn a clipper at rest and return its entity id.
fn spawn(t: &TestContext, x: f32, z: f32, yaw: f32) -> u64 {
    call(t, 0, |ctx| dev_spawn_ship(ctx, 0, x, z, yaw)).unwrap();
    read(t, |ctx| ctx.db.vehicle().iter().map(|v| v.entity_id).max().unwrap())
}

fn pose(t: &TestContext, ship: u64) -> Pose {
    read(t, |ctx| deck(ctx, ship).unwrap().pose())
}

fn speed(t: &TestContext, ship: u64) -> f32 {
    read(t, |ctx| ctx.db.ship_motion().entity_id().find(ship).unwrap().speed)
}

fn vehicle_of(t: &TestContext, ship: u64) -> crate::ship::Vehicle {
    read(t, |ctx| ctx.db.vehicle().entity_id().find(ship).unwrap())
}

fn mv(t: &TestContext, player: u128, parent: u64, at: [f32; 3], mode: u8) {
    let seq = presence_of(t, player).last_seq + 1;
    call(t, player, |ctx| move_event(ctx, seq, parent, at[0], at[1], at[2], 0, 540, mode)).unwrap();
}

/// Appear in the water beside `(x, z)` on the ship's deck and climb aboard there.
fn board(t: &TestContext, player: u128, ship: u64, x: f32, z: f32) {
    let w = pose(t, ship).to_world([x, 0.0, z]);
    call(t, player, |ctx| dev_teleport(ctx, w[0], w[2])).unwrap();
    mv(t, player, ship, [x, 0.0, z], RUN);
}

/// Run the world for `ms`, one 100 ms tick at a time.
fn sail(t: &TestContext, ms: i64) {
    for _ in 0..ms / 100 {
        advance_ms(t, 100);
        tick(t);
    }
}

/// A started world with a clipper at `(x, z)` facing `yaw`, and player 2 at its helm.
fn crewed(x: f32, z: f32, yaw: f32) -> (TestContext, u64) {
    let t = started();
    tick(&t);
    let ship = spawn(&t, x, z, yaw);
    arrive(&t, 2, "Helmsman");
    board(&t, 2, ship, clipper().helm_x, clipper().helm_z);
    call(&t, 2, take_helm).unwrap();
    (t, ship)
}

#[test]
fn spawning_puts_a_ship_on_the_water() {
    let t = world();
    let ship = spawn(&t, 100.0, 50.0, 0.0);
    let e = read(&t, |ctx| ctx.db.entity().id().find(ship).unwrap());
    assert_eq!((e.kind, e.ref_id), (entity_kind::SHIP, Hull::Clipper as u64));
    let tr = transform_of(&t, ship);
    assert_eq!((tr.x, tr.y, tr.z, tr.parent, tr.cell), (100.0, clipper().deck_height_m, 50.0, 0, e.cell));
    let v = vehicle_of(&t, ship);
    assert_eq!((v.hp, v.helm, v.cannon_ready_at.len(), v.cell), (clipper().hp, 0, 2, e.cell));
    assert!(call(&t, 0, |ctx| dev_spawn_ship(ctx, 7, 0.0, 0.0, 0.0)).is_err(), "no such hull");
    assert!(call(&t, 0, |ctx| dev_spawn_ship(ctx, 0, f32::NAN, 0.0, 0.0)).is_err());
}

#[test]
fn the_tick_sails_the_ship_and_publishes_it_at_5hz() {
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    call(&t, 2, |ctx| helm(ctx, 100, 0)).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.tick_state().id().find(0).unwrap().ticks), 1);
    sail(&t, 100); // tick 2 publishes
    let published = transform_of(&t, ship);
    assert!(published.z < 0.0 && published.speed == 15, "{published:?}");
    sail(&t, 100); // tick 3 steps but doesn't publish
    assert!(pose(&t, ship).z < published.z);
    assert_eq!(transform_of(&t, ship), published);
    sail(&t, 100);
    let m = read(&t, |ctx| ctx.db.ship_motion().entity_id().find(ship).unwrap());
    let cell = transform_of(&t, ship).cell;
    let expected = crate::ship::ship_transform(ship, cell, clipper(), &Motion { x: m.x, z: m.z, yaw: m.yaw, speed: m.speed });
    assert_eq!(transform_of(&t, ship), expected);
    // A ship at rest isn't rewritten.
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    sail(&t, 1_000);
    assert_eq!(transform_of(&t, ship).z, 0.0);
}

use sim::ship::Motion;

#[test]
fn walking_on_deck_while_the_ship_turns_is_never_corrected() {
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    call(&t, 2, |ctx| helm(ctx, 100, 100)).unwrap();
    let walker = arrive(&t, 1, "Mira");
    board(&t, 1, ship, 0.0, -6.0);
    // Pace bow to stern and back for 30 s while the ship comes up to speed and turns.
    let mut z = -6.0;
    let mut dir = 1.0;
    for _ in 0..60 {
        sail(&t, 500);
        z += dir * STRIDE;
        if z.abs() > 6.0 {
            dir = -dir;
        }
        mv(&t, 1, ship, [0.5, 0.0, z], RUN);
    }
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.parent), (0, ship), "{:?}", suspicions(&t));
    let turned = std::f32::consts::TAU - pose(&t, ship).yaw;
    assert!(turned.to_degrees() > 150.0, "turned {}°", turned.to_degrees());
    // Observers see the walker on the deck, in the ship's cell, in ship-local coordinates.
    sail(&t, 100);
    let tr = transform_of(&t, walker.entity_id);
    assert_eq!((tr.parent, tr.cell, tr.x, tr.z), (ship, transform_of(&t, ship).cell, 0.5, z));
}

#[test]
fn off_the_deck_and_speed_hacks_on_deck_are_corrected() {
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    arrive(&t, 1, "Mira");
    board(&t, 1, ship, 2.0, 0.0);
    // Past the rail, still within a stride.
    advance_ms(&t, 500);
    mv(&t, 1, ship, [4.2, 0.0, 0.0], RUN);
    assert_eq!(presence_of(&t, 1).corrections, 1);
    assert!(suspicions(&t)[0].starts_with("off the deck"), "{:?}", suspicions(&t));
    // Leaning over the rail is fine.
    advance_ms(&t, 500);
    mv(&t, 1, ship, [3.8, 0.5, 0.0], RUN);
    assert_eq!(presence_of(&t, 1).corrections, 1);
    // Back on deck; then a 10 m dash in half a second, and a dive through the deck.
    advance_ms(&t, 500);
    mv(&t, 1, ship, [2.0, 0.0, -2.5], RUN);
    advance_ms(&t, 500);
    mv(&t, 1, ship, [2.0, 0.0, 8.0], RUN);
    assert_eq!(presence_of(&t, 1).corrections, 2);
    advance_ms(&t, 500);
    mv(&t, 1, ship, [2.0, -1.5, -4.5], RUN);
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.parent, p.y, p.z), (3, ship, 0.0, -2.5));
}

#[test]
fn stepping_off_and_climbing_back_aboard_a_moving_ship() {
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    call(&t, 2, |ctx| helm(ctx, 100, 0)).unwrap();
    arrive(&t, 1, "Mira");
    board(&t, 1, ship, 2.0, 0.0);
    sail(&t, 10_000);
    assert!(speed(&t, ship) > 10.0);
    // Over the side into the water, a moment after the client last saw the ship.
    sail(&t, 300);
    let w = pose(&t, ship).to_world([3.5, 0.0, 1.0]);
    mv(&t, 1, 0, [w[0], 0.0, w[2]], SWIM);
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.parent, p.mode), (0, 0, SWIM), "{:?}", suspicions(&t));
    // The ship sails on; it's 30 m off by the next heartbeat, so climbing back is refused.
    sail(&t, 3_000);
    mv(&t, 1, ship, [2.0, 0.0, 0.0], RUN);
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.parent), (1, 0));
    // Alongside it again, climbing aboard works.
    board(&t, 1, ship, -2.0, 5.0);
    assert_eq!(presence_of(&t, 1).parent, ship);
    assert_eq!(presence_of(&t, 1).corrections, 1);
}

#[test]
fn a_ship_that_does_not_exist_is_refused() {
    let t = world();
    arrive(&t, 1, "Mira");
    mv(&t, 1, 999, [0.0, 0.0, 0.0], RUN);
    let p = presence_of(&t, 1);
    assert_eq!((p.corrections, p.parent), (1, 0));
    assert_eq!(suspicions(&t), ["no such ship"]);
}

#[test]
fn the_helm_is_taken_near_it_by_one_player_at_a_time() {
    let t = started();
    let ship = spawn(&t, 0.0, 0.0, 0.0);
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    assert_eq!(call(&t, 1, take_helm).unwrap_err(), "not aboard a ship");
    board(&t, 1, ship, 0.0, -7.0);
    assert_eq!(call(&t, 1, take_helm).unwrap_err(), "too far from the helm");
    assert_eq!(call(&t, 1, |ctx| helm(ctx, 100, 0)).unwrap_err(), "you don't have the helm");
    board(&t, 2, ship, 0.5, 7.0);
    call(&t, 2, take_helm).unwrap();
    for z in [0.0, 6.5] {
        advance_ms(&t, 2_000);
        mv(&t, 1, ship, [0.0, 0.0, z], RUN);
    }
    assert_eq!(call(&t, 1, take_helm).unwrap_err(), "someone else has the helm");
    assert!(call(&t, 2, |ctx| helm(ctx, 101, 0)).is_err());
    call(&t, 2, |ctx| helm(ctx, -100, 40)).unwrap();
    let v = vehicle_of(&t, ship);
    assert_eq!((v.helm, v.throttle, v.rudder), (presence_of(&t, 2).entity_id, -100, 40));
    call(&t, 2, leave_helm).unwrap();
    let v = vehicle_of(&t, ship);
    assert_eq!((v.helm, v.throttle, v.rudder), (0, 0, 0));
    call(&t, 1, take_helm).unwrap();
}

#[test]
fn walking_away_from_the_helm_lets_go_and_the_ship_drifts() {
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    call(&t, 2, |ctx| helm(ctx, 100, 50)).unwrap();
    sail(&t, 10_000);
    let cruising = speed(&t, ship);
    // Step forward, still within reach: nothing changes.
    mv(&t, 2, ship, [0.0, 0.0, 6.0], RUN);
    assert_eq!(vehicle_of(&t, ship).throttle, 100);
    sail(&t, 500);
    mv(&t, 2, ship, [0.0, 0.0, 3.6], RUN);
    let v = vehicle_of(&t, ship);
    assert_eq!((v.helm, v.throttle, v.rudder), (0, 0, 0));
    let yaw = pose(&t, ship).yaw;
    sail(&t, 2_000);
    assert!(speed(&t, ship) < cruising - 1.0, "{} vs {cruising}", speed(&t, ship));
    assert_eq!(pose(&t, ship).yaw, yaw, "a drifting ship holds its heading");
}

#[test]
fn leaving_the_world_lets_go_of_the_helm_and_saves_the_world_position() {
    let (t, ship) = crewed(1000.0, 1000.0, 1.0);
    let at = pose(&t, ship).to_world([clipper().helm_x, 0.0, clipper().helm_z]);
    call(&t, 2, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(vehicle_of(&t, ship).helm, 0);
    let c = read(&t, |ctx| ctx.db.character_private().owner().filter(identity(2)).next().unwrap());
    assert_eq!([c.x, c.y, c.z], at);
}

#[test]
fn passengers_follow_their_ship_into_new_cells() {
    // Facing +x, 10 m short of the border at x = 64.
    let (t, ship) = crewed(54.0, 10.0, -std::f32::consts::FRAC_PI_2);
    let p = arrive(&t, 1, "Mira");
    board(&t, 1, ship, 0.0, 0.0);
    call(&t, 2, |ctx| helm(ctx, 100, 0)).unwrap();
    let before = transform_of(&t, ship).cell;
    sail(&t, 6_000);
    let s = pose(&t, ship);
    assert!(s.x > 70.0, "{s:?}");
    let cell = sim::cell::cell_of(0, s.x, s.z);
    assert_ne!(cell, before);
    for id in [ship, p.entity_id, presence_of(&t, 2).entity_id] {
        assert_eq!(transform_of(&t, id).cell, cell, "transform of {id}");
        assert_eq!(read(&t, |ctx| ctx.db.entity().id().find(id).unwrap().cell), cell, "entity {id}");
    }
    assert_eq!(vehicle_of(&t, ship).cell, cell);
}

#[test]
fn swinging_from_a_deck_uses_world_positions() {
    let t = started();
    let ship = spawn(&t, 1000.0, 1000.0, 2.0);
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    board(&t, 1, ship, 2.0, 0.0);
    // 10 m off the starboard side, in the water.
    let w = pose(&t, ship).to_world([12.0, 0.0, 0.0]);
    call(&t, 2, |ctx| dev_teleport(ctx, w[0], w[2])).unwrap();
    call(&t, 1, |ctx| dev_swing(ctx, b.entity_id)).unwrap();
    call(&t, 2, |ctx| dev_swing(ctx, a.entity_id)).unwrap();
    // At the world origin, they'd be 1.4 km apart.
    call(&t, 2, |ctx| dev_teleport(ctx, 2.0, 0.0)).unwrap();
    assert_eq!(call(&t, 2, |ctx| dev_swing(ctx, a.entity_id)).unwrap_err(), "out of range");
}

/// A crewed ship facing -z at the origin, a gunner at its starboard cannon, and a target ship
/// 50 m to starboard.
fn gunnery() -> (TestContext, u64, u64) {
    let (t, ship) = crewed(0.0, 0.0, 0.0);
    let target = spawn(&t, 50.0, 0.0, 0.0);
    arrive(&t, 1, "Gunner");
    board(&t, 1, ship, 1.5, 0.5);
    (t, ship, target)
}

/// The pitch at which the starboard cannon lands `range` metres out, found by bisection.
fn pitch_for(range: f32) -> f32 {
    let h = clipper();
    let (mut lo, mut hi) = (h.cannon.min_pitch_deg.to_radians(), h.cannon.max_pitch_deg.to_radians());
    let pose = Pose { x: 0.0, y: h.deck_height_m, z: 0.0, yaw: 0.0 };
    let (from, yaw) = sim::ship::muzzle(h, &pose, &h.mounts[1], 0.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        let f = sim::ship::cannon_flight(from, yaw, mid, h.cannon.muzzle_speed_mps).unwrap();
        if f.landing[0] < range { lo = mid } else { hi = mid }
    }
    lo
}

fn shots(t: &TestContext) -> Vec<CannonShot> {
    read(t, |ctx| ctx.db.cannon_shot().iter().collect())
}

/// Land a shot now, as the scheduler would; returns the combat events it sent.
fn land(t: &TestContext, shot: CannonShot) -> Vec<crate::combat::CombatEvent> {
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| {
        cannon_impact(ctx, shot)?;
        Ok(ctx.db.combat_event().iter().collect())
    })
    .unwrap()
}

#[test]
fn firing_needs_reach_aim_and_a_loaded_cannon() {
    let (t, _, _) = gunnery();
    assert_eq!(call(&t, 1, |ctx| fire(ctx, 0, 0.0, 0.1)).unwrap_err(), "too far from the cannon");
    assert_eq!(call(&t, 1, |ctx| fire(ctx, 2, 0.0, 0.1)).unwrap_err(), "no such cannon");
    assert_eq!(call(&t, 1, |ctx| fire(ctx, 1, 0.7, 0.1)).unwrap_err(), "the cannon can't aim there");
    assert_eq!(call(&t, 1, |ctx| fire(ctx, 1, 0.0, f32::NAN)).unwrap_err(), "the cannon can't aim there");
    call(&t, 1, |ctx| fire(ctx, 1, 0.0, 0.1)).unwrap();
    assert_eq!(call(&t, 1, |ctx| fire(ctx, 1, 0.0, 0.1)).unwrap_err(), "reloading");
    advance_ms(&t, 5_000);
    call(&t, 1, |ctx| fire(ctx, 1, 0.0, 0.1)).unwrap();
    assert_eq!(shots(&t).len(), 2);
    arrive(&t, 3, "Swimmer");
    assert_eq!(call(&t, 3, |ctx| fire(ctx, 1, 0.0, 0.1)).unwrap_err(), "not aboard a ship");
}

#[test]
fn a_shot_is_scheduled_to_land_after_its_flight() {
    let (t, ship, _) = gunnery();
    let pitch = pitch_for(50.0);
    let fired_at = read(&t, |ctx| ctx.timestamp);
    call(&t, 1, |ctx| fire(ctx, 1, 0.0, pitch)).unwrap();
    let mut all = shots(&t);
    assert_eq!(all.len(), 1);
    let shot = all.pop().unwrap();
    let h = clipper();
    let (from, yaw) = sim::ship::muzzle(h, &pose(&t, ship), &h.mounts[1], 0.0);
    let f = sim::ship::cannon_flight(from, yaw, pitch, h.cannon.muzzle_speed_mps).unwrap();
    assert!(f.secs > 1.0, "{f:?}");
    let ScheduleAt::Time(lands) = shot.scheduled_at else { panic!("{:?}", shot.scheduled_at) };
    let micros = lands.to_micros_since_unix_epoch() - fired_at.to_micros_since_unix_epoch();
    assert_eq!(micros, (f.secs * 1e6) as i64);
    assert!((shot.x - 50.0).abs() < 0.1 && shot.z.abs() < 0.01, "lands at ({}, {})", shot.x, shot.z);
    assert_eq!((shot.ship, shot.gunner), (ship, presence_of(&t, 1).entity_id));
}

use spacetimedb::ScheduleAt;

#[test]
fn a_shot_that_lands_on_a_hull_damages_it() {
    let (t, ship, target) = gunnery();
    call(&t, 1, |ctx| fire(ctx, 1, 0.0, pitch_for(50.0))).unwrap();
    let shot = shots(&t).pop().unwrap();
    let events = land(&t, shot);
    let gunner = presence_of(&t, 1).entity_id;
    let damage = clipper().cannon.damage;
    assert_eq!(events.len(), 1);
    let e = &events[0];
    assert_eq!((e.source, e.target, e.amount, e.flags, e.skill_id), (gunner, target, damage, 1, 2));
    assert_eq!(e.cell, sim::cell::cell_of(0, 50.0, 0.0));
    assert_eq!(vehicle_of(&t, target).hp, clipper().hp - damage);
    assert_eq!(vehicle_of(&t, ship).hp, clipper().hp);
    // HP stops at zero.
    for _ in 0..10 {
        advance_ms(&t, 5_000);
        call(&t, 1, |ctx| fire(ctx, 1, 0.0, pitch_for(50.0))).unwrap();
        let shot = shots(&t).pop().unwrap();
        land(&t, shot);
    }
    assert_eq!(vehicle_of(&t, target).hp, 0);
}

#[test]
fn a_shot_into_open_water_splashes_and_never_hits_its_own_ship() {
    let (t, ship, target) = gunnery();
    call(&t, 1, |ctx| fire(ctx, 1, 0.0, pitch_for(30.0))).unwrap();
    let events = land(&t, shots(&t).pop().unwrap());
    assert_eq!((events[0].target, events[0].amount, events[0].flags), (0, 0, 0));
    assert_eq!(vehicle_of(&t, target).hp, clipper().hp);
    // A shot that somehow came down on its own deck.
    let own = CannonShot { scheduled_id: 0, scheduled_at: ScheduleAt::Time(Timestamp::UNIX_EPOCH), gunner: 1, ship, x: 0.0, z: 0.0 };
    let events = land(&t, own);
    assert_eq!(events[0].target, 0);
    assert_eq!(vehicle_of(&t, ship).hp, clipper().hp);
}

#[test]
fn clients_cannot_land_shots() {
    let (t, ship, _) = gunnery();
    let shot = CannonShot { scheduled_id: 0, scheduled_at: ScheduleAt::Time(Timestamp::UNIX_EPOCH), gunner: 1, ship, x: 50.0, z: 0.0 };
    assert!(call(&t, 1, |ctx| cannon_impact(ctx, shot)).is_err());
}

#[test]
fn a_ship_runs_aground_on_a_shoal_and_can_back_off() {
    use crate::terrain::{TerrainChunk, admin_put_terrain};
    // Deep water, except a shoal 0.5 m down north of z = -60.
    let (t, ship) = crewed(32.0, 32.0, 0.0);
    call(&t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::dev::dev_set_gm(ctx, gm::ADMIN)
    })
    .unwrap();
    let file = sim::terrain::HeightFile::generate(124, 122, 2, 4, |_, z| if z < -60.0 { -0.5 } else { -20.0 });
    let chunks = file.chunks().into_iter().map(|(cell, samples)| TerrainChunk { cell, samples }).collect();
    call(&t, ADMIN, |ctx| admin_put_terrain(ctx, chunks)).unwrap();
    call(&t, 2, |ctx| helm(ctx, 100, 0)).unwrap();
    sail(&t, 20_000);
    let stuck = pose(&t, ship);
    assert_eq!(speed(&t, ship), 0.0);
    // Its bow is 9 m ahead and stops short of the shoal's edge (whose slope starts 2 m out).
    let bow = stuck.z - clipper().deck_half_length_m;
    assert!(bow > -62.0 && bow < -55.0, "bow at {bow}");
    call(&t, 2, |ctx| helm(ctx, -100, 0)).unwrap();
    sail(&t, 3_000);
    assert!(pose(&t, ship).z > stuck.z + 2.0);
}

#[test]
fn removing_a_ship_puts_its_passengers_in_the_water() {
    let (t, ship) = crewed(500.0, 500.0, 0.5);
    let at = pose(&t, ship).to_world([clipper().helm_x, 0.0, clipper().helm_z]);
    call(&t, 0, |ctx| dev_remove_ship(ctx, ship)).unwrap();
    let p = presence_of(&t, 2);
    assert_eq!((p.parent, p.mode, p.corrections, p.x, p.y, p.z), (0, SWIM, 1, at[0], 0.0, at[2]));
    assert!(read(&t, |ctx| ctx.db.vehicle().count() + ctx.db.ship_motion().count()) == 0);
    assert!(read(&t, |ctx| ctx.db.transform().entity_id().find(ship).is_none()));
    // Swimming on from there is accepted, and the next tick publishes it in the world.
    advance_ms(&t, 500);
    mv(&t, 2, 0, [at[0] + 1.0, 0.0, at[2]], SWIM);
    tick(&t);
    let tr = transform_of(&t, p.entity_id);
    assert_eq!((tr.parent, tr.cell, presence_of(&t, 2).corrections), (0, sim::cell::cell_of(0, at[0] + 1.0, at[2]), 1));
    assert!(call(&t, 0, |ctx| dev_remove_ship(ctx, ship)).is_err());
}
