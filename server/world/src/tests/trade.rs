//! Trade packs (docs/design/trade.md).

use super::*;
use crate::crafting::{StationPlacement, admin_put_stations, craft, craft_done, craft_job, station};
use crate::items::{grant, item, place};
use crate::labor::labor;
use crate::trade::{
    DroppedPack, Pack, PostPlacement, TradeDemand, admin_clear_posts, admin_put_posts, admin_remove_posts, drop_pack, dropped_pack, my_pack,
    my_purse, pack, pick_up_pack, purse, sell_pack, trade_demand, trade_post,
};
use sim::content::item_by_id;
use sim::craft::{craft_micros, recipe_by_id, station_by_id};
use sim::movement::MoveMode;
use sim::trade::{FULL, TRADE, demand_key, post_by_id, price, route_pct};

const ADMIN: u128 = 99;
const H_MS: i64 = 3_600_000;

fn wire(id: &str) -> u16 {
    item_by_id(id).unwrap().wire
}

fn as_admin(t: &TestContext) {
    call(t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::session::record_owner(ctx);
        Ok(())
    })
    .unwrap();
}

/// A Packwright's Bench at `(x, 0, z)`; returns its id.
fn bench(t: &TestContext, x: f32, z: f32) -> u64 {
    as_admin(t);
    let p = StationPlacement { kind: station_by_id("packwright_bench").unwrap().wire, x, y: 0.0, z, yaw: 0 };
    call(t, ADMIN, |ctx| admin_put_stations(ctx, vec![p])).unwrap();
    read(t, |ctx| ctx.db.station().iter().map(|s| s.id).max().unwrap())
}

/// A post of kind `id` at `(x, 0, z)`; returns its id.
fn post(t: &TestContext, id: &str, x: f32, z: f32) -> u64 {
    as_admin(t);
    let p = PostPlacement { kind: post_by_id(id).unwrap().wire, x, y: 0.0, z, yaw: 0 };
    call(t, ADMIN, |ctx| admin_put_posts(ctx, vec![p])).unwrap();
    read(t, |ctx| ctx.db.trade_post().iter().map(|s| s.id).max().unwrap())
}

fn give(t: &TestContext, char_id: u64, id: &str, count: u32) {
    read(t, |ctx| grant(ctx, char_id, wire(id), count)).unwrap();
}

fn held(t: &TestContext, char_id: u64, item_id: &str) -> u32 {
    let w = wire(item_id);
    read(t, |ctx| ctx.db.item().by_slot().filter((char_id, place::BAG)).filter(|i| i.def == w).map(|i| i.count).sum())
}

fn points(t: &TestContext, player: u128) -> i32 {
    read(t, |ctx| ctx.db.labor().identity().find(identity(player)).unwrap().points)
}

fn back(t: &TestContext, char_id: u64) -> Option<Pack> {
    read(t, |ctx| ctx.db.pack().carrier().find(char_id))
}

fn shown(t: &TestContext, player: u128) -> u16 {
    let e = presence_of(t, player).entity_id;
    read(t, |ctx| ctx.db.entity().id().find(e).unwrap().pack)
}

fn copper(t: &TestContext, char_id: u64) -> u64 {
    read(t, |ctx| ctx.db.purse().char_id().find(char_id).map_or(0, |p| p.copper))
}

fn dropped(t: &TestContext) -> Vec<DroppedPack> {
    read(t, |ctx| ctx.db.dropped_pack().iter().collect())
}

/// Move `player` to `(x, 0, z)` `ms` later, running; returns whether the server accepted it.
fn walk(t: &TestContext, player: u128, x: f32, z: f32, ms: i64) -> bool {
    advance_ms(t, ms);
    let before = presence_of(t, player).corrections;
    let seq = presence_of(t, player).last_seq + 1;
    call(t, player, |ctx| move_event(ctx, seq, 0, x, 0.0, z, 0, 0, MoveMode::Run as u8)).unwrap();
    presence_of(t, player).corrections == before
}

