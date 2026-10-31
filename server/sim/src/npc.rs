//! NPCs (docs/design/npcs.md): where an NPC is along its segment, which skill it uses, whom it
//! fights, what it drops and who may take it. Positions come from the segment's timestamps, never
//! from stepping.

pub use crate::content::{LootData, NPC, NPC_KINDS, NpcKindData, npc_kind, npc_kind_by_id};
use crate::content::SkillData;

const MICROS_PER_S: i64 = 1_000_000;
const MICROS_PER_MS: i64 = 1_000;
/// The least an NPC closes to before it stops, m.
pub const MIN_APPROACH_M: f32 = 1.5;
/// An NPC closes to this share of its shortest skill range.
pub const APPROACH_SHARE: f32 = 0.8;

/// A straight walk in the world: from (`x`, `z`) along `yaw` at `speed` m/s, from `start` until
/// `end` (µs of game time). Still when `speed` is 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub speed: f32,
    pub start: i64,
    pub end: i64,
}

impl Segment {
    /// Standing at (x, z), facing `yaw`, from `at`.
    pub fn still(x: f32, z: f32, yaw: f32, at: i64) -> Self {
        Segment { x, z, yaw, speed: 0.0, start: at, end: at }
    }

    /// Where it is at `t`: the start before it begins, the end after it ends.
    pub fn at(&self, t: i64) -> [f32; 2] {
        let secs = (t.clamp(self.start, self.end) - self.start) as f32 / MICROS_PER_S as f32;
        let [fx, fz] = crate::ship::facing(self.yaw);
        [self.x + fx * self.speed * secs, self.z + fz * self.speed * secs]
    }

    pub fn moving(&self, t: i64) -> bool {
        self.speed > 0.0 && t < self.end
    }

    /// Where it stops.
    pub fn end_point(&self) -> [f32; 2] {
        self.at(self.end)
    }
}

pub fn distance2d(a: [f32; 2], b: [f32; 2]) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The yaw that faces from `from` to `to` (`sim::ship::facing`'s inverse); `keep` if they coincide.
pub fn yaw_to(from: [f32; 2], to: [f32; 2], keep: f32) -> f32 {
    let (dx, dz) = (to[0] - from[0], to[1] - from[1]);
    if dx * dx + dz * dz < 1e-6 { keep } else { (-dx).atan2(-dz) }
}

/// Walk from `from` towards `to` at `speed` m/s from `now`, stopping `short` m before it. Already
/// that close, it stands, turned to face `to`.
pub fn walk(from: [f32; 2], to: [f32; 2], speed: f32, short: f32, now: i64, yaw: f32) -> Segment {
    let yaw = yaw_to(from, to, yaw);
    let length = distance2d(from, to) - short;
    if length <= 0.01 || speed <= 0.0 {
        return Segment::still(from[0], from[1], yaw, now);
    }
    let micros = (f64::from(length) / f64::from(speed) * 1e6).round() as i64;
    Segment { x: from[0], z: from[1], yaw, speed, start: now, end: now + micros.max(1) }
}

/// How close it comes to its target before stopping to fight: 80% of its shortest skill range.
pub fn approach_m(skills: &[&SkillData]) -> f32 {
    let shortest = skills.iter().map(|s| s.range_m).fold(f32::INFINITY, f32::min);
    if shortest.is_finite() { (shortest * APPROACH_SHARE).max(MIN_APPROACH_M) } else { MIN_APPROACH_M }
}

/// The first of `skills` that is `ready` and reaches `distance_m`.
pub fn pick_skill<'a>(skills: &[&'a SkillData], distance_m: f32, ready: impl Fn(u16) -> bool) -> Option<&'a SkillData> {
    skills.iter().copied().find(|s| distance_m <= s.range_m && ready(s.wire))
}

/// The skills of `kind`, in order. Contentgen checked each exists.
pub fn skills_of(kind: &NpcKindData) -> Vec<&'static SkillData> {
    kind.skills.iter().filter_map(|&w| crate::combat::skill(w)).collect()
}

/// Whom to fight among (entity, threat): the most threat, ties to the earlier entry.
pub fn top_threat(rows: &[(u64, i64)]) -> Option<u64> {
    let mut best: Option<(u64, i64)> = None;
    for &(id, amount) in rows {
        if best.is_none_or(|(_, b)| amount > b) {
            best = Some((id, amount));
        }
    }
    best.map(|(id, _)| id)
}

