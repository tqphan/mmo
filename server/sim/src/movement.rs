//! Server-side checks on client-predicted movement (PLAN §4.2, docs/design/movement.md).
//!
//! The client sends movement events; the server checks that the distance covered since the last
//! accepted event is possible at the mode's speed limit (`check_move`), then that the height is
//! possible over the terrain there (`check_vertical`, docs/design/terrain.md). Obstacle outlines
//! come later.

/// The modes and their speed limits come from `content/movement.toml`.
pub use crate::content::MoveMode;
use crate::terrain::Ground;

/// Fractional allowance on top of the speed limit, for clock jitter between client and server.
pub const SPEED_TOLERANCE: f32 = 0.15;
/// Flat allowance in metres, for float error and the last few ms of a stop.
pub const DISTANCE_SLACK_M: f32 = 0.5;
/// Gaps longer than this count as this long, so going silent and then jumping is still a teleport.
/// Clients send a heartbeat every 0.5 s while moving.
pub const MAX_CREDITED_GAP_S: f32 = 2.0;

/// The furthest a character may travel horizontally in `dt_s` seconds at the given limit.
pub fn allowed_distance(max_speed_mps: f32, dt_s: f32) -> f32 {
    let dt = dt_s.clamp(0.0, MAX_CREDITED_GAP_S);
    max_speed_mps * (1.0 + SPEED_TOLERANCE) * dt + DISTANCE_SLACK_M
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Ok,
    /// The move covers more ground than allowed; carries the distance and the allowance.
    TooFar { moved_m: f32, allowed_m: f32 },
    /// A coordinate is NaN or infinite.
    NotFinite,
}

/// Check a move from `from` to `to` (both `[x, y, z]`) made `dt_s` seconds after the last accepted
/// one. Only the horizontal plane is checked.
pub fn check_move(from: [f32; 3], to: [f32; 3], dt_s: f32, mode: MoveMode) -> Verdict {
    check_distance(from, to, allowed_distance(mode.data().max_speed_mps, dt_s))
}

/// Check that a move covers at most `allowed_m` horizontally.
pub fn check_distance(from: [f32; 3], to: [f32; 3], allowed_m: f32) -> Verdict {
    if !to.iter().all(|c| c.is_finite()) {
        return Verdict::NotFinite;
    }
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    let moved_m = (dx * dx + dz * dz).sqrt();
    if moved_m <= allowed_m {
        Verdict::Ok
    } else {
        Verdict::TooFar { moved_m, allowed_m }
    }
}

/// Highest a character may be above its anchor before slope credit: a 1.5 m jump plus slack.
/// The vertical bounds are placeholders and lenient on purpose (docs/design/terrain.md).
pub const JUMP_ALLOWANCE_M: f32 = 2.0;
/// Rise allowed per metre from the anchor, and the steepest ground that counts as standing: about
/// 51°, steeper than the client's 45° walkable limit.
pub const CLIMB_GRADIENT: f32 = 1.25;
/// While airborne, slope credit stops after this long at the mode's top speed: the ground walked
/// between the last reported contact and taking off.
pub const AIR_CREDIT_S: f32 = 1.0;
/// Within this of the ground counts as touching it.
pub const CONTACT_M: f32 = 0.3;
/// How far below the ground a position may be, for float error and the capsule's skin.
pub const BELOW_GROUND_SLACK_M: f32 = 0.5;
/// Airborne this long since the last ground contact before the fall rule applies: the heartbeat
/// gap, the time to a jump's apex, and network jitter.
pub const AIR_GRACE_S: f32 = 1.5;
/// The fall rule's gravity and top speed, well under the client's, so real falls always pass.
pub const FALL_GRAVITY: f32 = 6.0;
pub const FALL_MAX_SPEED: f32 = 20.0;
/// Until the land-use raster carries water heights, all water is the sea at this height.
pub const SEA_LEVEL_M: f32 = 0.0;
/// How far above the water's surface a swimmer may be.
pub const SWIM_SURFACE_SLACK_M: f32 = 1.0;

/// The last solid footing: where vertical moves are measured from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// What a character's feet were on after an accepted move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footing {
    /// No terrain there, so nothing was checked.
    Unknown,
    Standing,
    /// Touching ground too steep to stand on: sliding, or a cliff face.
    Steep,
    Airborne,
    Swimming,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Vertical {
    Ok(Footing),
    BelowGround { y: f32, ground: f32 },
    TooHigh { y: f32, ceiling: f32 },
}

