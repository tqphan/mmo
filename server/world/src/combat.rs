//! Combat (docs/design/combat.md): vitals, targeting, skills with casts, cooldowns and impacts,
//! statuses and crowd control, death and respawning at Wayshrines, levels and skillsets. HP and
//! mana come from timestamps by the rules in `sim::combat`; nothing is ticked but the damage and
//! healing statuses deal over time, which `world_tick` finds through `periodic`'s due-time index.

use spacetimedb::{Identity, ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext};
use sim::combat::{COMBAT, Caster, Control, EffectKind, Refusal, SkillData, SkillTarget, Slot, Target, shrine};
use sim::movement::MoveMode;

use crate::session::{Presence, entity, entity_kind, presence, require_admin, transform};
use crate::terrain::Terrain;
use crate::time::now;

/// Sight runs from the attacker's eyes to the target's chest, both above the feet.
pub const EYE_M: f32 = 1.6;
pub const CHEST_M: f32 = 1.0;
/// The dead get up this far in front of a Wayshrine, clear of its stone.
pub const SHRINE_STEP_M: f32 = 3.0;

/// One hit, heal or miss, broadcast to the cell it happened in.
#[spacetimedb::table(accessor = combat_event, public, event)]
#[derive(Debug)]
pub struct CombatEvent {
    pub cell: u64,
    pub source: u64,
    /// The entity hit, or 0 for a miss.
    pub target: u64,
    /// A skill's wire (100 and up), or one of `skill`'s.
    pub skill_id: u32,
    /// HP for a hit or heal; a status's duration in ms; experience gained; the new level.
    pub amount: i32,
    pub flags: u16,
    /// The status put on the target, with `STATUS` or `IMMUNE`.
    pub status: u8,
}

/// `combat_event.skill_id` for what isn't a skill from content.
pub mod skill {
    /// The dev stand-in swing (`dev_swing`).
    #[cfg(feature = "dev")]
    pub const SWING: u32 = 1;
    pub const CANNON: u32 = 2;
}

/// `combat_event.flags` bits.
pub mod flags {
    pub const HIT: u16 = 1;
    pub const HEAL: u16 = 2;
    pub const KILL: u16 = 4;
    /// `status` landed on the target for `amount` ms.
    pub const STATUS: u16 = 8;
    /// The target was immune to `status`, a control (diminishing returns).
    pub const IMMUNE: u16 = 16;
    /// The source gained `amount` experience.
    pub const XP: u16 = 32;
    /// The source reached level `amount`.
    pub const LEVEL: u16 = 64;
    /// A status's damage or healing over time.
    pub const TICK: u16 = 128;
}

/// A status on a fighter, from `start` until `until`. `amount` is what it deals or heals per tick.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct StatusSlot {
    pub status: u8,
    pub source: u64,
    pub start: Timestamp,
    pub until: Timestamp,
    pub amount: i32,
}

/// How many controls of a diminishing-returns group have landed in a row, and when the group's
/// window runs out.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct Resist {
    pub group: u8,
    pub count: u8,
    pub until: Timestamp,
}

/// Warm and public (Near tier): what anyone nearby may know about a fighter's health, target and
/// cast. HP and mana are as of `at`; regeneration since then comes from `sim::combat`.
#[spacetimedb::table(accessor = vitals, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Vitals {
    #[primary_key]
    pub entity_id: u64,
    #[index(btree)]
    pub cell: u64,
    pub hp: i32,
    pub hp_max: i32,
    pub mp: i32,
    pub mp_max: i32,
    pub at: Timestamp,
    pub combat_until: Timestamp,
    /// The entity targeted, or 0.
    pub target: u64,
    /// The skill being cast, or 0; its target and when it began and ends.
    pub cast_skill: u16,
    pub cast_target: u64,
    pub cast_start: Timestamp,
    pub cast_end: Timestamp,
    pub dead_at: Option<Timestamp>,
    pub level: u8,
    /// Buffs and debuffs. Ones past `until` are dropped on the next write.
    pub statuses: Vec<StatusSlot>,
    /// Diminishing returns on crowd control.
    pub resists: Vec<Resist>,
    /// A character with enough crime points (docs/design/crime.md).
    pub sought: bool,
}

/// When one skill can be used again.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct Cooldown {
    pub skill: u16,
    pub ready_at: Timestamp,
}

/// Private, one per character: cooldowns, death stacks, and its vitals while it's out of the world.
/// The owner reads it through `my_fighter`.
#[spacetimedb::table(accessor = fighter)]
#[derive(Clone, Debug, PartialEq)]
pub struct Fighter {
    #[primary_key]
    pub char_id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub gcd_until: Timestamp,
    pub cooldowns: Vec<Cooldown>,
    /// Deaths counting toward the wait to respawn, and when the last was.
    pub deaths: u32,
    pub last_death_at: Option<Timestamp>,
    /// HP and mana as of `at`, out of combat, kept while the character is out of the world.
    pub hp: i32,
    pub mp: i32,
    pub at: Timestamp,
    pub dead_at: Option<Timestamp>,
    pub level: u8,
    /// Experience into the level.
    pub xp: u32,
    /// The skillsets picked, in the order they were.
    pub skillsets: Vec<u8>,
}

/// Private: a status that deals or heals over time, due at `due` (µs of game time), which
/// `world_tick` finds through the index. One per fighter and status.
#[spacetimedb::table(accessor = periodic)]
#[derive(Clone, Debug, PartialEq)]
pub struct Periodic {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub entity_id: u64,
    pub status: u8,
    /// The skill that put it there, for events.
    pub skill: u16,
    pub source: u64,
    pub amount: i32,
    pub start: Timestamp,
    pub until: Timestamp,
    #[index(btree)]
    pub due: i64,
}