/// How many of a loot line drop for a random `roll`: its last two decimal digits against `pct`,
/// and the rest for the count from `min` to `max`.
pub fn loot_count(line: &LootData, roll: u32) -> u32 {
    if roll % 100 >= line.pct {
        return 0;
    }
    line.min + (roll / 100) % (line.max - line.min + 1)
}

/// The first hitter owns the loot with at least `tag_pct`% of all the damage.
pub fn owns_loot(tagger_damage: i64, total_damage: i64) -> bool {
    total_damage > 0 && tagger_damage.max(0) * 100 >= i64::from(NPC.tag_pct) * total_damage
}

/// Threat for healing `healed` HP.
pub fn heal_threat(healed: i32) -> i64 {
    i64::from(healed.max(0)) * i64::from(NPC.heal_threat_pct) / 100
}

/// A point within `radius` of `home`, spread evenly over the disc, from two random numbers.
pub fn wander_point(home: [f32; 2], radius: f32, a: u32, b: u32) -> [f32; 2] {
    let r = radius * (a as f32 / u32::MAX as f32).sqrt();
    let angle = b as f32 / u32::MAX as f32 * std::f32::consts::TAU;
    [home[0] + r * angle.cos(), home[1] + r * angle.sin()]
}

/// The pause before the next stroll, from `wander_min_s` to `wander_max_s`, for a random `roll`.
pub fn wander_pause_micros(roll: u32) -> i64 {
    let span = i64::from(NPC.wander_max_s - NPC.wander_min_s) * MICROS_PER_S;
    i64::from(NPC.wander_min_s) * MICROS_PER_S + if span > 0 { i64::from(roll) % (span + 1) } else { 0 }
}

/// The world ticks between brain steps.
pub fn brain_ticks() -> u64 {
    u64::try_from(NPC.brain_ms / 100).unwrap_or(1).max(1)
}

pub fn heartbeat_micros() -> i64 {
    i64::from(NPC.heartbeat_ms) * MICROS_PER_MS
}

pub fn corpse_micros() -> i64 {
    i64::from(NPC.corpse_s) * MICROS_PER_S
}

pub fn free_micros() -> i64 {
    i64::from(NPC.free_s) * MICROS_PER_S
}

pub fn respawn_micros(kind: &NpcKindData) -> i64 {
    i64::from(kind.respawn_s) * MICROS_PER_S
}

