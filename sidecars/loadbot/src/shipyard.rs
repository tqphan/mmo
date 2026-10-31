//! The `shipyard` scenario (docs/design/vehicles.md §2B). On the world's sea, or on the vehicle
//! scenario's test shore when the server has no terrain: bot A places a Swiftgull Shipyard; B, a
//! friend, delivers the Timber Bundle; A the rest; both fit it out; B can't launch, A does. A
//! summons the Swiftgull and loads a pack; a hit puts it under fire, and B repairs it once that's
//! over. A second hit sinks it: B is put in the water, swims to A's floating pack and takes it.
//! A's Swiftgull comes back as a wreck that won't summon until it's mended and the wait, run by
//! the dev clock, is over.

use std::time::Duration;

use sim::cell::{OPEN_WORLD, cell_of, window_cells};
use sim::content::{Hull, SHIPYARDS, VEHICLES, item_by_id};
use sim::terrain::HeightFile;

use crate::gather::{advance, call, subscribe};
use crate::land::{grant, refused};
use crate::module_bindings::*;
use crate::vehicle::{OWN_CELL, OWN_CELLS, Spot, bag_item, board, face, find, own_height, summon};
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// Give the bot a fresh pack of `id` and wait until it's on its back.
fn pack(bot: &Bot, id: &str) -> Check {
    call(bot, |tx| bot.conn.reducers.dev_give_pack_then(id.into(), tx))?;
    wait_for(id, || bot.conn.db.my_pack().count() == 1)
}