/// Private: a timed cast, due when it finishes. One per caster at most.
#[spacetimedb::table(accessor = cast_job, scheduled(cast_done))]
#[derive(Clone, Debug, PartialEq)]
pub struct CastJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    #[unique]
    pub entity_id: u64,
    pub char_id: u64,
    pub skill: u16,
    pub target: u64,
    /// Where the caster stood when it began, in the frame of `parent` (0 the world, or a ship).
    pub parent: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Private: a projectile in flight, due when it lands.
#[spacetimedb::table(accessor = skill_impact, scheduled(skill_lands))]
#[derive(Clone, Debug, PartialEq)]
pub struct SkillImpact {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub source: u64,
    pub target: u64,
    pub skill: u16,
}

/// Static and public: a Wayshrine, where the dead respawn. Written only when admins place or
/// remove it.
#[spacetimedb::table(accessor = shrine, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Shrine {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The Wayshrine kind's wire (`content/combat.toml`).
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Where an admin puts a Wayshrine.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct ShrinePlacement {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// The fighter rows of the caller's characters.
#[spacetimedb::view(accessor = my_fighter, public)]
pub(crate) fn my_fighter(ctx: &ViewContext) -> Vec<Fighter> {
    ctx.db.fighter().owner().filter(ctx.sender()).collect()
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

fn after(t: Timestamp, micros: i64) -> Timestamp {
    t + TimeDuration::from_micros(micros)
}

fn later(a: Timestamp, b: Timestamp) -> Timestamp {
    if a > b { a } else { b }
}

/// `v` with HP and mana settled to `at`.
fn settled(v: &Vitals, at: Timestamp) -> Vitals {
    let dead = v.dead_at.is_some();
    let (from, until, t) = (micros(v.at), micros(v.combat_until), micros(at));
    Vitals {
        hp: sim::combat::hp_now(v.hp, v.hp_max, from, until, t, dead),
        mp: sim::combat::mp_now(v.mp, v.mp_max, from, until, t, dead),
        at: later(v.at, at),
        ..v.clone()
    }
}

fn no_cast(v: Vitals) -> Vitals {
    Vitals { cast_skill: 0, cast_target: 0, ..v }
}

/// `v`'s statuses as the rules see them.
fn slots(v: &Vitals) -> Vec<Slot> {
    v.statuses.iter().map(|s| Slot { status: s.status, start: micros(s.start), until: micros(s.until) }).collect()
}

fn resists(v: &Vitals) -> Vec<sim::combat::Resist> {
    v.resists.iter().map(|r| sim::combat::Resist { group: r.group, count: r.count, until: micros(r.until) }).collect()
}

/// Stunned or knocked down now.
pub fn controlled(ctx: &ReducerContext, entity_id: u64) -> bool {
    ctx.db.vitals().entity_id().find(entity_id).is_some_and(|v| sim::combat::controlled(&slots(&v), micros(now(ctx))))
}

/// The speed limit's factor from controls for a move over `from` to `to` (`sim::combat::move_scale`).
pub fn move_scale(ctx: &ReducerContext, entity_id: u64, from: Timestamp, to: Timestamp) -> f32 {
    ctx.db.vitals().entity_id().find(entity_id).map_or(1.0, |v| sim::combat::move_scale(&slots(&v), micros(from), micros(to)))
}

fn event(ctx: &ReducerContext, cell: u64, source: u64, target: u64, skill_id: u32, amount: i32, flags: u16) {
    ctx.db.combat_event().insert(CombatEvent { cell, source, target, skill_id, amount, flags, status: 0 });
}

/// End every status on `entity_id` and stop what it deals over time.
pub(crate) fn clear_statuses(ctx: &ReducerContext, entity_id: u64) {
    for row in ctx.db.periodic().entity_id().filter(entity_id).collect::<Vec<_>>() {
        ctx.db.periodic().id().delete(row.id);
    }
}

pub fn alive(ctx: &ReducerContext, entity_id: u64) -> bool {
    ctx.db.vitals().entity_id().find(entity_id).is_some_and(|v| v.dead_at.is_none())
}

/// For moving: the dead can't.
pub fn require_living(ctx: &ReducerContext, p: &Presence) -> Result<(), String> {
    if alive(ctx, p.entity_id) { Ok(()) } else { Err(Refusal::Dead.message().into()) }
}

/// For reducers that act in the world, which neither the dead nor the stunned or knocked down can
/// use.
pub fn require_alive(ctx: &ReducerContext, p: &Presence) -> Result<(), String> {
    require_living(ctx, p)?;
    if controlled(ctx, p.entity_id) { Err(Refusal::Controlled.message().into()) } else { Ok(()) }
}

/// Where an entity is in world coordinates: a character wherever it stands, anything else where
/// its transform says.
pub(crate) fn world_at(ctx: &ReducerContext, entity_id: u64) -> Option<[f32; 3]> {
    if let Some(at) = crate::npc::position(ctx, entity_id) {
        return Some(at);
    }
    match ctx.db.presence().entity_id().find(entity_id) {
        Some(p) => Some(crate::ship::world_position(ctx, &p)),
        None => ctx.db.transform().entity_id().find(entity_id).filter(|t| t.parent == 0).map(|t| [t.x, t.y, t.z]),
    }
}

pub(crate) fn in_sight(ctx: &ReducerContext, from: [f32; 3], to: [f32; 3]) -> bool {
    sim::terrain::line_of_sight(&mut Terrain::new(ctx), [from[0], from[1] + EYE_M, from[2]], [to[0], to[1] + CHEST_M, to[2]])
}

/// Called by `enter_world`: give the new entity its vitals from the character's fighter row, made
/// at full health for a first entry.
pub fn enter(ctx: &ReducerContext, owner: Identity, char_id: u64, entity_id: u64, cell: u64) {
    let at = now(ctx);
    let f = match ctx.db.fighter().char_id().find(char_id) {
        Some(f) => f,
        None => ctx.db.fighter().insert(Fighter {
            char_id,
            owner,
            gcd_until: at,
            cooldowns: Vec::new(),
            deaths: 0,
            last_death_at: None,
            hp: COMBAT.hp_max,
            mp: COMBAT.mp_max,
            at,
            dead_at: None,
            level: 1,
            xp: 0,
            skillsets: Vec::new(),
        }),
    };
    ctx.db.vitals().insert(Vitals {
        entity_id,
        cell,
        hp: f.hp,
        hp_max: sim::combat::hp_max_at(f.level),
        mp: f.mp,
        mp_max: sim::combat::mp_max_at(f.level),
        at: f.at,
        combat_until: f.at,
        target: 0,
        cast_skill: 0,
        cast_target: 0,
        cast_start: at,
        cast_end: at,
        dead_at: f.dead_at,
        level: f.level,
        statuses: Vec::new(),
        resists: Vec::new(),
        sought: crate::crime::is_sought(ctx, char_id),
    });
}

/// Called by `despawn`: stop any cast and keep the character's vitals, settled to now, in its
/// fighter row.
pub fn leave(ctx: &ReducerContext, char_id: u64, entity_id: u64) {
    ctx.db.cast_job().entity_id().delete(entity_id);
    crate::npc::forget(ctx, entity_id);
    clear_statuses(ctx, entity_id);
    let Some(v) = ctx.db.vitals().entity_id().find(entity_id) else { return };
    let v = settled(&v, now(ctx));
    if let Some(f) = ctx.db.fighter().char_id().find(char_id) {
        ctx.db.fighter().char_id().update(Fighter { hp: v.hp, mp: v.mp, at: v.at, dead_at: v.dead_at, ..f });
    }
    ctx.db.vitals().entity_id().delete(entity_id);
}

/// Keep `vitals.cell` with the entity's cell.
pub fn recell(ctx: &ReducerContext, entity_id: u64, cell: u64) {
    if let Some(v) = ctx.db.vitals().entity_id().find(entity_id).filter(|v| v.cell != cell) {
        ctx.db.vitals().entity_id().update(Vitals { cell, ..v });
    }
}

/// Called by `move_event` for every accepted move: a caster that steps away or changes frame
/// stops casting. Turning on the spot doesn't.
pub fn moved(ctx: &ReducerContext, entity_id: u64, parent: u64, at: [f32; 3]) {
    let Some(job) = ctx.db.cast_job().entity_id().find(entity_id) else { return };
    if parent != job.parent || sim::combat::moved_off([job.x, job.y, job.z], at) {
        cancel(ctx, entity_id);
    }
}

/// End `entity_id`'s cast, if it has one. Nothing is spent.
pub fn cancel(ctx: &ReducerContext, entity_id: u64) {
    ctx.db.cast_job().entity_id().delete(entity_id);
    if let Some(v) = ctx.db.vitals().entity_id().find(entity_id).filter(|v| v.cast_skill != 0) {
        ctx.db.vitals().entity_id().update(no_cast(v));
    }
}

#[spacetimedb::reducer]
pub fn set_target(ctx: &ReducerContext, target: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if target != 0 && ctx.db.vitals().entity_id().find(target).is_none() {
        return Err("no such target".into());
    }
    let v = ctx.db.vitals().entity_id().find(p.entity_id).ok_or("not in the world")?;
    if v.target != target {
        ctx.db.vitals().entity_id().update(Vitals { target, ..v });
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn cancel_cast(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    cancel(ctx, p.entity_id);
    Ok(())
}

fn skill_of(wire: u16) -> Result<&'static SkillData, String> {
    sim::combat::skill(wire).ok_or_else(|| "no such skill".into())
}

/// Who a skill used on `asked` lands on: an ally skill on anything but a living character heals
/// the caster, and a caster skill is always on the caster.
fn resolve(ctx: &ReducerContext, s: &SkillData, caster: u64, asked: u64) -> u64 {
    match s.target {
        SkillTarget::Enemy => asked,
        SkillTarget::Ally => {
            let character = ctx.db.entity().id().find(asked).is_some_and(|e| e.kind == entity_kind::CHARACTER);
            if character && alive(ctx, asked) { asked } else { caster }
        }
        SkillTarget::Caster => caster,
    }
}

/// Use skill `skill` on `target` (docs/design/combat.md).
#[spacetimedb::reducer]
pub fn use_skill(ctx: &ReducerContext, skill: u16, target: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let v = ctx.db.vitals().entity_id().find(p.entity_id).ok_or("not in the world")?;
    if v.dead_at.is_some() {
        return Err(Refusal::Dead.message().into());
    }
    let s = skill_of(skill)?;
    let f = ctx.db.fighter().char_id().find(p.char_id).ok_or("no fighter")?;
    let at = now(ctx);
    let t = micros(at);
    let cooldown = f.cooldowns.iter().find(|c| c.skill == skill).map_or(f.gcd_until, |c| later(c.ready_at, f.gcd_until));
    let mine = settled(&v, at);
    let target = resolve(ctx, s, p.entity_id, target);
    let tv = ctx.db.vitals().entity_id().find(target).ok_or(Refusal::NoTarget.message())?;
    let me = crate::ship::world_position(ctx, &p);
    let them = world_at(ctx, target).ok_or(Refusal::NoTarget.message())?;
    let caster = Caster {
        at: me,
        yaw: sim::ship::yaw_from_u16(p.yaw) + yaw_of_frame(ctx, p.parent),
        alive: true,
        controlled: sim::combat::controlled(&slots(&v), t),
        knows: sim::combat::knows(s, f.level, &f.skillsets),
        casting: v.cast_skill != 0,
        mp: mine.mp,
        ready_at: micros(cooldown),
    };
    let is_caster = target == p.entity_id;
    sim::combat::check(s, &caster, &Target { at: them, alive: tv.dead_at.is_none(), is_caster }, t).map_err(|r| r.message().to_string())?;
    if !is_caster && !in_sight(ctx, me, them) {
        return Err(Refusal::NoSight.message().into());
    }
    crate::gathering::cancel(ctx, p.char_id);
    crate::crafting::cancel(ctx, p.char_id);
    ctx.db.fighter().char_id().update(Fighter { gcd_until: after(at, sim::combat::gcd_micros()), ..f });
    if s.cast_ms == 0 {
        return finish(ctx, &p, s, target);
    }
    let took = sim::combat::cast_micros(s);
    ctx.db.vitals().entity_id().update(Vitals { cast_skill: skill, cast_target: target, cast_start: at, cast_end: after(at, took), ..v });
    ctx.db.cast_job().insert(CastJob {
        scheduled_id: 0,
        scheduled_at: after(ctx.timestamp, took).into(),
        entity_id: p.entity_id,
        char_id: p.char_id,
        skill,
        target,
        parent: p.parent,
        x: p.x,
        y: p.y,
        z: p.z,
    });
    Ok(())
}

/// The yaw a frame adds to its passengers' own: a ship's, or nothing in the world.
fn yaw_of_frame(ctx: &ReducerContext, parent: u64) -> f32 {
    crate::ship::deck(ctx, parent).map_or(0.0, |d| d.pose().yaw)
}

/// A timed cast is due: finish it, unless it was interrupted. It never fails, since nobody would
/// hear about it; the reason goes to the log.
#[spacetimedb::reducer]
pub fn cast_done(ctx: &ReducerContext, job: CastJob) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("cast_done runs on the server's schedule only".into());
    }
    let Some(v) = ctx.db.vitals().entity_id().find(job.entity_id).filter(|v| v.cast_skill == job.skill && v.cast_target == job.target) else {
        return Ok(());
    };
    ctx.db.vitals().entity_id().update(no_cast(v));
    let result = match ctx.db.presence().entity_id().find(job.entity_id) {
        Some(p) => skill_of(job.skill).and_then(|s| finish(ctx, &p, s, job.target)),
        None => Err("left the world".into()),
    };
    if let Err(e) = result {
        log::info!("cast of skill {} by entity {} on {} failed: {e}", job.skill, job.entity_id, job.target);
    }
    Ok(())
}

/// Check the target again, spend the mana, start the cooldown, and land the skill now or send its
/// projectile. On `Err` nothing has changed.
fn finish(ctx: &ReducerContext, p: &Presence, s: &'static SkillData, target: u64) -> Result<(), String> {
    let at = now(ctx);
    let v = ctx.db.vitals().entity_id().find(p.entity_id).filter(|v| v.dead_at.is_none()).ok_or(Refusal::Dead.message())?;
    let tv = ctx.db.vitals().entity_id().find(target).ok_or(Refusal::NoTarget.message())?;
    let me = crate::ship::world_position(ctx, p);
    let them = world_at(ctx, target).ok_or(Refusal::NoTarget.message())?;
    let is_caster = target == p.entity_id;
    sim::combat::check_finish(s, me, &Target { at: them, alive: tv.dead_at.is_none(), is_caster }).map_err(|r| r.message().to_string())?;
    if !is_caster && !in_sight(ctx, me, them) {
        return Err(Refusal::NoSight.message().into());
    }
    let v = settled(&v, at);
    if v.mp < s.cost_mp {
        return Err(Refusal::NoMana.message().into());
    }
    ctx.db.vitals().entity_id().update(Vitals { mp: v.mp - s.cost_mp, ..v });
    if let Some(f) = ctx.db.fighter().char_id().find(p.char_id) {
        let mut cooldowns: Vec<Cooldown> = f.cooldowns.iter().filter(|c| c.skill != s.wire && c.ready_at > at).cloned().collect();
        if s.cooldown_ms > 0 {
            cooldowns.push(Cooldown { skill: s.wire, ready_at: after(at, sim::combat::cooldown_micros(s)) });
        }
        ctx.db.fighter().char_id().update(Fighter { cooldowns, ..f });
    }
    release(ctx, p.entity_id, me, target, s);
    Ok(())
}

/// A skill leaves `source` at `from`: it lands now, or its projectile is sent. Every check and cost
/// is already behind it.
pub(crate) fn release(ctx: &ReducerContext, source: u64, from: [f32; 3], target: u64, s: &'static SkillData) {
    let flight = world_at(ctx, target).map_or(0, |them| sim::combat::impact_micros(sim::combat::distance(from, them), s.speed_mps));
    if flight == 0 {
        land(ctx, source, target, s);
    } else {
        ctx.db.skill_impact().insert(SkillImpact { scheduled_id: 0, scheduled_at: after(ctx.timestamp, flight).into(), source, target, skill: s.wire });
    }
}

/// A projectile arrives. It homes, so it lands wherever its target went, if it's still alive.
#[spacetimedb::reducer]
pub fn skill_lands(ctx: &ReducerContext, impact: SkillImpact) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("skill_lands runs on the server's schedule only".into());
    }
    if let Some(s) = sim::combat::skill(impact.skill) {
        land(ctx, impact.source, impact.target, s);
    }
    Ok(())
}

