//! Crafting (docs/design/crafting.md): stations in the world, and timed crafts that turn inputs
//! into an output for labor, one unit at a time. The rules themselves live in `sim::craft`.

use spacetimedb::{Identity, ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext};
use sim::craft::{CRAFT_MAX_COUNT, RecipeData, craft_micros, in_reach, moved_off, recipe, station};
use sim::movement::MoveMode;

use crate::session::{presence, require_admin};
use crate::time::now;

/// Static and public: one crafting station. Written only when admins place or remove it.
#[spacetimedb::table(accessor = station, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The station kind's wire (`content/crafting.toml`).
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Where an admin puts a station.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct StationPlacement {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Private: a craft under way, due when its current unit finishes. One per character at most; its
/// owner reads it through `my_craft`.
#[spacetimedb::table(accessor = craft_job, scheduled(craft_done))]
#[derive(Clone, Debug, PartialEq)]
pub struct CraftJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    #[unique]
    pub char_id: u64,
    #[index(btree)]
    pub identity: Identity,
    pub entity_id: u64,
    #[index(btree)]
    pub station: u64,
    /// The recipe's wire.
    pub recipe: u16,
    /// Where the character stood when it started.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Units finished so far, and units asked for.
    pub made: u32,
    pub count: u32,
    /// When the current unit finishes, in game time.
    pub done_at: Timestamp,
}

/// The caller's crafts under way, one per character at most.
#[spacetimedb::view(accessor = my_craft, public)]
pub(crate) fn my_craft(ctx: &ViewContext) -> Vec<CraftJob> {
    ctx.db.craft_job().identity().filter(ctx.sender()).collect()
}

fn recipe_of(wire: u16) -> Result<&'static RecipeData, String> {
    recipe(wire).ok_or_else(|| "no such recipe".into())
}

/// One unit's inputs, as `items::exchange` takes them.
fn takes(r: &RecipeData) -> Vec<(u16, u32)> {
    r.inputs.iter().map(|i| (i.item, i.count)).collect()
}

/// `identity` can make one more unit of `r` for `char_id` now: the labor, the inputs and the room.
fn can_make(ctx: &ReducerContext, identity: Identity, char_id: u64, r: &RecipeData) -> Result<(), String> {
    let have = crate::labor::available(ctx, identity)?;
    if have < r.labor {
        return Err(format!("not enough labor: have {have}, need {}", r.labor));
    }
    crate::items::can_exchange(ctx, char_id, &takes(r), (r.output, r.output_count))
}

/// Start crafting `count` units of `recipe` at `station_id`, one after another. Each finishes in
/// `craft_done` unless the crafter moves away first.
#[spacetimedb::reducer]
pub fn craft(ctx: &ReducerContext, station_id: u64, recipe: u16, count: u32) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if p.parent != 0 {
        return Err("can't craft from a ship".into());
    }
    if p.mode == MoveMode::Swim as u8 {
        return Err("can't craft while swimming".into());
    }
    if ctx.db.craft_job().char_id().find(p.char_id).is_some() {
        return Err("already crafting".into());
    }
    if crate::gathering::busy(ctx, p.char_id) {
        return Err("already gathering".into());
    }
    let r = recipe_of(recipe)?;
    if !(1..=CRAFT_MAX_COUNT).contains(&count) {
        return Err(format!("craft 1 to {CRAFT_MAX_COUNT} at a time"));
    }
    let s = ctx.db.station().id().find(station_id).ok_or("no such station")?;
    if s.kind != r.station {
        return Err(format!("this needs a {}", station(r.station).map_or("different station", |k| k.name)));
    }
    let at = [p.x, p.y, p.z];
    if !in_reach([s.x, s.y, s.z], at) {
        return Err("out of reach".into());
    }
    can_make(ctx, ctx.sender(), p.char_id, r)?;
    let took = TimeDuration::from_micros(craft_micros(r));
    ctx.db.craft_job().insert(CraftJob {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + took).into(),
        char_id: p.char_id,
        identity: ctx.sender(),
        entity_id: p.entity_id,
        station: s.id,
        recipe,
        x: at[0],
        y: at[1],
        z: at[2],
        made: 0,
        count,
        done_at: now(ctx) + took,
    });
    Ok(())
}

