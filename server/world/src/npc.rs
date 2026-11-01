//! NPCs (docs/design/npcs.md): spawn points that wake near players, a brain that wanders, notices,
//! chases, fights, gives up and goes home, threat, and loot on corpses. An NPC's position comes from
//! its segment's timestamps; `transform` is written only to start, stop or turn, and on a heartbeat
//! while it moves, as a client's moves are (ADR 0030).

use spacetimedb::{ReducerContext, SpacetimeType, Table, Timestamp};
use sim::combat::SkillData;
use sim::movement::{MoveMode, SEA_LEVEL_M};
use sim::npc::{NPC, NpcKindData, Segment, npc_kind};

use crate::combat::{Cooldown, Vitals, vitals};
use crate::session::{Entity, Transform, entity, entity_kind, presence, require_admin, transform};
use crate::terrain::Terrain;
use crate::time::now;

/// `npc.state` values.
pub mod state {
    pub const IDLE: u8 = 0;
    pub const FIGHT: u8 = 1;
    pub const RETURN: u8 = 2;
}

/// Private: where an NPC lives. `npc` is the entity there now, or 0; it may spawn again from
/// `ready_at`.
#[spacetimedb::table(accessor = spawn)]
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The NPC kind's wire (`content/npcs.toml`).
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    pub npc: u64,
    pub ready_at: Timestamp,
}

/// Where an admin puts a spawn point.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct SpawnPlacement {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Private: an NPC's brain. It walks along a segment, from (`x`, `z`) along `yaw` at `speed` from
/// `start` to `end`.
#[spacetimedb::table(accessor = npc)]
#[derive(Clone, Debug, PartialEq)]
pub struct Npc {
    #[primary_key]
    pub entity_id: u64,
    #[unique]
    pub spawn_id: u64,
    pub kind: u8,
    pub home_x: f32,
    pub home_z: f32,
    pub state: u8,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub speed: f32,
    pub start: Timestamp,
    pub end: Timestamp,
    /// When `transform` was last written.
    pub sent_at: Timestamp,
    pub next_wander: Timestamp,
    pub gcd_until: Timestamp,
    pub cooldowns: Vec<Cooldown>,
    /// The first character to hurt it since it was last at home, and that character's `char_id`.
    pub tagger: u64,
    pub tagger_char: u64,
}

/// Private: how much an NPC wants to fight an entity, and the damage it has taken from it.
#[spacetimedb::table(accessor = threat)]
#[derive(Clone, Debug, PartialEq)]
pub struct Threat {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub npc: u64,
    #[index(btree)]
    pub entity: u64,
    pub amount: i64,
    pub damage: i64,
}

/// Public (Near tier): a stack on a corpse. Its owner (a `char_id`, or 0 for nobody) may take it at
/// once, anyone from `free_at`.
#[spacetimedb::table(accessor = loot, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Loot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    #[index(btree)]
    pub corpse: u64,
    pub owner: u64,
    pub free_at: Timestamp,
    pub def: u16,
    pub count: u32,
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

fn stamp(micros: i64) -> Timestamp {
    Timestamp::from_micros_since_unix_epoch(micros)
}

impl Npc {
    fn segment(&self) -> Segment {
        Segment { x: self.x, z: self.z, yaw: self.yaw, speed: self.speed, start: micros(self.start), end: micros(self.end) }
    }

    fn set_segment(&mut self, s: Segment) {
        (self.x, self.z, self.yaw, self.speed, self.start, self.end) = (s.x, s.z, s.yaw, s.speed, stamp(s.start), stamp(s.end));
    }

    fn home(&self) -> [f32; 2] {
        [self.home_x, self.home_z]
    }

    fn ready(&self, skill: u16, t: i64) -> bool {
        t >= micros(self.gcd_until) && self.cooldowns.iter().all(|c| c.skill != skill || micros(c.ready_at) <= t)
    }
}

pub fn is_npc(ctx: &ReducerContext, entity_id: u64) -> bool {
    ctx.db.npc().entity_id().find(entity_id).is_some()
}

fn ground_y(terrain: &mut Terrain, x: f32, z: f32) -> f32 {
    terrain.ground(x, z).map_or(0.0, |g| g.lo)
}

/// Above the sea, or where there's no terrain at all (tests).
fn dry(terrain: &mut Terrain, x: f32, z: f32) -> bool {
    terrain.ground(x, z).is_none_or(|g| g.lo > SEA_LEVEL_M)
}

