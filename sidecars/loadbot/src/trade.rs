//! The `trade` scenario (docs/design/trade.md): bot A crafts a Grain Bale at a Packwright's Bench
//! the scenario places on the trade island, walks with it at the carrier's speed without a
//! correction and is corrected running at full speed. It drops the pack; bot B picks it up (a
//! theft) and drops it again; A takes it back, is refused at a post too close to the bench, and
//! sells it at a post 600 m away for the price `sim::trade` gives. Demand there falls and
//! recovers by the dev clock, and a spoiled pack is refused. Then it cleans up.
//!
//! On a server with the world's terrain it follows the ground of the world's files; without
//! terrain, heights aren't checked.

use std::time::Duration;

use sim::terrain::{FileChunks, HeightFile, ground};
use sim::trade::{FULL, TRADE, after_sale, carry_limit, demand_key, demand_now, post_by_id, quote};

use crate::gather::{advance, call, game_now, held, subscribe};
use crate::land::{grant, refused};
use crate::module_bindings::*;
use crate::{Args, Bot, Check, RUN_SPEED, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// On the trade island west of spawn, away from the other scenarios' spots.
const BENCH: (f32, f32) = (-150.0, 30.0);
/// 600 m east, still on the island: far enough to buy the bench's packs.
const FAR: (f32, f32) = (450.0, 30.0);
const STEP: Duration = Duration::from_millis(250);

/// The ground's height, from the world's files when the server has terrain, else 0.
pub(crate) struct Ground(Option<HeightFile>);

impl Ground {
    /// The ground `bot`'s server stands on: the world's files if it has terrain, which must be
    /// theirs, else none.
    pub(crate) fn of(bot: &Bot) -> Result<Self, String> {
        if bot.server_terrain().hash == 0 {
            return Ok(Ground(None));
        }
        let w = worldtool::World::load(std::path::Path::new(worldtool::DEFAULT_DIR))?;
        if w.terrain_hash() != bot.server_terrain().hash {
            return Err("the server's terrain isn't the world's; run `cargo xtask world`".into());
        }
        Ok(Ground(Some(w.heights)))
    }

    pub(crate) fn at(&self, x: f32, z: f32) -> f32 {
        self.0.as_ref().map_or(0.0, |h| ground(&mut FileChunks::new(h), x, z).map_or(0.0, |g| g.hi))
    }
}

pub fn scenario(args: &Args) -> Check {
    let bale = sim::content::item_by_id("grain_bale").ok_or("no grain_bale in content")?;
    let recipe = sim::craft::recipe_by_id("grain_bale").ok_or("no grain_bale recipe")?;
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    let land = Ground::of(&a)?;
    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    let cells: Vec<u64> = [BENCH, FAR].iter().flat_map(|&(x, z)| sim::cell::window_cells(sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z), 1)).collect();
    for bot in [&a, &b] {
        subscribe(
            bot,
            &[
                "SELECT * FROM my_labor",
                "SELECT * FROM my_inventory",
                "SELECT * FROM game_clock",
                "SELECT * FROM my_craft",
                "SELECT * FROM my_pack",
                "SELECT * FROM my_purse",
                "SELECT * FROM trade_demand",
            ],
        )?;
        for c in &cells {
            subscribe(bot, &[&format!("SELECT * FROM station WHERE cell = {c}"), &format!("SELECT * FROM trade_post WHERE cell = {c}"), &format!("SELECT * FROM dropped_pack WHERE cell = {c}")])?;
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
    }
    let (pa, pb) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?);

    // Our bench and two posts: one too close to the bench, one far enough.
    let (bx, bz) = BENCH;
    let bench = place_station(&a, bx + 2.0, land.at(bx + 2.0, bz), bz)?;
    let near = place_post(&a, "redfen_post", bx + 30.0, land.at(bx + 30.0, bz), bz)?;
    let far = place_post(&a, "gullhaven_post", FAR.0, land.at(FAR.0, FAR.1), FAR.1)?;
    println!("placed a Packwright's Bench ({bench}) at ({:.0}, {bz:.0}), a post 30 m off ({near}) and one 600 m off ({far})", bx + 2.0);

    // Craft a Grain Bale onto A's back.
    a.teleport(bx, bz)?;
    grant(&a, "hearthgrain", 20)?;
    grant(&a, "reedcloth", 2)?;
    wait_for("the inputs", || held(&a, pa.char_id, "reedcloth") == 2)?;
    refused(&a, "two packs at once", "packs are made one at a time", |tx| a.conn.reducers.craft_then(bench, recipe.wire, 2, tx))?;
    call(&a, |tx| a.conn.reducers.craft_then(bench, recipe.wire, 1, tx))?;
    std::thread::sleep(Duration::from_secs_f32(recipe.craft_s));
    wait_for("the pack on A's back", || a.conn.db.my_pack().iter().any(|p| p.carrier == pa.char_id))?;
    let pk = a.conn.db.my_pack().iter().next().ok_or("no pack")?;
    if (pk.def, pk.owner, held(&a, pa.char_id, "hearthgrain"), held(&a, pa.char_id, "reedcloth")) != (bale.wire, pa.char_id, 0, 0) {
        return Err(format!("after crafting: {pk:?}, {} hearthgrain, {} reedcloth", held(&a, pa.char_id, "hearthgrain"), held(&a, pa.char_id, "reedcloth")));
    }
    refused(&a, "a second pack", "already carrying a pack", |tx| a.conn.reducers.craft_then(bench, recipe.wire, 1, tx))?;
    println!("A crafted a Grain Bale made at ({:.0}, {:.0}) onto its back", pk.origin_x, pk.origin_z);

    // Walk east at 95% of the carrier's speed for 4 s: no corrections.
    let corrections = |bot: &Bot| bot.presence().map_or(0, |p| p.corrections);
    let before = corrections(&a);
    let slow = carry_limit(RUN_SPEED) * 0.95;
    for _ in 0..16 {
        std::thread::sleep(STEP);
        let p = a.presence().ok_or("A left")?;
        walk(&a, &land, p.x + slow * STEP.as_secs_f32(), p.z)?;
    }
    if corrections(&a) != before {
        return Err(format!("A was corrected walking at the carrier's speed: {} corrections", corrections(&a) - before));
    }
    // Then a second at the full run speed: too fast with a pack.
    std::thread::sleep(Duration::from_secs(1));
    let p = a.presence().ok_or("A left")?;
    walk(&a, &land, p.x + RUN_SPEED, p.z)?;
    wait_for("the correction", || corrections(&a) == before + 1)?;
    println!("A walked {:.1} m at {slow:.2} m/s with the pack, then was corrected running at {RUN_SPEED} m/s", 16.0 * slow * STEP.as_secs_f32());

    // A drops it; B takes it (a theft) and drops it; A takes it back.
    call(&a, |tx| a.conn.reducers.drop_pack_then(tx))?;
    wait_for("the dropped pack", || a.conn.db.dropped_pack().iter().any(|d| d.owner == pa.char_id))?;
    let d = a.conn.db.dropped_pack().iter().find(|d| d.owner == pa.char_id).ok_or("no dropped pack")?;
    b.teleport(d.x + 1.0, d.z)?;
    call(&b, |tx| b.conn.reducers.pick_up_pack_then(d.id, tx))?;
    wait_for("the pack on B's back", || b.conn.db.my_pack().iter().any(|p| p.carrier == pb.char_id && p.owner == pa.char_id))?;
    refused(&a, "a pack someone took", "no such pack", |tx| a.conn.reducers.pick_up_pack_then(d.id, tx))?;
    call(&b, |tx| b.conn.reducers.drop_pack_then(tx))?;
    wait_for("the pack on the ground again", || a.conn.db.dropped_pack().iter().any(|d| d.owner == pa.char_id))?;
    let d = a.conn.db.dropped_pack().iter().find(|d| d.owner == pa.char_id).ok_or("no dropped pack")?;
    a.teleport(d.x - 1.0, d.z)?;
    call(&a, |tx| a.conn.reducers.pick_up_pack_then(d.id, tx))?;
    wait_for("the pack back on A", || a.conn.db.my_pack().iter().any(|p| p.carrier == pa.char_id))?;
    println!("B stole A's dropped pack and dropped it; A picked it up again");

    // Too close, then sold far away for what sim::trade says.
    a.teleport(bx + 31.0, bz)?;
    refused(&a, "selling by the bench", "too close to where the pack was made", |tx| a.conn.reducers.sell_pack_then(near, tx))?;
    a.teleport(FAR.0 + 1.0, FAR.1)?;
    let gull = post_by_id("gullhaven_post").ok_or("no gullhaven_post")?;
    let key = demand_key(gull.wire, bale.wire);
    let demand = |bot: &Bot| bot.conn.db.trade_demand().key().find(&key).map_or(FULL, |d| demand_now(u32::from(d.permille), d.at.to_micros_since_unix_epoch(), game_now(bot)));
    let pk = a.conn.db.my_pack().iter().next().ok_or("no pack")?;
    let d0 = demand(&a);
    let q = quote(pk.def, [pk.origin_x, pk.origin_z], pk.made_at.to_micros_since_unix_epoch(), [FAR.0, FAR.1], gull.bonus_pct, d0, game_now(&a))
        .map_err(|r| r.message().to_string())?;
    let purse = |bot: &Bot, char_id: u64| bot.conn.db.my_purse().iter().find(|p| p.char_id == char_id).map_or(0, |p| p.copper);
    let had = purse(&a, pa.char_id);
    call(&a, |tx| a.conn.reducers.sell_pack_then(far, tx))?;
    wait_for("the payment", || purse(&a, pa.char_id) > had)?;
    let paid = purse(&a, pa.char_id) - had;
    // Demand may have recovered a few permille between our quote and the sale.
    if paid < q.copper || paid > q.copper + q.copper / 100 {
        return Err(format!("paid {paid}, expected about {} ({q:?})", q.copper));
    }
    if a.conn.db.my_pack().count() != 0 {
        return Err("the pack is still on A's back".into());
    }
    let row = || a.conn.db.trade_demand().key().find(&key);
    wait_for("the demand row", || row().is_some())?;
    let fell = u32::from(row().unwrap().permille);
    if fell < after_sale(d0) || fell > after_sale(d0) + 5 {
        return Err(format!("demand {d0} became {fell}, expected about {}", after_sale(d0)));
    }
    println!("A sold the Grain Bale at the far post for {} (route {}%, demand {}‰ → {fell}‰)", sim::trade::coins(paid), q.route_pct, q.demand);

    // Demand recovers by the clock.
    advance(&a, 3_600_000_000)?;
    let back = demand(&a);
    if back < (fell + TRADE.recover_pct_per_h as u32 * 10).min(FULL) {
        return Err(format!("demand recovered only to {back} from {fell} in an hour"));
    }
    println!("an hour later the post's demand is {back}‰");

    // A spoiled pack is refused, and falls apart when dropped.
    call(&a, |tx| a.conn.reducers.dev_give_pack_then("grain_bale".into(), tx))?;
    wait_for("a fresh pack", || a.conn.db.my_pack().count() == 1)?;
    advance(&a, i64::from(TRADE.expire_s) * 1_000_000)?;
    a.teleport(FAR.0 + 1.0, FAR.1)?;
    refused(&a, "a spoiled pack", "this pack has spoiled", |tx| a.conn.reducers.sell_pack_then(far, tx))?;
    let dropped_before = a.conn.db.dropped_pack().count();
    call(&a, |tx| a.conn.reducers.drop_pack_then(tx))?;
    wait_for("the spoiled pack gone", || a.conn.db.my_pack().count() == 0)?;
    std::thread::sleep(Duration::from_millis(300));
    if a.conn.db.dropped_pack().count() != dropped_before {
        return Err("a spoiled pack was left on the ground".into());
    }
    println!("a spoiled pack was refused, and fell apart when dropped");

    // Clean up our bench and posts.
    call(&a, |tx| a.conn.reducers.admin_remove_stations_then(vec![bench], tx))?;
    call(&a, |tx| a.conn.reducers.admin_remove_posts_then(vec![near, far], tx))?;
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Move to `(x, z)` on the ground, running.
pub(crate) fn walk(bot: &Bot, land: &Ground, x: f32, z: f32) -> Check {
    let p = bot.presence().ok_or("left the world")?;
    let y = land.at(x, z);
    call(bot, |tx| bot.conn.reducers.move_event_then(p.last_seq + 1, 0, x, y, z, 0, (RUN_SPEED * 100.0) as i16, 1, tx))?;
    wait_for("the move", || bot.presence().is_some_and(|q| q.last_seq > p.last_seq))
}

/// A Packwright's Bench, as an admin; returns its id once we see it.
fn place_station(bot: &Bot, x: f32, y: f32, z: f32) -> Result<u64, String> {
    let kind = sim::craft::station_by_id("packwright_bench").ok_or("no packwright_bench")?;
    let p = StationPlacement { kind: kind.wire, x, y, z, yaw: 0 };
    call(bot, |tx| bot.conn.reducers.admin_put_stations_then(vec![p], tx))?;
    let fresh = || bot.conn.db.station().iter().find(|s| s.x == x && s.z == z && s.kind == kind.wire).map(|s| s.id);
    wait_for("the new bench", || fresh().is_some())?;
    fresh().ok_or_else(|| "the new bench vanished".into())
}

/// A post of kind `id`, as an admin; returns its id once we see it.
fn place_post(bot: &Bot, id: &str, x: f32, y: f32, z: f32) -> Result<u64, String> {
    let kind = post_by_id(id).ok_or_else(|| format!("no post {id}"))?;
    let p = PostPlacement { kind: kind.wire, x, y, z, yaw: 0 };
    call(bot, |tx| bot.conn.reducers.admin_put_posts_then(vec![p], tx))?;
    let fresh = || bot.conn.db.trade_post().iter().find(|s| s.x == x && s.z == z).map(|s| s.id);
    wait_for("the new post", || fresh().is_some())?;
    fresh().ok_or_else(|| "the new post vanished".into())
}
