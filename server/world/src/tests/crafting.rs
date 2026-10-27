//! Crafting (docs/design/crafting.md).

use super::*;
use crate::crafting::{
    CraftJob, Station, StationPlacement, admin_clear_stations, admin_put_stations, admin_remove_stations, cancel_craft, craft, craft_done,
    craft_job, my_craft, station,
};
use crate::gathering::resource_node;
use crate::items::{grant, item, place};
use crate::labor::labor;
use sim::content::{BAG_SLOTS, item_by_id};
use sim::craft::{RecipeData, craft_micros, recipe_by_id, station_by_id};

const ADMIN: u128 = 99;

fn recipe(id: &str) -> &'static RecipeData {
    recipe_by_id(id).unwrap()
}

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

/// Place a station of kind `id` at `(x, 0, z)` as the admin; returns its id.
fn put(t: &TestContext, id: &str, x: f32, z: f32) -> u64 {
    as_admin(t);
    let before: Vec<u64> = read(t, |ctx| ctx.db.station().iter().map(|s| s.id).collect());
    let p = StationPlacement { kind: station_by_id(id).unwrap().wire, x, y: 0.0, z, yaw: 0 };
    call(t, ADMIN, |ctx| admin_put_stations(ctx, vec![p])).unwrap();
    read(t, |ctx| ctx.db.station().iter().map(|s| s.id).find(|id| !before.contains(id)).unwrap())
}

fn job_of(t: &TestContext, char_id: u64) -> Option<CraftJob> {
    read(t, |ctx| ctx.db.craft_job().char_id().find(char_id))
}

fn points(t: &TestContext, player: u128) -> i32 {
    read(t, |ctx| ctx.db.labor().identity().find(identity(player)).unwrap().points)
}

fn give(t: &TestContext, char_id: u64, id: &str, count: u32) {
    read(t, |ctx| grant(ctx, char_id, wire(id), count)).unwrap();
}

/// How many of `item` are in `char_id`'s bag.
fn held(t: &TestContext, char_id: u64, item_id: &str) -> u32 {
    let w = wire(item_id);
    read(t, |ctx| ctx.db.item().by_slot().filter((char_id, place::BAG)).filter(|i| i.def == w).map(|i| i.count).sum())
}

/// Run the current unit when it falls due, as the scheduler would: the reducer, then the
/// scheduler's own delete of the row that fired.
fn finish_unit(t: &TestContext, char_id: u64) {
    let job = job_of(t, char_id).expect("a craft under way");
    advance_ms(t, craft_micros(sim::craft::recipe(job.recipe).unwrap()) / 1000);
    let id = job.scheduled_id;
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| craft_done(ctx, job)).unwrap();
    read(t, |ctx| ctx.db.craft_job().scheduled_id().delete(id));
}

fn mv(t: &TestContext, player: u128, x: f32, z: f32) {
    let seq = presence_of(t, player).last_seq + 1;
    call(t, player, |ctx| move_event(ctx, seq, 0, x, 0.0, z, 0, 0, 1)).unwrap();
}

#[test]
fn a_batch_takes_inputs_and_labor_per_unit_and_reschedules() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let hearth = put(&t, "cookhearth", 2.0, 0.0);
    let r = recipe("charcoal"); // 2 logs → 3 charcoal, 5 labor
    give(&t, me.char_id, "driftpine_log", 7);
    call(&t, 1, |ctx| craft(ctx, hearth, r.wire, 3)).unwrap();
    let job = job_of(&t, me.char_id).unwrap();
    assert_eq!((job.made, job.count, job.station), (0, 3, hearth));
    assert_eq!(job.done_at.to_micros_since_unix_epoch() - T0, craft_micros(r));
    assert_eq!((held(&t, me.char_id, "driftpine_log"), points(&t, 1)), (7, 5000), "nothing spent until a unit finishes");

    finish_unit(&t, me.char_id);
    assert_eq!(held(&t, me.char_id, "driftpine_log"), 5);
    assert_eq!(held(&t, me.char_id, "charcoal"), 3);
    assert_eq!(points(&t, 1), 5000 - r.labor);
    let next = job_of(&t, me.char_id).expect("the next unit is scheduled");
    assert_eq!(next.made, 1);
    assert_ne!(next.scheduled_id, job.scheduled_id);
    assert_eq!(next.done_at.to_micros_since_unix_epoch() - T0, 2 * craft_micros(r));

    finish_unit(&t, me.char_id);
    finish_unit(&t, me.char_id);
    assert!(job_of(&t, me.char_id).is_none(), "done after 3");
    assert_eq!(held(&t, me.char_id, "driftpine_log"), 1);
    assert_eq!(held(&t, me.char_id, "charcoal"), 9);
    assert_eq!(points(&t, 1), 5000 - 3 * r.labor);
}

