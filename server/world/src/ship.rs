//! Ships: the helm, cannons, and stepping ships in the world tick (docs/design/ships.md). The
//! rules themselves live in `sim::ship`; moving decks are handled by `movement`.

use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp};
use sim::ship::{Hull, HullData, Input, Motion, Pose};

use crate::combat::{CombatEvent, combat_event, flags, skill};
use crate::session::{Entity, Presence, Transform, entity, presence, transform};
use crate::terrain::Terrain;
use crate::time::now;

/// Warm and public: what anyone near a ship may know about it. Written only when these change.
#[spacetimedb::table(accessor = vehicle, public)]
pub struct Vehicle {
    #[primary_key]
    pub entity_id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The hull's wire id (`content/ships.toml`).
    pub hull: u8,
    pub hp: i32,
    /// The helmsman's settings, -100 to 100 percent; zero with nobody at the helm.
    pub throttle: i8,
    pub rudder: i8,
    /// The helmsman's entity id, or 0.
    pub helm: u64,
    /// When each mount may fire again, in the hull's mount order.
    pub cannon_ready_at: Vec<Timestamp>,
}

/// Private: the exact state the tick steps. `transform` carries a rounded copy at 5 Hz.
#[spacetimedb::table(accessor = ship_motion)]
pub struct ShipMotion {
    #[primary_key]
    pub entity_id: u64,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub speed: f32,
}

/// Private: a cannon shot in flight, due when it lands.
#[spacetimedb::table(accessor = cannon_shot, scheduled(cannon_impact))]
pub struct CannonShot {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    /// Who fired, and from which ship (which the shot can't hit).
    pub gunner: u64,
    pub ship: u64,
    /// Where it comes down, at sea level.
    pub x: f32,
    pub z: f32,
}

/// A ship as the rules see it.
#[derive(Clone, Copy)]
pub struct Deck {
    pub ship: u64,
    pub hull: &'static HullData,
    pub motion: Motion,
}

impl Deck {
    pub fn pose(&self) -> Pose {
        self.motion.pose(self.hull)
    }
}

pub fn deck(ctx: &ReducerContext, ship: u64) -> Option<Deck> {
    let m = ctx.db.ship_motion().entity_id().find(ship)?;
    let v = ctx.db.vehicle().entity_id().find(ship)?;
    let hull = Hull::from_u8(v.hull)?.data();
    Some(Deck { ship, hull, motion: Motion { x: m.x, z: m.z, yaw: m.yaw, speed: m.speed } })
}

/// A character's position in world coordinates, wherever it stands.
pub fn world_position(ctx: &ReducerContext, p: &Presence) -> [f32; 3] {
    let at = [p.x, p.y, p.z];
    match p.parent {
        0 => at,
        ship => deck(ctx, ship).map_or(at, |d| d.pose().to_world(at)),
    }
}

/// Give up the helm of `ship` if `who` holds it. The ship drifts.
pub fn let_go(ctx: &ReducerContext, ship: u64, who: u64) {
    if let Some(v) = ctx.db.vehicle().entity_id().find(ship).filter(|v| v.helm == who && who != 0) {
        ctx.db.vehicle().entity_id().update(Vehicle { helm: 0, throttle: 0, rudder: 0, ..v });
    }
}

/// The caller's presence and the deck it stands on.
fn aboard(ctx: &ReducerContext) -> Result<(Presence, Deck, Vehicle), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    let d = deck(ctx, p.parent).ok_or("not aboard a ship")?;
    let v = ctx.db.vehicle().entity_id().find(d.ship).ok_or("not aboard a ship")?;
    Ok((p, d, v))
}

#[spacetimedb::reducer]
pub fn take_helm(ctx: &ReducerContext) -> Result<(), String> {
    let (p, d, v) = aboard(ctx)?;
    if sim::ship::reach([p.x, p.y, p.z], d.hull.helm_x, d.hull.helm_z) > sim::ship::REACH_M {
        return Err("too far from the helm".into());
    }
    let held = v.helm != 0
        && v.helm != p.entity_id
        && ctx.db.presence().entity_id().find(v.helm).is_some_and(|h| h.parent == d.ship);
    if held {
        return Err("someone else has the helm".into());
    }
    ctx.db.vehicle().entity_id().update(Vehicle { helm: p.entity_id, ..v });
    Ok(())
}

#[spacetimedb::reducer]
pub fn leave_helm(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let_go(ctx, p.parent, p.entity_id);
    Ok(())
}

/// The helmsman sets the throttle and rudder, each -100 to 100 percent.
#[spacetimedb::reducer]
pub fn helm(ctx: &ReducerContext, throttle: i8, rudder: i8) -> Result<(), String> {
    let (p, _, v) = aboard(ctx)?;
    if v.helm != p.entity_id {
        return Err("you don't have the helm".into());
    }
    if !(Input { throttle, rudder }).valid() {
        return Err("throttle and rudder run from -100 to 100".into());
    }
    ctx.db.vehicle().entity_id().update(Vehicle { throttle, rudder, ..v });
    Ok(())
}