/// Where an NPC is now, in the world, on the ground.
pub fn position(ctx: &ReducerContext, entity_id: u64) -> Option<[f32; 3]> {
    let n = ctx.db.npc().entity_id().find(entity_id)?;
    let [x, z] = n.segment().at(micros(now(ctx)));
    Some([x, ground_y(&mut Terrain::new(ctx), x, z), z])
}

/// A character in the world, as the brain sees it. The dead keep NPCs awake but can't be fought.
struct Seen {
    entity: u64,
    at: [f32; 3],
    alive: bool,
}

impl Seen {
    fn flat(&self) -> [f32; 2] {
        [self.at[0], self.at[2]]
    }
}

fn characters(ctx: &ReducerContext) -> Vec<Seen> {
    ctx.db
        .presence()
        .iter()
        .map(|p| Seen { entity: p.entity_id, at: crate::ship::world_position(ctx, &p), alive: crate::combat::alive(ctx, p.entity_id) })
        .collect()
}

fn near(chars: &[Seen], at: [f32; 2], radius: f32) -> bool {
    chars.iter().any(|c| sim::npc::distance2d(c.flat(), at) <= radius)
}

/// Called by `world_tick` every `brain_ms`: wake spawn points near characters, then think for
/// every NPC.
pub fn think(ctx: &ReducerContext) {
    let at = now(ctx);
    let chars = characters(ctx);
    wake(ctx, &chars, at);
    let mut terrain = Terrain::new(ctx);
    for n in ctx.db.npc().iter().collect::<Vec<_>>() {
        step(ctx, &mut terrain, n, &chars, at);
    }
}

/// Spawn the NPC of every empty, ready spawn point with a character within `wake_m`.
fn wake(ctx: &ReducerContext, chars: &[Seen], at: Timestamp) {
    let radius = (NPC.wake_m / sim::cell::CELL_SIZE_M).ceil() as u16;
    let mut cells: Vec<u64> = chars.iter().flat_map(|c| sim::cell::window_cells(sim::cell::cell_of(sim::cell::OPEN_WORLD, c.at[0], c.at[2]), radius)).collect();
    cells.sort_unstable();
    cells.dedup();
    for cell in cells {
        for s in ctx.db.spawn().cell().filter(cell).collect::<Vec<_>>() {
            if s.npc == 0 && s.ready_at <= at && near(chars, [s.x, s.z], NPC.wake_m) {
                spawn_npc(ctx, s, at);
            }
        }
    }
}

fn spawn_npc(ctx: &ReducerContext, s: Spawn, at: Timestamp) -> Option<u64> {
    let kind = npc_kind(s.kind)?;
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, s.x, s.z);
    let e = ctx.db.entity().insert(Entity { id: 0, cell, kind: entity_kind::NPC, ref_id: u64::from(kind.wire), pack: 0 });
    let y = ground_y(&mut Terrain::new(ctx), s.x, s.z);
    ctx.db.transform().insert(Transform { entity_id: e.id, cell, parent: 0, x: s.x, y, z: s.z, yaw: s.yaw, speed: 0, mode: MoveMode::Run as u8 });
    ctx.db.vitals().insert(Vitals {
        entity_id: e.id,
        cell,
        hp: kind.hp,
        hp_max: kind.hp,
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
        level: kind.level,
        statuses: Vec::new(),
        resists: Vec::new(),
        sought: false,
    });
    let t = micros(at);
    ctx.db.npc().insert(Npc {
        entity_id: e.id,
        spawn_id: s.id,
        kind: kind.wire,
        home_x: s.x,
        home_z: s.z,
        state: state::IDLE,
        x: s.x,
        z: s.z,
        yaw: sim::ship::yaw_from_u16(s.yaw),
        speed: 0.0,
        start: at,
        end: at,
        sent_at: at,
        next_wander: stamp(t + sim::npc::wander_pause_micros(ctx.random::<u32>())),
        gcd_until: at,
        cooldowns: Vec::new(),
        tagger: 0,
        tagger_char: 0,
    });
    ctx.db.spawn().id().update(Spawn { npc: e.id, ..s });
    Some(e.id)
}