/// Apply a skill's effects to `target`, if it's alive, with an event for each. An effect with a
/// condition (a combo) happens only if the target had that status as the skill landed, so a skill
/// never combos off its own statuses.
fn land(ctx: &ReducerContext, source: u64, target: u64, s: &SkillData) {
    let t = micros(now(ctx));
    let Some(had) = ctx.db.vitals().entity_id().find(target).map(|v| slots(&v)) else { return };
    for e in s.effects {
        if e.when.is_some_and(|w| !sim::combat::has(&had, w, t)) {
            continue;
        }
        let Some(tv) = ctx.db.vitals().entity_id().find(target).filter(|v| v.dead_at.is_none()) else { return };
        match e.kind {
            EffectKind::Damage => {
                let dealt = ctx.db.vitals().entity_id().find(source).map_or(0, |v| sim::combat::modifier(&slots(&v), t, false));
                let amount = sim::combat::scaled_damage(e.amount, dealt, sim::combat::modifier(&had, t, true));
                harm(ctx, source, tv, u32::from(s.wire), amount, flags::HIT);
            }
            EffectKind::Heal => heal(ctx, source, tv, s.wire, e.amount, flags::HEAL),
            EffectKind::Status => {
                if let Some(status) = e.status {
                    afflict(ctx, source, tv, s.wire, status, e.ms, e.amount);
                }
            }
            EffectKind::Cleanse => cleanse(ctx, tv),
        }
    }
}

