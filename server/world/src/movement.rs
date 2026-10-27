//! Movement events from client prediction, checked against speed limits and the terrain (PLAN §4.2,
//! docs/design/terrain.md), in the world or on a ship's deck (docs/design/ships.md).
//!
//! Accepted moves update the private `presence` row at once, so the next check and the mover's own
//! client see them immediately. Everyone else sees them when `world_tick` publishes the batch to
//! `transform`, one transaction per tick instead of one per move (docs/spikes/mass-fight.md).

use spacetimedb::{ReducerContext, Table, Timestamp};
use sim::movement::{Anchor, Footing, MoveMode, Verdict, Vertical, allowed_distance, check_distance, check_vertical, settle};
use sim::terrain::Ground;

use crate::session::{Entity, Presence, Transform, entity, presence, transform};
use crate::ship::Deck;
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
/// every event the client sends; anything not newer than the last accepted one is dropped. `parent`
/// is the frame the position is in: 0 for the world, or the ship whose deck it is on.
#[spacetimedb::reducer]
#[allow(clippy::too_many_arguments)] // a reducer's arguments are its wire format
pub fn move_event(
    ctx: &ReducerContext,
    seq: u32,
    parent: u64,
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
    let entity_id = p.entity_id;
    let mut terrain = Terrain::new(ctx);
    // A ship that has gone takes its frame with it; its passengers were moved off already.
    let from = Frame::of(ctx, p.parent).unwrap_or(Frame::World);
    let to = [x, y, z];
    let scale = crate::trade::speed_scale(ctx, p.char_id, p.last_move_at);
    let move_ = Move { p: &p, to, dt: secs_between(p.last_move_at, at), since_contact: secs_between(p.contact_at, at), mode, scale };

    match Frame::of(ctx, parent).ok_or_else(|| "no such ship".to_string()).and_then(|f| judge(&mut terrain, &move_, from, f)) {
        Ok((footing, anchor)) => {
            let (anchor, touched) = anchor.after(to, footing);
            ctx.db.presence().identity().update(Presence {
                parent,
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
            crate::gathering::moved(ctx, p.char_id, parent, to);
            crate::crafting::moved(ctx, p.char_id, parent, to);
            // Walking away from the helm, or off the ship, lets go of it.
            if let Frame::Deck(d) = from
                && (parent != p.parent || sim::ship::reach(to, d.hull.helm_x, d.hull.helm_z) > sim::ship::REACH_M)
            {
                crate::ship::let_go(ctx, d.ship, entity_id);
            }
        }
        Err(reason) => {
            log::warn!("move rejected for char {}: {reason}", p.char_id);
            ctx.db.suspicion().insert(Suspicion { id: 0, char_id: p.char_id, at, reason });
            // Keep the server position, settled on the ground (or deck) so the client isn't left in
            // mid-air against the fall clock; stop the entity and tell the client to snap back.
            let old_mode = MoveMode::from_u8(p.mode).unwrap_or(MoveMode::Run);
            let settled = settle(from.ground(&mut terrain, p.x, p.z), [p.x, p.y, p.z], old_mode);
            ctx.db.presence().identity().update(Presence {
                parent: if matches!(from, Frame::World) { 0 } else { p.parent },
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
    mark_moved(ctx, entity_id);
    Ok(())
}

/// Queue an entity for the next tick's publish.
pub fn mark_moved(ctx: &ReducerContext, entity_id: u64) {
    if ctx.db.moved().entity_id().find(entity_id).is_none() {
        ctx.db.moved().insert(Moved { entity_id });
    }
}

/// The frame a position is in.
#[derive(Clone, Copy)]
pub enum Frame {
    World,
    Deck(Deck),
}

impl Frame {
    /// `None` for a ship that doesn't exist.
    pub fn of(ctx: &ReducerContext, parent: u64) -> Option<Self> {
        match parent {
            0 => Some(Frame::World),
            ship => crate::ship::deck(ctx, ship).map(Frame::Deck),
        }
    }

    pub fn ground(&self, terrain: &mut Terrain, x: f32, z: f32) -> Option<Ground> {
        match self {
            Frame::World => terrain.ground(x, z),
            Frame::Deck(d) => sim::ship::deck_ground(d.hull, x, z),
        }
    }

    pub fn to_world(self, at: [f32; 3]) -> [f32; 3] {
        match self {
            Frame::World => at,
            Frame::Deck(d) => d.pose().to_world(at),
        }
    }

    fn ship_speed(&self) -> f32 {
        match self {
            Frame::World => 0.0,
            Frame::Deck(d) => d.motion.speed.abs(),
        }
    }

    fn ship(&self) -> u64 {
        match self {
            Frame::World => 0,
            Frame::Deck(d) => d.ship,
        }
    }
}

/// A move as `judge` sees it.
struct Move<'a> {
    p: &'a Presence,
    to: [f32; 3],
    dt: f32,
    since_contact: f32,
    mode: MoveMode,
    /// The speed limit's factor: below 1 while carrying a pack (docs/design/trade.md).
    scale: f32,
}

/// Check a move horizontally, then its height. Within one frame both checks run in that frame;
/// changing frame is checked in world coordinates with allowance for the ships' speed
/// (docs/design/ships.md). `Ok` carries the footing it ends on and the anchor it was measured
/// from; `Err` says why it was refused, for the suspicion log.
fn judge(terrain: &mut Terrain, m: &Move, from: Frame, to_frame: Frame) -> Result<(Footing, Anchor), String> {
    let (p, to, mode) = (m.p, m.to, m.mode);
    let here = [p.x, p.y, p.z];
    let limit = mode.data().max_speed_mps * m.scale;
    let verdict = if from.ship() == to_frame.ship() {
        check_distance(here, to, allowed_distance(limit, m.dt))
    } else {
        let speed = from.ship_speed().max(to_frame.ship_speed());
        let allowed = sim::ship::frame_change_allowance(limit, speed, m.dt);
        check_distance(from.to_world(here), to_frame.to_world(to), allowed)
    };
    match verdict {
        Verdict::Ok => {}
        Verdict::TooFar { moved_m, allowed_m } => {
            return Err(format!("moved {moved_m:.1} m in {:.2} s as {mode:?}, allowed {allowed_m:.1} m", m.dt));
        }
        Verdict::NotFinite => return Err("non-finite position".into()),
    }
    if let Frame::Deck(d) = to_frame
        && !sim::ship::on_deck(d.hull, to[0], to[2], sim::ship::RAIL_M)
    {
        return Err(format!("off the deck of ship {} at ({:.1}, {:.1})", d.ship, to[0], to[2]));
    }
    // Boarding measures from the deck where you land; stepping off, from where you stood.
    let anchor = match (from.ship() == to_frame.ship(), to_frame) {
        (true, _) => Anchor { x: p.anchor_x, y: p.anchor_y, z: p.anchor_z },
        (false, Frame::Deck(_)) => Anchor { x: to[0], y: 0.0, z: to[2] },
        (false, Frame::World) => Anchor::at(from.to_world([p.anchor_x, p.anchor_y, p.anchor_z])),
    };
    let since_contact = if matches!(to_frame, Frame::Deck(_)) && from.ship() != to_frame.ship() { 0.0 } else { m.since_contact };
    match check_vertical(to_frame.ground(terrain, to[0], to[2]), to, mode, anchor, since_contact) {
        Vertical::Ok(footing) => Ok((footing, anchor)),
        Vertical::BelowGround { y, ground } => Err(format!("at {y:.1} m, below the ground at {ground:.1} m")),
        Vertical::TooHigh { y, ceiling } => Err(format!(
            "at {y:.1} m as {mode:?}, above the ceiling of {ceiling:.1} m ({since_contact:.1} s since contact)"
        )),
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
        // Passengers are in their ship's cell.
        let cell = match p.parent {
            0 => sim::cell::cell_of(sim::cell::OPEN_WORLD, p.x, p.z),
            ship => ctx.db.transform().entity_id().find(ship).map_or(t.cell, |s| s.cell),
        };
        if cell != t.cell
            && let Some(e) = ctx.db.entity().id().find(id)
        {
            ctx.db.entity().id().update(Entity { cell, ..e });
        }
        ctx.db.transform().entity_id().update(Transform {
            cell,
            parent: p.parent,
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