/// The least a character must have fallen `secs` after it last touched the ground.
pub fn min_fall_m(secs: f32) -> f32 {
    let t = (secs - AIR_GRACE_S).max(0.0);
    let t_top = FALL_MAX_SPEED / FALL_GRAVITY;
    if t <= t_top {
        0.5 * FALL_GRAVITY * t * t
    } else {
        0.5 * FALL_GRAVITY * t_top * t_top + FALL_MAX_SPEED * (t - t_top)
    }
}

/// Check the height of a move to `to`, already checked by `check_move`, against the `ground` there
/// (`None` where the terrain is unknown). `since_contact_s` is the time since the character last
/// touched the ground.
pub fn check_vertical(ground: Option<Ground>, to: [f32; 3], mode: MoveMode, anchor: Anchor, since_contact_s: f32) -> Vertical {
    let Some(g) = ground else { return Vertical::Ok(Footing::Unknown) };
    let y = to[1];
    if y < g.lo - BELOW_GROUND_SLACK_M {
        return Vertical::BelowGround { y, ground: g.lo };
    }
    if mode == MoveMode::Swim {
        let ceiling = SEA_LEVEL_M + SWIM_SURFACE_SLACK_M;
        return if y <= ceiling { Vertical::Ok(Footing::Swimming) } else { Vertical::TooHigh { y, ceiling } };
    }
    let airborne = y > g.hi + CONTACT_M;
    let (dx, dz) = (to[0] - anchor.x, to[2] - anchor.z);
    let mut run = (dx * dx + dz * dz).sqrt();
    let mut ceiling = anchor.y + JUMP_ALLOWANCE_M;
    if airborne {
        run = run.min(mode.data().max_speed_mps * AIR_CREDIT_S);
        ceiling -= min_fall_m(since_contact_s);
    }
    ceiling += CLIMB_GRADIENT * run;
    if y > ceiling {
        Vertical::TooHigh { y, ceiling }
    } else if airborne {
        Vertical::Ok(Footing::Airborne)
    } else if g.gradient <= CLIMB_GRADIENT {
        Vertical::Ok(Footing::Standing)
    } else {
        Vertical::Ok(Footing::Steep)
    }
}

impl Anchor {
    pub fn at(p: [f32; 3]) -> Self {
        Self { x: p[0], y: p[1], z: p[2] }
    }

    /// The anchor after an accepted move to `to`, and whether the character touched the ground (which
    /// restarts the fall clock). A steep face moves the anchor only downwards, so it can be slid
    /// down but not climbed.
    pub fn after(self, to: [f32; 3], footing: Footing) -> (Self, bool) {
        match footing {
            Footing::Unknown | Footing::Standing | Footing::Swimming => (Self::at(to), true),
            Footing::Steep => (if to[1] < self.y { Self::at(to) } else { self }, true),
            Footing::Airborne => (self, false),
        }
    }
}

