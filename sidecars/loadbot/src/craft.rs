//! The `craft` scenario (docs/design/crafting.md): a bot by stations the scenario places. The
//! wrong station, one out of reach, missing materials and missing labor are refused; a batch of 3
//! Charcoal takes 6 logs and 15 labor and gives 9 Charcoal, rescheduling between units; a batch
//! of 5 with logs for 2 stops after 2; moving away and `cancel_craft` both stop a craft at no
//! cost. It removes its stations at the end.

use std::time::{Duration, Instant};

use sim::craft::{RecipeData, recipe_by_id, station_by_id};

use crate::gather::{call, held, labor, set_labor, subscribe};
use crate::module_bindings::*;
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// In the town round spawn, away from its stations and from the gather scenario's spot, and far
/// enough from a cell border that the stations placed round it share one cell.
const SPOT: (f32, f32) = (40.0, 40.0);

pub fn scenario(args: &Args) -> Check {
    let coal = recipe_by_id("charcoal").ok_or("no charcoal recipe in content")?;
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    subscribe(&a, &["SELECT * FROM my_labor", "SELECT * FROM my_inventory", "SELECT * FROM game_clock", "SELECT * FROM my_craft"])?;
    a.arrive()?;
    call(&a, |tx| a.conn.reducers.dev_set_gm_then(3, tx))?;
    a.teleport(SPOT.0, SPOT.1)?;
    let me = a.presence().ok_or("A left the world")?;
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, me.x, me.z);
    subscribe(&a, &[&format!("SELECT * FROM station WHERE cell = {cell}")])?;

    // Our own stations, all in the cell we watch.
    let hearth = place(&a, "cookhearth", me.x + 2.0, me.y, me.z)?;
    let bench = place(&a, "joiners_bench", me.x, me.y, me.z + 2.5)?;
    let far = place(&a, "cookhearth", me.x + 9.0, me.y, me.z)?;
    println!("placed a Cookhearth ({hearth}), a Joiner's Bench ({bench}) and a Cookhearth 9 m off ({far}) at ({:.1}, {:.1})", me.x, me.z);
    let logs = |bot: &Bot| held(bot, me.char_id, "driftpine_log");
    let charcoal = |bot: &Bot| held(bot, me.char_id, "charcoal");

    refused(&a, "charcoal without logs", "not enough Driftpine Log", hearth, coal, 1)?;
    grant(&a, "driftpine_log", 6)?;
    refused(&a, "charcoal at a bench", "this needs a Cookhearth", bench, coal, 1)?;
    refused(&a, "a station out of reach", "out of reach", far, coal, 1)?;
    refused(&a, "a batch of 101", "craft 1 to 100 at a time", hearth, coal, 101)?;
    set_labor(&a, 0)?;
    refused(&a, "a craft without labor", "not enough labor", hearth, coal, 1)?;
    set_labor(&a, 5000)?;

    // A batch of 3 runs unit by unit.
    craft(&a, hearth, coal, 3)?;
    let most = wait_done(&a, coal, 3)?;
    if most < 2 {
        return Err(format!("the batch never showed more than {most} made: it didn't reschedule"));
    }
    if (logs(&a), charcoal(&a), labor(&a)) != (0, 9, 5000 - 3 * coal.labor) {
        return Err(format!("after 3 units: {} logs, {} charcoal, {} labor; expected 0, 9, {}", logs(&a), charcoal(&a), labor(&a), 5000 - 3 * coal.labor));
    }
    println!("A crafted 9 charcoal from 6 logs in 3 units for {} labor, the job showing up to {most} made", 3 * coal.labor);

    // A batch of 5 with logs for 2 stops after 2.
    grant(&a, "driftpine_log", 4)?;
    craft(&a, hearth, coal, 5)?;
    wait_done(&a, coal, 2)?;
    if (logs(&a), charcoal(&a), labor(&a)) != (0, 15, 5000 - 5 * coal.labor) {
        return Err(format!("a batch of 5 with logs for 2 left {} logs, {} charcoal, {} labor", logs(&a), charcoal(&a), labor(&a)));
    }
    println!("a batch of 5 with logs for 2 stopped after 2");

    // Moving away cancels, for free.
    grant(&a, "driftpine_log", 2)?;
    let spent = labor(&a);
    craft(&a, hearth, coal, 1)?;
    std::thread::sleep(Duration::from_millis(300));
    step(&a, 0.8)?;
    wait_for("the craft cancelled by a move", || a.conn.db.my_craft().count() == 0)?;
    std::thread::sleep(Duration::from_secs_f32(coal.craft_s + 0.5));
    if (logs(&a), charcoal(&a), labor(&a)) != (2, 15, spent) {
        return Err(format!("a craft cancelled by moving changed the bag or labor: {} logs, {} charcoal, {} labor", logs(&a), charcoal(&a), labor(&a)));
    }
    println!("moving 0.8 m cancelled a craft at no cost");

    // So does cancel_craft.
    step(&a, 0.0)?;
    craft(&a, hearth, coal, 1)?;
    call(&a, |tx| a.conn.reducers.cancel_craft_then(tx))?;
    wait_for("the craft cancelled", || a.conn.db.my_craft().count() == 0)?;
    if logs(&a) != 2 {
        return Err("cancel_craft took the logs".into());
    }
    println!("cancel_craft stopped a craft");

    // Clean up: our stations and A's items.
    call(&a, |tx| a.conn.reducers.admin_remove_stations_then(vec![hearth, bench, far], tx))?;
    wait_for("our stations removed", || [hearth, bench, far].iter().all(|id| a.conn.db.station().id().find(id).is_none()))?;
    let stacks: Vec<(u64, u32)> = a.conn.db.my_inventory().iter().filter(|i| i.holder == me.char_id).map(|i| (i.id, i.count)).collect();
    for (id, count) in stacks {
        call(&a, |tx| a.conn.reducers.destroy_item_then(id, count, tx))?;
    }
    a.conn.disconnect().map_err(|e| e.to_string())
}

