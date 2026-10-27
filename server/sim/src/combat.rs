//! Combat (docs/design/combat.md): HP and mana from timestamps, the checks before a skill is used,
//! interrupting casts, projectile flight times and the wait to respawn. Nothing is ticked. The
//! client ports this to refuse what it can see is wrong, and the server decides.

pub use crate::content::{
    COMBAT, EffectData, EffectKind, SHRINES, SKILLS, SKILLSETS, ShrineData, SkillData, SkillTarget, SkillsetData, shrine, shrine_by_id, skill,
    skill_by_id,
};

const MICROS_PER_S: i64 = 1_000_000;
const MICROS_PER_MS: i64 = 1_000;
/// Closer than this, a target always counts as in front: there's no telling which way it lies.
pub const FRONT_EXEMPT_M: f32 = 1.0;

/// HP or mana at `now`: `value` as it was at `at`, plus regeneration at `rate_in` per second until
/// `combat_until` and at `rate_out` after, never above `max`. Each part rounds down. A value
/// already at or above `max` stays as it is.
pub fn settle(value: i32, max: i32, at: i64, combat_until: i64, now: i64, rate_in: i32, rate_out: i32) -> i32 {
    if value >= max {
        return value;
    }
    let now = now.max(at);
    let split = combat_until.clamp(at, now);
    let part = |rate: i32, micros: i64| i128::from(rate.max(0)) * i128::from(micros) / i128::from(MICROS_PER_S);
    let gained = part(rate_in, split - at) + part(rate_out, now - split);
    i32::try_from((i128::from(value) + gained).min(i128::from(max))).unwrap_or(max)
}

/// HP at `now` by the content's rates; the dead don't regenerate.
pub fn hp_now(hp: i32, max: i32, at: i64, combat_until: i64, now: i64, dead: bool) -> i32 {
    if dead { hp } else { settle(hp, max, at, combat_until, now, COMBAT.hp_regen_in, COMBAT.hp_regen_out) }
}

/// Mana at `now` by the content's rates; the dead don't regenerate.
pub fn mp_now(mp: i32, max: i32, at: i64, combat_until: i64, now: i64, dead: bool) -> i32 {
    if dead { mp } else { settle(mp, max, at, combat_until, now, COMBAT.mp_regen_in, COMBAT.mp_regen_out) }
}

/// When a fight that touched a character at `now` lets it out of combat.
pub fn combat_until(now: i64) -> i64 {
    now.saturating_add(i64::from(COMBAT.combat_s) * MICROS_PER_S)
}

/// The global cooldown, µs.
pub fn gcd_micros() -> i64 {
    i64::from(COMBAT.gcd_ms) * MICROS_PER_MS
}

pub fn cast_micros(s: &SkillData) -> i64 {
    i64::from(s.cast_ms) * MICROS_PER_MS
}

pub fn cooldown_micros(s: &SkillData) -> i64 {
    i64::from(s.cooldown_ms) * MICROS_PER_MS
}

/// How long a projectile at `speed_mps` takes to cover `distance_m`; 0 for a skill that lands at
/// once.
pub fn impact_micros(distance_m: f32, speed_mps: f32) -> i64 {
    if speed_mps <= 0.0 {
        return 0;
    }
    (f64::from(distance_m.max(0.0)) / f64::from(speed_mps) * 1e6).round() as i64
}

/// The number of deaths that count toward the wait after a death at `now`, given the count and
/// when the last one was (`None` for never).
pub fn deaths_after(deaths: u32, last_death_at: Option<i64>, now: i64) -> u32 {
    let stacking = i64::from(COMBAT.death_stack_s) * MICROS_PER_S;
    match last_death_at {
        Some(last) if now.saturating_sub(last) < stacking => deaths.saturating_add(1),
        _ => 1,
    }
}

/// How long after dying a character with `deaths` counted deaths must wait to respawn.
pub fn respawn_wait_micros(deaths: u32) -> i64 {
    let extra = i64::from(deaths.saturating_sub(1)) * i64::from(COMBAT.respawn_step_s);
    (i64::from(COMBAT.respawn_first_s) + extra).min(i64::from(COMBAT.respawn_max_s)) * MICROS_PER_S
}

/// `pct` percent of `max`, rounded down: what a respawn restores.
pub fn respawn_share(max: i32) -> i32 {
    i32::try_from(i64::from(max) * i64::from(COMBAT.respawn_pct) / 100).unwrap_or(max)
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)
}

pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    distance2(a, b).sqrt()
}

/// `to` is within `range_m` of `from`, in 3D.
pub fn in_range(from: [f32; 3], to: [f32; 3], range_m: f32) -> bool {
    distance2(from, to) <= range_m * range_m
}

/// Someone at `from` facing `yaw` has `to` in front: within 90° either side, horizontally. Under
/// `FRONT_EXEMPT_M` away it always counts.
pub fn in_front(yaw: f32, from: [f32; 3], to: [f32; 3]) -> bool {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    if dx * dx + dz * dz < FRONT_EXEMPT_M * FRONT_EXEMPT_M {
        return true;
    }
    let [fx, fz] = crate::ship::facing(yaw);
    fx * dx + fz * dz >= 0.0
}

