//! Dev builds only: shortcuts for bots and load tests. Production builds compile all of this out.

use spacetimedb::{ReducerContext, Table};
use sim::movement::MoveMode;
use sim::ship::{Hull, Motion};

use crate::combat::{CombatEvent, combat_event, skill};
use crate::crime::{CrimeRecord, crime_record};
use crate::session::{Account, account, presence, transform};
use crate::ship::{deck, vehicle};
use crate::terrain::Terrain;

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
    let mode = MoveMode::from_u8(p.mode).unwrap_or(MoveMode::Run);
    crate::movement::relocate(ctx, p, x, z, mode, false);
    Ok(())
}

/// How far the stand-in swing reaches.
pub const SWING_RANGE_M: f32 = 25.0;

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
    let eye = [a[0], a[1] + crate::combat::EYE_M, a[2]];
    if !sim::terrain::line_of_sight(&mut Terrain::new(ctx), eye, [b[0], b[1] + crate::combat::CHEST_M, b[2]]) {
        return Err("no line of sight".into());
    }
    let cell = ctx.db.transform().entity_id().find(p.entity_id).map_or(0, |t| t.cell);
    ctx.db.combat_event().insert(CombatEvent { cell, source: p.entity_id, target, skill_id: skill::SWING, amount: 100, flags: 0, status: 0 });
    Ok(())
}

/// Put an unowned vehicle of any hull at (x, z), at full HP and at rest. A land hull must stand there.
#[spacetimedb::reducer]
pub fn dev_spawn_ship(ctx: &ReducerContext, hull: u8, x: f32, z: f32, yaw: f32) -> Result<(), String> {
    let kind = Hull::from_u8(hull).ok_or("no such hull")?;
    if ![x, z, yaw].iter().all(|v| v.is_finite()) {
        return Err("non-finite position".into());
    }
    let data = kind.data();
    let m = Motion { x, z, yaw: yaw.rem_euclid(std::f32::consts::TAU), speed: 0.0 };
    let mut terrain = Terrain::new(ctx);
    let y = match sim::ship::stand(data, &m, |x, z| terrain.ground(x, z)) {
        Some(y) => y,
        None if data.land => return Err("a cart can't stand there".into()),
        // Dev ships may be put in shallows, as before; they stop until moved off.
        None => sim::movement::SEA_LEVEL_M + data.deck_height_m,
    };
    crate::ship::launch(ctx, kind, m, y, 0);
    Ok(())
}

/// Remove a vehicle and its cargo. Anyone aboard is put where they stood and told to snap.
#[spacetimedb::reducer]
pub fn dev_remove_ship(ctx: &ReducerContext, ship: u64) -> Result<(), String> {
    let d = deck(ctx, ship).ok_or("no such ship")?;
    crate::ship::remove(ctx, &d);
    Ok(())
}

/// Damage any vehicle as a cannon would, through `ship::damage`: 0 HP sinks it.
#[spacetimedb::reducer]
pub fn dev_damage_vehicle(ctx: &ReducerContext, vehicle: u64, amount: i32) -> Result<(), String> {
    let v = ctx.db.vehicle().entity_id().find(vehicle).ok_or("no such vehicle")?;
    crate::ship::damage(ctx, v, amount);
    Ok(())
}

/// Set the caller's character's crime points and Notoriety, as a trial will in M2: bots and the
/// client's demo use it to be Sought (docs/design/crime.md).
#[spacetimedb::reducer]
pub fn dev_set_crime(ctx: &ReducerContext, crime_points: u32, notoriety: u32) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let reports = ctx.db.crime_record().char_id().find(p.char_id).map_or(0, |r| r.reports);
    crate::crime::record(ctx, CrimeRecord { char_id: p.char_id, owner: ctx.sender(), crime_points, notoriety, reports });
    Ok(())
}