/// Remove an NPC and everything about it. Its spawn point may spawn again from `ready_at`.
fn remove(ctx: &ReducerContext, entity_id: u64, ready_at: Timestamp) {
    let Some(n) = ctx.db.npc().entity_id().find(entity_id) else { return };
    crate::combat::clear_statuses(ctx, entity_id);
    for t in ctx.db.threat().npc().filter(entity_id).collect::<Vec<_>>() {
        ctx.db.threat().id().delete(t.id);
    }
    for l in ctx.db.loot().corpse().filter(entity_id).collect::<Vec<_>>() {
        ctx.db.loot().id().delete(l.id);
    }
    ctx.db.vitals().entity_id().delete(entity_id);
    ctx.db.transform().entity_id().delete(entity_id);
    ctx.db.entity().id().delete(entity_id);
    ctx.db.npc().entity_id().delete(entity_id);
    if let Some(s) = ctx.db.spawn().id().find(n.spawn_id) {
        ctx.db.spawn().id().update(Spawn { npc: 0, ready_at, ..s });
    }
}

/// One brain step for `n`.
fn step(ctx: &ReducerContext, terrain: &mut Terrain, n: Npc, chars: &[Seen], at: Timestamp) {
    let t = micros(at);
    let (Some(kind), Some(v)) = (npc_kind(n.kind), ctx.db.vitals().entity_id().find(n.entity_id)) else {
        remove(ctx, n.entity_id, at);
        return;
    };
    if let Some(dead_at) = v.dead_at {
        if t >= micros(dead_at) + sim::npc::corpse_micros() {
            remove(ctx, n.entity_id, stamp(micros(dead_at) + sim::npc::respawn_micros(kind)));
        }
        return;
    }
    let old = n.clone();
    let mut m = n;
    let here = m.segment().at(t);
    match m.state {
        state::IDLE => {
            if !m.segment().moving(t) && !near(chars, here, NPC.sleep_m) {
                remove(ctx, m.entity_id, at);
                return;
            }
            if kind.aggro_m > 0.0 && !has_threat(ctx, m.entity_id) {
                let y = ground_y(terrain, here[0], here[1]);
                let me = [here[0], y, here[1]];
                let d = |c: &Seen| sim::combat::distance(me, c.at);
                let seen = chars.iter().filter(|c| c.alive && d(c) <= kind.aggro_m).min_by(|a, b| d(a).total_cmp(&d(b)));
                if let Some(c) = seen.filter(|c| crate::combat::in_sight(ctx, me, c.at)) {
                    add_threat(ctx, m.entity_id, c.entity, 1, 0);
                }
            }
            if has_threat(ctx, m.entity_id) {
                m.state = state::FIGHT;
                fight(ctx, terrain, &mut m, kind, &v, chars, t);
            } else if !m.segment().moving(t) && t >= micros(m.next_wander) {
                let to = sim::npc::wander_point(m.home(), kind.wander_m, ctx.random::<u32>(), ctx.random::<u32>());
                let pause = sim::npc::wander_pause_micros(ctx.random::<u32>());
                if dry(terrain, to[0], to[1]) && kind.wander_m > 0.0 {
                    let speed = sim::npc::pct_of(kind.speed_mps, NPC.walk_pct) * snare(ctx, m.entity_id, at);
                    m.set_segment(sim::npc::walk(here, to, speed, 0.0, t, m.yaw));
                }
                m.next_wander = stamp(m.segment().end.max(t) + pause);
            }
        }
        state::FIGHT => fight(ctx, terrain, &mut m, kind, &v, chars, t),
        _ => {
            if !m.segment().moving(t) {
                if sim::npc::distance2d(here, m.home()) < 0.5 {
                    reset(ctx, &mut m, at);
                } else {
                    let speed = sim::npc::pct_of(kind.speed_mps, NPC.return_pct);
                    m.set_segment(sim::npc::walk(here, m.home(), speed, 0.0, t, m.yaw));
                }
            }
        }
    }
    publish(ctx, terrain, &old, &mut m, t);
    if m != old {
        ctx.db.npc().entity_id().update(m);
    }
}

/// Back at home: full HP, no statuses, no tag, idle.
fn reset(ctx: &ReducerContext, m: &mut Npc, at: Timestamp) {
    crate::combat::clear_statuses(ctx, m.entity_id);
    if let Some(v) = ctx.db.vitals().entity_id().find(m.entity_id) {
        ctx.db.vitals().entity_id().update(Vitals { hp: v.hp_max, at, combat_until: at, target: 0, statuses: Vec::new(), resists: Vec::new(), ..v });
    }
    m.state = state::IDLE;
    m.tagger = 0;
    m.tagger_char = 0;
    m.next_wander = stamp(micros(at) + sim::npc::wander_pause_micros(ctx.random::<u32>()));
}