/// Put `player` at `(x, 0, z)` as the server would after legal moves.
fn place_at(t: &TestContext, player: u128, x: f32, z: f32) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        ctx.db.presence().identity().update(Presence { x, y: 0.0, z, ..p });
    });
}

/// Put a fresh pack of `id` on `player`'s back, made where it stands, as a dev build's `dev_give_pack` does.
fn give_pack(t: &TestContext, player: u128, id: &str) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        crate::trade::made(ctx, &p, wire(id), [p.x, p.y, p.z]);
    });
}

/// Craft a Grain Bale at `bench_id` and let it finish.
fn craft_bale(t: &TestContext, player: u128, char_id: u64, bench_id: u64) {
    give(t, char_id, "hearthgrain", 20);
    give(t, char_id, "reedcloth", 2);
    let r = recipe_by_id("grain_bale").unwrap();
    call(t, player, |ctx| craft(ctx, bench_id, r.wire, 1)).unwrap();
    let job = read(t, |ctx| ctx.db.craft_job().char_id().find(char_id)).unwrap();
    advance_ms(t, craft_micros(r) / 1000);
    let id = job.scheduled_id;
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| craft_done(ctx, job)).unwrap();
    read(t, |ctx| ctx.db.craft_job().scheduled_id().delete(id));
}

#[test]
fn crafting_a_pack_puts_it_on_the_back() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let b = bench(&t, 2.0, 1.0);
    craft_bale(&t, 1, me.char_id, b);
    let pk = back(&t, me.char_id).expect("a pack on the back");
    assert_eq!((pk.def, pk.owner, pk.origin_x, pk.origin_z), (wire("grain_bale"), me.char_id, 2.0, 1.0));
    assert_eq!(held(&t, me.char_id, "grain_bale"), 0, "never in the bag");
    assert_eq!((held(&t, me.char_id, "hearthgrain"), held(&t, me.char_id, "reedcloth")), (0, 0));
    assert_eq!(points(&t, 1), 5000 - recipe_by_id("grain_bale").unwrap().labor);
    assert_eq!(shown(&t, 1), wire("grain_bale"));
    let mine = my_pack(&t.view_context(auth(1)));
    assert_eq!(mine.len(), 1);
    let other = arrive(&t, 2, "Tobin");
    assert!(my_pack(&t.view_context(auth(2))).is_empty(), "only your own");
    assert!(back(&t, other.char_id).is_none());
}

#[test]
fn pack_crafts_are_one_at_a_time_onto_a_free_back() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let b = bench(&t, 2.0, 0.0);
    let r = recipe_by_id("grain_bale").unwrap();
    give(&t, me.char_id, "hearthgrain", 40);
    give(&t, me.char_id, "reedcloth", 4);
    assert_eq!(call(&t, 1, |ctx| craft(ctx, b, r.wire, 2)), Err("packs are made one at a time".into()));
    // A back filled while it crafts ends the job with nothing spent.
    call(&t, 1, |ctx| craft(ctx, b, r.wire, 1)).unwrap();
    give_pack(&t, 1, "preserves_crate");
    let job = read(&t, |ctx| ctx.db.craft_job().char_id().find(me.char_id)).unwrap();
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| craft_done(ctx, job)).unwrap();
    assert_eq!(back(&t, me.char_id).map(|p| p.def), Some(wire("preserves_crate")));
    assert_eq!((held(&t, me.char_id, "hearthgrain"), points(&t, 1)), (40, 5000));
    read(&t, |ctx| ctx.db.craft_job().char_id().delete(me.char_id));
    assert_eq!(call(&t, 1, |ctx| craft(ctx, b, r.wire, 1)), Err("already carrying a pack".into()));
}

