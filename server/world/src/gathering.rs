//! Gathering (docs/design/gathering.md): resource nodes in the world, timed gathers that cost labor
//! and yield items, and nodes that regrow from a timestamp. The rules themselves live in
//! `sim::gather`.

use spacetimedb::{Identity, ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, Timestamp};
use sim::gather::{NodeData, gather_micros, in_reach, moved_off, node, ready, regrown_at, yield_for};
use sim::movement::MoveMode;

use crate::session::{presence, require_admin};
use crate::time::now;

/// Static and public: one node. Written when someone starts or stops gathering it, and when it's
/// taken. Readiness is `ready_at`, compared with game time; nothing ticks.
#[spacetimedb::table(accessor = resource_node, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceNode {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The node kind's wire (`content/gathering.toml`).
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    /// It can be gathered from this game time on.
    pub ready_at: Timestamp,
    /// The entity gathering it, or 0.
    pub gatherer: u64,
    /// When the current gather finishes, in game time; meaningless while `gatherer` is 0.
    pub done_at: Timestamp,
}

/// Where an admin puts a node.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct NodePlacement {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Private: a gather under way, due when it finishes. One per character at most.
#[spacetimedb::table(accessor = gather_job, scheduled(gather_done))]
pub struct GatherJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    #[unique]
    pub char_id: u64,
    pub identity: Identity,
    pub entity_id: u64,
    #[index(btree)]
    pub node: u64,
    /// Where the character stood when it started.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// The yield, rolled at the start.
    pub count: u32,
}

fn kind_of(n: &ResourceNode) -> Result<&'static NodeData, String> {
    node(n.kind).ok_or_else(|| "no such node".into())
}

/// Start gathering `node_id`. It finishes in `gather_done` unless the gatherer moves away first.
#[spacetimedb::reducer]
pub fn gather(ctx: &ReducerContext, node_id: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if p.parent != 0 {
        return Err("can't gather from a ship".into());
    }
    if p.mode == MoveMode::Swim as u8 {
        return Err("can't gather while swimming".into());
    }
    if busy(ctx, p.char_id) {
        return Err("already gathering".into());
    }
    if crate::crafting::busy(ctx, p.char_id) {
        return Err("already crafting".into());
    }
    let n = ctx.db.resource_node().id().find(node_id).ok_or("no such node")?;
    let kind = kind_of(&n)?;
    let at = [p.x, p.y, p.z];
    if !in_reach([n.x, n.y, n.z], at) {
        return Err("out of reach".into());
    }
    let t = now(ctx);
    if !ready(n.ready_at.to_micros_since_unix_epoch(), t.to_micros_since_unix_epoch()) {
        return Err("nothing to gather here yet".into());
    }
    if n.gatherer != 0 {
        return Err("someone else is gathering this".into());
    }
    let have = crate::labor::available(ctx, ctx.sender())?;
    if have < kind.labor {
        return Err(format!("not enough labor: have {have}, need {}", kind.labor));
    }
    let count = yield_for(kind, ctx.random::<u32>());
    let room = crate::items::room_for(ctx, p.char_id, kind.item);
    if room < count {
        return Err(format!("bag is full: room for {room} of {count}"));
    }
    let took = TimeDuration::from_micros(gather_micros(kind));
    ctx.db.gather_job().insert(GatherJob {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + took).into(),
        char_id: p.char_id,
        identity: ctx.sender(),
        entity_id: p.entity_id,
        node: n.id,
        x: at[0],
        y: at[1],
        z: at[2],
        count,
    });
    ctx.db.resource_node().id().update(ResourceNode { gatherer: p.entity_id, done_at: t + took, ..n });
    Ok(())
}

/// A gather is due: grant the yield, spend the labor and take the node, or, if anything has
/// changed since it started, do none of that and free the node. It never fails, since nobody
/// would hear about it; the reason goes to the log.
#[spacetimedb::reducer]
pub fn gather_done(ctx: &ReducerContext, job: GatherJob) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("gather_done runs on the server's schedule only".into());
    }
    let Some(n) = ctx.db.resource_node().id().find(job.node).filter(|n| n.gatherer == job.entity_id) else {
        log::info!("gather by char {} ended: node {} gone or no longer theirs", job.char_id, job.node);
        return Ok(());
    };
    match finish(ctx, &job, &n) {
        Ok(kind) => {
            let ready_at = Timestamp::from_micros_since_unix_epoch(regrown_at(kind, now(ctx).to_micros_since_unix_epoch()));
            ctx.db.resource_node().id().update(ResourceNode { gatherer: 0, ready_at, ..n });
        }
        Err(e) => {
            log::info!("gather by char {} at node {} failed: {e}", job.char_id, job.node);
            ctx.db.resource_node().id().update(ResourceNode { gatherer: 0, ..n });
        }
    }
    Ok(())
}