/// Put status `status` on `tv` for `ms`, replacing any it had. A control lands for less, or not
/// at all, by the diminishing returns, and stops the target's cast; a stun or knockdown also
/// stops its gathering and crafting.
fn afflict(ctx: &ReducerContext, source: u64, tv: Vitals, skill: u16, status: u8, ms: u32, amount: i32) {
    let Some(d) = sim::combat::status(status) else { return };
    if d.debuff {
        crate::crime::harmed(ctx, source, &tv, false);
    }
    let at = now(ctx);
    let t = micros(at);
    let mut v = tv;
    let mut lasts = i64::from(ms) * 1000;
    if let Some(group) = sim::combat::control_group(d.control) {
        let Some(pct) = sim::combat::diminished(&resists(&v), group, t) else {
            ctx.db.combat_event().insert(CombatEvent { cell: v.cell, source, target: v.entity_id, skill_id: u32::from(skill), amount: 0, flags: flags::IMMUNE, status });
            return;
        };
        lasts = sim::combat::control_micros(ms, pct);
        v.resists = sim::combat::resisted(&resists(&v), group, t, lasts)
            .into_iter()
            .map(|r| Resist { group: r.group, count: r.count, until: Timestamp::from_micros_since_unix_epoch(r.until) })
            .collect();
    }
    let until = after(at, lasts);
    v.statuses.retain(|s| s.until > at && s.status != status);
    v.statuses.push(StatusSlot { status, source, start: at, until, amount });
    let entity_id = v.entity_id;
    ctx.db.combat_event().insert(CombatEvent {
        cell: v.cell,
        source,
        target: entity_id,
        skill_id: u32::from(skill),
        amount: i32::try_from(lasts / 1000).unwrap_or(i32::MAX),
        flags: flags::STATUS,
        status,
    });
    ctx.db.vitals().entity_id().update(v);
    if d.debuff && source != entity_id {
        crate::npc::provoked(ctx, entity_id, source, i64::from(sim::npc::NPC.status_threat), 0);
    }
    if let Some(old) = ctx.db.periodic().entity_id().filter(entity_id).find(|r| r.status == status) {
        ctx.db.periodic().id().delete(old.id);
    }
    if let Some(due) = sim::combat::next_tick(t, micros(until), d.tick_ms, t) {
        ctx.db.periodic().insert(Periodic { id: 0, entity_id, status, skill, source, amount, start: at, until, due });
    }
    if d.control != Control::None {
        cancel(ctx, entity_id);
    }
    if matches!(d.control, Control::Stun | Control::Knockdown)
        && let Some(p) = ctx.db.presence().entity_id().find(entity_id)
    {
        crate::gathering::cancel(ctx, p.char_id);
        crate::crafting::cancel(ctx, p.char_id);
    }
}