pub fn scenario(args: &Args) -> Check {
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    call(&a, |tx| a.conn.reducers.dev_set_gm_then(3, tx))?;
    let own = a.server_terrain().hash == 0;
    let (file, near) = if own {
        let file = HeightFile::generate(OWN_CELL, OWN_CELL, OWN_CELLS, OWN_CELLS, own_height);
        let chunks: Vec<TerrainChunk> = file.chunks().into_iter().map(|(cell, samples)| TerrainChunk { cell, samples }).collect();
        call(&a, |tx| a.conn.reducers.admin_put_terrain_then(chunks, tx))?;
        let (x0, _) = file.origin();
        (file, (x0 + 110.0, x0 + 96.0))
    } else {
        let w = worldtool::World::load(std::path::Path::new(worldtool::DEFAULT_DIR))?;
        if w.terrain_hash() != a.server_terrain().hash {
            return Err("the server's terrain isn't the world's; run `cargo xtask world`".into());
        }
        (w.heights, (-400.0, 30.0))
    };
    let hull = Hull::Clipper.data();
    let spot: Spot = find(&file, hull, near, 0.0).ok_or("no water deep enough for a shipyard")?;
    println!("on {}: a shipyard off ({:.0}, {:.0})", if own { "our own test shore" } else { "the world's sea" }, spot.x, spot.z);

    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    for bot in [&a, &b] {
        subscribe(bot, &["SELECT * FROM my_inventory", "SELECT * FROM my_pack", "SELECT * FROM my_wrecks", "SELECT * FROM my_vehicles", "SELECT * FROM game_clock"])?;
        for c in window_cells(cell_of(OPEN_WORLD, spot.x, spot.z), 1) {
            subscribe(
                bot,
                &[
                    &format!("SELECT * FROM shipyard WHERE cell = {c}"),
                    &format!("SELECT * FROM vehicle WHERE cell = {c}"),
                    &format!("SELECT * FROM transform WHERE cell = {c}"),
                    &format!("SELECT * FROM dropped_pack WHERE cell = {c}"),
                ],
            )?;
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
        call(bot, |tx| bot.conn.reducers.dev_set_labor_then(5000, tx))?;
    }
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);
    let corrections = |bot: &Bot| bot.presence().map_or(0, |p| p.corrections);
    let (ca, cb) = (corrections(&a), corrections(&b));

    // A places the yard; a second is refused.
    grant(&a, "swiftgull_design", 2)?;
    let design = bag_item(&a, pa.char_id, "swiftgull_design")?;
    a.teleport(spot.x, spot.z)?;
    face(&a, spot.yaw)?;
    call(&a, |tx| a.conn.reducers.place_shipyard_then(design, tx))?;
    let yard = || a.conn.db.shipyard().iter().find(|y| y.owner == pa.char_id);
    wait_for("the shipyard", || yard().is_some())?;
    let id = yard().ok_or("no yard")?.id;
    let other = bag_item(&a, pa.char_id, "swiftgull_design")?;
    refused(&a, "a second shipyard", "you already have a shipyard", |tx| a.conn.reducers.place_shipyard_then(other, tx))?;
    println!("A placed a Swiftgull Shipyard ({id})");

    // The stages: B brings the timber, A the iron and sailcloth; both fit it out.
    let stage = || yard().map_or(u8::MAX, |y| y.stage);
    let work = |bot: &Bot, want: u8| -> Check {
        call(bot, |tx| bot.conn.reducers.work_shipyard_then(id, tx))?;
        wait_for("the next stage", || stage() == want)
    };
    b.teleport(spot.x, spot.z + 1.0)?;
    refused(&b, "a stage without its pack", "this stage needs a Timber Bundle", |tx| b.conn.reducers.work_shipyard_then(id, tx))?;
    pack(&b, "timber_bundle")?;
    work(&b, 1)?;
    pack(&a, "grain_bale")?;
    refused(&a, "the wrong pack", "this stage needs an Ironwork Crate", |tx| a.conn.reducers.work_shipyard_then(id, tx))?;
    call(&a, |tx| a.conn.reducers.dev_discard_pack_then(tx))?;
    wait_for("the bale gone", || a.conn.db.my_pack().count() == 0)?;
    for (i, p) in ["ironwork_crate", "sailcloth_bale"].into_iter().enumerate() {
        pack(&a, p)?;
        work(&a, 2 + i as u8)?;
    }
    work(&b, 4)?;
    work(&a, 5)?;
    refused(&b, "B launching A's ship", "only the owner can launch it", |tx| b.conn.reducers.work_shipyard_then(id, tx))?;
    let stages = SHIPYARDS[0].stages.len();
    call(&a, |tx| a.conn.reducers.work_shipyard_then(id, tx))?;
    wait_for("the launch", || yard().is_none())?;
    bag_item(&a, pa.char_id, "swiftgull")?;
    println!("all {stages} stages done, B helping with two; A launched a Swiftgull into its bag");

    // A sails it out and loads a pack.
    let ship = summon(&a, pa.char_id, "swiftgull", spot)?;
    let slot = &hull.slots[0];
    board(&a, ship, slot.x, slot.z - 1.5)?;
    pack(&a, "grain_bale")?;
    call(&a, |tx| a.conn.reducers.load_pack_then(ship, 0, tx))?;
    board(&b, ship, 0.0, 0.0)?;
    let hp = || a.conn.db.vehicle().entity_id().find(&ship).map_or(-1, |v| v.hp);

    // A hit: under fire, then repaired by B.
    call(&b, |tx| b.conn.reducers.dev_damage_vehicle_then(ship, 3000, tx))?;
    wait_for("the hit", || hp() == hull.hp - 3000)?;
    refused(&a, "stowing under fire", "it's under fire", |tx| a.conn.reducers.stow_vehicle_then(ship, tx))?;
    grant(&b, "hullmenders_kit", 1)?;
    bag_item(&b, pb.char_id, "hullmenders_kit")?;
    refused(&b, "repairing under fire", "can't repair it under fire", |tx| b.conn.reducers.repair_vehicle_then(ship, tx))?;
    std::thread::sleep(Duration::from_secs(VEHICLES.under_fire_s as u64 + 1));
    call(&b, |tx| b.conn.reducers.repair_vehicle_then(ship, tx))?;
    wait_for("the repair", || hp() == hull.hp - 3000 + hull.hp / 10)?;
    println!("a hit put the Swiftgull under fire; B repaired it to {} HP once that was over", hp());

    // It sinks: B is put in the water and takes A's floating pack.
    call(&b, |tx| b.conn.reducers.dev_damage_vehicle_then(ship, 99_999, tx))?;
    wait_for("the sinking", || hp() == 0)?;
    refused(&a, "stowing a sinking ship", "it's sinking", |tx| a.conn.reducers.stow_vehicle_then(ship, tx))?;
    let gone = || a.conn.db.vehicle().entity_id().find(&ship).is_none();
    let sink = Duration::from_secs(VEHICLES.sink_s as u64 + 5);
    let started = std::time::Instant::now();
    while !gone() && started.elapsed() < sink {
        std::thread::sleep(Duration::from_millis(100));
    }
    if !gone() {
        return Err("the ship never went down".into());
    }
    wait_for("B in the water", || b.presence().is_some_and(|p| p.parent == 0 && p.mode == sim::movement::MoveMode::Swim as u8))?;
    let bale = item_by_id("grain_bale").ok_or("no grain_bale")?.wire;
    let floating = || b.conn.db.dropped_pack().iter().find(|d| d.owner == pa.char_id && d.def == bale);
    wait_for("the floating pack", || floating().is_some())?;
    let d = floating().ok_or("no pack")?;
    b.teleport(d.x, d.z)?;
    call(&b, |tx| b.conn.reducers.pick_up_pack_then(d.id, tx))?;
    wait_for("the pack on B's back", || b.conn.db.my_pack().iter().any(|p| p.owner == pa.char_id))?;
    println!("the Swiftgull sank after {:.1} s; B swam to A's floating Grain Bale at y = {:.1} and took it", started.elapsed().as_secs_f32(), d.y);

    // The wreck: refused until mended and the wait is over.
    let wreck = || a.conn.db.my_wrecks().iter().next();
    wait_for("the wreck", || wreck().is_some())?;
    let item = wreck().ok_or("no wreck")?.item_id;
    a.teleport(spot.x, spot.z)?;
    face(&a, spot.yaw)?;
    refused(&a, "summoning a wreck", "it's wrecked: mend it first", |tx| a.conn.reducers.summon_vehicle_then(item, tx))?;
    refused(&a, "mending without kits", "mending it takes", |tx| a.conn.reducers.mend_vehicle_then(item, tx))?;
    grant(&a, "hullmenders_kit", hull.mend_kits as u32)?;
    bag_item(&a, pa.char_id, "hullmenders_kit")?;
    call(&a, |tx| a.conn.reducers.mend_vehicle_then(item, tx))?;
    wait_for("mending", || wreck().is_some_and(|w| w.mending))?;
    refused(&a, "summoning it too soon", "still being mended", |tx| a.conn.reducers.summon_vehicle_then(item, tx))?;
    advance(&a, (i64::from(hull.mend_s) + 1) * 1_000_000)?;
    call(&a, |tx| a.conn.reducers.summon_vehicle_then(item, tx))?;
    wait_for("the mended Swiftgull", || a.conn.db.my_vehicles().iter().any(|v| v.hp == hull.hp))?;
    let again = a.conn.db.my_vehicles().iter().next().ok_or("no vehicle")?.entity_id;
    println!("A mended the wreck with {} kits, waited {} s by the dev clock and summoned it again", hull.mend_kits, hull.mend_s);

    // Only the sinking snaps anyone: both were aboard and were put in the water.
    let (da, db) = (corrections(&a) - ca, corrections(&b) - cb);
    println!("corrections: A {da}, B {db} (each put in the water as it sank)");
    if da > 1 || db > 1 {
        return Err(format!("unexpected corrections: A {da}, B {db}"));
    }

    // Clean up: the ship, the items, B's pack and our test shore.
    call(&a, |tx| a.conn.reducers.stow_vehicle_then(again, tx))?;
    call(&b, |tx| b.conn.reducers.dev_discard_pack_then(tx))?;
    for (bot, char_id) in [(&a, pa.char_id), (&b, pb.char_id)] {
        let stacks: Vec<(u64, u32)> = bot.conn.db.my_inventory().iter().filter(|i| i.holder == char_id).map(|i| (i.id, i.count)).collect();
        for (id, count) in stacks {
            call(bot, |tx| bot.conn.reducers.destroy_item_then(id, count, tx))?;
        }
    }
    if own {
        let cells: Vec<u64> = file.chunks().into_iter().map(|(cell, _)| cell).collect();
        call(&a, |tx| a.conn.reducers.admin_remove_terrain_then(cells, tx))?;
    }
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}
