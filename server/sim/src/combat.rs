//! Combat (docs/design/combat.md): HP and mana from timestamps, the checks before a skill is used,
//! interrupting casts, projectile flight times, the wait to respawn, statuses and crowd control
//! with diminishing returns, and levels and experience. Nothing is ticked. The client ports this
//! to refuse what it can see is wrong, and the server decides.

pub use crate::content::{
    COMBAT, CONTROL, Control, EffectData, EffectKind, PROGRESS, SHRINES, SKILLS, SKILLSETS, STATUSES, ShrineData, SkillData, SkillTarget, SkillsetData,
    StatusData, shrine, shrine_by_id, skill, skill_by_id, status, status_by_id,
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
    Controlled = 11,
    Unlearned = 12,
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
            Refusal::Controlled => "you can't act right now",
            Refusal::Unlearned => "you haven't learned that skill",
        }
    }
}

/// The caster as a skill check sees it, at the time of the check.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caster {
    pub at: [f32; 3],
    pub yaw: f32,
    pub alive: bool,
    /// Stunned or knocked down.
    pub controlled: bool,
    /// Its skillset is picked and the character's level is high enough.
    pub knows: bool,
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
    if c.controlled {
        return Err(Refusal::Controlled);
    }
    if !c.knows {
        return Err(Refusal::Unlearned);
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

/// The highest level.
pub fn level_max() -> u8 {
    u8::try_from(PROGRESS.xp_to_next.len() + 1).unwrap_or(u8::MAX)
}

/// The most HP at `level`.
pub fn hp_max_at(level: u8) -> i32 {
    COMBAT.hp_max + PROGRESS.hp_per_level * i32::from(level.clamp(1, level_max()) - 1)
}

/// The most mana at `level`.
pub fn mp_max_at(level: u8) -> i32 {
    COMBAT.mp_max + PROGRESS.mp_per_level * i32::from(level.clamp(1, level_max()) - 1)
}

/// The experience from `level` to the next; 0 at the cap.
pub fn xp_to_next(level: u8) -> u32 {
    PROGRESS.xp_to_next.get(usize::from(level.max(1)) - 1).copied().unwrap_or(0)
}

/// How many skillsets a character at `level` may have picked.
pub fn skillset_slots(level: u8) -> usize {
    PROGRESS.skillset_at.iter().filter(|&&l| l <= level).count()
}

/// A character at `level` with skillsets `picks` can use `s`.
pub fn knows(s: &SkillData, level: u8, picks: &[u8]) -> bool {
    level >= s.level && picks.contains(&s.skillset)
}

/// Experience for a character at `killer` killing a foe at `victim` that isn't a character.
pub fn kill_xp(victim: u8, killer: u8) -> u32 {
    let base = u64::from(PROGRESS.kill_xp) + u64::from(PROGRESS.kill_xp_per_level) * u64::from(victim.max(1) - 1);
    let gap = u64::from(killer.saturating_sub(victim));
    let pct = 100u64.saturating_sub(gap * u64::from(PROGRESS.kill_xp_gap_pct));
    u32::try_from(base * pct / 100).unwrap_or(u32::MAX)
}

/// Level and experience into it after gaining `gained` at `level` with `xp`. Experience stops at
/// the cap.
pub fn gain_xp(level: u8, xp: u32, gained: u32) -> (u8, u32) {
    let (mut level, mut xp) = (level.clamp(1, level_max()), u64::from(xp) + u64::from(gained));
    while level < level_max() && xp >= u64::from(xp_to_next(level)) {
        xp -= u64::from(xp_to_next(level));
        level += 1;
    }
    if level == level_max() {
        xp = 0;
    }
    (level, u32::try_from(xp).unwrap_or(u32::MAX))
}

/// Experience into the level after a death to anything but another character: down by a share of
/// what the level needs, never below the level's start.
pub fn xp_after_death(level: u8, xp: u32) -> u32 {
    let loss = u64::from(xp_to_next(level)) * u64::from(PROGRESS.death_xp_pct) / 100;
    xp.saturating_sub(u32::try_from(loss).unwrap_or(u32::MAX))
}

/// A status on a fighter, as the rules need it: from `start` until `until`, µs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub status: u8,
    pub start: i64,
    pub until: i64,
}

