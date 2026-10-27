//! Gathering (docs/design/gathering.md).

use super::*;
use crate::gathering::{
    GatherJob, NodePlacement, ResourceNode, admin_clear_nodes, admin_put_nodes, admin_remove_nodes, cancel_gather, gather, gather_done,
    gather_job, resource_node,
};
use crate::items::{grant, item, place};
use crate::labor::labor;
use sim::content::{BAG_SLOTS, item_by_id};
use sim::gather::{NodeData, node_by_id};

const ADMIN: u128 = 99;
const REED: &str = "saltreed_clump";

fn kind(id: &str) -> &'static NodeData {
    node_by_id(id).unwrap()
}

fn as_admin(t: &TestContext) {
    call(t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::session::record_owner(ctx);
        Ok(())
    })
    .unwrap();
}

/// Place a node of `id` at `(x, 0, z)` as the admin; returns its id.
fn put(t: &TestContext, id: &str, x: f32, z: f32) -> u64 {
    as_admin(t);
    let before: Vec<u64> = read(t, |ctx| ctx.db.resource_node().iter().map(|n| n.id).collect());
    let p = NodePlacement { kind: kind(id).wire, x, y: 0.0, z, yaw: 0 };
    call(t, ADMIN, |ctx| admin_put_nodes(ctx, vec![p])).unwrap();
    read(t, |ctx| ctx.db.resource_node().iter().map(|n| n.id).find(|id| !before.contains(id)).unwrap())
}

fn node_of(t: &TestContext, id: u64) -> ResourceNode {
    read(t, |ctx| ctx.db.resource_node().id().find(id).unwrap())
}

fn job_of(t: &TestContext, char_id: u64) -> Option<GatherJob> {
    read(t, |ctx| ctx.db.gather_job().char_id().find(char_id))
}

fn points(t: &TestContext, player: u128) -> i32 {
    read(t, |ctx| ctx.db.labor().identity().find(identity(player)).unwrap().points)
}

/// How many of `item` are in `char_id`'s bag.
fn held(t: &TestContext, char_id: u64, item_id: &str) -> u32 {
    let wire = item_by_id(item_id).unwrap().wire;
    read(t, |ctx| ctx.db.item().by_slot().filter((char_id, place::BAG)).filter(|i| i.def == wire).map(|i| i.count).sum())
}

/// Run the job when it falls due, as the scheduler would.
fn finish(t: &TestContext, char_id: u64) {
    let job = job_of(t, char_id).expect("a gather under way");
    advance_ms(t, sim::gather::gather_micros(kind(REED)) / 1000);
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| gather_done(ctx, job)).unwrap();
    read(t, |ctx| ctx.db.gather_job().char_id().delete(char_id));
}

fn mv(t: &TestContext, player: u128, x: f32, z: f32) {
    let seq = presence_of(t, player).last_seq + 1;
    call(t, player, |ctx| move_event(ctx, seq, 0, x, 0.0, z, 0, 0, 1)).unwrap();
}

#[test]
fn a_gather_spends_labor_grants_the_yield_and_takes_the_node() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let n = put(&t, REED, 1.0, 1.0);
    let k = kind(REED);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    let job = job_of(&t, me.char_id).unwrap();
    assert!((k.yield_min..=k.yield_max).contains(&job.count));
    let busy = node_of(&t, n);
    assert_eq!(busy.gatherer, me.entity_id);
    assert_eq!(busy.done_at.to_micros_since_unix_epoch() - T0, sim::gather::gather_micros(k));
    assert_eq!(points(&t, 1), 5000, "nothing is spent until it finishes");

    finish(&t, me.char_id);
    assert_eq!(held(&t, me.char_id, "saltreed"), job.count);
    assert_eq!(points(&t, 1), 5000 - k.labor);
    let taken = node_of(&t, n);
    assert_eq!(taken.gatherer, 0);
    let done = T0 + sim::gather::gather_micros(k);
    assert_eq!(taken.ready_at.to_micros_since_unix_epoch(), done + i64::from(k.regrow_s) * 1_000_000);
    assert_eq!(call(&t, 1, |ctx| gather(ctx, n)), Err("nothing to gather here yet".into()));

    // The node regrows with the clock and nothing else.
    advance_ms(&t, i64::from(k.regrow_s) * 1000 - 1);
    assert_eq!(call(&t, 1, |ctx| gather(ctx, n)), Err("nothing to gather here yet".into()));
    advance_ms(&t, 1);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
}

#[test]
fn refusals_change_nothing() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let them = arrive(&t, 2, "Tobin");
    let near = put(&t, REED, 2.0, 0.0);
    let far = put(&t, REED, 3.5, 0.0);
    assert_eq!(call(&t, 1, |ctx| gather(ctx, far)), Err("out of reach".into()));
    assert_eq!(call(&t, 1, |ctx| gather(ctx, 12345)), Err("no such node".into()));
    assert_eq!(call(&t, 3, |ctx| gather(ctx, near)), Err("not in the world".into()));

    // Someone else got there first; and nobody gathers two at once.
    call(&t, 2, |ctx| gather(ctx, near)).unwrap();
    assert_eq!(call(&t, 1, |ctx| gather(ctx, near)), Err("someone else is gathering this".into()));
    let other = put(&t, REED, 0.0, 2.0);
    assert_eq!(call(&t, 2, |ctx| gather(ctx, other)), Err("already gathering".into()));
    assert_eq!(node_of(&t, other).gatherer, 0);
    assert_eq!(node_of(&t, near).gatherer, them.entity_id);

    // Not enough labor.
    read(&t, |ctx| crate::labor::spend(ctx, identity(1), 4998)).unwrap();
    assert_eq!(call(&t, 1, |ctx| gather(ctx, other)), Err("not enough labor: have 2, need 5".into()));

    // A full bag.
    read(&t, |ctx| grant(ctx, me.char_id, item_by_id("reedcutter_knife").unwrap().wire, u32::from(BAG_SLOTS))).unwrap();
    advance_ms(&t, 3_600_000); // labor back
    let e = call(&t, 1, |ctx| gather(ctx, other)).unwrap_err();
    assert!(e.starts_with("bag is full: room for 0 of"), "{e}");
    assert!(job_of(&t, me.char_id).is_none());
    assert_eq!(node_of(&t, other).gatherer, 0);
}

