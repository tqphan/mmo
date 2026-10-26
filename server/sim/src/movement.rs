//! Server-side checks on client-predicted movement (PLAN §4.2, docs/design/movement.md).
//!
//! The client sends movement events; the server only checks that the distance covered since the
//! last accepted event is possible at the mode's speed limit. Vertical and obstacle checks need the
//! heightmap and come later (M0 spike 5).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MoveMode {
    Walk = 0,
    Run = 1,
    Swim = 2,
}

impl MoveMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Walk),
            1 => Some(Self::Run),
            2 => Some(Self::Swim),
            _ => None,
        }
    }

    /// Horizontal speed limit in m/s. Placeholder numbers, confidence C (see the spec).
    pub fn max_speed_mps(self) -> f32 {
        match self {
            Self::Walk => 2.0,
            Self::Run => 5.4,
            Self::Swim => 3.5,
        }
    }
}

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
    if !to.iter().all(|c| c.is_finite()) {
        return Verdict::NotFinite;
    }
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    let moved_m = (dx * dx + dz * dz).sqrt();
    let allowed_m = allowed_distance(mode.max_speed_mps(), dt_s);
    if moved_m <= allowed_m {
        Verdict::Ok
    } else {
        Verdict::TooFar { moved_m, allowed_m }
    }
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
    fn height_is_ignored_for_now() {
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
}