/// Fire cannon `mount`, aimed `yaw` radians off its facing and `pitch` radians up. The shot lands
/// when the scheduled `cannon_impact` runs.
#[spacetimedb::reducer]
pub fn fire(ctx: &ReducerContext, mount: u8, yaw: f32, pitch: f32) -> Result<(), String> {
    let (p, d, v) = aboard(ctx)?;
    let i = usize::from(mount);
    let m = d.hull.mounts.get(i).ok_or("no such cannon")?;
    if sim::ship::reach([p.x, p.y, p.z], m.x, m.z) > sim::ship::REACH_M {
        return Err("too far from the cannon".into());
    }
    let cannon = &d.hull.cannon;
    if !sim::ship::aim_ok(cannon, yaw, pitch) {
        return Err("the cannon can't aim there".into());
    }
    let at = now(ctx);
    if v.cannon_ready_at.get(i).is_some_and(|&ready| at < ready) {
        return Err("reloading".into());
    }
    let (from, world_yaw) = sim::ship::muzzle(d.hull, &d.pose(), m, yaw);
    let flight = sim::ship::cannon_flight(from, world_yaw, pitch, cannon.muzzle_speed_mps).ok_or("the cannon is under water")?;
    let lands = ctx.timestamp + TimeDuration::from_micros((flight.secs * 1e6) as i64);
    ctx.db.cannon_shot().insert(CannonShot {
        scheduled_id: 0,
        scheduled_at: lands.into(),
        gunner: p.entity_id,
        ship: d.ship,
        x: flight.landing[0],
        z: flight.landing[2],
    });
    let mut ready = v.cannon_ready_at.clone();
    ready.resize(d.hull.mounts.len(), at);
    ready[i] = at + TimeDuration::from_micros((cannon.reload_s * 1e6) as i64);
    ctx.db.vehicle().entity_id().update(Vehicle { cannon_ready_at: ready, ..v });
    Ok(())
}

/// A shot lands: it hits the nearest ship it lands on (never the one it came from), or splashes.
#[spacetimedb::reducer]
pub fn cannon_impact(ctx: &ReducerContext, shot: CannonShot) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("cannon_impact runs on the server's schedule only".into());
    }
    let landing = [shot.x, sim::movement::SEA_LEVEL_M, shot.z];
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, shot.x, shot.z);
    let target = sim::cell::window_cells(cell, 1)
        .into_iter()
        .flat_map(|c| ctx.db.vehicle().cell().filter(c))
        .filter(|v| v.entity_id != shot.ship)
        .filter_map(|v| deck(ctx, v.entity_id))
        .filter(|d| sim::ship::hits(d.hull, &d.pose(), landing))
        .min_by(|a, b| distance2(a, landing).total_cmp(&distance2(b, landing)));
    let damage = Hull::from_u8(ctx.db.vehicle().entity_id().find(shot.ship).map_or(0, |v| v.hull))
        .map_or(0, |h| h.data().cannon.damage);
    let event = |target, amount, flags| CombatEvent { cell, source: shot.gunner, target, skill_id: skill::CANNON, amount, flags };
    match target.and_then(|d| ctx.db.vehicle().entity_id().find(d.ship)) {
        Some(v) => {
            ctx.db.combat_event().insert(event(v.entity_id, damage, flags::HIT));
            ctx.db.vehicle().entity_id().update(Vehicle { hp: (v.hp - damage).max(0), ..v });
        }
        None => {
            ctx.db.combat_event().insert(event(0, 0, 0));
        }
    }
    Ok(())
}

fn distance2(d: &Deck, at: [f32; 3]) -> f32 {
    (d.motion.x - at[0]).powi(2) + (d.motion.z - at[2]).powi(2)
}

/// Called by `world_tick`: step every ship by `dt`, carry passengers into new cells, and write each
/// ship's `transform` when `publish` is set (every other tick) or it changed cell.
pub fn step_ships(ctx: &ReducerContext, dt: f32, publish: bool) {
    let ships: Vec<ShipMotion> = ctx.db.ship_motion().iter().collect();
    let mut terrain = Terrain::new(ctx);
    for s in ships {
        let Some(v) = ctx.db.vehicle().entity_id().find(s.entity_id) else { continue };
        let Some(hull) = Hull::from_u8(v.hull).map(Hull::data) else { continue };
        let was = Motion { x: s.x, z: s.z, yaw: s.yaw, speed: s.speed };
        let mut next = sim::ship::step(was, hull, Input { throttle: v.throttle, rudder: v.rudder }, dt);
        if !sim::ship::afloat(hull, &next, |x, z| terrain.ground(x, z)) {
            next = Motion { speed: 0.0, ..was };
        }
        if next != was {
            ctx.db.ship_motion().entity_id().update(ShipMotion { x: next.x, z: next.z, yaw: next.yaw, speed: next.speed, ..s });
        }
        let id = s.entity_id;
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, next.x, next.z);
        let new_cell = cell != v.cell;
        if new_cell {
            ctx.db.vehicle().entity_id().update(Vehicle { cell, ..v });
            let aboard = ctx.db.presence().parent().filter(id).map(|p| p.entity_id);
            for e in std::iter::once(id).chain(aboard) {
                if let Some(row) = ctx.db.entity().id().find(e) {
                    ctx.db.entity().id().update(Entity { cell, ..row });
                }
                if let Some(t) = ctx.db.transform().entity_id().find(e) {
                    ctx.db.transform().entity_id().update(Transform { cell, ..t });
                }
                crate::combat::recell(ctx, e, cell);
            }
        }
        if (publish || new_cell)
            && let Some(t) = ctx.db.transform().entity_id().find(id)
        {
            let shown = ship_transform(id, cell, hull, &next);
            if shown != t {
                ctx.db.transform().entity_id().update(shown);
            }
        }
    }
}

/// How a ship appears in `transform`: the deck's centre, yaw, and speed in cm/s.
pub fn ship_transform(entity_id: u64, cell: u64, hull: &HullData, m: &Motion) -> Transform {
    let pose = m.pose(hull);
    Transform {
        entity_id,
        cell,
        parent: 0,
        x: pose.x,
        y: pose.y,
        z: pose.z,
        yaw: sim::ship::yaw_to_u16(pose.yaw),
        speed: (m.speed * 100.0).round() as i16,
        mode: 0,
    }
}