/// A share of `speed`, in percent.
pub fn pct_of(speed: f32, pct: i32) -> f32 {
    speed * pct as f32 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::skill_by_id;

    const S: i64 = MICROS_PER_S;

    #[test]
    fn a_segment_runs_from_start_to_end() {
        // Facing +x is yaw -π/2 (`facing` is -sin, -cos).
        let seg = walk([0.0, 0.0], [10.0, 0.0], 5.0, 0.0, S, 0.0);
        assert!((seg.yaw + std::f32::consts::FRAC_PI_2).abs() < 1e-5, "{seg:?}");
        assert_eq!(seg.end, 3 * S);
        assert_eq!(seg.at(0), [0.0, 0.0]);
        let mid = seg.at(2 * S);
        assert!((mid[0] - 5.0).abs() < 1e-4 && mid[1].abs() < 1e-4, "{mid:?}");
        let end = seg.at(10 * S);
        assert!((end[0] - 10.0).abs() < 1e-4, "{end:?}");
        assert!(seg.moving(2 * S) && !seg.moving(3 * S));
    }

    #[test]
    fn walking_stops_short_or_stands() {
        let seg = walk([0.0, 0.0], [0.0, -10.0], 2.0, 4.0, 0, 1.0);
        assert!(seg.yaw.abs() < 1e-5, "facing -z is yaw 0: {seg:?}");
        let end = seg.end_point();
        assert!(end[0].abs() < 1e-4 && (end[1] + 6.0).abs() < 1e-4, "{end:?}");
        let near = walk([0.0, 0.0], [0.0, 3.0], 2.0, 4.0, 7, 0.0);
        assert_eq!((near.speed, near.start, near.end), (0.0, 7, 7));
        assert!((near.yaw.abs() - std::f32::consts::PI).abs() < 1e-5, "turned to face it: {near:?}");
        assert_eq!(yaw_to([1.0, 1.0], [1.0, 1.0], 0.25), 0.25);
    }

    #[test]
    fn skills_are_tried_in_order_by_range_and_readiness() {
        let (knock, slash) = (skill_by_id("shield_knock").unwrap(), skill_by_id("cutlass_slash").unwrap());
        let skills = [knock, slash];
        assert_eq!(approach_m(&skills), 3.2);
        assert_eq!(approach_m(&[skill_by_id("pinch").unwrap()]), 2.4);
        assert_eq!(approach_m(&[]), MIN_APPROACH_M);
        assert_eq!(pick_skill(&skills, 3.0, |_| true).map(|s| s.id), Some("shield_knock"));
        assert_eq!(pick_skill(&skills, 3.0, |w| w != knock.wire).map(|s| s.id), Some("cutlass_slash"));
        assert_eq!(pick_skill(&skills, 4.5, |_| true), None);
        assert_eq!(pick_skill(&skills, 3.0, |_| false), None);
    }

    #[test]
    fn the_most_threat_wins_and_ties_go_to_the_first() {
        assert_eq!(top_threat(&[]), None);
        assert_eq!(top_threat(&[(5, 10), (6, 30), (7, 30), (8, 2)]), Some(6));
        assert_eq!(top_threat(&[(5, 0)]), Some(5));
    }

    #[test]
    fn loot_rolls_chance_then_count() {
        let line = LootData { item: 1, min: 2, max: 4, pct: 40 };
        assert_eq!(loot_count(&line, 40), 0);
        assert_eq!(loot_count(&line, 99), 0);
        assert_eq!(loot_count(&line, 39), 2);
        assert_eq!(loot_count(&line, 139), 3);
        assert_eq!(loot_count(&line, 239), 4);
        assert_eq!(loot_count(&line, 339), 2);
        assert_eq!(loot_count(&line, u32::MAX), 0);
        let sure = LootData { item: 1, min: 1, max: 1, pct: 100 };
        assert!((0..1000).all(|r| loot_count(&sure, r * 7919) == 1));
    }

    #[test]
    fn the_tagger_needs_enough_of_the_damage() {
        assert!(owns_loot(50, 100));
        assert!(!owns_loot(49, 100));
        assert!(owns_loot(1, 1));
        assert!(!owns_loot(0, 0));
        assert_eq!(heal_threat(100), 50);
        assert_eq!(heal_threat(-5), 0);
    }

    #[test]
    fn strolls_stay_near_home_and_pauses_in_their_range() {
        for (a, b) in [(0, 0), (u32::MAX, u32::MAX), (12345, 678_901), (u32::MAX / 2, 3)] {
            let p = wander_point([100.0, -50.0], 16.0, a, b);
            assert!(distance2d(p, [100.0, -50.0]) <= 16.0 + 1e-3, "{p:?}");
        }
        for roll in [0, 1, 999_999, u32::MAX] {
            let m = wander_pause_micros(roll);
            assert!((i64::from(NPC.wander_min_s) * S..=i64::from(NPC.wander_max_s) * S).contains(&m), "{m}");
        }
        assert_eq!(brain_ticks(), 2);
    }

    /// About ten kinds, levels spread over the cap, every skill a Wildcraft one and every loot item real.
    #[test]
    fn the_kinds_are_complete() {
        assert_eq!(NPC_KINDS.len(), 10);
        let wild = crate::combat::SKILLSETS.iter().find(|s| s.npc).unwrap();
        for k in &NPC_KINDS {
            assert!(!k.skills.is_empty(), "{}", k.id);
            for s in skills_of(k) {
                assert_eq!(s.skillset, wild.wire, "{}: {}", k.id, s.id);
            }
            assert_eq!(skills_of(k).len(), k.skills.len());
            for l in k.loot {
                assert!(crate::content::item(l.item).is_some(), "{}", k.id);
            }
        }
        assert_eq!(NPC_KINDS.iter().map(|k| k.level).min(), Some(1));
        assert_eq!(NPC_KINDS.iter().map(|k| k.level).max(), Some(crate::combat::level_max()));
        assert!(NPC_KINDS.iter().any(|k| k.aggro_m == 0.0) && NPC_KINDS.iter().any(|k| k.aggro_m > 0.0));
    }
}
