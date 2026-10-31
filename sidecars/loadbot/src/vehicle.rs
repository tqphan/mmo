//! The `vehicle` scenario (docs/design/vehicles.md). On the world's ground, or on a little test
//! shore it uploads far from spawn and removes at the end when the server has no terrain: bot A
//! summons a Furrow Cart from its bag and loads a pack; bot B loads its own, is refused unloading
//! A's and taking the reins, and rides in the bed while A drives it, with no corrections for
//! either. A is refused stowing it loaded, unloads both packs (one of them B's, a theft) and stows
//! it, which puts B off on the ground. Then A summons a Driftpine Skiff off the shore, B boards,
//! A rows it, and stowing it puts B in the water.

use std::time::Duration;

use sim::cell::{OPEN_WORLD, cell_of, window_cells};
use sim::ship::{Hull, HullData, Motion, stand, summon_motion, yaw_to_u16};
use sim::terrain::{FileChunks, HeightFile, ground};

use crate::gather::{call, subscribe};
use crate::land::{grant, refused};
use crate::module_bindings::*;
use crate::{Args, Bot, Check, RUN_SPEED, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// Our own test shore when the server has no terrain: 3 × 3 cells in the north-east corner, the
/// same as the land scenario's corner. Land 3 m up for 100 m from its west edge, then a 0.4 slope
/// into the sea.
pub(crate) const OWN_CELL: u16 = 225;
pub(crate) const OWN_CELLS: u16 = 3;
const DRIVE_S: f32 = 5.0;

pub(crate) fn own_height(x: f32, _z: f32) -> f32 {
    let lx = x - (f32::from(OWN_CELL) * sim::cell::CELL_SIZE_M - sim::cell::WORLD_HALF_M);
    if lx < 100.0 { 3.0 } else { (3.0 - 0.4 * (lx - 100.0)).max(-6.0) }
}

/// Where a bot standing at (x, z) facing `yaw` summons a vehicle of `hull` that can stand there and
/// for `ahead` metres straight on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Spot {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
}

pub(crate) fn find(file: &HeightFile, hull: &HullData, near: (f32, f32), ahead: f32) -> Option<Spot> {
    let mut chunks = FileChunks::new(file);
    // On known ground only: unknown terrain would float a boat.
    let mut ok = |m: &Motion| ground(&mut chunks, m.x, m.z).is_some() && stand(hull, m, |x, z| ground(&mut chunks, x, z)).is_some();
    let mut best: Option<(f32, Spot)> = None;
    for j in -150..=150 {
        for i in -150..=150 {
            let (x, z) = (near.0 + i as f32 * 4.0, near.1 + j as f32 * 4.0);
            let d = (x - near.0).powi(2) + (z - near.1).powi(2);
            if best.is_some_and(|(b, _)| b <= d) {
                continue;
            }
            for k in 0..8 {
                let yaw = k as f32 * std::f32::consts::FRAC_PI_4;
                let m = summon_motion(hull, [x, 0.0, z], yaw);
                let [fx, fz] = sim::ship::facing(yaw);
                let clear = (0..=(ahead as i32 / 2)).all(|s| ok(&Motion { x: m.x + fx * s as f32 * 2.0, z: m.z + fz * s as f32 * 2.0, ..m }));
                if clear {
                    best = Some((d, Spot { x, z, yaw }));
                    break;
                }
            }
        }
    }
    best.map(|(_, s)| s)
}

/// Face `yaw` where we stand, so a summon lands ahead of us.
pub(crate) fn face(bot: &Bot, yaw: f32) -> Check {
    let p = bot.presence().ok_or("left the world")?;
    call(bot, |tx| bot.conn.reducers.move_event_then(p.last_seq + 1, 0, p.x, p.y, p.z, yaw_to_u16(yaw), 0, p.mode, tx))?;
    wait_for("the turn", || bot.presence().is_some_and(|q| q.yaw == yaw_to_u16(yaw)))
}

/// The world position of a deck point of `v` as the bot sees it now.
pub(crate) fn deck_point(bot: &Bot, v: u64, l: [f32; 3]) -> Result<[f32; 3], String> {
    let t = bot.conn.db.transform().entity_id().find(&v).ok_or("can't see the vehicle")?;
    let pose = sim::ship::Pose { x: t.x, y: t.y, z: t.z, yaw: sim::ship::yaw_from_u16(t.yaw) };
    Ok(pose.to_world(l))
}

/// Stand beside deck point (x, z) of `v` and step onto it.
pub(crate) fn board(bot: &Bot, v: u64, x: f32, z: f32) -> Check {
    let w = deck_point(bot, v, [x, 0.0, z])?;
    bot.teleport(w[0], w[2])?;
    std::thread::sleep(Duration::from_millis(300));
    let p = bot.presence().ok_or("left the world")?;
    call(bot, |tx| bot.conn.reducers.move_event_then(p.last_seq + 1, v, x, 0.0, z, 0, (RUN_SPEED * 100.0) as i16, 1, tx))?;
    wait_for("boarding", || bot.presence().is_some_and(|q| q.parent == v))
}