/// The speed factor a snare leaves an NPC now (0 when stunned or knocked down).
fn snare(ctx: &ReducerContext, entity_id: u64, at: Timestamp) -> f32 {
    crate::combat::move_scale(ctx, entity_id, at, at)
}

/// Stand where it is now, facing `yaw`.
fn halt(m: &mut Npc, t: i64, yaw: f32) {
    let [x, z] = m.segment().at(t);
    m.set_segment(Segment::still(x, z, yaw, t));
}

/// Give up: forget the threat, stop casting and go home faster than it came.
fn go_home(ctx: &ReducerContext, m: &mut Npc, kind: &NpcKindData, t: i64) {
    for row in ctx.db.threat().npc().filter(m.entity_id).collect::<Vec<_>>() {
        ctx.db.threat().id().delete(row.id);
    }
    crate::combat::cancel(ctx, m.entity_id);
    set_target(ctx, m.entity_id, 0);
    m.state = state::RETURN;
    let here = m.segment().at(t);
    m.set_segment(sim::npc::walk(here, m.home(), sim::npc::pct_of(kind.speed_mps, NPC.return_pct), 0.0, t, m.yaw));
}

fn set_target(ctx: &ReducerContext, entity_id: u64, target: u64) {
    if let Some(v) = ctx.db.vitals().entity_id().find(entity_id).filter(|v| v.target != target) {
        ctx.db.vitals().entity_id().update(Vitals { target, ..v });
    }
}

fn has_threat(ctx: &ReducerContext, npc: u64) -> bool {
    ctx.db.threat().npc().filter(npc).next().is_some()
}

/// The fight: drop those gone from the threat list, leash, then cast, strike or chase.
fn fight(ctx: &ReducerContext, terrain: &mut Terrain, m: &mut Npc, kind: &NpcKindData, v: &Vitals, chars: &[Seen], t: i64) {
    let mut rows: Vec<Threat> = ctx.db.threat().npc().filter(m.entity_id).collect();
    rows.sort_by_key(|r| r.id);
    let fightable = |r: &Threat| chars.iter().any(|c| c.alive && c.entity == r.entity);
    for r in rows.iter().filter(|r| !fightable(r)) {
        ctx.db.threat().id().delete(r.id);
    }
    rows.retain(fightable);
    let here = m.segment().at(t);
    let pairs: Vec<(u64, i64)> = rows.iter().map(|r| (r.entity, r.amount)).collect();
    let target = sim::npc::top_threat(&pairs).and_then(|id| chars.iter().find(|c| c.entity == id));
    // Too far from home, or chasing someone who is: give up.
    let leashed = |c: &&Seen| sim::npc::distance2d(here, m.home()) <= NPC.leash_m && sim::npc::distance2d(c.flat(), m.home()) <= NPC.leash_m;
    let Some(target) = target.filter(leashed) else {
        go_home(ctx, m, kind, t);
        return;
    };
    set_target(ctx, m.entity_id, target.entity);
    let at = stamp(t);
    let facing = sim::npc::yaw_to(here, target.flat(), m.yaw);
    if crate::combat::controlled(ctx, m.entity_id) {
        if m.segment().moving(t) {
            let yaw = m.yaw;
            halt(m, t, yaw);
        }
        return;
    }
    let me = [here[0], ground_y(terrain, here[0], here[1]), here[1]];
    if v.cast_skill != 0 {
        if t >= micros(v.cast_end) {
            finish_cast(ctx, m, v, me);
        }
        return;
    }
    let skills = sim::npc::skills_of(kind);
    let distance = sim::combat::distance(me, target.at);
    let pick = sim::npc::pick_skill(&skills, distance, |w| m.ready(w, t)).filter(|_| crate::combat::in_sight(ctx, me, target.at));
    if let Some(s) = pick {
        halt(m, t, facing);
        use_skill(ctx, m, s, target.entity, me, at);
        return;
    }
    let approach = sim::npc::approach_m(&skills);
    if sim::npc::distance2d(here, target.flat()) <= approach + 0.3 {
        // In reach but waiting on a cooldown: stand and face it.
        if m.segment().moving(t) || (m.yaw - facing).abs() > 0.2 {
            halt(m, t, facing);
        }
        return;
    }
    let speed = kind.speed_mps * snare(ctx, m.entity_id, at);
    let plan = sim::npc::walk(here, target.flat(), speed, approach, t, m.yaw);
    // A chase stops at the shore.
    let ahead = plan.at(t + 1_000_000);
    if !dry(terrain, ahead[0], ahead[1]) {
        if m.segment().moving(t) {
            halt(m, t, facing);
        }
        return;
    }
    let current = m.segment();
    let off = sim::npc::distance2d(current.end_point(), plan.end_point());
    if !current.moving(t) || off > 1.0 || (current.speed - plan.speed).abs() > 0.1 {
        m.set_segment(plan);
    }
}

