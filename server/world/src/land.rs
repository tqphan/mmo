//! Land (docs/design/land.md): the land-use raster on the server, plots claimed with deeds, their
//! weekly tax, and demolition when it goes unpaid. A plot's standing is computed from
//! `tax_paid_until`; only the demolition itself is a timer. The rules live in `sim::land`.

use std::collections::HashMap;

use spacetimedb::{Identity, ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp};
use sim::land::{LAND_CHUNK_LEN, LandChunks, PlotData, Rect, Standing, check_site, demolish_at, in_reach, land_chunk_hash, plot, plot_by_deed};
use sim::movement::MoveMode;

use crate::items::{self, place};
use crate::session::{character_private, presence, require_admin};
use crate::terrain::{Terrain, set_terrain_info, terrain_info};
use crate::time::now;

/// `Plot::access` values. Family and guild come with those systems (M2).
pub mod access {
    pub const PRIVATE: u8 = 0;
    pub const PUBLIC: u8 = 1;
}

/// Private: one 64 m cell of the land-use raster, 16 × 16 texels of 4 m, rows of increasing z. A
/// cell without a row is all wild and temperate.
#[spacetimedb::table(accessor = land_chunk)]
pub struct LandChunk {
    #[primary_key]
    pub cell: u64,
    pub texels: Vec<u8>,
}

/// Static and public: a claimed plot. Its standing (paid, overdue, demolished) comes from
/// `tax_paid_until` and the game clock, so nothing changes on it as time passes.
#[spacetimedb::table(accessor = plot, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Plot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    /// The centre's cell.
    #[index(btree)]
    pub cell: u64,
    /// The plot kind's wire (`content/land.toml`).
    pub kind: u8,
    /// The owner's `char_id`.
    #[index(btree)]
    pub owner: u64,
    /// The centre, on the 2 m grid, and the lowest ground under the plot.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Quarter turns of the marker, 0–3.
    pub yaw: u8,
    pub access: u8,
    pub placed_at: Timestamp,
    /// Game time the tax is paid until.
    pub tax_paid_until: Timestamp,
}

/// Private: a plot's demolition, due at its demolition time. One per plot.
#[spacetimedb::table(accessor = plot_demolition, scheduled(demolish_plot))]
#[derive(Clone, Debug, PartialEq)]
pub struct PlotDemolition {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    #[unique]
    pub plot: u64,
}

/// Reads land chunks from the database once per reducer call.
pub struct Land<'a> {
    ctx: &'a ReducerContext,
    cache: HashMap<u64, Option<Vec<u8>>>,
}

impl<'a> Land<'a> {
    pub fn new(ctx: &'a ReducerContext) -> Self {
        Self { ctx, cache: HashMap::new() }
    }
}

impl LandChunks for Land<'_> {
    fn land(&mut self, cell: u64) -> Option<&[u8]> {
        let ctx = self.ctx;
        self.cache.entry(cell).or_insert_with(|| ctx.db.land_chunk().cell().find(cell).map(|c| c.texels)).as_deref()
    }
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

fn kind_of(p: &Plot) -> Result<&'static PlotData, String> {
    plot(p.kind).ok_or_else(|| "no such plot kind".into())
}

fn rect_of(p: &Plot) -> Option<Rect> {
    plot(p.kind).map(|k| Rect::plot(k, p.x, p.z))
}

pub fn standing(ctx: &ReducerContext, p: &Plot) -> Standing {
    sim::land::standing(micros(p.tax_paid_until), micros(now(ctx)))
}

/// Plots with their centres in the 3 × 3 cells around `(x, z)`: every plot that can reach a plot
/// centred there, since no plot is wider than a cell.
pub fn plots_near(ctx: &ReducerContext, x: f32, z: f32) -> Vec<Plot> {
    let centre = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    sim::cell::window_cells(centre, 1).into_iter().flat_map(|c| ctx.db.plot().cell().filter(c).collect::<Vec<_>>()).collect()
}

/// The account's characters.
pub fn characters_of(ctx: &ReducerContext, identity: Identity) -> Vec<u64> {
    ctx.db.character_private().owner().filter(identity).map(|c| c.char_id).collect()
}

/// Standing plots owned by the account's characters.
fn plots_owned(ctx: &ReducerContext, identity: Identity) -> usize {
    characters_of(ctx, identity)
        .into_iter()
        .flat_map(|c| ctx.db.plot().owner().filter(c).collect::<Vec<_>>())
        .filter(|p| standing(ctx, p) != Standing::Demolished)
        .count()
}

/// The caller, in the world on land, and a plot it owns within reach, not yet demolished.
fn my_plot_in_reach(ctx: &ReducerContext, plot_id: u64) -> Result<(Plot, u64), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    let pl = ctx.db.plot().id().find(plot_id).filter(|pl| standing(ctx, pl) != Standing::Demolished).ok_or("no such plot")?;
    if !characters_of(ctx, ctx.sender()).contains(&pl.owner) {
        return Err("that isn't your plot".into());
    }
    let r = rect_of(&pl).ok_or("no such plot kind")?;
    if p.parent != 0 || !in_reach(&r, p.x, p.z) {
        return Err("too far from the plot".into());
    }
    Ok((pl, p.char_id))
}