#[test]
fn carriers_are_slower_except_on_the_first_move() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let run = MoveMode::Run.data().max_speed_mps;
    // About 90% of the run limit over a second: fine without a pack.
    assert!(walk(&t, 1, run * 0.9, 0.0, 1000));
    // The pack goes on mid-stride: the move under way is still judged at full speed.
    give_pack(&t, 1, "grain_bale");
    assert!(walk(&t, 1, run * 1.8, 0.0, 1000), "the first move after the pack went on");
    // From then on, the slower limit.
    assert!(!walk(&t, 1, run * 2.7, 0.0, 1000), "a full-speed move with a pack is corrected");
    let slow = sim::trade::carry_limit(run);
    let x = presence_of(&t, 1).x;
    assert!(walk(&t, 1, x + slow, 0.0, 1000), "the carrier's speed is fine");
    // Dropped, full speed again.
    call(&t, 1, drop_pack).unwrap();
    let x = presence_of(&t, 1).x;
    assert!(walk(&t, 1, x + run, 0.0, 1000));
    assert_eq!(me.char_id, presence_of(&t, 1).char_id);
}

#[test]
fn dropping_and_picking_up() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let thief = arrive(&t, 2, "Tobin");
    give_pack(&t, 1, "grain_bale");
    place_at(&t, 1, 10.0, 10.0);
    call(&t, 1, drop_pack).unwrap();
    assert!(back(&t, me.char_id).is_none());
    assert_eq!(shown(&t, 1), 0);
    let d = dropped(&t).pop().expect("on the ground");
    assert_eq!((d.x, d.z, d.owner, d.def), (10.0, 10.0, me.char_id, wire("grain_bale")));
    assert_eq!(d.cell, sim::cell::cell_of(sim::cell::OPEN_WORLD, 10.0, 10.0));
    assert_eq!(call(&t, 1, drop_pack), Err("not carrying a pack".into()));

    // Out of reach, then theft by another account is allowed.
    place_at(&t, 2, 10.0 + TRADE.reach_m + 0.5, 10.0);
    assert_eq!(call(&t, 2, |ctx| pick_up_pack(ctx, d.id)), Err("out of reach".into()));
    place_at(&t, 2, 11.0, 10.0);
    call(&t, 2, |ctx| pick_up_pack(ctx, d.id)).unwrap();
    let pk = back(&t, thief.char_id).unwrap();
    assert_eq!((pk.owner, pk.made_at, pk.origin_x), (me.char_id, d.made_at, d.origin_x), "it keeps its owner and origin");
    assert_eq!(shown(&t, 2), wire("grain_bale"));
    assert_eq!(call(&t, 2, |ctx| pick_up_pack(ctx, d.id)), Err("already carrying a pack".into()));
    call(&t, 2, drop_pack).unwrap();
    let d = dropped(&t).pop().unwrap();
    assert_eq!(call(&t, 1, |ctx| pick_up_pack(ctx, d.id + 100)), Err("no such pack".into()));

    // Aboard and swimming are refused.
    place_at(&t, 1, 11.0, 10.0);
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { mode: MoveMode::Swim as u8, ..p });
    });
    assert_eq!(call(&t, 1, |ctx| pick_up_pack(ctx, d.id)), Err("can't pick up a pack while swimming".into()));
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { mode: 1, parent: 7, ..p });
    });
    assert_eq!(call(&t, 1, |ctx| pick_up_pack(ctx, d.id)), Err("can't pick up a pack from a ship".into()));
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { parent: 0, ..p });
    });
    // The owner takes it back.
    call(&t, 1, |ctx| pick_up_pack(ctx, d.id)).unwrap();
    assert!(dropped(&t).is_empty());
}

#[test]
fn dropping_is_refused_aboard_and_swimming() {
    let t = world();
    arrive(&t, 1, "Mira");
    give_pack(&t, 1, "grain_bale");
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { mode: MoveMode::Swim as u8, ..p });
    });
    assert_eq!(call(&t, 1, drop_pack), Err("can't drop a pack in the water".into()));
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { mode: 1, parent: 7, ..p });
    });
    assert_eq!(call(&t, 1, drop_pack), Err("can't drop a pack on a ship yet".into()));
}