/// Place a station of kind `id` as an admin; returns its id once we see it.
fn place(bot: &Bot, id: &str, x: f32, y: f32, z: f32) -> Result<u64, String> {
    let kind = station_by_id(id).ok_or_else(|| format!("no station {id}"))?;
    let before: Vec<u64> = bot.conn.db.station().iter().map(|s| s.id).collect();
    let p = StationPlacement { kind: kind.wire, x, y, z, yaw: 0 };
    call(bot, |tx| bot.conn.reducers.admin_put_stations_then(vec![p], tx))?;
    let fresh = || bot.conn.db.station().iter().find(|s| !before.contains(&s.id) && s.x == x && s.z == z).map(|s| s.id);
    wait_for("the new station", || fresh().is_some())?;
    fresh().ok_or_else(|| "the new station vanished".into())
}

fn grant(bot: &Bot, item: &str, count: u32) -> Check {
    call(bot, |tx| bot.conn.reducers.dev_grant_item_then(item.to_string(), count, tx))
}

fn craft(bot: &Bot, station: u64, r: &RecipeData, count: u32) -> Check {
    call(bot, |tx| bot.conn.reducers.craft_then(station, r.wire, count, tx)).map_err(|e| format!("{}: craft {} × {count}: {e}", bot.label, r.id))
}

fn refused(bot: &Bot, what: &str, want: &str, station: u64, r: &RecipeData, count: u32) -> Check {
    match call(bot, |tx| bot.conn.reducers.craft_then(station, r.wire, count, tx)) {
        Ok(()) => Err(format!("{what} was accepted")),
        Err(e) if e.contains(want) => {
            println!("{}: refused {what}: {e}", bot.label);
            Ok(())
        }
        Err(e) => Err(format!("{what}: unexpected refusal {e:?}")),
    }
}

/// Wait out `units` units of `r` until the job is gone; returns the most units the job showed made.
fn wait_done(bot: &Bot, r: &RecipeData, units: u32) -> Result<u32, String> {
    let limit = Instant::now() + Duration::from_secs_f32(units as f32 * r.craft_s + 5.0);
    let mut most = 0;
    while Instant::now() < limit {
        match bot.conn.db.my_craft().iter().next() {
            Some(j) => most = most.max(j.made),
            None => return Ok(most),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("the craft of {units} {} didn't finish in time", r.id))
}

/// Move `dx` metres east of the spot, at the ground's height where we stand.
fn step(bot: &Bot, dx: f32) -> Check {
    let p = bot.presence().ok_or("left the world")?;
    let x = SPOT.0 + dx;
    call(bot, |tx| bot.conn.reducers.move_event_then(p.last_seq + 1, 0, x, p.y, p.z, 0, 0, 1, tx))?;
    wait_for("the move", || bot.presence().is_some_and(|q| q.x == x))
}