/// A caster that began at `start` and is now at `at` has moved far enough to interrupt its cast.
pub fn moved_off(start: [f32; 3], at: [f32; 3]) -> bool {
    distance2(start, at) > COMBAT.cast_move_m * COMBAT.cast_move_m
}

/// Why a skill can't be used. The discriminant is the code in the client's vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Dead = 1,
    Casting = 2,
    NotReady = 3,
    NoMana = 4,
    Yourself = 5,
    TargetDead = 6,
    OutOfRange = 7,
    NotInFront = 8,
    NoSight = 9,
    NoTarget = 10,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Refusal::Dead => "you are dead",
            Refusal::Casting => "already casting",
            Refusal::NotReady => "not ready yet",
            Refusal::NoMana => "not enough mana",
            Refusal::Yourself => "can't target yourself",
            Refusal::TargetDead => "the target is dead",
            Refusal::OutOfRange => "out of range",
            Refusal::NotInFront => "the target isn't in front of you",
            Refusal::NoSight => "no line of sight",
            Refusal::NoTarget => "no target",
        }
    }
}

/// The caster as a skill check sees it, at the time of the check.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caster {
    pub at: [f32; 3],
    pub yaw: f32,
    pub alive: bool,
    pub casting: bool,
    /// Mana now.
    pub mp: i32,
    /// When the later of the global cooldown and this skill's cooldown ends.
    pub ready_at: i64,
}

/// The target, already resolved: for an ally skill on an enemy, or a caster skill, the caster.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub at: [f32; 3],
    pub alive: bool,
    pub is_caster: bool,
}

/// Everything about using `skill` at `now` but line of sight, in the order players are told.
pub fn check(skill: &SkillData, c: &Caster, t: &Target, now: i64) -> Result<(), Refusal> {
    if !c.alive {
        return Err(Refusal::Dead);
    }
    if c.casting {
        return Err(Refusal::Casting);
    }
    if now < c.ready_at {
        return Err(Refusal::NotReady);
    }
    if c.mp < skill.cost_mp {
        return Err(Refusal::NoMana);
    }
    if skill.target == SkillTarget::Enemy && t.is_caster {
        return Err(Refusal::Yourself);
    }
    if !t.alive {
        return Err(Refusal::TargetDead);
    }
    if t.is_caster {
        return Ok(());
    }
    if !in_range(c.at, t.at, skill.range_m) {
        return Err(Refusal::OutOfRange);
    }
    if skill.facing && !in_front(c.yaw, c.at, t.at) {
        return Err(Refusal::NotInFront);
    }
    Ok(())
}