/// Check the gatherer is still there, then spend and grant. On `Err` nothing has changed.
fn finish(ctx: &ReducerContext, job: &GatherJob, n: &ResourceNode) -> Result<&'static NodeData, String> {
    let p = ctx.db.presence().identity().find(job.identity).filter(|p| p.entity_id == job.entity_id).ok_or("left the world")?;
    if p.parent != 0 || moved_off([job.x, job.y, job.z], [p.x, p.y, p.z]) {
        return Err("moved away".into());
    }
    let kind = kind_of(n)?;
    let room = crate::items::room_for(ctx, p.char_id, kind.item);
    if room < job.count {
        return Err(format!("bag is full: room for {room} of {}", job.count));
    }
    crate::labor::spend(ctx, job.identity, kind.labor)?;
    crate::items::grant(ctx, p.char_id, kind.item, job.count)?;
    Ok(kind)
}

/// `char_id` is gathering.
pub fn busy(ctx: &ReducerContext, char_id: u64) -> bool {
    ctx.db.gather_job().char_id().find(char_id).is_some()
}

/// Stop `char_id`'s gather, if it has one, and free its node. Costs nothing.
pub fn cancel(ctx: &ReducerContext, char_id: u64) {
    let Some(job) = ctx.db.gather_job().char_id().find(char_id) else { return };
    ctx.db.gather_job().scheduled_id().delete(job.scheduled_id);
    if let Some(n) = ctx.db.resource_node().id().find(job.node).filter(|n| n.gatherer == job.entity_id) {
        ctx.db.resource_node().id().update(ResourceNode { gatherer: 0, ..n });
    }
}

/// Called by `move_event` for every accepted move: a gatherer that steps away or boards a ship
/// stops gathering.
pub fn moved(ctx: &ReducerContext, char_id: u64, parent: u64, at: [f32; 3]) {
    let Some(job) = ctx.db.gather_job().char_id().find(char_id) else { return };
    if parent != 0 || moved_off([job.x, job.y, job.z], at) {
        cancel(ctx, char_id);
    }
}

#[spacetimedb::reducer]
pub fn cancel_gather(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    cancel(ctx, p.char_id);
    Ok(())
}

/// Add nodes, ready to gather now.
#[spacetimedb::reducer]
pub fn admin_put_nodes(ctx: &ReducerContext, nodes: Vec<NodePlacement>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = nodes.iter().find(|n| node(n.kind).is_none() || ![n.x, n.y, n.z].iter().all(|v| v.is_finite())) {
        return Err(format!("bad node {bad:?}: unknown kind or a non-finite position"));
    }
    let half = sim::cell::WORLD_HALF_M;
    if let Some(bad) = nodes.iter().find(|n| n.x.abs() >= half || n.z.abs() >= half) {
        return Err(format!("bad node {bad:?}: off the map"));
    }
    let t = now(ctx);
    for n in &nodes {
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, n.x, n.z);
        ctx.db.resource_node().insert(ResourceNode {
            id: 0,
            cell,
            kind: n.kind,
            x: n.x,
            y: n.y,
            z: n.z,
            yaw: n.yaw,
            ready_at: t,
            gatherer: 0,
            done_at: t,
        });
    }
    log::info!("{} resource nodes placed", nodes.len());
    Ok(())
}

/// Remove nodes by id, stopping anyone gathering them. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_nodes(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        remove(ctx, id);
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_nodes(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let ids: Vec<u64> = ctx.db.resource_node().iter().map(|n| n.id).collect();
    for &id in &ids {
        remove(ctx, id);
    }
    log::info!("{} resource nodes cleared", ids.len());
    Ok(())
}

fn remove(ctx: &ReducerContext, id: u64) {
    let jobs: Vec<u64> = ctx.db.gather_job().node().filter(id).map(|j| j.scheduled_id).collect();
    for j in jobs {
        ctx.db.gather_job().scheduled_id().delete(j);
    }
    ctx.db.resource_node().id().delete(id);
}
