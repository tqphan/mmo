//! Farming (docs/design/farming.md): crops, trees and livestock planted from seeds, ripe and
//! withered by timestamps alone, harvested for labor and uprooted. Nothing ticks. The rules live
//! in `sim::farm`; who may tend a planting on a plot comes from the plot's owner and access.

use spacetimedb::{Identity, ReducerContext, Table, Timestamp};
use sim::farm::{CropData, Planting, PlotSpot, Stage, check_spot, crop, crop_by_seed, in_reach, regrown_at, ripe_at, seed_back, stage, yield_for};
use sim::land::{Rect, Standing};
use sim::movement::MoveMode;

use crate::land::{Land, Plot, access, characters_of, plot, plots_near, standing};
use crate::session::{presence, require_admin};
use crate::terrain::Terrain;
use crate::time::now;

/// Static and public: one crop, tree or animal. Written when it's planted, harvested or removed;
/// its stage comes from `ripe_at` and game time.
#[spacetimedb::table(accessor = crop, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Crop {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The crop kind's wire (`content/farming.toml`).
    pub kind: u8,
    /// The planter's `char_id`.
    pub owner: u64,
    /// The plot it stands on, or 0.
    #[index(btree)]
    pub plot: u64,
    pub x: f32,
    /// The lowest ground at the spot.
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    /// It stands in its ideal climate, so it grows faster.
    pub ideal: bool,
    pub planted_at: Timestamp,
    /// When its current growth began: planting, or the last harvest.
    pub grown_from: Timestamp,
    pub ripe_at: Timestamp,
    pub harvests: u32,
}

/// What a caller may do with a planting.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rights {
    /// Harvest and uproot it: its plot's owner or a public plot, its planter, or clearing a
    /// withered or orphaned planting.
    Tend,
    /// Take someone else's ripe planting off a plot: allowed, and a crime.
    Steal,
    None,
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

fn kind_of(c: &Crop) -> Result<&'static CropData, String> {
    crop(c.kind).ok_or_else(|| "no such crop".into())
}

/// The plot a planting stands on, if it's still standing by game time.
fn plot_of(ctx: &ReducerContext, c: &Crop) -> Option<Plot> {
    ctx.db.plot().id().find(c.plot).filter(|p| standing(ctx, p) != Standing::Demolished)
}

/// The account may tend plantings on `p`.
fn may_tend_plot(ctx: &ReducerContext, p: &Plot, who: Identity) -> bool {
    p.access == access::PUBLIC || characters_of(ctx, who).contains(&p.owner)
}

/// The planting's kind is gone from content, it has withered, or the plot it stood on is gone.
fn gone(ctx: &ReducerContext, c: &Crop) -> bool {
    match crop(c.kind) {
        None => true,
        Some(k) => stage(k, micros(c.ripe_at), micros(now(ctx))) == Stage::Withered || (c.plot != 0 && plot_of(ctx, c).is_none()),
    }
}

fn rights(ctx: &ReducerContext, c: &Crop, who: Identity) -> Rights {
    if c.plot != 0 {
        return match plot_of(ctx, c) {
            Some(p) if may_tend_plot(ctx, &p, who) => Rights::Tend,
            Some(_) => Rights::None,
            // Its plot is gone: anyone may clear what's left.
            None => Rights::Tend,
        };
    }
    if characters_of(ctx, who).contains(&c.owner) {
        return Rights::Tend;
    }
    match crop(c.kind).map(|k| stage(k, micros(c.ripe_at), micros(now(ctx)))) {
        Some(Stage::Ripe) => Rights::Steal,
        Some(Stage::Growing) => Rights::None,
        Some(Stage::Withered) | None => Rights::Tend,
    }
}

/// Plantings with their centres in the 3 × 3 cells around `(x, z)`, which covers any that can
/// be too close to a spot there.
fn crops_near(ctx: &ReducerContext, x: f32, z: f32) -> Vec<Crop> {
    let centre = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
    sim::cell::window_cells(centre, 1).into_iter().flat_map(|c| ctx.db.crop().cell().filter(c).collect::<Vec<_>>()).collect()
}

/// The caller in the world, on land and not swimming; `what` names the action for the refusals.
fn on_foot(ctx: &ReducerContext, what: &str) -> Result<crate::session::Presence, String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if p.parent != 0 {
        return Err(format!("can't {what} from a ship"));
    }
    if p.mode == MoveMode::Swim as u8 {
        return Err(format!("can't {what} while swimming"));
    }
    Ok(p)
}

/// Crime hook: the crime task (BACKLOG M1 #6) turns this into evidence and crime points.
fn theft(thief: u64, c: &Crop, what: &str) {
    log::info!("crime: char {thief} {what} crop {} of char {} at ({}, {})", c.id, c.owner, c.x, c.z);
}