#[test]
fn spoiled_packs_fall_apart_and_cannot_be_picked_up_or_sold() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let far = post(&t, "gullhaven_post", 2000.0, 0.0);
    give_pack(&t, 1, "grain_bale");
    call(&t, 1, drop_pack).unwrap();
    let d = dropped(&t).pop().unwrap();
    advance_ms(&t, i64::from(TRADE.expire_s) * 1000);
    assert_eq!(call(&t, 1, |ctx| pick_up_pack(ctx, d.id)), Err("this pack has spoiled".into()));
    give_pack(&t, 1, "grain_bale");
    advance_ms(&t, i64::from(TRADE.expire_s) * 1000);
    place_at(&t, 1, 2000.0, 1.0);
    assert_eq!(call(&t, 1, |ctx| sell_pack(ctx, far)), Err("this pack has spoiled".into()));
    call(&t, 1, drop_pack).unwrap();
    assert_eq!(dropped(&t).len(), 1, "the spoiled pack fell apart instead of being dropped");
    assert!(back(&t, me.char_id).is_none());
}

#[test]
fn selling_pays_by_route_demand_and_freshness() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let near = post(&t, "redfen_post", 100.0, 0.0);
    let far = post(&t, "gullhaven_post", 3000.0, 0.0);
    let value = sim::trade::pack(wire("grain_bale")).unwrap().value;
    let bonus = post_by_id("gullhaven_post").unwrap().bonus_pct;
    let route = route_pct([0.0, 0.0], [3000.0, 0.0], bonus).unwrap();

    assert_eq!(call(&t, 1, |ctx| sell_pack(ctx, far)), Err("not carrying a pack".into()));
    give_pack(&t, 1, "grain_bale");
    place_at(&t, 1, 100.0, 1.0);
    assert_eq!(call(&t, 1, |ctx| sell_pack(ctx, near)), Err("this post is too close to where the pack was made".into()));
    assert_eq!(call(&t, 1, |ctx| sell_pack(ctx, far)), Err("out of reach".into()));
    assert_eq!(call(&t, 1, |ctx| sell_pack(ctx, 12345)), Err("no such post".into()));
    place_at(&t, 1, 3000.0, 1.0);
    call(&t, 1, |ctx| sell_pack(ctx, far)).unwrap();
    assert_eq!(copper(&t, me.char_id), price(value, route, FULL, FULL));
    assert!(back(&t, me.char_id).is_none());
    assert_eq!(shown(&t, 1), 0);
    let key = demand_key(post_by_id("gullhaven_post").unwrap().wire, wire("grain_bale"));
    let drop = TRADE.sale_drop_pct as u32 * 10;
    let row = read(&t, |ctx| ctx.db.trade_demand().key().find(key)).unwrap();
    assert_eq!(u32::from(row.permille), FULL - drop);

    // A second sale pays less, and adds to the purse. Another pack kind's demand is untouched.
    give_pack(&t, 1, "grain_bale");
    read(&t, |ctx| {
        let pk = ctx.db.pack().carrier().find(me.char_id).unwrap();
        ctx.db.pack().carrier().update(Pack { origin_x: 0.0, origin_z: 0.0, ..pk });
    });
    let before = copper(&t, me.char_id);
    call(&t, 1, |ctx| sell_pack(ctx, far)).unwrap();
    assert_eq!(copper(&t, me.char_id) - before, price(value, route, FULL - drop, FULL));
    assert!(read(&t, |ctx| ctx.db.trade_demand().key().find(demand_key(2, wire("preserves_crate")))).is_none());

    // Demand recovers by the clock.
    advance_ms(&t, H_MS);
    give_pack(&t, 1, "grain_bale");
    read(&t, |ctx| {
        let pk = ctx.db.pack().carrier().find(me.char_id).unwrap();
        ctx.db.pack().carrier().update(Pack { origin_x: 0.0, origin_z: 0.0, ..pk });
    });
    let before = copper(&t, me.char_id);
    call(&t, 1, |ctx| sell_pack(ctx, far)).unwrap();
    let recovered = (FULL - 2 * drop + TRADE.recover_pct_per_h as u32 * 10).min(FULL);
    assert_eq!(copper(&t, me.char_id) - before, price(value, route, recovered, FULL));

    // A stale pack sells for less.
    give_pack(&t, 1, "grain_bale");
    read(&t, |ctx| {
        let pk = ctx.db.pack().carrier().find(me.char_id).unwrap();
        let made_at = Timestamp::from_micros_since_unix_epoch(pk.made_at.to_micros_since_unix_epoch() - i64::from(TRADE.expire_s) * 999_999);
        ctx.db.pack().carrier().update(Pack { origin_x: 0.0, origin_z: 0.0, made_at, ..pk });
    });
    let demand = read(&t, |ctx| ctx.db.trade_demand().key().find(key)).map(|d: TradeDemand| u32::from(d.permille)).unwrap();
    let before = copper(&t, me.char_id);
    call(&t, 1, |ctx| sell_pack(ctx, far)).unwrap();
    let paid = copper(&t, me.char_id) - before;
    assert!(paid < price(value, route, demand, FULL) * 81 / 100 && paid > 0, "{paid}");

    let mine = my_purse(&t.view_context(auth(1)));
    assert_eq!(mine.len(), 1);
    arrive(&t, 2, "Tobin");
    assert!(my_purse(&t.view_context(auth(2))).is_empty());
}