impl Slot {
    pub fn active(&self, now: i64) -> bool {
        self.start <= now && now < self.until
    }

    pub fn data(&self) -> Option<&'static StatusData> {
        status(self.status)
    }
}

/// `slots` has `status` active at `now`.
pub fn has(slots: &[Slot], status: u8, now: i64) -> bool {
    slots.iter().any(|s| s.status == status && s.active(now))
}

/// Stunned or knocked down at `now`: no moving or skills.
pub fn controlled(slots: &[Slot], now: i64) -> bool {
    slots.iter().any(|s| s.active(now) && s.data().is_some_and(|d| matches!(d.control, Control::Stun | Control::Knockdown)))
}

/// The speed limit's factor for a move made over `from` to `to`: 0 under a stun or knockdown and a
/// snare's share under a snare, counting only controls that already held before `from` and still
/// held at `to`. One that lands or ends mid-move is the client's to catch up with, so it doesn't
/// count; the lowest factor wins.
pub fn move_scale(slots: &[Slot], from: i64, to: i64) -> f32 {
    slots
        .iter()
        .filter(|s| s.start < from && s.until >= to)
        .filter_map(|s| s.data())
        .map(|d| match d.control {
            Control::Stun | Control::Knockdown => 0.0,
            Control::Snare => d.speed_pct as f32 / 100.0,
            Control::None => 1.0,
        })
        .fold(1.0, f32::min)
}

/// The sum of the active statuses' `dealt_pct` (or `taken_pct` with `taken`), in percent.
pub fn modifier(slots: &[Slot], now: i64, taken: bool) -> i32 {
    slots.iter().filter(|s| s.active(now)).filter_map(|s| s.data()).map(|d| if taken { d.taken_pct } else { d.dealt_pct }).sum()
}

/// Direct damage `amount` after the dealer's and the target's modifiers, each at least -100%,
/// rounded down.
pub fn scaled_damage(amount: i32, dealt_pct: i32, taken_pct: i32) -> i32 {
    let f = |pct: i32| i64::from((100 + pct).max(0));
    i32::try_from(i64::from(amount.max(0)) * f(dealt_pct) * f(taken_pct) / 10_000).unwrap_or(i32::MAX)
}

/// The diminishing-returns group a control falls in: stuns and knockdowns share one.
pub fn control_group(c: Control) -> Option<u8> {
    match c {
        Control::Stun | Control::Knockdown => Some(0),
        Control::Snare => Some(1),
        Control::None => None,
    }
}

/// How many controls of a group have landed on a fighter, and when its window runs out, µs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resist {
    pub group: u8,
    pub count: u8,
    pub until: i64,
}

/// The share of its duration, in percent, a control in `group` lands for at `now`; `None` when
/// the target is immune.
pub fn diminished(resists: &[Resist], group: u8, now: i64) -> Option<u32> {
    let landed = resists.iter().find(|r| r.group == group && now < r.until).map_or(0, |r| usize::from(r.count));
    CONTROL.steps_pct.get(landed).copied()
}

/// `resists` after a control in `group` landed at `now` for `lasts` µs: one more in a row, and the
/// window running `window_s` past its end. Run-out windows are dropped.
pub fn resisted(resists: &[Resist], group: u8, now: i64, lasts: i64) -> Vec<Resist> {
    let count = resists.iter().find(|r| r.group == group && now < r.until).map_or(0, |r| r.count);
    let mut out: Vec<Resist> = resists.iter().filter(|r| r.group != group && now < r.until).copied().collect();
    let until = now.saturating_add(lasts).saturating_add(i64::from(CONTROL.window_s) * MICROS_PER_S);
    out.push(Resist { group, count: count.saturating_add(1), until });
    out
}

/// A control lasting `ms` landing at `pct` percent: how long it lasts, µs.
pub fn control_micros(ms: u32, pct: u32) -> i64 {
    i64::from(ms) * MICROS_PER_MS * i64::from(pct) / 100
}

