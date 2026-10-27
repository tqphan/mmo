//! The `land` scenario (docs/design/land.md): two bots on housing land. On a server with the
//! world's land use uploaded it finds a site in the world; otherwise it uploads a little flat
//! housing ground of its own far from spawn and removes it at the end. Both bots place a garden
//! over the same spot at once and exactly one wins; refusals; tax paid ahead to the limit; the
//! dev clock moves the plot to overdue and then past its demolition, the plot comes down, the
//! deed comes back, and the other bot claims the spot.

use std::sync::mpsc;

use sim::cell::{CELL_SIZE_M, OPEN_WORLD, cell_of};
use sim::land::{LAND_CHUNK_LEN, LandChunks, PLOTS, PROPERTY, Rect, Standing, check_site, demolish_at, week_micros};
use sim::terrain::{CHUNK_LEN, encode};

use crate::gather::{advance, call, game_now, held, subscribe};
use crate::module_bindings::*;
use crate::terrain::flatten;
use crate::{Args, Bot, Check, WAIT, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// Our own ground when the world's isn't there: 2 × 2 cells, 10 m up, all housing.
pub(crate) const OWN_CELLS: [(f32, f32); 4] = [(6420.0, 6420.0), (6484.0, 6420.0), (6420.0, 6484.0), (6484.0, 6484.0)];
const OWN_SITE: (f32, f32) = (6464.0, 6464.0);

pub fn scenario(args: &Args) -> Check {
    let garden = &PLOTS[0];
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    call(&a, |tx| a.conn.reducers.dev_set_gm_then(3, tx))?;
    let own = a.server_terrain().land_chunks == 0;
    let site = if own { upload_own(&a)? } else { world_site(garden)? };
    println!("site: ({:.0}, {:.0}) on {}", site.0, site.1, if own { "our own test ground" } else { "the world's housing land" });

    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    let cell = cell_of(OPEN_WORLD, site.0, site.1);
    let mine = ["SELECT * FROM my_inventory", "SELECT * FROM game_clock"];
    for bot in [&a, &b] {
        subscribe(bot, &mine)?;
        subscribe(bot, &[&format!("SELECT * FROM plot WHERE cell = {cell}")])?;
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
        bot.teleport(site.0, site.1 - 6.0)?;
        grant(bot, "garden_deed", 1)?;
        grant(bot, "levy_seal", 50)?;
    }
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);

    // The land rush: both place at once, and one wins.
    let (ta, ra) = mpsc::channel();
    let (tb, rb) = mpsc::channel();
    let (da, db) = (deed(&a, pa.char_id)?, deed(&b, pb.char_id)?);
    a.conn.reducers.place_plot_then(da, site.0, site.1, 0, move |_, r| drop(ta.send(flatten(r)))).map_err(|e| e.to_string())?;
    b.conn.reducers.place_plot_then(db, site.0 + 1.0, site.1, 0, move |_, r| drop(tb.send(flatten(r)))).map_err(|e| e.to_string())?;
    let (oa, ob) = (ra.recv_timeout(WAIT).map_err(|_| "no reply for A")?, rb.recv_timeout(WAIT).map_err(|_| "no reply for B")?);
    let (winner, loser, wp, lp) = match (&oa, &ob) {
        (Ok(()), Err(e)) if e.contains("taken") => (&a, &b, &pa, &pb),
        (Err(e), Ok(())) if e.contains("taken") => (&b, &a, &pb, &pa),
        _ => return Err(format!("the land rush should have one winner: A {oa:?}, B {ob:?}")),
    };
    wait_for("the plot", || winner.conn.db.plot().iter().any(|p| p.owner == wp.char_id))?;
    let plot = winner.conn.db.plot().iter().find(|p| p.owner == wp.char_id).ok_or("no plot")?;
    if winner.conn.db.plot().iter().count() != 1 || held(winner, wp.char_id, "levy_seal") != 45 || held(loser, lp.char_id, "levy_seal") != 50 {
        return Err("after the rush: the wrong plots or seals".into());
    }
    println!("{} won the land rush; {} was told the land is taken", winner.label, loser.label);

    // Refusals.
    refused(loser, "paying someone else's tax", "isn't your plot", |tx| loser.conn.reducers.pay_tax_then(plot.id, tx))?;
    let ld = deed(loser, lp.char_id)?;
    refused(loser, "a plot out of reach", "too far", |tx| loser.conn.reducers.place_plot_then(ld, site.0, site.1 + 40.0, 0, tx))?;
    let seal = held(loser, lp.char_id, "levy_seal");
    let seal_id = loser.conn.db.my_inventory().iter().find(|i| i.holder == lp.char_id && i.count == seal).map(|i| i.id).ok_or("no seals")?;
    refused(loser, "placing seals", "isn't a deed", |tx| loser.conn.reducers.place_plot_then(seal_id, site.0, site.1, 0, tx))?;

    // Tax ahead to the limit.
    for _ in 1..PROPERTY.prepay_weeks {
        call(winner, |tx| winner.conn.reducers.pay_tax_then(plot.id, tx))?;
    }
    refused(winner, "paying too far ahead", "weeks ahead", |tx| winner.conn.reducers.pay_tax_then(plot.id, tx))?;
    let p = winner.conn.db.plot().id().find(&plot.id).ok_or("plot gone")?;
    let paid = p.tax_paid_until.to_micros_since_unix_epoch();
    if paid - plot.tax_paid_until.to_micros_since_unix_epoch() != i64::from(PROPERTY.prepay_weeks - 1) * week_micros() {
        return Err("prepaying didn't add a week each time".into());
    }
    println!("{} paid {} weeks ahead and was refused a {}th", winner.label, PROPERTY.prepay_weeks, PROPERTY.prepay_weeks + 1);

    // Overdue, then demolished by the dev clock.
    advance(winner, paid - game_now(winner) + 1_000_000)?;
    let standing = |bot: &Bot| bot.conn.db.plot().id().find(&plot.id).map(|p| sim::land::standing(p.tax_paid_until.to_micros_since_unix_epoch(), game_now(bot)));
    if standing(winner) != Some(Standing::Overdue) {
        return Err(format!("expected the plot overdue, got {:?}", standing(winner)));
    }
    println!("overdue; demolition at {} µs game time", demolish_at(paid));
    let deeds = held(winner, wp.char_id, "garden_deed");
    advance(winner, demolish_at(paid) - game_now(winner) + 1_000_000)?;
    wait_for("the demolition", || winner.conn.db.plot().id().find(&plot.id).is_none())?;
    wait_for("the deed returned", || held(winner, wp.char_id, "garden_deed") == deeds + 1)?;
    println!("the plot came down and {}'s deed came back", winner.label);

    // The loser takes the land.
    let ld = deed(loser, lp.char_id)?;
    call(loser, |tx| loser.conn.reducers.place_plot_then(ld, site.0, site.1, 2, tx))?;
    wait_for("the new plot", || loser.conn.db.plot().iter().any(|p| p.owner == lp.char_id))?;
    println!("{} claimed the freed land", loser.label);

    // Clean up: the plot, the items, our ground.
    let ours: Vec<u64> = a.conn.db.plot().iter().filter(|p| p.owner == pa.char_id || p.owner == pb.char_id).map(|p| p.id).collect();
    call(&a, |tx| a.conn.reducers.admin_remove_plots_then(ours.clone(), tx))?;
    for (bot, char_id) in [(&a, pa.char_id), (&b, pb.char_id)] {
        wait_for("plots removed", || bot.conn.db.plot().iter().all(|p| !ours.contains(&p.id)))?;
        let stacks: Vec<(u64, u32)> = bot.conn.db.my_inventory().iter().filter(|i| i.holder == char_id).map(|i| (i.id, i.count)).collect();
        for (id, count) in stacks {
            call(bot, |tx| bot.conn.reducers.destroy_item_then(id, count, tx))?;
        }
    }
    if own {
        let cells: Vec<u64> = OWN_CELLS.iter().map(|&(x, z)| cell_of(OPEN_WORLD, x, z)).collect();
        call(&a, |tx| a.conn.reducers.admin_remove_land_then(cells.clone(), tx))?;
        call(&a, |tx| a.conn.reducers.admin_remove_terrain_then(cells, tx))?;
    }
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Upload flat housing ground for our own cells; returns the site.
pub(crate) fn upload_own(admin: &Bot) -> Result<(f32, f32), String> {
    let cells: Vec<u64> = OWN_CELLS.iter().map(|&(x, z)| cell_of(OPEN_WORLD, x, z)).collect();
    let chunks: Vec<TerrainChunk> = cells.iter().map(|&cell| TerrainChunk { cell, samples: vec![encode(10.0); CHUNK_LEN] }).collect();
    call(admin, |tx| admin.conn.reducers.admin_put_terrain_then(chunks, tx))?;
    let land: Vec<LandChunk> = cells.iter().map(|&cell| LandChunk { cell, texels: vec![1; LAND_CHUNK_LEN] }).collect();
    call(admin, |tx| admin.conn.reducers.admin_put_land_then(land, tx))?;
    wait_for("our land", || admin.server_terrain().land_chunks >= 4)?;
    Ok(OWN_SITE)
}

/// The world's land use read as chunks.
struct WorldLand<'a> {
    world: &'a worldtool::World,
    cut: std::collections::HashMap<u64, Vec<u8>>,
}

