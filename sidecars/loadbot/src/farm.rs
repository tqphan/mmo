//! The `farm` scenario (docs/design/farming.md): on the world's housing land, or on test ground it
//! uploads (as the `land` scenario does), bot A places a farm and plants Hearthgrain and a
//! Woolback on it, and Hearthgrain off it. Livestock off the plot, plantings too close together
//! and B planting on A's private farm are refused. The dev clock ripens everything; A harvests,
//! the Woolback stays to be shorn again, B is refused A's plot and steals the crop off it. Then
//! it cleans up.

use sim::cell::{OPEN_WORLD, cell_of};
use sim::farm::{Stage, crop_by_id, stage};
use sim::land::PLOTS;

use crate::gather::{advance, call, game_now, held, subscribe};
use crate::land::{OWN_CELLS, grant, refused, upload_own, world_site};
use crate::module_bindings::*;
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

pub fn scenario(args: &Args) -> Check {
    let farm = &PLOTS[1];
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    call(&a, |tx| a.conn.reducers.dev_set_gm_then(3, tx))?;
    let own = a.server_terrain().land_chunks == 0;
    let (sx, sz) = if own { upload_own(&a)? } else { world_site(farm)? };
    println!("site: ({sx:.0}, {sz:.0}) on {}", if own { "our own test ground" } else { "the world's housing land" });

    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    let cell = cell_of(OPEN_WORLD, sx, sz);
    let cells: Vec<u64> = sim::cell::window_cells(cell, 1);
    for bot in [&a, &b] {
        subscribe(bot, &["SELECT * FROM my_inventory", "SELECT * FROM game_clock", "SELECT * FROM my_labor"])?;
        subscribe(bot, &[&format!("SELECT * FROM plot WHERE cell = {cell}")])?;
        for c in &cells {
            subscribe(bot, &[&format!("SELECT * FROM crop WHERE cell = {c}")])?;
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
        grant(bot, "hearthgrain_seed", 5)?;
        grant(bot, "woolback_lamb", 2)?;
    }
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);
    grant(&a, "farm_deed", 1)?;
    grant(&a, "levy_seal", 20)?;

    // A's farm, ±8 m round the site.
    a.teleport(sx, sz - 6.0)?;
    let deed = stack(&a, pa.char_id, farm.deed)?;
    call(&a, |tx| a.conn.reducers.place_plot_then(deed, sx, sz, 0, tx))?;
    wait_for("the farm", || a.conn.db.plot().iter().any(|p| p.owner == pa.char_id))?;
    let plot = a.conn.db.plot().iter().find(|p| p.owner == pa.char_id).ok_or("no farm")?;

    // Two rows on the farm and a Woolback, then a crop just off it.
    let (grain, wool) = (crop_by_id("hearthgrain").unwrap(), crop_by_id("woolback").unwrap());
    a.teleport(sx - 4.0, sz - 4.0)?;
    plant(&a, pa.char_id, grain.seed, sx - 5.0, sz - 4.0)?;
    plant(&a, pa.char_id, grain.seed, sx - 4.0, sz - 4.0)?;
    plant(&a, pa.char_id, wool.seed, sx - 2.5, sz - 2.5)?;
    let gs = stack(&a, pa.char_id, grain.seed)?;
    refused(&a, "a crop too close", "too close", |tx| a.conn.reducers.plant_then(gs, sx - 4.5, sz - 4.0, tx))?;
    a.teleport(sx - 10.0, sz - 4.0)?;
    plant(&a, pa.char_id, grain.seed, sx - 10.0, sz - 4.0)?;
    let ws = stack(&a, pa.char_id, wool.seed)?;
    refused(&a, "a Woolback off the plot", "only be kept on a plot", |tx| a.conn.reducers.plant_then(ws, sx - 11.0, sz - 6.0, tx))?;
    wait_for("four plantings", || a.conn.db.crop().iter().filter(|c| c.owner == pa.char_id).count() == 4)?;
    let mine: Vec<Crop> = a.conn.db.crop().iter().filter(|c| c.owner == pa.char_id).collect();
    if mine.iter().filter(|c| c.plot == plot.id).count() != 3 {
        return Err(format!("three plantings should be on the farm: {mine:?}"));
    }
    println!("A planted three on the farm and one beside it; too close and livestock off the plot were refused");

    b.teleport(sx - 6.0, sz - 7.0)?;
    let bs = stack(&b, pb.char_id, grain.seed)?;
    refused(&b, "planting on A's private farm", "isn't yours to plant", |tx| b.conn.reducers.plant_then(bs, sx - 6.0, sz - 6.0, tx))?;

    // Ripe by the dev clock.
    let last = mine.iter().map(|c| c.ripe_at.to_micros_since_unix_epoch()).max().unwrap();
    advance(&a, last - game_now(&a) + 1_000_000)?;
    let now = game_now(&a);
    if mine.iter().any(|c| stage(sim::farm::crop(c.kind).unwrap(), c.ripe_at.to_micros_since_unix_epoch(), now) != Stage::Ripe) {
        return Err("everything should be ripe".into());
    }
    let sheep = mine.iter().find(|c| c.kind == wool.wire).unwrap().clone();
    b.teleport(sx - 3.0, sz - 3.5)?;
    refused(&b, "harvesting A's Woolback", "isn't yours to harvest", |tx| b.conn.reducers.harvest_then(sheep.id, tx))?;

    a.teleport(sx - 4.0, sz - 4.0)?;
    for c in mine.iter().filter(|c| c.plot == plot.id) {
        call(&a, |tx| a.conn.reducers.harvest_then(c.id, tx))?;
    }
    wait_for("the harvest", || held(&a, pa.char_id, "hearthgrain") >= 2 * grain.yield_min && held(&a, pa.char_id, "woolback_fleece") >= wool.yield_min)?;
    wait_for("the Woolback shorn", || a.conn.db.crop().id().find(&sheep.id).is_some_and(|c| c.harvests == 1))?;
    println!("A harvested {} Hearthgrain and {} Woolback Fleece; the Woolback grows again", held(&a, pa.char_id, "hearthgrain"), held(&a, pa.char_id, "woolback_fleece"));

    let off = mine.iter().find(|c| c.plot == 0).unwrap().clone();
    b.teleport(sx - 10.0, sz - 5.0)?;
    call(&b, |tx| b.conn.reducers.harvest_then(off.id, tx))?;
    wait_for("the theft", || held(&b, pb.char_id, "hearthgrain") >= grain.yield_min)?;
    wait_for("the stolen crop gone", || a.conn.db.crop().id().find(&off.id).is_none())?;
    println!("B stole {} Hearthgrain from A's crop off the plot", held(&b, pb.char_id, "hearthgrain"));

    // Clean up: the farm takes its Woolback with it; then the items and our ground.
    call(&a, |tx| a.conn.reducers.admin_remove_plots_then(vec![plot.id], tx))?;
    wait_for("the farm's plantings removed", || a.conn.db.crop().iter().all(|c| c.owner != pa.char_id))?;
    for (bot, char_id) in [(&a, pa.char_id), (&b, pb.char_id)] {
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

/// A stack of the item with wire `def` in the bot's bag.
fn stack(bot: &Bot, char_id: u64, def: u16) -> Result<u64, String> {
    wait_for("a stack", || bot.conn.db.my_inventory().iter().any(|i| i.holder == char_id && i.def == def))?;
    bot.conn.db.my_inventory().iter().find(|i| i.holder == char_id && i.def == def).map(|i| i.id).ok_or_else(|| "no stack".into())
}

fn plant(bot: &Bot, char_id: u64, seed: u16, x: f32, z: f32) -> Check {
    let item = stack(bot, char_id, seed)?;
    call(bot, |tx| bot.conn.reducers.plant_then(item, x, z, tx))
}
