//! Ships: poses and ship-local frames, motion under throttle and rudder, draft, and cannon flight
//! (PLAN §8.4, docs/design/ships.md). The server steps ships with these in `world_tick`; the
//! helmsman's client predicts its own ship and previews cannon arcs with the same functions.
//!
//! Yaws are Godot's: a rotation about +y, 0 facing -z, positive turning left. Ship-local
//! coordinates have their origin at the deck's centre on its surface, -z the bow and +x starboard.

use std::f32::consts::TAU;

pub use crate::content::{CannonData, Hull, HullData, Mount};
use crate::movement::SEA_LEVEL_M;
use crate::terrain::Ground;

/// Throttle and rudder run from -100 to 100 percent.
pub const INPUT_MAX: i8 = 100;
/// Below this share of top speed, the rudder's authority falls off in proportion to speed.
pub const FULL_AUTHORITY_SHARE: f32 = 0.3;
pub const GRAVITY: f32 = 9.8;
/// Just outside the deck rectangle (leaning over the rail, stepping off) counts as aboard, with no
/// ground under you.
pub const RAIL_M: f32 = 1.5;
/// How close to the helm or a cannon a player must stand to use it.
pub const REACH_M: f32 = 2.5;
/// How far a client's view of a ship may lag the server's, allowed for when changing frame.
pub const POSE_LAG_S: f32 = 0.5;

/// Where a ship is: the centre of its deck in world coordinates, and its yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
}

impl Pose {
    /// Ship-local to world: Godot's `Basis(Vector3.UP, yaw)` and then the ship's position.
    pub fn to_world(&self, l: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        [self.x + l[0] * c + l[2] * s, self.y + l[1], self.z - l[0] * s + l[2] * c]
    }

    pub fn to_local(&self, w: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        let (dx, dz) = (w[0] - self.x, w[2] - self.z);
        [dx * c - dz * s, w[1] - self.y, dx * s + dz * c]
    }
}

/// The horizontal unit vector `[x, z]` a yaw faces.
pub fn facing(yaw: f32) -> [f32; 2] {
    let (s, c) = yaw.sin_cos();
    [-s, -c]
}

/// A yaw on the wire: `yaw / 2π × 65536`, wrapping.
pub fn yaw_to_u16(yaw: f32) -> u16 {
    (yaw.rem_euclid(TAU) / TAU * 65536.0).round() as u32 as u16
}

pub fn yaw_from_u16(v: u16) -> f32 {
    v as f32 / 65536.0 * TAU
}

/// What the tick steps for each ship. `yaw` stays within 0 to 2π; `speed` is negative astern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub speed: f32,
}

impl Motion {
    pub fn pose(&self, hull: &HullData) -> Pose {
        Pose { x: self.x, y: SEA_LEVEL_M + hull.deck_height_m, z: self.z, yaw: self.yaw }
    }
}

/// The helmsman's settings, each -100 to 100 percent. They persist until changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub throttle: i8,
    /// Positive turns to starboard.
    pub rudder: i8,
}

impl Input {
    pub fn valid(self) -> bool {
        [self.throttle, self.rudder].iter().all(|v| (-INPUT_MAX..=INPUT_MAX).contains(v))
    }
}

/// The speed the throttle asks for, m/s.
pub fn target_speed(hull: &HullData, throttle: i8) -> f32 {
    let share = f32::from(throttle) / f32::from(INPUT_MAX);
    share * if throttle >= 0 { hull.max_speed_mps } else { hull.astern_speed_mps }
}