/// Put status `status` on `entity_id` until `until`, replacing any it had, with no diminishing
/// returns and no event: the crime mark (docs/design/crime.md), seen in `vitals` like any status.
pub(crate) fn mark(ctx: &ReducerContext, entity_id: u64, status: u8, until: Timestamp) {
    let Some(mut v) = ctx.db.vitals().entity_id().find(entity_id) else { return };
    let at = now(ctx);
    v.statuses.retain(|s| s.until > at && s.status != status);
    v.statuses.push(StatusSlot { status, source: entity_id, start: at, until, amount: 0 });
    ctx.db.vitals().entity_id().update(v);
}

/// End `tv`'s crowd control: every debuff that stuns, knocks down or snares.
fn cleanse(ctx: &ReducerContext, tv: Vitals) {
    let controls = |s: &StatusSlot| sim::combat::status(s.status).is_some_and(|d| d.control != Control::None);
    if !tv.statuses.iter().any(controls) {
        return;
    }
    let statuses = tv.statuses.iter().filter(|s| !controls(s)).cloned().collect();
    ctx.db.vitals().entity_id().update(Vitals { statuses, ..tv });
}

/// Deal and heal what statuses do over time, for every tick due by now. Called by `world_tick`;
/// a dev clock jump catches up on every tick it skipped.
pub fn run_periodic(ctx: &ReducerContext) {
    let t = micros(now(ctx));
    for mut row in ctx.db.periodic().due().filter(..=t).collect::<Vec<_>>() {
        let Some(d) = sim::combat::status(row.status) else {
            ctx.db.periodic().id().delete(row.id);
            continue;
        };
        loop {
            let Some(tv) = ctx.db.vitals().entity_id().find(row.entity_id).filter(|v| v.dead_at.is_none()) else {
                ctx.db.periodic().id().delete(row.id);
                break;
            };
            if d.debuff {
                harm(ctx, row.source, tv, u32::from(row.skill), row.amount, flags::HIT | flags::TICK);
            } else {
                heal(ctx, row.source, tv, row.skill, row.amount, flags::HEAL | flags::TICK);
            }
            match sim::combat::next_tick(micros(row.start), micros(row.until), d.tick_ms, row.due) {
                Some(due) if due <= t => row.due = due,
                Some(due) => {
                    ctx.db.periodic().id().update(Periodic { due, ..row });
                    break;
                }
                None => {
                    ctx.db.periodic().id().delete(row.id);
                    break;
                }
            }
        }
    }
}

