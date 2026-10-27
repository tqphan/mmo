//! Gathering (docs/design/gathering.md): who may gather a node, when it's ready, and how far a
//! gather has got. Readiness comes from a timestamp and is never ticked. The client ports this to
//! show prompts and the progress bar.

pub use crate::content::{GATHER_REACH_M, NODES, NodeData, node, node_by_id};

const MICROS_PER_S: i64 = 1_000_000;

/// A gather is cancelled once the gatherer stands further than this from where it started.
pub const MOVE_CANCEL_M: f32 = 0.5;

/// A character standing at `at` (its feet) can reach a node at `node`: within the reach
/// horizontally, and within it up or down.
pub fn in_reach(node: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - node[0], at[2] - node[2]);
    dx * dx + dz * dz <= GATHER_REACH_M * GATHER_REACH_M && (at[1] - node[1]).abs() <= GATHER_REACH_M
}

/// A node whose `ready_at` (µs since the Unix epoch) has come can be gathered.
pub fn ready(ready_at: i64, now: i64) -> bool {
    now >= ready_at
}

/// When a node taken at `now` can be gathered again.
pub fn regrown_at(kind: &NodeData, now: i64) -> i64 {
    now.saturating_add(i64::from(kind.regrow_s) * MICROS_PER_S)
}

/// When a gather started at `now` finishes, in µs.
pub fn done_at(kind: &NodeData, now: i64) -> i64 {
    now.saturating_add(gather_micros(kind))
}

pub fn gather_micros(kind: &NodeData) -> i64 {
    (f64::from(kind.gather_s) * MICROS_PER_S as f64).round() as i64
}

/// How far a gather finishing at `done_at` has got at `now`, from 0 to 1.
pub fn progress(kind: &NodeData, done_at: i64, now: i64) -> f32 {
    let total = gather_micros(kind);
    let left = done_at.saturating_sub(now);
    (1.0 - left as f64 / total as f64).clamp(0.0, 1.0) as f32
}

/// The gatherer has moved off the spot it started from (horizontally).
pub fn moved_off(start: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - start[0], at[2] - start[2]);
    dx * dx + dz * dz > MOVE_CANCEL_M * MOVE_CANCEL_M
}

/// The yield for a random number `r`, from `yield_min` to `yield_max` inclusive.
pub fn yield_for(kind: &NodeData, r: u32) -> u32 {
    kind.yield_min + r % (kind.yield_max - kind.yield_min + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reed() -> &'static NodeData {
        node_by_id("saltreed_clump").unwrap()
    }

    #[test]
    fn reach_is_horizontal_and_vertical() {
        let n = [10.0, 5.0, -4.0];
        assert!(in_reach(n, n));
        assert!(in_reach(n, [10.0 + GATHER_REACH_M, 5.0, -4.0]), "the edge counts");
        assert!(in_reach(n, [12.0, 5.0, -2.0]), "2.83 m on the diagonal");
        assert!(!in_reach(n, [10.0, 5.0, -4.0 - GATHER_REACH_M - 0.01]));
        assert!(!in_reach(n, [12.2, 5.0, -1.8]), "3.1 m on the diagonal");
        assert!(in_reach(n, [10.0, 5.0 + GATHER_REACH_M, -4.0]));
        assert!(!in_reach(n, [10.0, 5.0 - GATHER_REACH_M - 0.01, -4.0]), "too far below");
    }

    #[test]
    fn readiness_comes_from_the_timestamp() {
        let t = 1_800_000_000_000_000;
        assert!(ready(t, t));
        assert!(ready(t, t + 1));
        assert!(!ready(t, t - 1));
        let again = regrown_at(reed(), t);
        assert_eq!(again - t, i64::from(reed().regrow_s) * 1_000_000);
        assert!(!ready(again, t + 1) && ready(again, again));
        assert_eq!(regrown_at(reed(), i64::MAX), i64::MAX, "saturates");
    }

    #[test]
    fn progress_runs_from_zero_to_one() {
        let t = 1_800_000_000_000_000;
        let k = reed();
        let done = done_at(k, t);
        assert_eq!(done - t, gather_micros(k));
        assert_eq!(progress(k, done, t), 0.0);
        assert_eq!(progress(k, done, t - 5_000_000), 0.0, "clamped before the start");
        assert!((progress(k, done, t + gather_micros(k) / 2) - 0.5).abs() < 1e-6);
        assert_eq!(progress(k, done, done), 1.0);
        assert_eq!(progress(k, done, done + 1_000_000), 1.0, "clamped after the end");
    }

    #[test]
    fn moving_off_the_spot() {
        let s = [3.0, 1.0, 3.0];
        assert!(!moved_off(s, s));
        assert!(!moved_off(s, [3.3, 4.0, 3.3]), "0.42 m, and height doesn't count");
        assert!(moved_off(s, [3.0, 1.0, 3.0 + MOVE_CANCEL_M + 0.01]));
    }

    #[test]
    fn yields_cover_the_range() {
        for k in &NODES {
            let got: Vec<u32> = (0..20).map(|r| yield_for(k, r)).collect();
            assert_eq!(got.iter().min(), Some(&k.yield_min), "{}", k.id);
            assert_eq!(got.iter().max(), Some(&k.yield_max), "{}", k.id);
        }
        assert!(NODES.iter().all(|k| (0..1000).all(|r| yield_for(k, u32::MAX - r) >= k.yield_min)));
    }

    #[test]
    fn every_node_yields_an_item() {
        for k in &NODES {
            assert!(crate::content::item(k.item).is_some(), "{}", k.id);
            assert_eq!(node(k.wire), Some(k));
        }
    }
}