/// Step a ship by `dt` seconds: speed toward the throttle's target, then yaw by the rudder, then
/// position along the new yaw (docs/design/ships.md).
pub fn step(m: Motion, hull: &HullData, input: Input, dt: f32) -> Motion {
    let target = target_speed(hull, input.throttle);
    let rate = if input.throttle == 0 { hull.drift_decel_mps2 } else { hull.accel_mps2 };
    let change = (target - m.speed).clamp(-rate * dt, rate * dt);
    let speed = m.speed + change;

    let authority = (speed.abs() / (FULL_AUTHORITY_SHARE * hull.max_speed_mps)).min(1.0);
    let rudder = f32::from(input.rudder) / f32::from(INPUT_MAX);
    let yaw_rate = -rudder * hull.turn_rate_dps.to_radians() * authority * speed.signum();
    let yaw = (m.yaw + yaw_rate * dt).rem_euclid(TAU);

    let [fx, fz] = facing(yaw);
    Motion { x: m.x + fx * speed * dt, z: m.z + fz * speed * dt, yaw, speed }
}

/// How far a character may move, in world coordinates, when it boards a ship, steps off one or
/// crosses between two: its own limit, plus the ship's travel during `POSE_LAG_S` for the client's
/// view of the ship running behind the server's. Both ends are placed with the ship's current
/// pose, so its travel since the last move needs no credit; and nobody keeps a ship's momentum
/// on stepping off.
pub fn frame_change_allowance(own_speed_mps: f32, ship_speed_mps: f32, dt_s: f32) -> f32 {
    crate::movement::allowed_distance(own_speed_mps, dt_s) + ship_speed_mps.abs() * POSE_LAG_S
}

/// The radius of the circle a ship sails at full speed and full rudder.
pub fn turning_radius(hull: &HullData) -> f32 {
    hull.max_speed_mps / hull.turn_rate_dps.to_radians()
}

/// Whether the water is deep enough for the hull at its bow, centre and stern. `ground` reads the
/// terrain; unknown terrain counts as deep water.
pub fn afloat(hull: &HullData, m: &Motion, mut ground: impl FnMut(f32, f32) -> Option<Ground>) -> bool {
    let pose = m.pose(hull);
    let floor = SEA_LEVEL_M - hull.draft_m;
    [-hull.deck_half_length_m, 0.0, hull.deck_half_length_m].into_iter().all(|lz| {
        let [x, _, z] = pose.to_world([0.0, 0.0, lz]);
        ground(x, z).is_none_or(|g| g.hi <= floor)
    })
}

/// Whether a ship-local point is on the deck rectangle grown by `margin`.
pub fn on_deck(hull: &HullData, x: f32, z: f32, margin: f32) -> bool {
    x.abs() <= hull.deck_half_width_m + margin && z.abs() <= hull.deck_half_length_m + margin
}

/// The ground on deck, in ship-local coordinates: flat at y = 0 on the deck, none off it.
pub fn deck_ground(hull: &HullData, x: f32, z: f32) -> Option<Ground> {
    on_deck(hull, x, z, 0.0).then_some(Ground { lo: 0.0, hi: 0.0, gradient: 0.0 })
}

/// Horizontal distance on deck from a ship-local point to `(x, z)`.
pub fn reach(from: [f32; 3], x: f32, z: f32) -> f32 {
    ((from[0] - x).powi(2) + (from[2] - z).powi(2)).sqrt()
}

/// Whether a cannon may be aimed `yaw` radians off its mount's facing at `pitch` radians.
pub fn aim_ok(cannon: &CannonData, yaw: f32, pitch: f32) -> bool {
    yaw.is_finite()
        && pitch.is_finite()
        && yaw.abs() <= cannon.arc_deg.to_radians()
        && (cannon.min_pitch_deg.to_radians()..=cannon.max_pitch_deg.to_radians()).contains(&pitch)
}

/// Where a mount's shot leaves from, in world coordinates, and the world yaw it flies along when
/// aimed `yaw` off the mount's facing.
pub fn muzzle(hull: &HullData, pose: &Pose, mount: &Mount, yaw: f32) -> ([f32; 3], f32) {
    let at = pose.to_world([mount.x, hull.cannon.muzzle_height_m, mount.z]);
    (at, pose.yaw + mount.facing_deg.to_radians() + yaw)
}

/// A cannon shot's flight time and where it comes down to sea level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub secs: f32,
    pub landing: [f32; 3],
}