/// Put `entity_id` in combat until `combat_s` from now.
fn engage(ctx: &ReducerContext, entity_id: u64, at: Timestamp) {
    let until = Timestamp::from_micros_since_unix_epoch(sim::combat::combat_until(micros(at)));
    if let Some(v) = ctx.db.vitals().entity_id().find(entity_id).filter(|v| v.combat_until < until) {
        let v = settled(&v, at);
        ctx.db.vitals().entity_id().update(Vitals { combat_until: until, ..v });
    }
}

/// Take `amount` HP from `tv`, putting both sides in combat; at 0 it dies.
fn harm(ctx: &ReducerContext, source: u64, tv: Vitals, skill_id: u32, amount: i32, hit: u16) {
    let at = now(ctx);
    engage(ctx, source, at);
    let v = settled(&tv, at);
    let until = Timestamp::from_micros_since_unix_epoch(sim::combat::combat_until(micros(at)));
    let hp = (v.hp - amount).max(0);
    let flags = if hp == 0 { hit | flags::KILL } else { hit };
    event(ctx, v.cell, source, v.entity_id, skill_id, amount, flags);
    ctx.db.vitals().entity_id().update(Vitals { hp, combat_until: later(v.combat_until, until), ..v.clone() });
    let dealt = i64::from(v.hp - hp);
    crate::npc::provoked(ctx, v.entity_id, source, dealt, dealt);
    crate::crime::harmed(ctx, source, &v, hp == 0);
    if hp == 0 {
        die(ctx, v.entity_id, source);
    }
}

/// Give `tv` up to `amount` HP; healing someone in combat puts the healer in it too.
fn heal(ctx: &ReducerContext, source: u64, tv: Vitals, skill: u16, amount: i32, flags: u16) {
    let at = now(ctx);
    let v = settled(&tv, at);
    let hp = (v.hp + amount).min(v.hp_max).max(v.hp);
    event(ctx, v.cell, source, v.entity_id, u32::from(skill), hp - v.hp, flags);
    let fighting = v.combat_until > at;
    crate::npc::healed(ctx, source, v.entity_id, hp - v.hp);
    ctx.db.vitals().entity_id().update(Vitals { hp, ..v });
    if fighting && source != tv.entity_id {
        engage(ctx, source, at);
    }
}

/// `entity_id` has reached 0 HP. A Straw Dummy is back to full at once; a character dies.
fn die(ctx: &ReducerContext, entity_id: u64, killer: u64) {
    let at = now(ctx);
    let Some(v) = ctx.db.vitals().entity_id().find(entity_id) else { return };
    clear_statuses(ctx, entity_id);
    let character = |id: u64| ctx.db.entity().id().find(id).is_some_and(|e| e.kind == entity_kind::CHARACTER);
    let npc = crate::npc::is_npc(ctx, entity_id);
    if !character(entity_id) && killer != entity_id && !npc {
        reward(ctx, killer, v.level);
    }
    if ctx.db.entity().id().find(entity_id).is_some_and(|e| e.kind == entity_kind::DUMMY) {
        ctx.db.vitals().entity_id().update(Vitals { hp: v.hp_max, at, combat_until: at, statuses: Vec::new(), resists: Vec::new(), ..v });
        return;
    }
    ctx.db.cast_job().entity_id().delete(entity_id);
    ctx.db.vitals().entity_id().update(no_cast(Vitals { hp: 0, at, dead_at: Some(at), target: 0, statuses: Vec::new(), resists: Vec::new(), ..v }));
    if npc {
        crate::npc::died(ctx, entity_id, killer);
        return;
    }
    crate::npc::forget(ctx, entity_id);
    let Some(p) = ctx.db.presence().entity_id().find(entity_id) else { return };
    let pve = killer == entity_id || !character(killer);
    crate::gathering::cancel(ctx, p.char_id);
    crate::crafting::cancel(ctx, p.char_id);
    crate::ship::let_go(ctx, p.parent, entity_id);
    crate::trade::drop_on_death(ctx, &p);
    if let Some(f) = ctx.db.fighter().char_id().find(p.char_id) {
        let deaths = sim::combat::deaths_after(f.deaths, f.last_death_at.map(micros), micros(at));
        let xp = if pve { sim::combat::xp_after_death(f.level, f.xp) } else { f.xp };
        ctx.db.fighter().char_id().update(Fighter { deaths, last_death_at: Some(at), xp, ..f });
    }
    log::info!("char {} died to entity {killer}", p.char_id);
}

