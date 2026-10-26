//! Movement events from client prediction, checked against speed limits and the terrain (PLAN §4.2,
//! docs/design/terrain.md).
//!
//! Accepted moves update the private `presence` row at once, so the next check and the mover's own
//! client see them immediately. Everyone else sees them when `world_tick` publishes the batch to
//! `transform`, one transaction per tick instead of one per move (docs/spikes/mass-fight.md).

use spacetimedb::{ReducerContext, Table, Timestamp};
use sim::movement::{Anchor, Footing, MoveMode, Verdict, Vertical, check_move, check_vertical, settle};

use crate::session::{Entity, Presence, Transform, entity, presence, transform};
use crate::terrain::Terrain;
use crate::time::{now, secs_between};

/// Private log of rejected moves, for spotting cheats and tuning the limits.
#[spacetimedb::table(accessor = suspicion)]
pub struct Suspicion {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub char_id: u64,
    pub at: Timestamp,
    pub reason: String,
}

/// Private: entities whose `presence` position changed since the last tick published it.
#[spacetimedb::table(accessor = moved)]
pub struct Moved {
    #[primary_key]
    pub entity_id: u64,
}

/// One movement event: start, stop, turn, jump, mode change or heartbeat. `seq` increases with
/// every event the client sends; anything not newer than the last accepted one is dropped.
#[spacetimedb::reducer]
#[allow(clippy::too_many_arguments)] // a reducer's arguments are its wire format
pub fn move_event(
    ctx: &ReducerContext,
    seq: u32,
    x: f32,
    y: f32,
    z: f32,
    yaw: u16,
    speed: i16,
    mode: u8,
) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if seq <= p.last_seq {
        return Ok(()); // stale or duplicate; ordering is the client's job, not an error
    }
    let mode = MoveMode::from_u8(mode).ok_or("unknown movement mode")?;
    let at = now(ctx);
    let dt = secs_between(p.last_move_at, at);
    let entity_id = p.entity_id;
    let mut terrain = Terrain::new(ctx);
    let anchor = Anchor { x: p.anchor_x, y: p.anchor_y, z: p.anchor_z };
    let to = [x, y, z];

    match judge(&mut terrain, &p, to, dt, secs_between(p.contact_at, at), mode, anchor) {
        Ok(footing) => {
            let (anchor, touched) = anchor.after(to, footing);
            ctx.db.presence().identity().update(Presence {
                last_seq: seq,
                last_move_at: at,
                x,
                y,
                z,
                yaw,
                speed,
                mode: mode as u8,
                anchor_x: anchor.x,
                anchor_y: anchor.y,
                anchor_z: anchor.z,
                contact_at: if touched { at } else { p.contact_at },
                ..p
            });
        }
        Err(reason) => {
            log::warn!("move rejected for char {}: {reason}", p.char_id);
            ctx.db.suspicion().insert(Suspicion { id: 0, char_id: p.char_id, at, reason });
            // Keep the server position, settled on the ground so the client isn't left in mid-air
            // against the fall clock; stop the entity and tell the client to snap back.
            let old_mode = MoveMode::from_u8(p.mode).unwrap_or(MoveMode::Run);
            let settled = settle(terrain.ground(p.x, p.z), [p.x, p.y, p.z], old_mode);
            ctx.db.presence().identity().update(Presence {
                last_seq: seq,
                last_move_at: at,
                corrections: p.corrections + 1,
                y: settled,
                speed: 0,
                anchor_x: p.x,
                anchor_y: settled,
                anchor_z: p.z,
                contact_at: at,
                ..p
            });
        }
    }
    if ctx.db.moved().entity_id().find(entity_id).is_none() {
        ctx.db.moved().insert(Moved { entity_id });
    }
    Ok(())
}

/// Check a move horizontally, then against the terrain. `Ok` carries the footing it ends on; `Err`
/// says why it was refused, for the suspicion log.
fn judge(
    terrain: &mut Terrain,
    p: &Presence,
    to: [f32; 3],
    dt: f32,
    since_contact: f32,
    mode: MoveMode,
    anchor: Anchor,
) -> Result<Footing, String> {
    match check_move([p.x, p.y, p.z], to, dt, mode) {
        Verdict::Ok => {}
        Verdict::TooFar { moved_m, allowed_m } => {
            return Err(format!("moved {moved_m:.1} m in {dt:.2} s as {mode:?}, allowed {allowed_m:.1} m"));
        }
        Verdict::NotFinite => return Err("non-finite position".into()),
    }
    match check_vertical(terrain.ground(to[0], to[2]), to, mode, anchor, since_contact) {
        Vertical::Ok(footing) => Ok(footing),
        Vertical::BelowGround { y, ground } => Err(format!("at {y:.1} m, below the ground at {ground:.1} m")),
        Vertical::TooHigh { y, ceiling } => {
            Err(format!("at {y:.1} m as {mode:?}, above the ceiling of {ceiling:.1} m ({since_contact:.1} s since contact)"))
        }
    }
}

/// Called by `world_tick`: copy every moved position from `presence` into the public `transform`,
/// moving entities between cells as needed. Returns how many it published.
pub fn publish_moves(ctx: &ReducerContext) -> usize {
    let ids: Vec<u64> = ctx.db.moved().iter().map(|m| m.entity_id).collect();
    for &id in &ids {
        ctx.db.moved().entity_id().delete(id);
        let (Some(p), Some(t)) = (ctx.db.presence().entity_id().find(id), ctx.db.transform().entity_id().find(id))
        else {
            continue;
        };
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, p.x, p.z);
        if cell != t.cell
            && let Some(e) = ctx.db.entity().id().find(id)
        {
            ctx.db.entity().id().update(Entity { cell, ..e });
        }
        ctx.db.transform().entity_id().update(Transform {
            cell,
            x: p.x,
            y: p.y,
            z: p.z,
            yaw: p.yaw,
            speed: p.speed,
            mode: p.mode,
            ..t
        });
    }
    ids.len()
}