/// Schedule `p`'s demolition for its demolition time, replacing any earlier schedule. Game time
/// may run ahead of the host's (the dev clock), so the delay is measured in game time.
fn schedule(ctx: &ReducerContext, p: &Plot) {
    ctx.db.plot_demolition().plot().delete(p.id);
    let wait = (demolish_at(micros(p.tax_paid_until)) - micros(now(ctx))).max(0);
    let at = ctx.timestamp + TimeDuration::from_micros(wait);
    ctx.db.plot_demolition().insert(PlotDemolition { scheduled_id: 0, scheduled_at: at.into(), plot: p.id });
}

/// Called when the dev clock moves: every demolition is rescheduled against the new game time.
#[cfg(feature = "dev")]
pub fn clock_moved(ctx: &ReducerContext) {
    for p in ctx.db.plot().iter().collect::<Vec<_>>() {
        schedule(ctx, &p);
    }
}

/// Remove a plot, and what grew on it, and give its deed back: to the owner's bag, or held for
/// mail if it doesn't fit.
fn demolish(ctx: &ReducerContext, p: &Plot) {
    ctx.db.plot_demolition().plot().delete(p.id);
    ctx.db.plot().id().delete(p.id);
    crate::farming::clear_plot(ctx, p.id);
    if let Some(k) = plot(p.kind)
        && items::grant(ctx, p.owner, k.deed, 1).is_err()
    {
        items::hold(ctx, p.owner, place::RETURNS, k.deed, 1);
    }
    log::info!("plot {} of char {} demolished", p.id, p.owner);
}

/// Claim a plot with the deed `item_id`, centred near `(x, z)`. The first week's tax is taken with
/// the deed, at the rate the account pays with this plot counted.
#[spacetimedb::reducer]
pub fn place_plot(ctx: &ReducerContext, item_id: u64, x: f32, z: f32, yaw: u8) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    if p.parent != 0 {
        return Err("can't place a plot from a ship".into());
    }
    if p.mode == MoveMode::Swim as u8 {
        return Err("can't place a plot while swimming".into());
    }
    let deed = items::held(ctx, p.char_id, item_id).ok_or("no such item")?;
    let kind = plot_by_deed(deed.def).ok_or("that isn't a deed")?;
    if !x.is_finite() || !z.is_finite() {
        return Err("non-finite position".into());
    }
    let (x, z) = (sim::land::snap(x), sim::land::snap(z));
    let r = Rect::plot(kind, x, z);
    if !in_reach(&r, p.x, p.z) {
        return Err("too far from the plot".into());
    }
    // Whoever calls first wins the land; plots past their demolition time are cleared first.
    let mut others = Vec::new();
    for o in plots_near(ctx, x, z) {
        let Some(or) = rect_of(&o) else { continue };
        if standing(ctx, &o) == Standing::Demolished {
            if or.overlaps(&r) {
                demolish(ctx, &o);
            }
        } else {
            others.push(or);
        }
    }
    let y = check_site(&mut Land::new(ctx), &mut Terrain::new(ctx), kind, x, z, others).map_err(|e| e.message().to_string())?;
    let owned = plots_owned(ctx, ctx.sender()) + 1;
    let tax = sim::land::weekly_tax(kind, owned).ok_or_else(|| format!("you own the most plots allowed ({})", sim::land::max_plots()))?;
    let seal = sim::land::PROPERTY.tax_item;
    items::take(ctx, p.char_id, &[(deed.def, 1), (seal, tax)])?;
    let at = now(ctx);
    let pl = ctx.db.plot().insert(Plot {
        id: 0,
        cell: sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z),
        kind: kind.wire,
        owner: p.char_id,
        x,
        y,
        z,
        yaw: yaw % 4,
        access: access::PRIVATE,
        placed_at: at,
        tax_paid_until: at + TimeDuration::from_micros(sim::land::week_micros()),
    });
    schedule(ctx, &pl);
    log::info!("char {} placed {} {} at ({x}, {z}) for {tax} seals", p.char_id, kind.id, pl.id);
    Ok(())
}

/// Pay one week of a plot's tax at the account's current rate. Each week extends the paid time,
/// so an overdue plot pays its arrears first.
#[spacetimedb::reducer]
pub fn pay_tax(ctx: &ReducerContext, plot_id: u64) -> Result<(), String> {
    let (pl, char_id) = my_plot_in_reach(ctx, plot_id)?;
    let kind = kind_of(&pl)?;
    if !sim::land::can_pay(micros(pl.tax_paid_until), micros(now(ctx))) {
        return Err(format!("tax can be paid at most {} weeks ahead", sim::land::PROPERTY.prepay_weeks));
    }
    let tax = sim::land::weekly_tax(kind, plots_owned(ctx, ctx.sender()).max(1)).ok_or("you own more plots than allowed")?;
    items::take(ctx, char_id, &[(sim::land::PROPERTY.tax_item, tax)])?;
    let pl = Plot { tax_paid_until: pl.tax_paid_until + TimeDuration::from_micros(sim::land::week_micros()), ..pl };
    ctx.db.plot().id().update(pl.clone());
    schedule(ctx, &pl);
    Ok(())
}