/// A character's kill of a foe at `victim` that isn't a character: experience, and maybe levels.
pub(crate) fn reward(ctx: &ReducerContext, killer: u64, victim: u8) {
    let Some(p) = ctx.db.presence().entity_id().find(killer) else { return };
    let Some(f) = ctx.db.fighter().char_id().find(p.char_id) else { return };
    let gained = sim::combat::kill_xp(victim, f.level);
    if gained == 0 {
        return;
    }
    let (level, xp) = sim::combat::gain_xp(f.level, f.xp, gained);
    let cell = ctx.db.vitals().entity_id().find(killer).map_or(0, |v| v.cell);
    event(ctx, cell, killer, killer, 0, i32::try_from(gained).unwrap_or(i32::MAX), flags::XP);
    let was = f.level;
    ctx.db.fighter().char_id().update(Fighter { level, xp, ..f });
    if level != was {
        set_level(ctx, killer, level);
        event(ctx, cell, killer, killer, 0, i32::from(level), flags::LEVEL);
    }
}

/// Give `entity_id`'s vitals the maximums of `level`, keeping what it had below them and adding
/// what the maximums grew by.
fn set_level(ctx: &ReducerContext, entity_id: u64, level: u8) {
    let Some(v) = ctx.db.vitals().entity_id().find(entity_id) else { return };
    let v = settled(&v, now(ctx));
    let (hp_max, mp_max) = (sim::combat::hp_max_at(level), sim::combat::mp_max_at(level));
    let grow = |value: i32, old: i32, new: i32| if v.dead_at.is_some() { value.min(new) } else { (value + (new - old).max(0)).min(new) };
    let (hp, mp) = (grow(v.hp, v.hp_max, hp_max), grow(v.mp, v.mp_max, mp_max));
    ctx.db.vitals().entity_id().update(Vitals { level, hp, hp_max, mp, mp_max, ..v });
}

/// Pick a skillset: one at each of `progress.skillset_at`'s levels. Its skills open by level.
#[spacetimedb::reducer]
pub fn pick_skillset(ctx: &ReducerContext, skillset: u8) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if sim::combat::SKILLSETS.get(usize::from(skillset)).is_none_or(|s| s.npc) {
        return Err("no such skillset".into());
    }
    let mut f = ctx.db.fighter().char_id().find(p.char_id).ok_or("no fighter")?;
    if f.skillsets.contains(&skillset) {
        return Err("already picked".into());
    }
    if f.skillsets.len() >= sim::combat::skillset_slots(f.level) {
        return Err(match sim::combat::PROGRESS.skillset_at.get(f.skillsets.len()) {
            Some(l) => format!("you can pick another skillset at level {l}"),
            None => "every skillset slot is taken".into(),
        });
    }
    f.skillsets.push(skillset);
    ctx.db.fighter().char_id().update(f);
    Ok(())
}

/// When `f`'s character, dead since `dead_at`, may respawn.
fn respawn_at(f: &Fighter, dead_at: Timestamp) -> Timestamp {
    after(dead_at, sim::combat::respawn_wait_micros(f.deaths))
}

/// Get up again at the nearest Wayshrine, with part of the HP and mana.
#[spacetimedb::reducer]
pub fn respawn(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let v = ctx.db.vitals().entity_id().find(p.entity_id).ok_or("not in the world")?;
    let dead_at = v.dead_at.ok_or("you aren't dead")?;
    let f = ctx.db.fighter().char_id().find(p.char_id).ok_or("no fighter")?;
    let at = now(ctx);
    let ready = respawn_at(&f, dead_at);
    if at < ready {
        let secs = (micros(ready) - micros(at) + 999_999) / 1_000_000;
        return Err(format!("you can respawn in {secs} s"));
    }
    let here = crate::ship::world_position(ctx, &p);
    let [x, z] = nearest_shrine(ctx, here);
    let cell = crate::movement::relocate(ctx, p, x, z, MoveMode::Run, true);
    let v = ctx.db.vitals().entity_id().find(v.entity_id).unwrap_or(v);
    ctx.db.vitals().entity_id().update(Vitals {
        cell,
        hp: sim::combat::respawn_share(v.hp_max),
        mp: sim::combat::respawn_share(v.mp_max),
        at,
        combat_until: at,
        dead_at: None,
        ..v
    });
    Ok(())
}