#[test]
fn a_batch_stops_when_the_inputs_run_out() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let hearth = put(&t, "cookhearth", 2.0, 0.0);
    give(&t, me.char_id, "driftpine_log", 5);
    call(&t, 1, |ctx| craft(ctx, hearth, recipe("charcoal").wire, 10)).unwrap();
    finish_unit(&t, me.char_id);
    assert_eq!(job_of(&t, me.char_id).map(|j| j.made), Some(1));
    finish_unit(&t, me.char_id);
    assert!(job_of(&t, me.char_id).is_none(), "one log left: no third unit is scheduled");
    assert_eq!((held(&t, me.char_id, "driftpine_log"), held(&t, me.char_id, "charcoal")), (1, 6));
}

#[test]
fn refusals_change_nothing() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let _them = arrive(&t, 2, "Tobin");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    let far = put(&t, "joiners_bench", 4.5, 0.0);
    let hearth = put(&t, "cookhearth", 0.0, 2.0);
    let twine = recipe("saltreed_twine"); // 4 saltreed, 5 labor
    give(&t, me.char_id, "saltreed", 4);
    let try_craft = |player: u128, st: u64, r: u16, n: u32| call(&t, player, |ctx| craft(ctx, st, r, n));
    assert_eq!(try_craft(3, bench, twine.wire, 1), Err("not in the world".into()));
    assert_eq!(try_craft(1, bench, 999, 1), Err("no such recipe".into()));
    assert_eq!(try_craft(1, bench, twine.wire, 0), Err("craft 1 to 100 at a time".into()));
    assert_eq!(try_craft(1, bench, twine.wire, 101), Err("craft 1 to 100 at a time".into()));
    assert_eq!(try_craft(1, 12345, twine.wire, 1), Err("no such station".into()));
    assert_eq!(try_craft(1, hearth, twine.wire, 1), Err("this needs a Joiner's Bench".into()));
    assert_eq!(try_craft(1, far, twine.wire, 1), Err("out of reach".into()));
    assert_eq!(try_craft(2, bench, twine.wire, 1), Err("not enough Saltreed: have 0, need 4".into()));

    read(&t, |ctx| crate::labor::spend(ctx, identity(1), 4996)).unwrap();
    assert_eq!(try_craft(1, bench, twine.wire, 1), Err("not enough labor: have 4, need 5".into()));
    advance_ms(&t, 3_600_000); // labor back

    // A full bag: taking 4 of 1000 reeds frees no slot for the twine.
    give(&t, me.char_id, "saltreed", 996);
    read(&t, |ctx| grant(ctx, me.char_id, wire("reedcutter_knife"), u32::from(BAG_SLOTS) - 1)).unwrap();
    assert_eq!(try_craft(1, bench, twine.wire, 1), Err("bag is full: room for 0 of 1".into()));
    assert!(job_of(&t, me.char_id).is_none());
    assert_eq!(held(&t, me.char_id, "saltreed"), 1000);

    // Already crafting.
    let them = presence_of(&t, 2);
    give(&t, them.char_id, "saltreed", 8);
    try_craft(2, bench, twine.wire, 1).unwrap();
    assert_eq!(try_craft(2, bench, twine.wire, 1), Err("already crafting".into()));
}

#[test]
fn an_output_may_take_the_slot_its_inputs_free() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 4);
    read(&t, |ctx| grant(ctx, me.char_id, wire("reedcutter_knife"), u32::from(BAG_SLOTS) - 1)).unwrap();
    call(&t, 1, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 1)).unwrap();
    finish_unit(&t, me.char_id);
    assert_eq!((held(&t, me.char_id, "saltreed"), held(&t, me.char_id, "saltreed_twine")), (0, 1));
}

#[test]
fn swimmers_and_gatherers_cannot_craft_and_crafters_cannot_gather() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 8);
    let twine = recipe("saltreed_twine").wire;
    let seq = me.last_seq + 1;
    call(&t, 1, |ctx| move_event(ctx, seq, 0, 0.0, 0.0, 0.0, 0, 0, sim::movement::MoveMode::Swim as u8)).unwrap();
    assert_eq!(call(&t, 1, |ctx| craft(ctx, bench, twine, 1)), Err("can't craft while swimming".into()));
    mv(&t, 1, 0.0, 0.0);

    as_admin(&t);
    let reed = sim::gather::node_by_id("saltreed_clump").unwrap().wire;
    let p = crate::gathering::NodePlacement { kind: reed, x: 1.0, y: 0.0, z: 0.0, yaw: 0 };
    call(&t, ADMIN, |ctx| crate::gathering::admin_put_nodes(ctx, vec![p])).unwrap();
    let node = read(&t, |ctx| ctx.db.resource_node().iter().next().unwrap().id);
    call(&t, 1, |ctx| crate::gathering::gather(ctx, node)).unwrap();
    assert_eq!(call(&t, 1, |ctx| craft(ctx, bench, twine, 1)), Err("already gathering".into()));
    call(&t, 1, crate::gathering::cancel_gather).unwrap();
    call(&t, 1, |ctx| craft(ctx, bench, twine, 1)).unwrap();
    assert_eq!(call(&t, 1, |ctx| crate::gathering::gather(ctx, node)), Err("already crafting".into()));
}