/// Start skill `s` on `target`: an instant one leaves at once, a timed one is cast.
fn use_skill(ctx: &ReducerContext, m: &mut Npc, s: &'static SkillData, target: u64, me: [f32; 3], at: Timestamp) {
    let t = micros(at);
    m.gcd_until = stamp(t + sim::combat::gcd_micros());
    m.cooldowns.retain(|c| c.skill != s.wire && micros(c.ready_at) > t);
    if s.cooldown_ms > 0 {
        m.cooldowns.push(Cooldown { skill: s.wire, ready_at: stamp(t + sim::combat::cooldown_micros(s)) });
    }
    if s.cast_ms == 0 {
        crate::combat::release(ctx, m.entity_id, me, target, s);
        return;
    }
    if let Some(v) = ctx.db.vitals().entity_id().find(m.entity_id) {
        let end = stamp(t + sim::combat::cast_micros(s));
        ctx.db.vitals().entity_id().update(Vitals { cast_skill: s.wire, cast_target: target, cast_start: at, cast_end: end, ..v });
    }
}

/// A timed cast is due: it lands if the target is still alive, in range (with the leeway) and in
/// sight; otherwise it fizzles.
fn finish_cast(ctx: &ReducerContext, m: &Npc, v: &Vitals, me: [f32; 3]) {
    let (skill, target) = (v.cast_skill, v.cast_target);
    crate::combat::cancel(ctx, m.entity_id);
    let Some(s) = sim::combat::skill(skill) else { return };
    let Some(them) = crate::combat::world_at(ctx, target) else { return };
    let alive = crate::combat::alive(ctx, target);
    let check = sim::combat::check_finish(s, me, &sim::combat::Target { at: them, alive, is_caster: false });
    if check.is_ok() && crate::combat::in_sight(ctx, me, them) {
        crate::combat::release(ctx, m.entity_id, me, target, s);
    }
}

/// Write `transform` (and the cells that follow it) when the segment changed, when it has just
/// stopped, or on the heartbeat while moving.
fn publish(ctx: &ReducerContext, terrain: &mut Terrain, old: &Npc, m: &mut Npc, t: i64) {
    let Some(tr) = ctx.db.transform().entity_id().find(m.entity_id) else { return };
    let seg = m.segment();
    let changed = seg != old.segment();
    let stopped = !seg.moving(t) && tr.speed != 0;
    let beat = seg.moving(t) && t - micros(m.sent_at) >= sim::npc::heartbeat_micros();
    if !(changed || stopped || beat) {
        return;
    }
    let [x, z] = seg.at(t);
    let y = ground_y(terrain, x, z);
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    if cell != tr.cell {
        if let Some(e) = ctx.db.entity().id().find(m.entity_id) {
            ctx.db.entity().id().update(Entity { cell, ..e });
        }
        crate::combat::recell(ctx, m.entity_id, cell);
    }
    let speed = if seg.moving(t) { (seg.speed * 100.0).round() as i16 } else { 0 };
    ctx.db.transform().entity_id().update(Transform { cell, x, y, z, yaw: sim::ship::yaw_to_u16(seg.yaw), speed, ..tr });
    m.sent_at = stamp(t);
}

/// Add `amount` threat and `damage` for `entity` on NPC `npc`.
fn add_threat(ctx: &ReducerContext, npc: u64, entity: u64, amount: i64, damage: i64) {
    match ctx.db.threat().npc().filter(npc).find(|r| r.entity == entity) {
        Some(r) => {
            ctx.db.threat().id().update(Threat { amount: r.amount + amount, damage: r.damage + damage, ..r });
        }
        None => {
            ctx.db.threat().insert(Threat { id: 0, npc, entity, amount, damage });
        }
    }
}