/// The bot's vehicle item `id`, once it's in the bag.
pub(crate) fn bag_item(bot: &Bot, char_id: u64, id: &str) -> Result<u64, String> {
    let wire = sim::content::item_by_id(id).ok_or_else(|| format!("no item {id}"))?.wire;
    let find = || bot.conn.db.my_inventory().iter().find(|i| i.holder == char_id && i.def == wire).map(|i| i.id);
    wait_for(id, || find().is_some())?;
    find().ok_or_else(|| format!("no {id}"))
}

/// Summon `id` from the bag at `spot`; returns the vehicle once we see it.
pub(crate) fn summon(bot: &Bot, char_id: u64, id: &str, spot: Spot) -> Result<u64, String> {
    grant(bot, id, 1)?;
    let item = bag_item(bot, char_id, id)?;
    bot.teleport(spot.x, spot.z)?;
    face(bot, spot.yaw)?;
    call(bot, |tx| bot.conn.reducers.summon_vehicle_then(item, tx))?;
    let mine = || bot.conn.db.vehicle().iter().find(|v| v.owner == char_id).map(|v| v.entity_id);
    wait_for("the summoned vehicle", || mine().is_some_and(|v| bot.conn.db.transform().entity_id().find(&v).is_some()))?;
    mine().ok_or_else(|| "the vehicle vanished".into())
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
        (file, (x0 + 50.0, x0 + 96.0))
    } else {
        let w = worldtool::World::load(std::path::Path::new(worldtool::DEFAULT_DIR))?;
        if w.terrain_hash() != a.server_terrain().hash {
            return Err("the server's terrain isn't the world's; run `cargo xtask world`".into());
        }
        (w.heights, (-150.0, 30.0))
    };
    let cart_hull = Hull::FurrowCart.data();
    let skiff_hull = Hull::Skiff.data();
    let cart_spot = find(&file, cart_hull, near, 24.0).ok_or("no ground for a cart")?;
    let skiff_spot = find(&file, skiff_hull, (cart_spot.x, cart_spot.z), 24.0).ok_or("no water for a skiff")?;
    println!("on {}: a cart at ({:.0}, {:.0}), a skiff at ({:.0}, {:.0})", if own { "our own test shore" } else { "the world's ground" }, cart_spot.x, cart_spot.z, skiff_spot.x, skiff_spot.z);

    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    for bot in [&a, &b] {
        subscribe(bot, &["SELECT * FROM my_inventory", "SELECT * FROM my_pack"])?;
        for spot in [cart_spot, skiff_spot] {
            for c in window_cells(cell_of(OPEN_WORLD, spot.x, spot.z), 1) {
                subscribe(bot, &[&format!("SELECT * FROM vehicle WHERE cell = {c}"), &format!("SELECT * FROM transform WHERE cell = {c}")])?;
            }
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
    }
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);
    let corrections = |bot: &Bot| bot.presence().map_or(0, |p| p.corrections);
    let cargo = |v: u64| a.conn.db.vehicle().entity_id().find(&v).map(|v| v.cargo).unwrap_or_default();
    let bale = sim::content::item_by_id("grain_bale").ok_or("no grain_bale")?.wire;

    // A summons a cart and loads its pack from the ground behind it.
    let cart = summon(&a, pa.char_id, "furrow_cart", cart_spot)?;
    grant(&a, "furrow_cart", 1)?;
    let second = bag_item(&a, pa.char_id, "furrow_cart")?;
    refused(&a, "a second cart", "already have a land vehicle out", |tx| a.conn.reducers.summon_vehicle_then(second, tx))?;
    call(&a, |tx| a.conn.reducers.dev_give_pack_then("grain_bale".into(), tx))?;
    wait_for("A's pack", || a.conn.db.my_pack().count() == 1)?;
    call(&a, |tx| a.conn.reducers.load_pack_then(cart, 0, tx))?;
    wait_for("the pack in slot 0", || cargo(cart) == vec![bale, 0])?;
    println!("A summoned a Furrow Cart ({cart}) and loaded its pack");

    // B loads its own, can't take A's off or drive, and rides in the bed.
    let slot1 = &cart_hull.slots[1];
    let w = deck_point(&b, cart, [slot1.x + 1.5, 0.0, slot1.z])?;
    b.teleport(w[0], w[2])?;
    refused(&b, "loading without a pack", "not carrying a pack", |tx| b.conn.reducers.load_pack_then(cart, 1, tx))?;
    call(&b, |tx| b.conn.reducers.dev_give_pack_then("grain_bale".into(), tx))?;
    wait_for("B's pack", || b.conn.db.my_pack().count() == 1)?;
    refused(&b, "a full slot", "that slot is full", |tx| b.conn.reducers.load_pack_then(cart, 0, tx))?;
    call(&b, |tx| b.conn.reducers.load_pack_then(cart, 1, tx))?;
    wait_for("B's pack in slot 1", || cargo(cart) == vec![bale, bale])?;
    refused(&b, "unloading A's cart", "only the owner can unload this", |tx| b.conn.reducers.unload_pack_then(cart, 0, tx))?;
    board(&b, cart, cart_hull.helm_x, cart_hull.helm_z)?;
    refused(&b, "B at the reins", "only the owner can drive this", |tx| b.conn.reducers.take_helm_then(tx))?;
    let pb_now = b.presence().ok_or("B left")?;
    call(&b, |tx| b.conn.reducers.move_event_then(pb_now.last_seq + 1, cart, 0.0, 0.0, 1.0, 0, (RUN_SPEED * 100.0) as i16, 1, tx))?;
    println!("B loaded its pack, was refused A's pack and the reins, and climbed into the bed");

    // A drives for a few seconds with B aboard: no corrections.
    let (ca, cb) = (corrections(&a), corrections(&b));
    board(&a, cart, cart_hull.helm_x, cart_hull.helm_z + 0.5)?;
    call(&a, |tx| a.conn.reducers.take_helm_then(tx))?;
    let from = a.conn.db.transform().entity_id().find(&cart).ok_or("no cart")?;
    call(&a, |tx| a.conn.reducers.helm_then(100, 0, tx))?;
    std::thread::sleep(Duration::from_secs_f32(DRIVE_S));
    call(&a, |tx| a.conn.reducers.helm_then(0, 0, tx))?;
    std::thread::sleep(Duration::from_secs(2));
    let to = a.conn.db.transform().entity_id().find(&cart).ok_or("no cart")?;
    let moved = ((to.x - from.x).powi(2) + (to.z - from.z).powi(2)).sqrt();
    if moved < 10.0 {
        return Err(format!("the cart moved only {moved:.1} m in {DRIVE_S} s"));
    }
    if (corrections(&a), corrections(&b)) != (ca, cb) {
        return Err(format!("corrections while driving: A {}, B {}", corrections(&a) - ca, corrections(&b) - cb));
    }
    if b.presence().map(|p| p.parent) != Some(cart) {
        return Err("B fell off the cart".into());
    }
    println!("A drove the cart {moved:.1} m with B riding, deck {:.2} → {:.2} m, no corrections", from.y, to.y);

    // A unloads both packs (B's is a theft) and stows the cart, which puts B down.
    refused(&a, "stowing it loaded", "unload its cargo first", |tx| a.conn.reducers.stow_vehicle_then(cart, tx))?;
    for slot in [0, 1] {
        call(&a, |tx| a.conn.reducers.unload_pack_then(cart, slot, tx))?;
        wait_for("the pack on A's back", || a.conn.db.my_pack().count() == 1)?;
        let pk = a.conn.db.my_pack().iter().next().ok_or("no pack")?;
        if pk.owner != if slot == 0 { pa.char_id } else { pb.char_id } {
            return Err(format!("slot {slot} held {pk:?}"));
        }
        call(&a, |tx| a.conn.reducers.dev_discard_pack_then(tx))?;
        wait_for("the pack gone", || a.conn.db.my_pack().count() == 0)?;
    }
    let cb = corrections(&b);
    call(&a, |tx| a.conn.reducers.stow_vehicle_then(cart, tx))?;
    wait_for("B put down", || b.presence().is_some_and(|p| p.parent == 0) && corrections(&b) == cb + 1)?;
    wait_for("the cart gone", || a.conn.db.vehicle().entity_id().find(&cart).is_none())?;
    bag_item(&a, pa.char_id, "furrow_cart")?;
    println!("A unloaded both packs and stowed the cart back into its bag; B was put down on the ground");

    // The skiff: summoned off the shore, rowed with B aboard, stowed with B in the water.
    let skiff = summon(&a, pa.char_id, "driftpine_skiff", skiff_spot)?;
    board(&b, skiff, 0.0, 1.0)?;
    board(&a, skiff, skiff_hull.helm_x, skiff_hull.helm_z)?;
    call(&a, |tx| a.conn.reducers.take_helm_then(tx))?;
    let (ca, cb) = (corrections(&a), corrections(&b));
    call(&a, |tx| a.conn.reducers.helm_then(100, 0, tx))?;
    std::thread::sleep(Duration::from_secs_f32(DRIVE_S));
    call(&a, |tx| a.conn.reducers.helm_then(0, 0, tx))?;
    if (corrections(&a), corrections(&b)) != (ca, cb) {
        return Err(format!("corrections while rowing: A {}, B {}", corrections(&a) - ca, corrections(&b) - cb));
    }
    let cb = corrections(&b);
    call(&a, |tx| a.conn.reducers.stow_vehicle_then(skiff, tx))?;
    wait_for("B in the water", || b.presence().is_some_and(|p| p.parent == 0 && p.mode == sim::movement::MoveMode::Swim as u8) && corrections(&b) == cb + 1)?;
    println!("A rowed the skiff with B aboard and stowed it; B was left swimming");

    // Clean up: the items and our test shore.
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