/// A unit is due: take its inputs, grant its output and spend its labor, then schedule the next
/// unit if one is wanted and can be made. If anything has changed since it started, it does none
/// of that and the job ends. It never fails, since nobody would hear about it; the reason goes to
/// the log.
#[spacetimedb::reducer]
pub fn craft_done(ctx: &ReducerContext, job: CraftJob) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("craft_done runs on the server's schedule only".into());
    }
    // The scheduler drops this row after we return; drop it now so the next unit can take the
    // character's place in the unique index.
    ctx.db.craft_job().scheduled_id().delete(job.scheduled_id);
    let r = match finish(ctx, &job) {
        Ok(r) => r,
        Err(e) => {
            log::info!("craft by char {} of recipe {} ended after {} of {}: {e}", job.char_id, job.recipe, job.made, job.count);
            return Ok(());
        }
    };
    let made = job.made + 1;
    if made >= job.count {
        return Ok(());
    }
    if let Err(e) = can_make(ctx, job.identity, job.char_id, r) {
        log::info!("craft by char {} of {} stopped after {made} of {}: {e}", job.char_id, r.id, job.count);
        return Ok(());
    }
    let took = TimeDuration::from_micros(craft_micros(r));
    ctx.db.craft_job().insert(CraftJob { scheduled_id: 0, scheduled_at: (ctx.timestamp + took).into(), made, done_at: now(ctx) + took, ..job });
    Ok(())
}

/// Check the crafter is still there, then exchange and spend. On `Err` nothing has changed.
fn finish(ctx: &ReducerContext, job: &CraftJob) -> Result<&'static RecipeData, String> {
    let p = ctx.db.presence().identity().find(job.identity).filter(|p| p.entity_id == job.entity_id).ok_or("left the world")?;
    if p.parent != 0 || moved_off([job.x, job.y, job.z], [p.x, p.y, p.z]) {
        return Err("moved away".into());
    }
    ctx.db.station().id().find(job.station).ok_or("station gone")?;
    let r = recipe_of(job.recipe)?;
    let have = crate::labor::available(ctx, job.identity)?;
    if have < r.labor {
        return Err(format!("not enough labor: have {have}, need {}", r.labor));
    }
    crate::items::exchange(ctx, p.char_id, &takes(r), (r.output, r.output_count))?;
    crate::labor::spend(ctx, job.identity, r.labor)?;
    Ok(r)
}

/// `char_id` is crafting.
pub fn busy(ctx: &ReducerContext, char_id: u64) -> bool {
    ctx.db.craft_job().char_id().find(char_id).is_some()
}

/// Stop `char_id`'s craft, if it has one. The unit under way costs nothing.
pub fn cancel(ctx: &ReducerContext, char_id: u64) {
    ctx.db.craft_job().char_id().delete(char_id);
}

/// Called by `move_event` for every accepted move: a crafter that steps away or boards a ship
/// stops crafting.
pub fn moved(ctx: &ReducerContext, char_id: u64, parent: u64, at: [f32; 3]) {
    let Some(job) = ctx.db.craft_job().char_id().find(char_id) else { return };
    if parent != 0 || moved_off([job.x, job.y, job.z], at) {
        cancel(ctx, char_id);
    }
}

#[spacetimedb::reducer]
pub fn cancel_craft(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    cancel(ctx, p.char_id);
    Ok(())
}

/// Add stations.
#[spacetimedb::reducer]
pub fn admin_put_stations(ctx: &ReducerContext, stations: Vec<StationPlacement>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = stations.iter().find(|s| station(s.kind).is_none() || ![s.x, s.y, s.z].iter().all(|v| v.is_finite())) {
        return Err(format!("bad station {bad:?}: unknown kind or a non-finite position"));
    }
    let half = sim::cell::WORLD_HALF_M;
    if let Some(bad) = stations.iter().find(|s| s.x.abs() >= half || s.z.abs() >= half) {
        return Err(format!("bad station {bad:?}: off the map"));
    }
    for s in &stations {
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, s.x, s.z);
        ctx.db.station().insert(Station { id: 0, cell, kind: s.kind, x: s.x, y: s.y, z: s.z, yaw: s.yaw });
    }
    log::info!("{} stations placed", stations.len());
    Ok(())
}

/// Remove stations by id, stopping anyone crafting at them. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_stations(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        remove(ctx, id);
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_stations(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let ids: Vec<u64> = ctx.db.station().iter().map(|s| s.id).collect();
    for &id in &ids {
        remove(ctx, id);
    }
    log::info!("{} stations cleared", ids.len());
    Ok(())
}

fn remove(ctx: &ReducerContext, id: u64) {
    let jobs: Vec<u64> = ctx.db.craft_job().station().filter(id).map(|j| j.scheduled_id).collect();
    for j in jobs {
        ctx.db.craft_job().scheduled_id().delete(j);
    }
    ctx.db.station().id().delete(id);
}