/// `source` did something to NPC `target` worth `amount` threat, `damage` of it damage. A character
/// is the first to tag it; an idle NPC starts to fight, and a returning one takes no notice.
pub fn provoked(ctx: &ReducerContext, target: u64, source: u64, amount: i64, damage: i64) {
    let Some(n) = ctx.db.npc().entity_id().find(target).filter(|n| n.state != state::RETURN) else { return };
    let Some(p) = ctx.db.presence().entity_id().find(source) else { return };
    add_threat(ctx, target, source, amount, damage);
    if n.tagger == 0 || n.state == state::IDLE {
        let (tagger, tagger_char) = if n.tagger == 0 { (source, p.char_id) } else { (n.tagger, n.tagger_char) };
        ctx.db.npc().entity_id().update(Npc { state: state::FIGHT, tagger, tagger_char, ..n });
    }
}

/// `healer` healed `target` by `healed`: every NPC with threat on `target` gains some on the healer.
pub fn healed(ctx: &ReducerContext, healer: u64, target: u64, healed: i32) {
    let amount = sim::npc::heal_threat(healed);
    if amount == 0 || healer == target {
        return;
    }
    for npc in ctx.db.threat().entity().filter(target).map(|r| r.npc).collect::<Vec<_>>() {
        provoked(ctx, npc, healer, amount, 0);
    }
}

/// Drop `entity` from every threat list: it died or left the world.
pub fn forget(ctx: &ReducerContext, entity: u64) {
    for r in ctx.db.threat().entity().filter(entity).collect::<Vec<_>>() {
        ctx.db.threat().id().delete(r.id);
    }
}

/// Called by `combat::die` once NPC `entity_id`'s vitals say it is dead: it stops; the tagger owns
/// the loot with enough of the damage and gets the experience, or else the killer does.
pub fn died(ctx: &ReducerContext, entity_id: u64, killer: u64) {
    let Some(mut m) = ctx.db.npc().entity_id().find(entity_id) else { return };
    let Some(kind) = npc_kind(m.kind) else { return };
    let at = now(ctx);
    let t = micros(at);
    let rows: Vec<Threat> = ctx.db.threat().npc().filter(entity_id).collect();
    let total: i64 = rows.iter().map(|r| r.damage).sum();
    // The tagger's party fights for it: their damage counts towards the loot as the tagger's.
    let party = crate::party::mates(ctx, m.tagger_char);
    let ours = |e: u64| e == m.tagger || ctx.db.presence().entity_id().find(e).is_some_and(|p| party.contains(&p.char_id));
    let tagged: i64 = rows.iter().filter(|r| m.tagger != 0 && ours(r.entity)).map(|r| r.damage).sum();
    let owner = if m.tagger != 0 && sim::npc::owns_loot(tagged, total) { m.tagger_char } else { 0 };
    for r in rows {
        ctx.db.threat().id().delete(r.id);
    }
    let in_world = ctx.db.presence().entity_id().find(m.tagger).is_some_and(|p| p.char_id == m.tagger_char);
    let earner = if owner != 0 && in_world { m.tagger } else { killer };
    crate::combat::reward(ctx, earner, entity_id, kind.level);
    crate::quests::killed(ctx, earner, entity_id, kind.wire);
    let cell = ctx.db.vitals().entity_id().find(entity_id).map_or(0, |v| v.cell);
    let free_at = stamp(t + sim::npc::free_micros());
    for line in kind.loot {
        let count = sim::npc::loot_count(line, ctx.random::<u32>());
        if count > 0 {
            ctx.db.loot().insert(Loot { id: 0, cell, corpse: entity_id, owner, free_at, def: line.item, count });
        }
    }
    let old = m.clone();
    let yaw = m.yaw;
    halt(&mut m, t, yaw);
    m.state = state::IDLE;
    publish(ctx, &mut Terrain::new(ctx), &old, &mut m, t);
    ctx.db.npc().entity_id().update(m);
}