impl LandChunks for WorldLand<'_> {
    fn land(&mut self, cell: u64) -> Option<&[u8]> {
        let w = self.world;
        Some(self.cut.entry(cell).or_insert_with(|| worldtool::land_chunk(w, cell)))
    }
}

/// A site on the world's housing land, nearest spawn first, where a garden fits with room for one
/// more beside it.
pub(crate) fn world_site(kind: &sim::land::PlotData) -> Result<(f32, f32), String> {
    let world = worldtool::World::load(std::path::Path::new(worldtool::DEFAULT_DIR))?;
    let mut land = WorldLand { world: &world, cut: Default::default() };
    let mut heights = sim::terrain::FileChunks::new(&world.heights);
    let step = CELL_SIZE_M / 4.0;
    let mut best: Option<(f32, (f32, f32))> = None;
    let n = (sim::cell::WORLD_HALF_M / step) as i32 - 1;
    for j in -n..n {
        for i in -n..n {
            let (x, z) = (i as f32 * step, j as f32 * step);
            let d = x * x + z * z;
            if best.is_some_and(|(b, _)| b <= d) || world.land_at(x, z).is_none_or(|t| t & 0x0f != 1) {
                continue;
            }
            let mut fits = |x: f32| check_site(&mut land, &mut heights, kind, x, z, std::iter::empty::<Rect>()).is_ok();
            if fits(x) && fits(x + kind.size_m as f32) {
                best = Some((d, (x, z)));
            }
        }
    }
    best.map(|(_, s)| s).ok_or_else(|| "no housing site in the world".into())
}

pub(crate) fn grant(bot: &Bot, item: &str, count: u32) -> Check {
    call(bot, |tx| bot.conn.reducers.dev_grant_item_then(item.to_string(), count, tx))
}

/// The bot's garden deed stack.
fn deed(bot: &Bot, char_id: u64) -> Result<u64, String> {
    let wire = PLOTS[0].deed;
    wait_for("a deed", || bot.conn.db.my_inventory().iter().any(|i| i.holder == char_id && i.def == wire))?;
    bot.conn.db.my_inventory().iter().find(|i| i.holder == char_id && i.def == wire).map(|i| i.id).ok_or_else(|| "no deed".into())
}

pub(crate) fn refused(bot: &Bot, what: &str, want: &str, send: impl FnOnce(crate::gather::Then) -> spacetimedb_sdk::Result<()>) -> Check {
    match call(bot, send) {
        Ok(()) => Err(format!("{what} was accepted")),
        Err(e) if e.contains(want) => {
            println!("{}: refused {what}: {e}", bot.label);
            Ok(())
        }
        Err(e) => Err(format!("{what}: unexpected refusal {e:?}")),
    }
}