/// Plant the seed `item_id` at `(x, z)`.
#[spacetimedb::reducer]
pub fn plant(ctx: &ReducerContext, item_id: u64, x: f32, z: f32) -> Result<(), String> {
    let p = on_foot(ctx, "plant")?;
    let seed = crate::items::held(ctx, p.char_id, item_id).ok_or("no such item")?;
    let kind = crop_by_seed(seed.def).ok_or("that isn't a seed")?;
    if !x.is_finite() || !z.is_finite() {
        return Err("non-finite position".into());
    }
    if !in_reach(x, z, p.x, p.z) {
        return Err("too far away".into());
    }
    let plots: Vec<Plot> = plots_near(ctx, x, z).into_iter().filter(|pl| standing(ctx, pl) != Standing::Demolished).collect();
    let spots: Vec<PlotSpot> = plots
        .iter()
        .filter_map(|pl| sim::land::plot(pl.kind).map(|k| PlotSpot { rect: Rect::plot(k, pl.x, pl.z), may_tend: may_tend_plot(ctx, pl, ctx.sender()) }))
        .collect();
    // Gone plantings make room: the ones in the way are cleared.
    let mut others = Vec::new();
    for c in crops_near(ctx, x, z) {
        let Some(k) = crop(c.kind) else { continue };
        if gone(ctx, &c) {
            let r = k.space_m + kind.space_m;
            if (c.x - x).powi(2) + (c.z - z).powi(2) < r * r {
                ctx.db.crop().id().delete(c.id);
            }
        } else {
            others.push(Planting { x: c.x, z: c.z, space_m: k.space_m });
        }
    }
    let spot = check_spot(&mut Land::new(ctx), &mut Terrain::new(ctx), kind, x, z, &spots, others).map_err(|e| e.message().to_string())?;
    crate::items::take(ctx, p.char_id, &[(seed.def, 1)])?;
    let t = now(ctx);
    let ripe = Timestamp::from_micros_since_unix_epoch(ripe_at(kind, micros(t), spot.ideal));
    let c = ctx.db.crop().insert(Crop {
        id: 0,
        cell: sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z),
        kind: kind.wire,
        owner: p.char_id,
        plot: spot.plot.map_or(0, |k| plots[k].id),
        x,
        y: spot.y,
        z,
        yaw: ctx.random::<u16>(),
        ideal: spot.ideal,
        planted_at: t,
        grown_from: t,
        ripe_at: ripe,
        harvests: 0,
    });
    log::info!("char {} planted {} {} at ({x}, {z}), plot {}, ideal {}", p.char_id, kind.id, c.id, c.plot, c.ideal);
    Ok(())
}

/// The caller standing within reach of a planting that isn't gone.
fn in_reach_of(ctx: &ReducerContext, crop_id: u64, what: &str) -> Result<(crate::session::Presence, Crop, &'static CropData), String> {
    let p = on_foot(ctx, what)?;
    let c = ctx.db.crop().id().find(crop_id).ok_or("no such crop")?;
    let kind = kind_of(&c)?;
    if !in_reach(c.x, c.z, p.x, p.z) {
        return Err("too far away".into());
    }
    Ok((p, c, kind))
}

/// Harvest a ripe planting: the yield (and sometimes a seed) for its labor. A kind that regrows
/// starts again; any other is removed.
#[spacetimedb::reducer]
pub fn harvest(ctx: &ReducerContext, crop_id: u64) -> Result<(), String> {
    let (p, c, kind) = in_reach_of(ctx, crop_id, "harvest")?;
    let t = now(ctx);
    match stage(kind, micros(c.ripe_at), micros(t)) {
        Stage::Growing => return Err("it isn't ripe yet".into()),
        Stage::Withered => return Err("it has withered".into()),
        Stage::Ripe => {}
    }
    if c.plot != 0 && plot_of(ctx, &c).is_none() {
        return Err("its plot is gone".into());
    }
    let rights = rights(ctx, &c, ctx.sender());
    if rights == Rights::None {
        return Err("this isn't yours to harvest".into());
    }
    let count = yield_for(kind, ctx.random::<u32>());
    let seed = seed_back(kind, ctx.random::<u32>());
    let have = crate::labor::available(ctx, ctx.sender())?;
    if have < kind.labor {
        return Err(format!("not enough labor: have {have}, need {}", kind.labor));
    }
    // A failed grant returns `Err`, which rolls back the labor too.
    crate::labor::spend(ctx, ctx.sender(), kind.labor)?;
    crate::items::grant(ctx, p.char_id, kind.product, count)?;
    if seed {
        crate::items::grant(ctx, p.char_id, kind.seed, 1)?;
    }
    if rights == Rights::Steal {
        theft(p.char_id, &c, "harvested");
    }
    match regrown_at(kind, micros(t), c.ideal) {
        Some(again) => {
            let ripe_at = Timestamp::from_micros_since_unix_epoch(again);
            ctx.db.crop().id().update(Crop { grown_from: t, ripe_at, harvests: c.harvests + 1, ..c });
        }
        None => {
            ctx.db.crop().id().delete(c.id);
        }
    }
    Ok(())
}

/// Pull up a planting; nothing comes back.
#[spacetimedb::reducer]
pub fn uproot(ctx: &ReducerContext, crop_id: u64) -> Result<(), String> {
    let (p, c, _) = in_reach_of(ctx, crop_id, "uproot")?;
    match rights(ctx, &c, ctx.sender()) {
        Rights::None => return Err("this isn't yours to uproot".into()),
        Rights::Steal => theft(p.char_id, &c, "uprooted"),
        Rights::Tend => {}
    }
    ctx.db.crop().id().delete(c.id);
    Ok(())
}

/// Called when a plot comes down or is given up: what grew on it goes with it.
pub fn clear_plot(ctx: &ReducerContext, plot_id: u64) {
    let ids: Vec<u64> = ctx.db.crop().plot().filter(plot_id).map(|c| c.id).collect();
    for id in ids {
        ctx.db.crop().id().delete(id);
    }
}

/// Remove plantings by id. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_crops(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        ctx.db.crop().id().delete(id);
    }
    Ok(())
}