/// A shot from `from` along world `yaw`, `pitch` above the horizontal, at `speed` m/s, falling under
/// `GRAVITY` until it reaches sea level. `None` from at or below the water.
pub fn cannon_flight(from: [f32; 3], yaw: f32, pitch: f32, speed: f32) -> Option<Flight> {
    let height = from[1] - SEA_LEVEL_M;
    if height <= 0.0 || !from.iter().all(|c| c.is_finite()) {
        return None;
    }
    let (vy, vh) = (speed * pitch.sin(), speed * pitch.cos());
    let secs = (vy + (vy * vy + 2.0 * GRAVITY * height).sqrt()) / GRAVITY;
    let [fx, fz] = facing(yaw);
    Some(Flight { secs, landing: [from[0] + fx * vh * secs, SEA_LEVEL_M, from[2] + fz * vh * secs] })
}

/// Whether a landing point hits a ship at `pose`: inside its deck rectangle grown by the splash.
pub fn hits(hull: &HullData, pose: &Pose, landing: [f32; 3]) -> bool {
    let l = pose.to_local(landing);
    on_deck(hull, l[0], l[2], hull.cannon.splash_m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn clipper() -> &'static HullData {
        Hull::Clipper.data()
    }

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    const AT_REST: Motion = Motion { x: 0.0, z: 0.0, yaw: 0.0, speed: 0.0 };

    /// Run `secs` of 10 Hz ticks.
    fn sail(mut m: Motion, input: Input, secs: f32) -> Motion {
        for _ in 0..(secs * 10.0).round() as usize {
            m = step(m, clipper(), input, 0.1);
        }
        m
    }

    #[test]
    fn local_and_world_round_trip() {
        let pose = Pose { x: 100.0, y: 2.0, z: -50.0, yaw: 0.7 };
        let l = [1.5, 0.3, -6.0];
        let back = pose.to_local(pose.to_world(l));
        for i in 0..3 {
            assert!(close(back[i], l[i], 1e-4), "{back:?}");
        }
    }

    #[test]
    fn the_frame_matches_godots_basis() {
        // A quarter turn left: the bow (-z) now faces -x, and starboard (+x) faces -z.
        let pose = Pose { x: 0.0, y: 0.0, z: 0.0, yaw: FRAC_PI_2 };
        let bow = pose.to_world([0.0, 0.0, -1.0]);
        let starboard = pose.to_world([1.0, 0.0, 0.0]);
        assert!(close(bow[0], -1.0, 1e-6) && close(bow[2], 0.0, 1e-6), "{bow:?}");
        assert!(close(starboard[0], 0.0, 1e-6) && close(starboard[2], -1.0, 1e-6), "{starboard:?}");
        assert_eq!(facing(0.0), [-0.0, -1.0]);
    }

    #[test]
    fn yaws_wrap_on_the_wire() {
        assert_eq!(yaw_to_u16(0.0), 0);
        assert_eq!(yaw_to_u16(PI), 32768);
        assert_eq!(yaw_to_u16(-FRAC_PI_2), 49152);
        assert_eq!(yaw_to_u16(TAU), 0);
        assert!(close(yaw_from_u16(yaw_to_u16(1.234)), 1.234, 1e-4));
    }

    #[test]
    fn full_throttle_reaches_top_speed_and_drift_slows_it() {
        let ahead = Input { throttle: 100, rudder: 0 };
        let m = sail(AT_REST, ahead, 3.0);
        assert!(close(m.speed, 4.5, 1e-3), "{}", m.speed);
        let m = sail(m, ahead, 10.0);
        assert_eq!(m.speed, clipper().max_speed_mps);
        // Straight ahead is -z.
        assert!(m.z < -80.0 && close(m.x, 0.0, 1e-3), "{m:?}");
        let drifting = sail(m, Input::default(), 5.0);
        assert!(close(drifting.speed, 11.0 - 4.0, 1e-3), "{}", drifting.speed);
        assert_eq!(sail(drifting, Input::default(), 20.0).speed, 0.0);
        let half = sail(m, Input { throttle: 50, rudder: 0 }, 10.0);
        assert!(close(half.speed, 5.5, 1e-4), "{}", half.speed);
    }

    #[test]
    fn astern_is_slower_and_backwards() {
        let m = sail(AT_REST, Input { throttle: -100, rudder: 0 }, 10.0);
        assert_eq!(m.speed, -clipper().astern_speed_mps);
        assert!(m.z > 0.0);
    }

    #[test]
    fn full_rudder_sails_the_turning_circle() {
        let cruising = Motion { speed: clipper().max_speed_mps, ..AT_REST };
        let input = Input { throttle: 100, rudder: 100 };
        let r = turning_radius(clipper());
        assert!(close(r, 52.52, 0.01), "{r}");
        // Starboard is +x, so the centre of a starboard turn from the origin facing -z is (r, 0).
        let mut m = cruising;
        let mut turned = 0.0;
        for _ in 0..300 {
            let next = step(m, clipper(), input, 0.1);
            turned += (m.yaw - next.yaw).rem_euclid(TAU);
            m = next;
            let d = ((m.x - r).powi(2) + m.z.powi(2)).sqrt();
            assert!(close(d, r, 1.0), "{d} from the centre at {m:?}");
        }
        // 30 s at 12°/s, turning right, so the yaw goes down.
        assert!(close(turned.to_degrees(), 360.0, 0.5), "{}", turned.to_degrees());
    }

    #[test]
    fn a_ship_barely_moving_barely_turns() {
        let input = Input { throttle: 0, rudder: 100 };
        assert_eq!(step(AT_REST, clipper(), input, 0.1).yaw, 0.0);
        // 3% throttle holds 0.33 m/s, a tenth of the speed for full authority.
        let creeping = Motion { speed: 0.33, ..AT_REST };
        let m = step(creeping, clipper(), Input { throttle: 3, rudder: 100 }, 1.0);
        let full = clipper().turn_rate_dps.to_radians();
        let turned = TAU - m.yaw;
        assert!(close(turned, 0.1 * full, 1e-4), "{turned}");
    }

    #[test]
    fn port_rudder_turns_left_and_astern_reverses_the_rudder() {
        let cruising = Motion { speed: 11.0, ..AT_REST };
        let port = step(cruising, clipper(), Input { throttle: 100, rudder: -100 }, 0.1);
        assert!(port.yaw > 0.0 && port.yaw < 0.1, "{}", port.yaw);
        let backing = Motion { speed: -3.0, ..AT_REST };
        let m = step(backing, clipper(), Input { throttle: -100, rudder: -100 }, 0.1);
        assert!(m.yaw > PI, "{}", m.yaw);
    }

    #[test]
    fn stepping_is_deterministic_and_validates_input() {
        let input = Input { throttle: 80, rudder: -35 };
        assert_eq!(sail(AT_REST, input, 20.0), sail(AT_REST, input, 20.0));
        assert!(input.valid());
        assert!(!Input { throttle: -101, rudder: 0 }.valid());
        assert!(!Input { throttle: 0, rudder: 127 }.valid());
    }

    #[test]
    fn shallow_water_grounds_the_hull() {
        let m = AT_REST;
        let deep = |_: f32, _: f32| Some(Ground { lo: -20.0, hi: -20.0, gradient: 0.0 });
        assert!(afloat(clipper(), &m, deep));
        assert!(afloat(clipper(), &m, |_, _| None));
        // A shoal only under the bow, which is 9 m ahead (-z).
        let shoal = |_: f32, z: f32| Some(Ground { lo: -0.5, hi: if z < -8.0 { -0.5 } else { -20.0 }, gradient: 0.0 });
        assert!(!afloat(clipper(), &m, shoal));
        let draft = -clipper().draft_m;
        assert!(afloat(clipper(), &m, |_, _| Some(Ground { lo: draft, hi: draft, gradient: 0.0 })));
    }

    #[test]
    fn changing_frame_allows_for_the_ships_speed_and_lag() {
        let still = frame_change_allowance(5.4, 0.0, 0.5);
        assert_eq!(still, crate::movement::allowed_distance(5.4, 0.5));
        let fast = frame_change_allowance(5.4, -11.0, 2.0);
        assert!(close(fast - frame_change_allowance(5.4, 0.0, 2.0), 11.0 * POSE_LAG_S, 1e-4), "{fast}");
    }

    #[test]
    fn the_deck_is_a_flat_rectangle() {
        let h = clipper();
        assert_eq!(deck_ground(h, 2.4, -8.9), Some(Ground { lo: 0.0, hi: 0.0, gradient: 0.0 }));
        assert_eq!(deck_ground(h, 2.6, 0.0), None);
        assert!(on_deck(h, 3.9, 0.0, RAIL_M) && !on_deck(h, 4.1, 0.0, RAIL_M));
    }

    #[test]
    fn a_flat_shot_matches_the_closed_form() {
        let f = cannon_flight([0.0, 3.0, 0.0], 0.0, 0.0, 40.0).unwrap();
        let t = (2.0 * 3.0 / GRAVITY).sqrt();
        assert!(close(f.secs, t, 1e-5));
        assert!(close(f.landing[2], -40.0 * t, 1e-3) && close(f.landing[0], 0.0, 1e-4), "{f:?}");
        assert_eq!(f.landing[1], SEA_LEVEL_M);
    }

    #[test]
    fn higher_shots_fly_further_and_longer_up_to_the_cannons_limits() {
        let c = &clipper().cannon;
        let from = [0.0, 3.0, 0.0];
        let range = |deg: f32| {
            let f = cannon_flight(from, 0.0, deg.to_radians(), c.muzzle_speed_mps).unwrap();
            (-f.landing[2], f.secs)
        };
        let (low, low_t) = range(c.min_pitch_deg);
        let (high, high_t) = range(c.max_pitch_deg);
        assert!(low > 20.0 && low < 30.0, "{low}");
        assert!(high > 85.0 && high < 95.0, "{high}");
        assert!(high_t > low_t);
        assert!(cannon_flight([0.0, 0.0, 0.0], 0.0, 0.1, 40.0).is_none());
        assert!(cannon_flight([f32::NAN, 3.0, 0.0], 0.0, 0.1, 40.0).is_none());
    }

    #[test]
    fn cannons_aim_within_their_arc() {
        let c = &clipper().cannon;
        assert!(aim_ok(c, 0.5, 0.1));
        assert!(!aim_ok(c, 0.6, 0.1), "past 30°");
        assert!(!aim_ok(c, 0.0, 0.3), "past 15°");
        assert!(!aim_ok(c, 0.0, -0.1), "below -5°");
        assert!(!aim_ok(c, f32::NAN, 0.0));
    }

    #[test]
    fn the_starboard_mount_fires_to_starboard() {
        let h = clipper();
        let pose = Pose { x: 10.0, y: 2.0, z: 10.0, yaw: 0.0 };
        let mount = &h.mounts[1];
        let (from, yaw) = muzzle(h, &pose, mount, 0.0);
        assert_eq!(from, [12.0, 3.0, 10.0]);
        let f = cannon_flight(from, yaw, 0.0, h.cannon.muzzle_speed_mps).unwrap();
        assert!(f.landing[0] > 40.0 && close(f.landing[2], 10.0, 1e-3), "{f:?}");
    }

    #[test]
    fn hits_land_on_the_hull_or_within_the_splash() {
        let h = clipper();
        let pose = Pose { x: 0.0, y: 2.0, z: 0.0, yaw: FRAC_PI_2 };
        // Turned a quarter left, the ship lies along x.
        assert!(hits(h, &pose, [8.0, 0.0, 0.0]));
        assert!(hits(h, &pose, [0.0, 0.0, 4.4]));
        assert!(!hits(h, &pose, [0.0, 0.0, 4.6]));
        assert!(!hits(h, &pose, [11.5, 0.0, 0.0]));
    }
}