/// Where to get up (x, z): `SHRINE_STEP_M` in front of the Wayshrine nearest `at`, or the world's
/// spawn if there's none.
fn nearest_shrine(ctx: &ReducerContext, at: [f32; 3]) -> [f32; 2] {
    let d2 = |s: &Shrine| (s.x - at[0]).powi(2) + (s.z - at[2]).powi(2);
    let spawn = crate::session::SPAWN;
    ctx.db.shrine().iter().min_by(|a, b| d2(a).total_cmp(&d2(b))).map_or([spawn[0], spawn[2]], |s| {
        let [fx, fz] = sim::ship::facing(sim::ship::yaw_from_u16(s.yaw));
        [s.x + fx * SHRINE_STEP_M, s.z + fz * SHRINE_STEP_M]
    })
}

/// Add Wayshrines.
#[spacetimedb::reducer]
pub fn admin_put_shrines(ctx: &ReducerContext, shrines: Vec<ShrinePlacement>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = shrines.iter().find(|s| shrine(s.kind).is_none() || ![s.x, s.y, s.z].iter().all(|v| v.is_finite())) {
        return Err(format!("bad shrine {bad:?}: unknown kind or a non-finite position"));
    }
    let half = sim::cell::WORLD_HALF_M;
    if let Some(bad) = shrines.iter().find(|s| s.x.abs() >= half || s.z.abs() >= half) {
        return Err(format!("bad shrine {bad:?}: off the map"));
    }
    for s in &shrines {
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, s.x, s.z);
        ctx.db.shrine().insert(Shrine { id: 0, cell, kind: s.kind, x: s.x, y: s.y, z: s.z, yaw: s.yaw });
    }
    log::info!("{} Wayshrines placed", shrines.len());
    Ok(())
}

/// Remove Wayshrines by id. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_shrines(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        ctx.db.shrine().id().delete(id);
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_shrines(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let ids: Vec<u64> = ctx.db.shrine().iter().map(|s| s.id).collect();
    for &id in &ids {
        ctx.db.shrine().id().delete(id);
    }
    log::info!("{} Wayshrines cleared", ids.len());
    Ok(())
}

/// Dev builds only: a Straw Dummy on the ground at (x, z), for practice until NPCs.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_spawn_dummy(ctx: &ReducerContext, x: f32, z: f32) -> Result<(), String> {
    use crate::session::{Entity, Transform};
    if !x.is_finite() || !z.is_finite() || x.abs() >= sim::cell::WORLD_HALF_M || z.abs() >= sim::cell::WORLD_HALF_M {
        return Err("off the map".into());
    }
    let y = sim::movement::settle(Terrain::new(ctx).ground(x, z), [x, 0.0, z], MoveMode::Run);
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    let e = ctx.db.entity().insert(Entity { id: 0, cell, kind: entity_kind::DUMMY, ref_id: 0, pack: 0 });
    ctx.db.transform().insert(Transform { entity_id: e.id, cell, parent: 0, x, y, z, yaw: 0, speed: 0, mode: MoveMode::Run as u8 });
    let at = now(ctx);
    ctx.db.vitals().insert(Vitals {
        entity_id: e.id,
        cell,
        hp: COMBAT.dummy_hp,
        hp_max: COMBAT.dummy_hp,
        mp: 0,
        mp_max: 0,
        at,
        combat_until: at,
        target: 0,
        cast_skill: 0,
        cast_target: 0,
        cast_start: at,
        cast_end: at,
        dead_at: None,
        level: 1,
        statuses: Vec::new(),
        resists: Vec::new(),
        sought: false,
    });
    log::info!("Straw Dummy {} at ({x:.1}, {z:.1})", e.id);
    Ok(())
}

/// Dev builds only: remove a Straw Dummy.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_remove_dummy(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if !ctx.db.entity().id().find(id).is_some_and(|e| e.kind == entity_kind::DUMMY) {
        return Err("no such dummy".into());
    }
    ctx.db.vitals().entity_id().delete(id);
    ctx.db.transform().entity_id().delete(id);
    ctx.db.entity().id().delete(id);
    Ok(())
}

/// Dev builds only: set the caller's HP and mana, within their maximums. 0 HP kills, as a hit
/// would; more than 0 brings the dead back where they lie.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_set_vitals(ctx: &ReducerContext, hp: i32, mp: i32) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let v = ctx.db.vitals().entity_id().find(p.entity_id).ok_or("not in the world")?;
    let at = now(ctx);
    let (hp, mp) = (hp.clamp(0, v.hp_max), mp.clamp(0, v.mp_max));
    let dead_at = if hp == 0 { v.dead_at } else { None };
    ctx.db.vitals().entity_id().update(Vitals { hp, mp, at, dead_at, ..v.clone() });
    if hp == 0 && v.dead_at.is_none() {
        die(ctx, p.entity_id, p.entity_id);
    }
    Ok(())
}

/// Dev builds only: set the caller's level, with no experience into it, and its HP and mana to the
/// new maximums. Skillsets picked beyond the level's slots are forgotten, the latest first.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_set_level(ctx: &ReducerContext, level: u8) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if !(1..=sim::combat::level_max()).contains(&level) {
        return Err(format!("levels are 1 to {}", sim::combat::level_max()));
    }
    let mut f = ctx.db.fighter().char_id().find(p.char_id).ok_or("no fighter")?;
    f.skillsets.truncate(sim::combat::skillset_slots(level));
    ctx.db.fighter().char_id().update(Fighter { level, xp: 0, ..f });
    set_level(ctx, p.entity_id, level);
    let v = ctx.db.vitals().entity_id().find(p.entity_id).ok_or("not in the world")?;
    if v.dead_at.is_none() {
        ctx.db.vitals().entity_id().update(Vitals { hp: v.hp_max, mp: v.mp_max, at: now(ctx), ..v });
    }
    Ok(())
}