/// A cast finishing: the target must still be alive and within range plus the leeway. Facing
/// isn't checked again.
pub fn check_finish(skill: &SkillData, from: [f32; 3], t: &Target) -> Result<(), Refusal> {
    if !t.alive {
        return Err(Refusal::TargetDead);
    }
    if !t.is_caster && !in_range(from, t.at, skill.range_m + COMBAT.cast_leeway_m) {
        return Err(Refusal::OutOfRange);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = MICROS_PER_S;

    fn sk(id: &str) -> &'static SkillData {
        skill_by_id(id).unwrap()
    }

    #[test]
    fn settle_regenerates_in_and_out_of_combat_and_caps() {
        // 10 s in combat at 4/s, then 5 s out at 20/s.
        assert_eq!(settle(100, 1000, 0, 10 * S, 15 * S, 4, 20), 100 + 40 + 100);
        // Out of combat throughout.
        assert_eq!(settle(100, 1000, 20 * S, 10 * S, 25 * S, 4, 20), 200);
        // In combat throughout.
        assert_eq!(settle(100, 1000, 0, 100 * S, 5 * S, 4, 20), 120);
        // Capped, and never lowered.
        assert_eq!(settle(990, 1000, 0, 0, 100 * S, 4, 20), 1000);
        assert_eq!(settle(1200, 1000, 0, 0, 100 * S, 4, 20), 1200);
        // Before `at`, and fractions round down.
        assert_eq!(settle(100, 1000, 10 * S, 0, 5 * S, 4, 20), 100);
        assert_eq!(settle(100, 1000, 0, 0, S / 2 + 1, 4, 3), 101);
        // Huge spans don't overflow.
        assert_eq!(settle(0, 1000, 0, 0, i64::MAX, 4, 20), 1000);
    }

    #[test]
    fn the_dead_dont_regenerate() {
        assert_eq!(hp_now(0, 1000, 0, 0, 100 * S, true), 0);
        assert_eq!(hp_now(0, 1000, 0, 0, 10 * S, false), 10 * COMBAT.hp_regen_out);
        assert_eq!(mp_now(5, 800, 0, 0, 10 * S, true), 5);
    }

    #[test]
    fn deaths_stack_within_the_window_and_the_wait_grows_to_the_cap() {
        let stack = i64::from(COMBAT.death_stack_s) * S;
        assert_eq!(deaths_after(0, None, 0), 1);
        assert_eq!(deaths_after(1, Some(0), stack - 1), 2);
        assert_eq!(deaths_after(4, Some(0), stack), 1);
        assert_eq!(respawn_wait_micros(1), i64::from(COMBAT.respawn_first_s) * S);
        assert_eq!(respawn_wait_micros(2), i64::from(COMBAT.respawn_first_s + COMBAT.respawn_step_s) * S);
        assert_eq!(respawn_wait_micros(0), respawn_wait_micros(1));
        assert_eq!(respawn_wait_micros(u32::MAX), i64::from(COMBAT.respawn_max_s) * S);
        assert_eq!(respawn_share(1000), 10 * COMBAT.respawn_pct);
    }

    #[test]
    fn range_front_and_moving() {
        assert!(in_range([0.0; 3], [3.0, 0.0, 4.0], 5.0));
        assert!(!in_range([0.0; 3], [3.0, 1.0, 4.0], 5.0));
        // Yaw 0 faces -z.
        assert!(in_front(0.0, [0.0; 3], [0.0, 0.0, -10.0]));
        assert!(in_front(0.0, [0.0; 3], [10.0, 0.0, 0.0]), "exactly abeam counts");
        assert!(!in_front(0.0, [0.0; 3], [0.0, 0.0, 10.0]));
        assert!(in_front(0.0, [0.0; 3], [0.0, 5.0, 0.5]), "close by always counts");
        assert!(in_front(std::f32::consts::PI, [0.0; 3], [0.0, 0.0, 10.0]));
        assert!(!moved_off([0.0; 3], [COMBAT.cast_move_m * 0.9, 0.0, 0.0]));
        assert!(moved_off([0.0; 3], [0.0, COMBAT.cast_move_m * 1.1, 0.0]));
        assert_eq!(impact_micros(30.0, 30.0), S);
        assert_eq!(impact_micros(30.0, 0.0), 0);
    }

    #[test]
    fn check_refuses_in_order() {
        let bolt = sk("cinder_dart");
        let ok = Caster { at: [0.0; 3], yaw: 0.0, alive: true, casting: false, mp: 800, ready_at: 0 };
        let them = Target { at: [0.0, 0.0, -10.0], alive: true, is_caster: false };
        assert_eq!(check(bolt, &ok, &them, 0), Ok(()));
        let all_wrong = Caster { alive: false, casting: true, mp: 0, ready_at: 10, ..ok };
        assert_eq!(check(bolt, &all_wrong, &them, 0), Err(Refusal::Dead));
        assert_eq!(check(bolt, &Caster { alive: true, ..all_wrong }, &them, 0), Err(Refusal::Casting));
        assert_eq!(check(bolt, &Caster { alive: true, casting: false, ..all_wrong }, &them, 0), Err(Refusal::NotReady));
        assert_eq!(check(bolt, &Caster { alive: true, casting: false, ready_at: 0, ..all_wrong }, &them, 0), Err(Refusal::NoMana));
        assert_eq!(check(bolt, &ok, &Target { is_caster: true, ..them }, 0), Err(Refusal::Yourself));
        assert_eq!(check(bolt, &ok, &Target { alive: false, ..them }, 0), Err(Refusal::TargetDead));
        assert_eq!(check(bolt, &ok, &Target { at: [0.0, 0.0, -30.0], ..them }, 0), Err(Refusal::OutOfRange));
        assert_eq!(check(bolt, &ok, &Target { at: [0.0, 0.0, 10.0], ..them }, 0), Err(Refusal::NotInFront));
        // Heals don't need facing, and on the caster need no range.
        let heal = sk("mending_word");
        assert_eq!(check(heal, &ok, &Target { at: [0.0, 0.0, 10.0], ..them }, 0), Ok(()));
        assert_eq!(check(heal, &ok, &Target { at: [0.0; 3], alive: true, is_caster: true }, 0), Ok(()));
    }

    #[test]
    fn a_finishing_cast_allows_the_leeway() {
        let bolt = sk("cinder_dart");
        let at = |d: f32| Target { at: [0.0, 0.0, -d], alive: true, is_caster: false };
        assert_eq!(check_finish(bolt, [0.0; 3], &at(bolt.range_m + COMBAT.cast_leeway_m - 0.1)), Ok(()));
        assert_eq!(check_finish(bolt, [0.0; 3], &at(bolt.range_m + COMBAT.cast_leeway_m + 0.1)), Err(Refusal::OutOfRange));
        assert_eq!(check_finish(bolt, [0.0; 3], &Target { alive: false, ..at(1.0) }), Err(Refusal::TargetDead));
    }

    #[test]
    fn content_skills_are_usable_as_specified() {
        assert_eq!(SKILLS.len(), 9);
        assert!(SKILLS.iter().all(|s| s.wire >= 100));
        assert_eq!(SKILLSETS.iter().map(|s| s.name).collect::<Vec<_>>(), ["Emberlore", "Ironvow", "Wellspring"]);
        assert_eq!(skill(100).map(|s| s.id), Some("cinder_dart"));
        assert!(skill(99).is_none());
        assert_eq!(shrine_by_id("gullhaven_wayshrine").map(|s| s.wire), Some(0));
    }
}
