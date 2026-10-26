//! Labor (docs/design/labor.md): one pool per account that regenerates from a timestamp and is
//! never ticked. The client ports this to show the pool counting up.

pub use crate::content::{LABOR, LaborRules};

const MICROS_PER_S: i64 = 1_000_000;

/// A pool as stored: `points` as of `at`, in µs since the Unix epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pool {
    pub points: i32,
    pub at: i64,
}

/// Not enough labor for a cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Short {
    pub have: i32,
    pub need: i32,
}

impl Pool {
    /// A new account's pool.
    pub fn new(rules: &LaborRules, at: i64) -> Self {
        Self { points: rules.start_points, at }
    }

    /// The pool as of `now`: every whole period since `at` adds `regen_points`, up to the cap.
    /// Below the cap `at` moves on by those whole periods only, so progress toward the next points
    /// is kept. At the cap it moves to `now`, since a full pool builds no progress.
    pub fn settle(self, rules: &LaborRules, now: i64) -> Self {
        if self.points >= rules.cap {
            return Self { points: self.points, at: now };
        }
        let period = i64::from(rules.regen_period_s) * MICROS_PER_S;
        let periods = now.saturating_sub(self.at).max(0) / period;
        let points = i64::from(self.points) + periods.saturating_mul(i64::from(rules.regen_points));
        if points >= i64::from(rules.cap) {
            Self { points: rules.cap, at: now }
        } else {
            // Below the cap, so the cast is exact.
            Self { points: points as i32, at: self.at + periods * period }
        }
    }

    pub fn current(self, rules: &LaborRules, now: i64) -> i32 {
        self.settle(rules, now).points
    }

    /// When the next points arrive, or `None` while the pool is full.
    pub fn next_regen_at(self, rules: &LaborRules, now: i64) -> Option<i64> {
        let s = self.settle(rules, now);
        (s.points < rules.cap).then(|| s.at + i64::from(rules.regen_period_s) * MICROS_PER_S)
    }

    /// Settle at `now`, then take `cost` off. `cost` must be above 0.
    pub fn spend(self, rules: &LaborRules, now: i64, cost: i32) -> Result<Self, Short> {
        let s = self.settle(rules, now);
        if cost <= 0 || cost > s.points {
            return Err(Short { have: s.points, need: cost });
        }
        Ok(Self { points: s.points - cost, at: s.at })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: LaborRules = LaborRules { cap: 5000, regen_points: 10, regen_period_s: 300, start_points: 5000 };
    const T0: i64 = 1_800_000_000_000_000;
    const P: i64 = 300 * MICROS_PER_S;

    fn pool(points: i32) -> Pool {
        Pool { points, at: T0 }
    }

    #[test]
    fn only_whole_periods_count_and_progress_is_kept() {
        assert_eq!(pool(100).settle(&R, T0 + P - 1), pool(100));
        assert_eq!(pool(100).settle(&R, T0 + P), Pool { points: 110, at: T0 + P });
        assert_eq!(pool(100).settle(&R, T0 + 3 * P + 7), Pool { points: 130, at: T0 + 3 * P });
        // Settling in steps or at once gives the same pool.
        let stepped = pool(100).settle(&R, T0 + P + 5).settle(&R, T0 + 3 * P + 7);
        assert_eq!(stepped, pool(100).settle(&R, T0 + 3 * P + 7));
    }

    #[test]
    fn a_full_pool_anchors_at_now() {
        assert_eq!(pool(4995).settle(&R, T0 + P), Pool { points: 5000, at: T0 + P });
        assert_eq!(pool(4995).settle(&R, T0 + 2 * P + 9), Pool { points: 5000, at: T0 + 2 * P + 9 });
        assert_eq!(pool(5000).settle(&R, T0 + 42), Pool { points: 5000, at: T0 + 42 });
        // Above the cap (a future grant) is kept, not cut down.
        assert_eq!(pool(6000).current(&R, T0 + 10 * P), 6000);
    }

    #[test]
    fn spending_from_a_full_pool_starts_a_full_period() {
        let now = T0 + 1234;
        let after = pool(5000).spend(&R, now, 25).unwrap();
        assert_eq!(after, Pool { points: 4975, at: now });
        assert_eq!(after.current(&R, now + P - 1), 4975);
        assert_eq!(after.current(&R, now + P), 4985);
        assert_eq!(after.next_regen_at(&R, now), Some(now + P));
    }

    #[test]
    fn spending_below_the_cap_keeps_progress() {
        let after = pool(100).spend(&R, T0 + P + 100, 5).unwrap();
        assert_eq!(after, Pool { points: 105, at: T0 + P });
        assert_eq!(after.next_regen_at(&R, T0 + P + 100), Some(T0 + 2 * P));
    }

    #[test]
    fn refusals() {
        assert_eq!(pool(10).spend(&R, T0, 11), Err(Short { have: 10, need: 11 }));
        assert_eq!(pool(10).spend(&R, T0 + P, 20).map(|p| p.points), Ok(0));
        assert_eq!(pool(10).spend(&R, T0, 0), Err(Short { have: 10, need: 0 }));
        assert_eq!(pool(10).spend(&R, T0, -5), Err(Short { have: 10, need: -5 }));
    }

    #[test]
    fn a_clock_before_the_anchor_adds_nothing() {
        assert_eq!(pool(100).settle(&R, T0 - 10 * P), pool(100));
    }

    #[test]
    fn huge_gaps_saturate_at_the_cap() {
        assert_eq!(pool(0).current(&R, i64::MAX), 5000);
        assert_eq!(Pool { points: 0, at: i64::MIN / 2 }.current(&R, i64::MAX / 2), 5000);
    }

    #[test]
    fn a_full_pool_has_no_next_regen() {
        assert_eq!(pool(5000).next_regen_at(&R, T0), None);
        assert_eq!(pool(0).next_regen_at(&R, T0 + 7), Some(T0 + P));
    }

    #[test]
    fn new_pools_start_with_the_content_value() {
        assert_eq!(Pool::new(&LABOR, T0).points, LABOR.start_points);
    }
}