/// The height to put a character at when the server places it: on entering, teleporting or being
/// corrected. On the ground, unless it is swimming in open water; unchanged where the terrain is
/// unknown.
pub fn settle(ground: Option<Ground>, pos: [f32; 3], mode: MoveMode) -> f32 {
    let Some(g) = ground else { return pos[1] };
    let afloat = (g.lo..=SEA_LEVEL_M + SWIM_SURFACE_SLACK_M).contains(&pos[1]);
    if mode == MoveMode::Swim && afloat { pos[1] } else { g.hi }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_at_the_limit_passes() {
        let to = [5.4 * 0.5, 0.0, 0.0];
        assert_eq!(check_move([0.0; 3], to, 0.5, MoveMode::Run), Verdict::Ok);
    }

    #[test]
    fn twice_the_run_speed_fails() {
        let to = [0.0, 0.0, 5.4 * 2.0 * 1.0];
        assert!(matches!(check_move([0.0; 3], to, 1.0, MoveMode::Run), Verdict::TooFar { .. }));
    }

    #[test]
    fn walking_speed_is_lower() {
        let to = [4.0, 0.0, 0.0];
        assert_eq!(check_move([0.0; 3], to, 1.0, MoveMode::Run), Verdict::Ok);
        assert!(matches!(check_move([0.0; 3], to, 1.0, MoveMode::Walk), Verdict::TooFar { .. }));
    }

    #[test]
    fn check_move_ignores_height() {
        assert_eq!(check_move([0.0; 3], [0.0, 500.0, 0.0], 0.1, MoveMode::Run), Verdict::Ok);
    }

    #[test]
    fn long_silence_does_not_buy_a_teleport() {
        let to = [100.0, 0.0, 0.0];
        assert!(matches!(check_move([0.0; 3], to, 60.0, MoveMode::Run), Verdict::TooFar { .. }));
        assert_eq!(allowed_distance(5.4, 60.0), allowed_distance(5.4, MAX_CREDITED_GAP_S));
    }

    #[test]
    fn negative_dt_only_gets_the_slack() {
        assert_eq!(allowed_distance(5.4, -3.0), DISTANCE_SLACK_M);
    }

    #[test]
    fn non_finite_targets_are_rejected() {
        assert_eq!(check_move([0.0; 3], [f32::NAN, 0.0, 0.0], 1.0, MoveMode::Run), Verdict::NotFinite);
        assert_eq!(check_move([0.0; 3], [0.0, f32::INFINITY, 0.0], 1.0, MoveMode::Run), Verdict::NotFinite);
    }

    #[test]
    fn unknown_modes_are_none() {
        assert_eq!(MoveMode::from_u8(1), Some(MoveMode::Run));
        assert_eq!(MoveMode::from_u8(200), None);
    }
    fn flat(h: f32) -> Option<Ground> {
        Some(Ground { lo: h, hi: h, gradient: 0.0 })
    }

    fn sloped(h: f32, gradient: f32) -> Option<Ground> {
        Some(Ground { lo: h, hi: h, gradient })
    }

    const ORIGIN: Anchor = Anchor { x: 0.0, y: 0.0, z: 0.0 };

    #[test]
    fn walking_up_a_40_degree_slope_passes() {
        let mut anchor = ORIGIN;
        let g = 40f32.to_radians().tan();
        for i in 1..=10 {
            let x = 2.7 * i as f32;
            let to = [x, x * g, 0.0];
            let v = check_vertical(sloped(x * g, g), to, MoveMode::Run, anchor, 0.5);
            assert_eq!(v, Vertical::Ok(Footing::Standing), "step {i}");
            anchor = anchor.after(to, Footing::Standing).0;
        }
    }

    #[test]
    fn teleporting_up_a_cliff_fails() {
        let v = check_vertical(flat(20.0), [3.0, 20.0, 0.0], MoveMode::Run, ORIGIN, 0.5);
        assert!(matches!(v, Vertical::TooHigh { .. }));
    }

    #[test]
    fn a_cliff_face_cannot_be_climbed_in_small_steps() {
        // A face rising 5 m per metre from x = 3.
        let mut anchor = ORIGIN;
        for i in 1..=40 {
            let x = 3.0 + 0.1 * i as f32;
            let to = [x, 0.5 * i as f32, 0.0];
            match check_vertical(sloped(to[1], 5.0), to, MoveMode::Run, anchor, 0.1) {
                Vertical::Ok(f) => {
                    assert_eq!(f, Footing::Steep);
                    anchor = anchor.after(to, f).0;
                }
                Vertical::TooHigh { .. } => {
                    // No higher than a jump plus the credit for the 3 m walked up to the face.
                    assert!(to[1] <= JUMP_ALLOWANCE_M + CLIMB_GRADIENT * x + 0.5, "climbed to {} m", to[1]);
                    return;
                }
                v => panic!("{v:?}"),
            }
        }
        panic!("climbed the whole face");
    }

    #[test]
    fn a_jump_passes_and_a_fly_hack_does_not() {
        let jump = check_vertical(flat(0.0), [1.4, 1.5, 0.0], MoveMode::Run, ORIGIN, 0.3);
        assert_eq!(jump, Vertical::Ok(Footing::Airborne));
        let fly = check_vertical(flat(0.0), [2.0, 5.0, 0.0], MoveMode::Run, ORIGIN, 0.5);
        assert!(matches!(fly, Vertical::TooHigh { .. }));
    }

    #[test]
    fn falling_off_a_cliff_passes() {
        // Off a 50 m cliff at run speed, falling at 9.8 m/s² from the edge.
        let top = Anchor { x: 0.0, y: 50.0, z: 0.0 };
        for t in [0.5f32, 1.0, 1.5, 2.0, 2.5, 3.0] {
            let to = [5.4 * t, 50.0 - 4.9 * t * t, 0.0];
            assert_eq!(check_vertical(flat(0.0), to, MoveMode::Run, top, t), Vertical::Ok(Footing::Airborne), "{t}");
        }
        let landed = check_vertical(flat(0.0), [17.0, 0.0, 0.0], MoveMode::Run, top, 3.2);
        assert_eq!(landed, Vertical::Ok(Footing::Standing));
    }

    #[test]
    fn hovering_is_caught_after_the_grace() {
        let top = Anchor { x: 0.0, y: 50.0, z: 0.0 };
        let hover = |t: f32| check_vertical(flat(0.0), [5.4 * t, 50.0, 0.0], MoveMode::Run, top, t);
        assert_eq!(hover(1.0), Vertical::Ok(Footing::Airborne));
        assert!(matches!(hover(3.5), Vertical::TooHigh { .. }));
        // Staying just above the ground is airborne too.
        let skim = check_vertical(flat(0.0), [0.0, 1.0, 0.0], MoveMode::Run, ORIGIN, 5.0);
        assert!(matches!(skim, Vertical::TooHigh { .. }));
    }

    #[test]
    fn the_fall_rule_starts_after_the_grace_and_tops_out() {
        assert_eq!(min_fall_m(AIR_GRACE_S), 0.0);
        assert_eq!(min_fall_m(-1.0), 0.0);
        assert!((min_fall_m(AIR_GRACE_S + 1.0) - 3.0).abs() < 1e-5);
        let t_top = FALL_MAX_SPEED / FALL_GRAVITY;
        let a = min_fall_m(AIR_GRACE_S + t_top + 1.0);
        let b = min_fall_m(AIR_GRACE_S + t_top + 2.0);
        assert!((b - a - FALL_MAX_SPEED).abs() < 1e-3);
    }

    #[test]
    fn below_ground_fails_past_the_slack() {
        let deep = check_vertical(flat(0.0), [0.0, -1.0, 0.0], MoveMode::Run, ORIGIN, 0.0);
        assert!(matches!(deep, Vertical::BelowGround { .. }));
        let skin = check_vertical(flat(0.0), [0.0, -0.4, 0.0], MoveMode::Run, ORIGIN, 0.0);
        assert_eq!(skin, Vertical::Ok(Footing::Standing));
    }

    #[test]
    fn swimming_needs_water() {
        let anywhere = Anchor { x: 0.0, y: 100.0, z: 0.0 };
        let sea = |y| check_vertical(flat(-10.0), [0.0, y, 0.0], MoveMode::Swim, anywhere, 9.0);
        assert_eq!(sea(-3.0), Vertical::Ok(Footing::Swimming));
        assert_eq!(sea(0.5), Vertical::Ok(Footing::Swimming));
        assert!(matches!(sea(5.0), Vertical::TooHigh { .. }));
        assert!(matches!(sea(-11.0), Vertical::BelowGround { .. }));
        let land = check_vertical(flat(10.0), [0.0, 10.0, 0.0], MoveMode::Swim, anywhere, 0.0);
        assert!(matches!(land, Vertical::TooHigh { .. }));
    }

    #[test]
    fn unknown_terrain_is_not_checked() {
        let v = check_vertical(None, [0.0, 900.0, 0.0], MoveMode::Run, ORIGIN, 60.0);
        assert_eq!(v, Vertical::Ok(Footing::Unknown));
    }

    #[test]
    fn the_anchor_follows_footing() {
        let a = Anchor { x: 0.0, y: 10.0, z: 0.0 };
        assert_eq!(a.after([1.0, 12.0, 0.0], Footing::Standing), (Anchor::at([1.0, 12.0, 0.0]), true));
        assert_eq!(a.after([1.0, 12.0, 0.0], Footing::Steep), (a, true));
        assert_eq!(a.after([1.0, 8.0, 0.0], Footing::Steep), (Anchor::at([1.0, 8.0, 0.0]), true));
        assert_eq!(a.after([1.0, 11.0, 0.0], Footing::Airborne), (a, false));
        assert!(a.after([1.0, -2.0, 0.0], Footing::Swimming).1);
    }

    #[test]
    fn settling_puts_you_on_the_ground_unless_afloat() {
        let g = Some(Ground { lo: -10.0, hi: -9.5, gradient: 0.0 });
        assert_eq!(settle(g, [0.0, 30.0, 0.0], MoveMode::Run), -9.5);
        assert_eq!(settle(g, [0.0, -3.0, 0.0], MoveMode::Swim), -3.0);
        assert_eq!(settle(g, [0.0, 30.0, 0.0], MoveMode::Swim), -9.5);
        assert_eq!(settle(None, [0.0, 30.0, 0.0], MoveMode::Run), 30.0);
    }
}