#[test]
fn demand_falls_to_a_floor() {
    let t = world();
    arrive(&t, 1, "Mira");
    let far = post(&t, "cairnwick_post", 1000.0, 0.0);
    let key = demand_key(post_by_id("cairnwick_post").unwrap().wire, wire("preserves_crate"));
    for _ in 0..40 {
        give_pack(&t, 1, "preserves_crate");
        read(&t, |ctx| {
            let pk = ctx.db.pack().carrier().find(presence_of_ctx(ctx, 1)).unwrap();
            ctx.db.pack().carrier().update(Pack { origin_x: 0.0, origin_z: 0.0, ..pk });
        });
        place_at(&t, 1, 1000.0, 1.0);
        call(&t, 1, |ctx| sell_pack(ctx, far)).unwrap();
    }
    let row = read(&t, |ctx| ctx.db.trade_demand().key().find(key)).unwrap();
    assert_eq!(u32::from(row.permille), TRADE.demand_min_pct as u32 * 10);
}

fn presence_of_ctx(ctx: &ReducerContext, player: u128) -> u64 {
    ctx.db.presence().identity().find(identity(player)).unwrap().char_id
}

#[test]
fn the_pack_stays_on_the_back_across_leaving() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    give_pack(&t, 1, "grain_bale");
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert!(back(&t, me.char_id).is_some());
    call(&t, 1, |ctx| enter_world(ctx, me.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    assert_eq!(shown(&t, 1), wire("grain_bale"));
}

#[test]
fn admins_place_and_remove_posts() {
    let t = world();
    arrive(&t, 1, "Mira");
    let p = PostPlacement { kind: 0, x: 1.0, y: 0.0, z: 1.0, yaw: 0 };
    assert_eq!(call(&t, 1, |ctx| admin_put_posts(ctx, vec![p.clone()])), Err("admins only".into()));
    as_admin(&t);
    assert!(call(&t, ADMIN, |ctx| admin_put_posts(ctx, vec![PostPlacement { kind: 200, ..p.clone() }])).unwrap_err().contains("unknown kind"));
    assert!(call(&t, ADMIN, |ctx| admin_put_posts(ctx, vec![PostPlacement { x: 9000.0, ..p.clone() }])).unwrap_err().contains("off the map"));
    let a = post(&t, "cairnwick_post", 1.0, 1.0);
    post(&t, "redfen_post", 2.0, 1.0);
    call(&t, ADMIN, |ctx| admin_remove_posts(ctx, vec![a])).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.trade_post().count()), 1);
    call(&t, ADMIN, admin_clear_posts).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.trade_post().count()), 0);
}
