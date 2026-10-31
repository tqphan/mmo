//! Parties (docs/design/social.md): invitations that lapse, and how a kill's experience is shared.
//! The module keeps the members and decides who is in range.

pub use crate::content::{PARTY, PartyRules};

const MICROS_PER_S: i64 = 1_000_000;

/// The most characters in a party.
pub fn size() -> usize {
    PARTY.size.unsigned_abs() as usize
}

/// When an invitation made at `made_at` lapses, both in µs since the Unix epoch.
pub fn lapses_at(made_at: i64) -> i64 {
    made_at + i64::from(PARTY.invite_s) * MICROS_PER_S
}

/// Whether an invitation made at `made_at` has lapsed by `now`.
pub fn lapsed(made_at: i64, now: i64) -> bool {
    now >= lapses_at(made_at)
}

/// Whether a member at `at` is near enough to a kill at `kill` to share it: horizontally only, so
/// a member on a cliff or a deck above still counts.
pub fn shares(kill: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - kill[0], at[2] - kill[2]);
    dx * dx + dz * dz <= PARTY.share_m * PARTY.share_m
}

/// One sharer's part of a kill worth `xp` to it, among `sharers` (at least 1): the whole with the
/// bonus for each sharer beyond the first, split evenly, rounded down. Alone, it's all of `xp`.
pub fn share(xp: u32, sharers: usize) -> u32 {
    let n = sharers.max(1) as u64;
    let pct = 100 + u64::from(PARTY.bonus_pct.unsigned_abs()) * (n - 1);
    u32::try_from(u64::from(xp) * pct / (100 * n)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitations_lapse_at_their_time() {
        let made = 1_000 * MICROS_PER_S;
        let at = made + 60 * MICROS_PER_S;
        assert_eq!(lapses_at(made), at);
        assert!(!lapsed(made, at - 1));
        assert!(lapsed(made, at));
    }

    #[test]
    fn sharing_is_horizontal() {
        let kill = [0.0, 0.0, 0.0];
        assert!(shares(kill, [100.0, 0.0, 0.0]));
        assert!(!shares(kill, [71.0, 0.0, 71.0]));
        assert!(shares(kill, [0.0, 500.0, 99.0]));
    }

    #[test]
    fn shares_add_a_bonus_and_split_evenly() {
        assert_eq!(size(), 5);
        assert_eq!(share(100, 1), 100);
        assert_eq!(share(100, 0), 100);
        // 110% split two ways, then 140% five ways.
        assert_eq!(share(100, 2), 55);
        assert_eq!(share(100, 5), 28);
        assert_eq!(share(7, 3), 2);
        assert_eq!(share(u32::MAX, 2), 2_362_232_012);
    }
}