/// Take what we may from a corpse into the bag.
#[spacetimedb::reducer]
pub fn loot_corpse(ctx: &ReducerContext, corpse: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    let rows: Vec<Loot> = ctx.db.loot().corpse().filter(corpse).collect();
    let there = position(ctx, corpse).filter(|_| !rows.is_empty()).ok_or("nothing to loot")?;
    let me = crate::ship::world_position(ctx, &p);
    if sim::npc::distance2d([me[0], me[2]], [there[0], there[2]]) > NPC.loot_m || (me[1] - there[1]).abs() > NPC.loot_m {
        return Err("too far away".into());
    }
    let at = now(ctx);
    if rows.iter().any(|r| r.owner != p.char_id && !crate::party::together(ctx, r.owner, p.char_id) && at < r.free_at) {
        return Err("that isn't yours to loot yet".into());
    }
    let mut took = 0;
    for r in rows {
        let n = crate::items::room_for(ctx, p.char_id, r.def).min(r.count);
        if n == 0 {
            continue;
        }
        crate::items::grant(ctx, p.char_id, r.def, n)?;
        took += 1;
        if n == r.count {
            ctx.db.loot().id().delete(r.id);
        } else {
            ctx.db.loot().id().update(Loot { count: r.count - n, ..r });
        }
    }
    if took == 0 {
        return Err("your bag is full".into());
    }
    Ok(())
}

fn check_placements(spawns: &[SpawnPlacement]) -> Result<(), String> {
    let half = sim::cell::WORLD_HALF_M;
    if let Some(bad) = spawns.iter().find(|s| npc_kind(s.kind).is_none() || ![s.x, s.y, s.z].iter().all(|v| v.is_finite()) || s.x.abs() >= half || s.z.abs() >= half) {
        return Err(format!("bad spawn point {bad:?}: unknown kind, a non-finite position or off the map"));
    }
    Ok(())
}

fn put_spawn(ctx: &ReducerContext, s: &SpawnPlacement) -> Spawn {
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, s.x, s.z);
    ctx.db.spawn().insert(Spawn { id: 0, cell, kind: s.kind, x: s.x, y: s.y, z: s.z, yaw: s.yaw, npc: 0, ready_at: Timestamp::UNIX_EPOCH })
}

/// Remove spawn points by id, with their NPCs. Unknown ids are ignored.
fn remove_spawns(ctx: &ReducerContext, ids: &[u64]) {
    let at = now(ctx);
    for &id in ids {
        if let Some(s) = ctx.db.spawn().id().find(id) {
            remove(ctx, s.npc, at);
            ctx.db.spawn().id().delete(id);
        }
    }
}

/// Add spawn points.
#[spacetimedb::reducer]
pub fn admin_put_spawns(ctx: &ReducerContext, spawns: Vec<SpawnPlacement>) -> Result<(), String> {
    require_admin(ctx)?;
    check_placements(&spawns)?;
    for s in &spawns {
        put_spawn(ctx, s);
    }
    log::info!("{} spawn points placed", spawns.len());
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_remove_spawns(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    remove_spawns(ctx, &ids);
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_spawns(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let ids: Vec<u64> = ctx.db.spawn().iter().map(|s| s.id).collect();
    remove_spawns(ctx, &ids);
    log::info!("{} spawn points cleared", ids.len());
    Ok(())
}

/// Dev builds only: a spawn point for `kind` (a content id) on the ground at (x, z), with its NPC
/// there at once.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_spawn_npc(ctx: &ReducerContext, kind: String, x: f32, z: f32) -> Result<(), String> {
    let k = sim::npc::npc_kind_by_id(&kind).ok_or_else(|| format!("no NPC kind {kind:?}"))?;
    let y = ground_y(&mut Terrain::new(ctx), x, z);
    let placement = SpawnPlacement { kind: k.wire, x, y, z, yaw: 0 };
    check_placements(std::slice::from_ref(&placement))?;
    let s = put_spawn(ctx, &placement);
    let id = spawn_npc(ctx, s, now(ctx)).ok_or("no such kind")?;
    log::info!("{} {id} at ({x:.1}, {z:.1})", k.name);
    Ok(())
}

/// Dev builds only: remove an NPC and its spawn point.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_remove_npc(ctx: &ReducerContext, entity_id: u64) -> Result<(), String> {
    let n = ctx.db.npc().entity_id().find(entity_id).ok_or("no such NPC")?;
    remove_spawns(ctx, &[n.spawn_id]);
    Ok(())
}