#[test]
fn moving_away_cancels_and_standing_still_does_not() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 8);
    call(&t, 1, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 2)).unwrap();
    advance_ms(&t, 500);
    mv(&t, 1, 0.3, 0.0);
    assert!(job_of(&t, me.char_id).is_some(), "a shuffle inside the tolerance");
    advance_ms(&t, 500);
    mv(&t, 1, 0.0, 0.8);
    assert!(job_of(&t, me.char_id).is_none(), "moved 0.8 m away");
    assert_eq!((held(&t, me.char_id, "saltreed"), points(&t, 1)), (8, 5000), "a cancelled unit is free");
}

#[test]
fn cancelling_or_leaving_stops_the_craft() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 8);
    let twine = recipe("saltreed_twine").wire;
    call(&t, 1, |ctx| craft(ctx, bench, twine, 2)).unwrap();
    call(&t, 1, cancel_craft).unwrap();
    assert!(job_of(&t, me.char_id).is_none());
    call(&t, 1, |ctx| craft(ctx, bench, twine, 2)).unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert!(job_of(&t, me.char_id).is_none());
    assert_eq!(held(&t, me.char_id, "saltreed"), 8);
}

#[test]
fn a_unit_whose_inputs_are_gone_changes_nothing_and_ends_the_job() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 4);
    call(&t, 1, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 1)).unwrap();
    let stack = read(&t, |ctx| ctx.db.item().by_slot().filter((me.char_id, place::BAG)).next().unwrap().id);
    call(&t, 1, |ctx| crate::items::destroy_item(ctx, stack, 1)).unwrap();
    finish_unit(&t, me.char_id);
    assert!(job_of(&t, me.char_id).is_none());
    assert_eq!((held(&t, me.char_id, "saltreed"), held(&t, me.char_id, "saltreed_twine"), points(&t, 1)), (3, 0, 5000));

    // And labor spent elsewhere meanwhile.
    give(&t, me.char_id, "saltreed", 1);
    call(&t, 1, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 1)).unwrap();
    read(&t, |ctx| crate::labor::spend(ctx, identity(1), 4999)).unwrap();
    finish_unit(&t, me.char_id);
    assert_eq!((held(&t, me.char_id, "saltreed"), held(&t, me.char_id, "saltreed_twine")), (4, 0));
}

#[test]
fn only_the_schedule_finishes_a_unit() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 4);
    call(&t, 1, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 1)).unwrap();
    let job = job_of(&t, me.char_id).unwrap();
    assert!(call(&t, 1, |ctx| craft_done(ctx, job)).is_err());
    assert_eq!(held(&t, me.char_id, "saltreed_twine"), 0);
    assert!(job_of(&t, me.char_id).is_some());
}

#[test]
fn the_view_shows_only_your_own_craft() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let them = arrive(&t, 2, "Tobin");
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    for c in [me.char_id, them.char_id] {
        give(&t, c, "saltreed", 4);
    }
    for p in [1, 2] {
        call(&t, p, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 1)).unwrap();
    }
    let mine = my_craft(&t.view_context(auth(1)));
    assert_eq!(mine.iter().map(|j| j.char_id).collect::<Vec<_>>(), [me.char_id]);
}

#[test]
fn admins_place_remove_and_clear_stations() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let p = |x| StationPlacement { kind: station_by_id("bloomery").unwrap().wire, x, y: 4.0, z: 70.0, yaw: 9 };
    assert_eq!(call(&t, 1, |ctx| admin_put_stations(ctx, vec![p(0.0)])), Err("admins only".into()));
    as_admin(&t);
    call(&t, ADMIN, |ctx| admin_put_stations(ctx, vec![p(0.0), p(10.0)])).unwrap();
    let rows: Vec<Station> = read(&t, |ctx| ctx.db.station().iter().collect());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cell, sim::cell::cell_of(sim::cell::OPEN_WORLD, 0.0, 70.0));
    assert_eq!((rows[0].y, rows[0].yaw), (4.0, 9));
    assert!(call(&t, ADMIN, |ctx| admin_put_stations(ctx, vec![StationPlacement { kind: 200, ..p(0.0) }])).unwrap_err().contains("unknown kind"));
    assert!(call(&t, ADMIN, |ctx| admin_put_stations(ctx, vec![p(f32::NAN)])).is_err());
    assert!(call(&t, ADMIN, |ctx| admin_put_stations(ctx, vec![p(9000.0)])).unwrap_err().contains("off the map"));

    // Removing a station someone is crafting at stops the craft.
    let bench = put(&t, "joiners_bench", 2.0, 0.0);
    give(&t, me.char_id, "saltreed", 4);
    call(&t, 1, |ctx| craft(ctx, bench, recipe("saltreed_twine").wire, 1)).unwrap();
    assert_eq!(call(&t, 1, |ctx| admin_remove_stations(ctx, vec![bench])), Err("admins only".into()));
    call(&t, ADMIN, |ctx| admin_remove_stations(ctx, vec![bench, 777])).unwrap();
    assert!(job_of(&t, me.char_id).is_none());
    call(&t, ADMIN, admin_clear_stations).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.station().count()), 0);
}