#[test]
fn swimmers_and_sailors_cannot_gather() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let n = put(&t, REED, 1.0, 0.0);
    let seq = me.last_seq + 1;
    call(&t, 1, |ctx| move_event(ctx, seq, 0, 0.0, 0.0, 0.0, 0, 0, sim::movement::MoveMode::Swim as u8)).unwrap();
    assert_eq!(call(&t, 1, |ctx| gather(ctx, n)), Err("can't gather while swimming".into()));
}

#[test]
fn moving_away_cancels_and_standing_still_does_not() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let n = put(&t, REED, 1.0, 0.0);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    advance_ms(&t, 500);
    mv(&t, 1, 0.3, 0.0); // a shuffle, inside the tolerance
    assert!(job_of(&t, me.char_id).is_some());
    advance_ms(&t, 500);
    mv(&t, 1, 0.0, 0.8);
    assert!(job_of(&t, me.char_id).is_none(), "moved 0.8 m away");
    assert_eq!(node_of(&t, n).gatherer, 0);
    assert_eq!(held(&t, me.char_id, "saltreed"), 0);
    assert_eq!(points(&t, 1), 5000, "a cancelled gather is free");
    // And it can be started again at once.
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
}

#[test]
fn cancelling_or_leaving_frees_the_node() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let n = put(&t, REED, 1.0, 0.0);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    call(&t, 1, cancel_gather).unwrap();
    assert!(job_of(&t, me.char_id).is_none());
    assert_eq!(node_of(&t, n).gatherer, 0);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert!(job_of(&t, me.char_id).is_none());
    assert_eq!(node_of(&t, n).gatherer, 0);
}

#[test]
fn a_finish_that_no_longer_fits_changes_nothing_but_frees_the_node() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let n = put(&t, REED, 1.0, 0.0);
    // Labor spent elsewhere in the meantime (another character on the account, say).
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    read(&t, |ctx| crate::labor::spend(ctx, identity(1), 4999)).unwrap();
    finish(&t, me.char_id);
    assert_eq!(held(&t, me.char_id, "saltreed"), 0);
    assert_eq!(points(&t, 1), 1);
    let free = node_of(&t, n);
    assert_eq!(free.gatherer, 0);
    assert!(free.ready_at.to_micros_since_unix_epoch() <= T0 + 5_000_000, "not taken");

    // The bag filled up in the meantime.
    advance_ms(&t, 3_600_000);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    read(&t, |ctx| grant(ctx, me.char_id, item_by_id("reedcutter_knife").unwrap().wire, u32::from(BAG_SLOTS))).unwrap();
    let before = points(&t, 1);
    finish(&t, me.char_id);
    assert_eq!(points(&t, 1), before);
    assert_eq!(node_of(&t, n).gatherer, 0);
}

#[test]
fn only_the_schedule_finishes_a_gather() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let n = put(&t, REED, 1.0, 0.0);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    let job = job_of(&t, me.char_id).unwrap();
    assert!(call(&t, 1, |ctx| gather_done(ctx, job)).is_err());
    assert_eq!(held(&t, me.char_id, "saltreed"), 0);
}

#[test]
fn admins_place_remove_and_clear_nodes() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let p = |x| NodePlacement { kind: kind("hollowoak").wire, x, y: 4.0, z: 70.0, yaw: 9 };
    assert_eq!(call(&t, 1, |ctx| admin_put_nodes(ctx, vec![p(0.0)])), Err("admins only".into()));
    as_admin(&t);
    call(&t, ADMIN, |ctx| admin_put_nodes(ctx, vec![p(0.0), p(10.0)])).unwrap();
    let rows: Vec<ResourceNode> = read(&t, |ctx| ctx.db.resource_node().iter().collect());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cell, sim::cell::cell_of(sim::cell::OPEN_WORLD, 0.0, 70.0));
    assert_eq!((rows[0].y, rows[0].yaw, rows[0].ready_at.to_micros_since_unix_epoch()), (4.0, 9, T0));
    let bad = NodePlacement { kind: 200, ..p(0.0) };
    assert!(call(&t, ADMIN, |ctx| admin_put_nodes(ctx, vec![bad])).unwrap_err().contains("unknown kind"));
    assert!(call(&t, ADMIN, |ctx| admin_put_nodes(ctx, vec![p(f32::NAN)])).is_err());
    assert!(call(&t, ADMIN, |ctx| admin_put_nodes(ctx, vec![p(9000.0)])).unwrap_err().contains("off the map"));

    // Removing a node someone is gathering stops the gather.
    let n = put(&t, REED, 1.0, 0.0);
    call(&t, 1, |ctx| gather(ctx, n)).unwrap();
    assert_eq!(call(&t, 1, |ctx| admin_remove_nodes(ctx, vec![n])), Err("admins only".into()));
    call(&t, ADMIN, |ctx| admin_remove_nodes(ctx, vec![n, 777])).unwrap();
    assert!(job_of(&t, me.char_id).is_none());
    call(&t, ADMIN, admin_clear_nodes).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.resource_node().count()), 0);
}
