//! Dev builds only: shortcuts for bots and load tests. Production builds compile all of this out.

use spacetimedb::{ReducerContext, Table};
use sim::movement::{MoveMode, SEA_LEVEL_M};
use sim::ship::{Hull, Motion};

use crate::combat::{CombatEvent, combat_event, skill};
use crate::movement::mark_moved;
use crate::session::{Account, Entity, Presence, Transform, account, entity, entity_kind, presence, transform};
use crate::ship::{ShipMotion, Vehicle, cannon_shot, deck, ship_motion, vehicle};
use crate::terrain::Terrain;
use crate::time::now;

/// Make the caller a GM of any level, `gm::ADMIN` included. Bots use it to upload test terrain.
#[spacetimedb::reducer]
pub fn dev_set_gm(ctx: &ReducerContext, level: u8) -> Result<(), String> {
    let a = ctx.db.account().identity().find(ctx.sender()).ok_or("no account")?;
    ctx.db.account().identity().update(Account { gm_level: level, ..a });
    Ok(())
}

/// Move the caller's character anywhere in the world, as a GM would, landing on the ground where
/// there is terrain and off any ship. Load tests use it to spread bots out.
#[spacetimedb::reducer]
pub fn dev_teleport(ctx: &ReducerContext, x: f32, z: f32) -> Result<(), String> {
    if !x.is_finite() || !z.is_finite() {
        return Err("non-finite position".into());
    }
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let t = ctx.db.transform().entity_id().find(p.entity_id).ok_or("presence without transform")?;
    let mode = MoveMode::from_u8(p.mode).unwrap_or(MoveMode::Run);
    let y = sim::movement::settle(Terrain::new(ctx).ground(x, z), [x, p.y, z], mode);
    crate::ship::let_go(ctx, p.parent, p.entity_id);
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    if let Some(e) = ctx.db.entity().id().find(p.entity_id) {
        ctx.db.entity().id().update(Entity { cell, ..e });
    }
    ctx.db.transform().entity_id().update(Transform { cell, parent: 0, x, y, z, speed: 0, ..t });
    // Moves are measured from here and now.
    let at = now(ctx);
    ctx.db.presence().identity().update(Presence {
        parent: 0,
        last_move_at: at,
        x,
        y,
        z,
        speed: 0,
        anchor_x: x,
        anchor_y: y,
        anchor_z: z,
        contact_at: at,
        ..p
    });
    Ok(())
}

/// How far the stand-in swing reaches.
pub const SWING_RANGE_M: f32 = 25.0;
/// Sight runs from the attacker's eyes to the target's chest, both above the feet.
pub const EYE_M: f32 = 1.6;
pub const CHEST_M: f32 = 1.0;

/// Stand-in for combat until M1 (docs/spikes/throughput.md): a hit broadcast to the attacker's
/// cell. Works between decks and the world, in world coordinates.
#[spacetimedb::reducer]
pub fn dev_swing(ctx: &ReducerContext, target: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let them = ctx.db.presence().entity_id().find(target).ok_or("no such target")?;
    let (a, b) = (crate::ship::world_position(ctx, &p), crate::ship::world_position(ctx, &them));
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    if target == p.entity_id || dx * dx + dz * dz > SWING_RANGE_M * SWING_RANGE_M {
        return Err("out of range".into());
    }
    let eye = [a[0], a[1] + EYE_M, a[2]];
    if !sim::terrain::line_of_sight(&mut Terrain::new(ctx), eye, [b[0], b[1] + CHEST_M, b[2]]) {
        return Err("no line of sight".into());
    }
    let cell = ctx.db.transform().entity_id().find(p.entity_id).map_or(0, |t| t.cell);
    ctx.db.combat_event().insert(CombatEvent { cell, source: p.entity_id, target, skill_id: skill::SWING, amount: 100, flags: 0 });
    Ok(())
}

/// Put a ship on the water at full HP, at rest. Ships are built at shipyards from M1.
#[spacetimedb::reducer]
pub fn dev_spawn_ship(ctx: &ReducerContext, hull: u8, x: f32, z: f32, yaw: f32) -> Result<(), String> {
    let kind = Hull::from_u8(hull).ok_or("no such hull")?;
    if ![x, z, yaw].iter().all(|v| v.is_finite()) {
        return Err("non-finite position".into());
    }
    let data = kind.data();
    let m = Motion { x, z, yaw: yaw.rem_euclid(std::f32::consts::TAU), speed: 0.0 };
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    let e = ctx.db.entity().insert(Entity { id: 0, cell, kind: entity_kind::SHIP, ref_id: u64::from(hull), pack: 0 });
    ctx.db.transform().insert(crate::ship::ship_transform(e.id, cell, data, &m));
    ctx.db.vehicle().insert(Vehicle {
        entity_id: e.id,
        cell,
        hull,
        hp: data.hp,
        throttle: 0,
        rudder: 0,
        helm: 0,
        cannon_ready_at: vec![now(ctx); data.mounts.len()],
    });
    ctx.db.ship_motion().insert(ShipMotion { entity_id: e.id, x: m.x, z: m.z, yaw: m.yaw, speed: 0.0 });
    log::info!("ship {} ({}) spawned at ({x:.1}, {z:.1})", e.id, data.id);
    Ok(())
}

/// Remove a ship. Anyone aboard is put in the water where they stood, swimming, and told to snap.
#[spacetimedb::reducer]
pub fn dev_remove_ship(ctx: &ReducerContext, ship: u64) -> Result<(), String> {
    let d = deck(ctx, ship).ok_or("no such ship")?;
    let at = now(ctx);
    let mut terrain = Terrain::new(ctx);
    let aboard: Vec<Presence> = ctx.db.presence().parent().filter(ship).collect();
    for p in aboard {
        let [x, _, z] = d.pose().to_world([p.x, p.y, p.z]);
        let y = sim::movement::settle(terrain.ground(x, z), [x, SEA_LEVEL_M, z], MoveMode::Swim);
        ctx.db.presence().identity().update(Presence {
            parent: 0,
            last_move_at: at,
            corrections: p.corrections + 1,
            x,
            y,
            z,
            speed: 0,
            mode: MoveMode::Swim as u8,
            anchor_x: x,
            anchor_y: y,
            anchor_z: z,
            contact_at: at,
            ..p
        });
        mark_moved(ctx, p.entity_id);
    }
    let shots: Vec<u64> = ctx.db.cannon_shot().iter().filter(|s| s.ship == ship).map(|s| s.scheduled_id).collect();
    for id in shots {
        ctx.db.cannon_shot().scheduled_id().delete(id);
    }
    ctx.db.ship_motion().entity_id().delete(ship);
    ctx.db.vehicle().entity_id().delete(ship);
    ctx.db.transform().entity_id().delete(ship);
    ctx.db.entity().id().delete(ship);
    Ok(())
}