#[spacetimedb::reducer]
pub fn set_plot_access(ctx: &ReducerContext, plot_id: u64, access: u8) -> Result<(), String> {
    if ![access::PRIVATE, access::PUBLIC].contains(&access) {
        return Err("access is private (0) or public (1)".into());
    }
    let (pl, _) = my_plot_in_reach(ctx, plot_id)?;
    ctx.db.plot().id().update(Plot { access, ..pl });
    Ok(())
}

/// Give a plot up, and what grows on it. The deed goes back to the bag; paid tax isn't refunded.
#[spacetimedb::reducer]
pub fn abandon_plot(ctx: &ReducerContext, plot_id: u64) -> Result<(), String> {
    let (pl, char_id) = my_plot_in_reach(ctx, plot_id)?;
    let kind = kind_of(&pl)?;
    items::grant(ctx, char_id, kind.deed, 1)?;
    ctx.db.plot_demolition().plot().delete(pl.id);
    ctx.db.plot().id().delete(pl.id);
    crate::farming::clear_plot(ctx, pl.id);
    Ok(())
}

/// A plot's demolition time has come by the host's clock. If the plot was paid since, or game time
/// says it isn't due yet, it's rescheduled; otherwise it comes down. It never fails, since nobody
/// would hear about it.
#[spacetimedb::reducer]
pub fn demolish_plot(ctx: &ReducerContext, job: PlotDemolition) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("demolish_plot runs on the server's schedule only".into());
    }
    ctx.db.plot_demolition().scheduled_id().delete(job.scheduled_id);
    let Some(p) = ctx.db.plot().id().find(job.plot) else { return Ok(()) };
    if standing(ctx, &p) == Standing::Demolished {
        demolish(ctx, &p);
    } else {
        schedule(ctx, &p);
    }
    Ok(())
}

/// Add or replace land chunks.
#[spacetimedb::reducer]
pub fn admin_put_land(ctx: &ReducerContext, chunks: Vec<LandChunk>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = chunks.iter().find(|c| c.texels.len() != LAND_CHUNK_LEN || !sim::terrain::valid_chunk_cell(c.cell)) {
        return Err(format!("land chunk {:#x}: needs {LAND_CHUNK_LEN} texels and an open-world cell on the map", bad.cell));
    }
    if let Some(bad) = chunks.iter().find(|c| c.texels.iter().any(|t| t & 0x80 != 0)) {
        return Err(format!("land chunk {:#x}: bit 7 of a texel is reserved", bad.cell));
    }
    let n = chunks.len();
    let mut info = terrain_info(ctx);
    for c in chunks {
        match ctx.db.land_chunk().cell().find(c.cell) {
            Some(old) => {
                info.land_hash = info.land_hash.wrapping_sub(land_chunk_hash(old.cell, &old.texels));
                ctx.db.land_chunk().cell().delete(c.cell);
            }
            None => info.land_chunks += 1,
        }
        info.land_hash = info.land_hash.wrapping_add(land_chunk_hash(c.cell, &c.texels));
        ctx.db.land_chunk().insert(c);
    }
    log::info!("{n} land chunks uploaded; {} in all, land hash {:016x}", info.land_chunks, info.land_hash);
    set_terrain_info(ctx, info);
    Ok(())
}

/// Remove land chunks by cell, leaving those cells wild and temperate. Unknown cells are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_land(ctx: &ReducerContext, cells: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    let mut info = terrain_info(ctx);
    for cell in cells {
        if let Some(old) = ctx.db.land_chunk().cell().find(cell) {
            info.land_hash = info.land_hash.wrapping_sub(land_chunk_hash(old.cell, &old.texels));
            info.land_chunks -= 1;
            ctx.db.land_chunk().cell().delete(cell);
        }
    }
    set_terrain_info(ctx, info);
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_land(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let cells: Vec<u64> = ctx.db.land_chunk().iter().map(|c| c.cell).collect();
    for &cell in &cells {
        ctx.db.land_chunk().cell().delete(cell);
    }
    set_terrain_info(ctx, crate::terrain::TerrainInfo { land_chunks: 0, land_hash: 0, ..terrain_info(ctx) });
    log::info!("{} land chunks cleared", cells.len());
    Ok(())
}

/// Remove plots by id, returning their deeds. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_plots(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        if let Some(p) = ctx.db.plot().id().find(id) {
            demolish(ctx, &p);
        }
    }
    Ok(())
}
