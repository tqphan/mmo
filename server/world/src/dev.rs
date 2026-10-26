//! Dev builds only: shortcuts for bots and load tests. Production builds compile all of this out.

use spacetimedb::{ReducerContext, Table};

use crate::session::{Entity, Presence, Transform, entity, presence, transform};
use crate::time::now;

/// Move the caller's character anywhere, as a GM would. Load tests use it to spread bots out.
#[spacetimedb::reducer]
pub fn dev_teleport(ctx: &ReducerContext, x: f32, z: f32) -> Result<(), String> {
    if !x.is_finite() || !z.is_finite() {
        return Err("non-finite position".into());
    }
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let t = ctx.db.transform().entity_id().find(p.entity_id).ok_or("presence without transform")?;
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    if let Some(e) = ctx.db.entity().id().find(p.entity_id) {
        ctx.db.entity().id().update(Entity { cell, ..e });
    }
    ctx.db.transform().entity_id().update(Transform { cell, x, z, speed: 0, ..t });
    // Moves are measured from here and now.
    ctx.db.presence().identity().update(Presence { last_move_at: now(ctx), ..p });
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

#[spacetimedb::reducer]
pub fn dev_swing(ctx: &ReducerContext, target: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let me = ctx.db.transform().entity_id().find(p.entity_id).ok_or("presence without transform")?;
    let them = ctx.db.transform().entity_id().find(target).ok_or("no such target")?;
    let (dx, dz) = (them.x - me.x, them.z - me.z);
    if target == p.entity_id || dx * dx + dz * dz > SWING_RANGE_M * SWING_RANGE_M {
        return Err("out of range".into());
    }
    ctx.db.combat_event().insert(CombatEvent {
        cell: me.cell,
        source: p.entity_id,
        target,
        skill_id: 1,
        amount: 100,
        flags: 0,
    });
    Ok(())
}
