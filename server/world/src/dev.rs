//! Dev builds only: shortcuts for bots and load tests. Production builds compile all of this out.

use spacetimedb::{ReducerContext, Table};

use crate::session::{Account, Entity, Presence, Transform, account, entity, presence, transform};
use crate::terrain::Terrain;
use crate::time::now;

/// Make the caller a GM of any level, `gm::ADMIN` included. Bots use it to upload test terrain.
#[spacetimedb::reducer]
pub fn dev_set_gm(ctx: &ReducerContext, level: u8) -> Result<(), String> {
    let a = ctx.db.account().identity().find(ctx.sender()).ok_or("no account")?;
    ctx.db.account().identity().update(Account { gm_level: level, ..a });
    Ok(())
}

/// Move the caller's character anywhere, as a GM would, landing on the ground where there is
/// terrain. Load tests use it to spread bots out.
#[spacetimedb::reducer]
pub fn dev_teleport(ctx: &ReducerContext, x: f32, z: f32) -> Result<(), String> {
    if !x.is_finite() || !z.is_finite() {
        return Err("non-finite position".into());
    }
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let t = ctx.db.transform().entity_id().find(p.entity_id).ok_or("presence without transform")?;
    let mode = sim::movement::MoveMode::from_u8(p.mode).unwrap_or(sim::movement::MoveMode::Run);
    let y = sim::movement::settle(Terrain::new(ctx).ground(x, z), [x, p.y, z], mode);
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    if let Some(e) = ctx.db.entity().id().find(p.entity_id) {
        ctx.db.entity().id().update(Entity { cell, ..e });
    }
    ctx.db.transform().entity_id().update(Transform { cell, x, y, z, speed: 0, ..t });
    // Moves are measured from here and now.
    let at = now(ctx);
    ctx.db.presence().identity().update(Presence {
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

/// Stand-in for combat until M1 (docs/spikes/throughput.md): a hit broadcast to the attacker's
/// cell, shaped like PLAN §5.2's `combat_event`.
#[spacetimedb::table(accessor = combat_event, public, event)]
pub struct CombatEvent {
    pub cell: u64,
    pub source: u64,
    pub target: u64,
    pub skill_id: u32,
    pub amount: i32,
    pub flags: u16,
}

/// How far the stand-in swing reaches.
pub const SWING_RANGE_M: f32 = 25.0;
/// Sight runs from the attacker's eyes to the target's chest, both above the feet.
pub const EYE_M: f32 = 1.6;
pub const CHEST_M: f32 = 1.0;

#[spacetimedb::reducer]
pub fn dev_swing(ctx: &ReducerContext, target: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let them = ctx.db.presence().entity_id().find(target).ok_or("no such target")?;
    let (dx, dz) = (them.x - p.x, them.z - p.z);
    if target == p.entity_id || dx * dx + dz * dz > SWING_RANGE_M * SWING_RANGE_M {
        return Err("out of range".into());
    }
    let eye = [p.x, p.y + EYE_M, p.z];
    if !sim::terrain::line_of_sight(&mut Terrain::new(ctx), eye, [them.x, them.y + CHEST_M, them.z]) {
        return Err("no line of sight".into());
    }
    ctx.db.combat_event().insert(CombatEvent {
        cell: sim::cell::cell_of(sim::cell::OPEN_WORLD, p.x, p.z),
        source: p.entity_id,
        target,
        skill_id: 1,
        amount: 100,
        flags: 0,
    });
    Ok(())
}