/// When a status with `tick_ms` that began at `start` next ticks after `last` (`start` for none
/// yet), if before `until`: ticks fall every `tick_ms` from `start`, and one exactly at `until`
/// counts.
pub fn next_tick(start: i64, until: i64, tick_ms: u32, last: i64) -> Option<i64> {
    if tick_ms == 0 {
        return None;
    }
    let every = i64::from(tick_ms) * MICROS_PER_MS;
    let n = (last - start).max(0) / every + 1;
    let due = start.saturating_add(n.saturating_mul(every));
    (due <= until).then_some(due)
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
        let ok = Caster { at: [0.0; 3], yaw: 0.0, alive: true, controlled: false, knows: true, casting: false, mp: 800, ready_at: 0 };
        let them = Target { at: [0.0, 0.0, -10.0], alive: true, is_caster: false };
        assert_eq!(check(bolt, &ok, &them, 0), Ok(()));
        let all_wrong = Caster { alive: false, controlled: true, knows: false, casting: true, mp: 0, ready_at: 10, ..ok };
        assert_eq!(check(bolt, &all_wrong, &them, 0), Err(Refusal::Dead));
        assert_eq!(check(bolt, &Caster { alive: true, ..all_wrong }, &them, 0), Err(Refusal::Controlled));
        let all_wrong = Caster { controlled: false, ..all_wrong };
        assert_eq!(check(bolt, &Caster { alive: true, ..all_wrong }, &them, 0), Err(Refusal::Unlearned));
        let all_wrong = Caster { knows: true, ..all_wrong };
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
        assert_eq!(SKILLS.iter().filter(|s| !SKILLSETS[usize::from(s.skillset)].npc).count(), 24);
        for set in SKILLSETS.iter().filter(|s| !s.npc) {
            assert_eq!(SKILLS.iter().filter(|s| s.skillset == set.wire).count(), 8, "{}", set.name);
            assert!(SKILLS.iter().any(|s| s.skillset == set.wire && s.level == 1), "{} has a skill at level 1", set.name);
        }
        assert!(SKILLS.iter().all(|s| s.wire >= 100));
        assert_eq!(SKILLSETS.iter().map(|s| (s.name, s.npc)).collect::<Vec<_>>(), [("Emberlore", false), ("Ironvow", false), ("Wellspring", false), ("Wildcraft", true)]);
        assert!(SKILLS.iter().filter(|s| SKILLSETS[usize::from(s.skillset)].npc).all(|s| s.cost_mp == 0 && s.target == SkillTarget::Enemy));
        assert_eq!(skill(100).map(|s| s.id), Some("cinder_dart"));
        assert!(skill(99).is_none());
        assert_eq!(shrine_by_id("gullhaven_wayshrine").map(|s| s.wire), Some(0));
    }

    #[test]
    fn levels_grow_the_maximums_and_open_skillsets() {
        assert_eq!(level_max(), 15);
        assert_eq!(hp_max_at(1), COMBAT.hp_max);
        assert_eq!(hp_max_at(3), COMBAT.hp_max + 2 * PROGRESS.hp_per_level);
        assert_eq!(mp_max_at(99), mp_max_at(15));
        assert_eq!((skillset_slots(1), skillset_slots(4), skillset_slots(5), skillset_slots(15)), (1, 1, 2, 3));
        let lance = sk("searing_lance");
        assert!(!knows(lance, 11, &[0, 1, 2]));
        assert!(!knows(lance, 12, &[1, 2]));
        assert!(knows(lance, 12, &[0]));
    }

    #[test]
    fn experience_levels_up_stops_at_the_cap_and_a_death_costs_some() {
        assert_eq!(gain_xp(1, 0, 99), (1, 99));
        assert_eq!(gain_xp(1, 90, 20), (2, 10));
        assert_eq!(gain_xp(1, 0, 100 + 150 + 5), (3, 5));
        assert_eq!(gain_xp(14, 0, u32::MAX), (15, 0));
        assert_eq!(gain_xp(15, 0, 500), (15, 0));
        assert_eq!(xp_to_next(15), 0);
        assert_eq!(kill_xp(1, 1), PROGRESS.kill_xp);
        assert_eq!(kill_xp(3, 3), PROGRESS.kill_xp + 2 * PROGRESS.kill_xp_per_level);
        assert_eq!(kill_xp(1, 2), PROGRESS.kill_xp * (100 - PROGRESS.kill_xp_gap_pct) / 100);
        assert_eq!(kill_xp(1, 15), 0);
        assert_eq!(kill_xp(9, 1), kill_xp(9, 9), "no bonus for killing above your level");
        let loss = xp_to_next(2) * PROGRESS.death_xp_pct / 100;
        assert_eq!(xp_after_death(2, 50), 50 - loss);
        assert_eq!(xp_after_death(2, 1), 0, "never below the level's start");
        assert_eq!(xp_after_death(15, 0), 0);
    }

    fn st(id: &str) -> u8 {
        status_by_id(id).unwrap().wire
    }

    #[test]
    fn statuses_are_active_from_start_to_until_and_controls_stop_or_slow() {
        let stun = Slot { status: st("stunned"), start: 10 * S, until: 12 * S };
        let slow = Slot { status: st("slowed"), start: 0, until: 20 * S };
        assert!(!stun.active(10 * S - 1) && stun.active(10 * S) && !stun.active(12 * S));
        assert!(has(&[stun, slow], st("slowed"), S));
        assert!(!controlled(&[slow], S));
        assert!(controlled(&[stun, slow], 11 * S));
        assert!(controlled(&[Slot { status: st("felled"), start: 0, until: S }], 0));
        // Moves: a control counts only if it held before the move began and still does at its end.
        let snare = status_by_id("slowed").unwrap().speed_pct as f32 / 100.0;
        assert_eq!(move_scale(&[slow], S, 2 * S), snare);
        assert_eq!(move_scale(&[slow, stun], 11 * S, 12 * S), 0.0);
        assert_eq!(move_scale(&[stun], 10 * S, 11 * S), 1.0, "landed as the move began");
        assert_eq!(move_scale(&[stun], 11 * S, 13 * S), 1.0, "ended mid-move");
        assert_eq!(move_scale(&[Slot { status: st("fervor"), start: 0, until: 100 * S }], S, 2 * S), 1.0);
    }

    #[test]
    fn damage_modifiers_add_up_and_never_go_negative() {
        let fervor = Slot { status: st("fervor"), start: 0, until: 10 * S };
        let shaken = Slot { status: st("shaken"), start: 0, until: 10 * S };
        let bulwark = Slot { status: st("bulwark"), start: 0, until: 10 * S };
        let d = |id: &str| status_by_id(id).unwrap();
        assert_eq!(modifier(&[fervor, shaken], S, false), d("fervor").dealt_pct + d("shaken").dealt_pct);
        assert_eq!(modifier(&[fervor, shaken], 11 * S, false), 0, "expired");
        assert_eq!(modifier(&[bulwark], S, true), d("bulwark").taken_pct);
        assert_eq!(scaled_damage(100, 20, -30), 84);
        assert_eq!(scaled_damage(100, -200, 0), 0);
        assert_eq!(scaled_damage(99, 0, 15), 113);
    }

    #[test]
    fn controls_diminish_in_a_row_then_immunity_until_the_window_ends() {
        let mut r: Vec<Resist> = Vec::new();
        let mut now = 0;
        let mut got = Vec::new();
        while let Some(pct) = diminished(&r, 0, now) {
            got.push(pct);
            let lasts = control_micros(2000, pct);
            r = resisted(&r, 0, now, lasts);
            now += lasts;
        }
        assert_eq!(got, CONTROL.steps_pct);
        // Snares are their own group.
        assert_eq!(diminished(&r, 1, now), Some(100));
        // The window runs from the end of the last control that landed.
        let until = r[0].until;
        assert_eq!(until, now + i64::from(CONTROL.window_s) * S);
        assert_eq!(diminished(&r, 0, until - 1), None);
        assert_eq!(diminished(&r, 0, until), Some(100));
        assert_eq!(resisted(&r, 1, until, S).len(), 1, "run-out windows are dropped");
        assert_eq!(control_micros(2000, 50), S);
    }

    #[test]
    fn ticks_fall_every_period_up_to_and_including_the_end() {
        assert_eq!(next_tick(0, 3 * S, 1000, 0), Some(S));
        assert_eq!(next_tick(0, 3 * S, 1000, S), Some(2 * S));
        assert_eq!(next_tick(0, 3 * S, 1000, 2 * S + 5), Some(3 * S));
        assert_eq!(next_tick(0, 3 * S, 1000, 3 * S), None);
        assert_eq!(next_tick(0, 3 * S, 0, 0), None);
    }
}
