//! Movement events from client prediction, checked against speed limits (PLAN §4.2).

use spacetimedb::{ReducerContext, Table, Timestamp};
use sim::movement::{MoveMode, Verdict, check_move};

use crate::session::{Presence, Transform, entity, presence, transform};
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
    let t = ctx.db.transform().entity_id().find(p.entity_id).ok_or("presence without transform")?;
    let at = now(ctx);
    let dt = secs_between(p.last_move_at, at);

    match check_move([t.x, t.y, t.z], [x, y, z], dt, mode) {
        Verdict::Ok => {
            let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
            if cell != t.cell
                && let Some(e) = ctx.db.entity().id().find(p.entity_id)
            {
                ctx.db.entity().id().update(crate::session::Entity { cell, ..e });
            }
            ctx.db.transform().entity_id().update(Transform { cell, x, y, z, yaw, speed, mode: mode as u8, ..t });
            ctx.db.presence().identity().update(Presence { last_seq: seq, last_move_at: at, ..p });
        }
        bad => {
            let reason = match bad {
                Verdict::TooFar { moved_m, allowed_m } => {
                    format!("moved {moved_m:.1} m in {dt:.2} s as {mode:?}, allowed {allowed_m:.1} m")
                }
                _ => "non-finite position".to_string(),
            };
            log::warn!("move rejected for char {}: {reason}", p.char_id);
            ctx.db.suspicion().insert(Suspicion { id: 0, char_id: p.char_id, at, reason });
            // Keep the server position, stop the entity and tell the client to snap back.
            ctx.db.transform().entity_id().update(Transform { speed: 0, ..t });
            ctx.db.presence().identity().update(Presence {
                last_seq: seq,
                last_move_at: at,
                corrections: p.corrections + 1,
                ..p
            });
        }
    }
    Ok(())
}
