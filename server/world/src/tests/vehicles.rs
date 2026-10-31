//! Owned vehicles, carts on land and pack slots (docs/design/vehicles.md). Dev builds, for
//! `dev_teleport`, `dev_set_vitals` and `dev_spawn_ship`.

use super::*;
use crate::combat::dev_set_vitals;
use crate::dev::{dev_spawn_ship, dev_teleport};
use crate::items::{Item, grant, item, place};
use crate::ship::{Vehicle, deck, helm, ship_motion, take_helm, vehicle};
use crate::terrain::{TerrainChunk, admin_put_terrain};
use crate::trade::{Pack, drop_pack, pack};
use crate::vehicle::{cargo, load_pack, stow_vehicle, summon_vehicle, unload_pack};
use sim::content::{BAG_SLOTS, item_by_id};
use sim::ship::{Hull, HullData, Pose};
use std::f32::consts::FRAC_PI_2;

pub(super) const ADMIN: u128 = 99;
pub(super) const RUN: u8 = 1;
pub(super) const SWIM: u8 = 2;

pub(super) fn cart() -> &'static HullData {
    Hull::FurrowCart.data()
}

pub(super) fn wire(id: &str) -> u16 {
    item_by_id(id).unwrap().wire
}

/// Ground 3 m up for x < 40, falling at 0.4 into the sea from there to -5 m at x = 60; a hill
/// rising at 0.2 from z = 10 to 7 m at z = 30; and a bank too steep to drive rising from 3 to 9 m
/// between z = -30 and z = -34. Covers x and z from -64 to 128.
pub(super) fn height(x: f32, z: f32) -> f32 {
    if x >= 40.0 {
        return (3.0 - 0.4 * (x - 40.0)).max(-5.0);
    }
    if z > 10.0 {
        3.0 + 0.2 * (z.min(30.0) - 10.0)
    } else if z < -30.0 {
        3.0 + 1.5 * (-30.0 - z).min(4.0)
    } else {
        3.0
    }
}

pub(super) fn shore() -> TestContext {
    let t = world();
    call(&t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::session::record_owner(ctx);
        Ok(())
    })
    .unwrap();
    let file = sim::terrain::HeightFile::generate(124, 124, 3, 3, height);
    let chunks = file.chunks().into_iter().map(|(cell, samples)| TerrainChunk { cell, samples }).collect();
    call(&t, ADMIN, |ctx| admin_put_terrain(ctx, chunks)).unwrap();
    read(&t, crate::init);
    tick(&t);
    t
}

/// `player` in the world at (x, z) on the ground facing `yaw`.
pub(super) fn stand_at(t: &TestContext, player: u128, x: f32, z: f32, yaw: f32) {
    call(t, player, |ctx| dev_teleport(ctx, x, z)).unwrap();
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        ctx.db.presence().identity().update(Presence { yaw: sim::ship::yaw_to_u16(yaw), ..p });
    });
}

/// Give `player`'s character one of `id` and return the stack's id.
pub(super) fn give(t: &TestContext, player: u128, id: &str) -> u64 {
    let c = presence_of(t, player).char_id;
    read(t, |ctx| grant(ctx, c, wire(id), 1)).unwrap();
    read(t, |ctx| ctx.db.item().by_slot().filter((c, place::BAG)).filter(|i| i.def == wire(id)).map(|i| i.id).max().unwrap())
}

pub(super) fn item_row(t: &TestContext, id: u64) -> Item {
    read(t, |ctx| ctx.db.item().id().find(id).unwrap())
}

/// Summon `id` as `player`; returns the vehicle's entity id.
pub(super) fn summon(t: &TestContext, player: u128, id: &str) -> u64 {
    let it = give(t, player, id);
    call(t, player, |ctx| summon_vehicle(ctx, it)).unwrap();
    read(t, |ctx| ctx.db.vehicle().iter().map(|v| v.entity_id).max().unwrap())
}

pub(super) fn vehicle_of(t: &TestContext, v: u64) -> Vehicle {
    read(t, |ctx| ctx.db.vehicle().entity_id().find(v).unwrap())
}

pub(super) fn pose(t: &TestContext, v: u64) -> Pose {
    read(t, |ctx| deck(ctx, v).unwrap().pose())
}

pub(super) fn back(t: &TestContext, player: u128) -> Option<Pack> {
    let c = presence_of(t, player).char_id;
    read(t, |ctx| ctx.db.pack().carrier().find(c))
}

pub(super) fn give_pack(t: &TestContext, player: u128) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        crate::trade::made(ctx, &p, wire("grain_bale"), [p.x, p.y, p.z]);
    });
}

pub(super) fn mv(t: &TestContext, player: u128, parent: u64, at: [f32; 3], mode: u8) {
    let seq = presence_of(t, player).last_seq + 1;
    call(t, player, |ctx| move_event(ctx, seq, parent, at[0], at[1], at[2], 0, 540, mode)).unwrap();
}

/// Stand beside the deck point (x, z) of vehicle `v` and step up onto it.
pub(super) fn board(t: &TestContext, player: u128, v: u64, x: f32, z: f32) {
    let w = pose(t, v).to_world([x, 0.0, z]);
    call(t, player, |ctx| dev_teleport(ctx, w[0], w[2])).unwrap();
    advance_ms(t, 300);
    mv(t, player, v, [x, 0.0, z], RUN);
}

pub(super) fn drive(t: &TestContext, ms: i64) {
    for _ in 0..ms / 100 {
        advance_ms(t, 100);
        tick(t);
    }
}

#[test]
fn a_cart_is_summoned_from_the_bag_ahead_of_its_owner() {
    let t = shore();
    arrive(&t, 1, "Mira");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    let it = give(&t, 1, "furrow_cart");
    call(&t, 1, |ctx| summon_vehicle(ctx, it)).unwrap();
    let v = read(&t, |ctx| ctx.db.vehicle().iter().next().unwrap());
    let c = presence_of(&t, 1).char_id;
    assert_eq!((v.hull, v.owner, v.cargo.clone(), v.hp), (Hull::FurrowCart as u8, c, vec![0, 0], cart().hp));
    let p = pose(&t, v.entity_id);
    assert!((p.x - 10.0).abs() < 1e-4 && (p.z + cart().deck_half_length_m + sim::ship::SUMMON_GAP_M).abs() < 1e-4, "{p:?}");
    assert!((p.y - (3.0 + cart().deck_height_m)).abs() < 1e-3, "on the ground: {p:?}");
    let e = read(&t, |ctx| ctx.db.entity().id().find(v.entity_id).unwrap());
    assert_eq!((e.kind, e.ref_id), (entity_kind::SHIP, Hull::FurrowCart as u64));
    let out = item_row(&t, it);
    assert_eq!((out.place, out.slot, out.holder), (place::OUT, Hull::FurrowCart as u16, c), "out of the bag");
}

#[test]
fn summoning_is_refused_when_it_should_be() {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    let grain = give(&t, 1, "hearthgrain");
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, grain)), Err("that isn't a vehicle".into()));
    let theirs = give(&t, 2, "furrow_cart");
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, theirs)), Err("no such item".into()));
    // Facing the steep bank, and into the sea.
    stand_at(&t, 1, 10.0, -26.0, 0.0);
    let it = give(&t, 1, "furrow_cart");
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, it)), Err("a cart needs firm, gentle ground there".into()));
    stand_at(&t, 1, 46.0, 0.0, -FRAC_PI_2);
    assert!(call(&t, 1, |ctx| summon_vehicle(ctx, it)).is_err(), "too deep to stand in");
    let skiff = give(&t, 1, "driftpine_skiff");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, skiff)), Err("a boat needs deep enough water there".into()));
    // One cart out; a second is refused, and so is anyone else's on top of it.
    call(&t, 1, |ctx| summon_vehicle(ctx, it)).unwrap();
    let second = give(&t, 1, "furrow_cart");
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, second)), Err("you already have a land vehicle out".into()));
    stand_at(&t, 2, 10.5, 0.0, 0.0);
    assert_eq!(call(&t, 2, |ctx| summon_vehicle(ctx, theirs)), Err("no room for it there".into()));
    // Not from aboard, and not dead.
    let v = read(&t, |ctx| ctx.db.vehicle().iter().next().unwrap().entity_id);
    board(&t, 1, v, 0.0, 0.0);
    assert_eq!(presence_of(&t, 1).parent, v);
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, skiff)), Err("can't summon a vehicle while aboard one".into()));
    stand_at(&t, 2, 30.0, 20.0, 0.0);
    call(&t, 2, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    assert!(call(&t, 2, |ctx| summon_vehicle(ctx, theirs)).is_err(), "the dead can't");
}

#[test]
fn a_cart_and_a_boat_can_be_out_together() {
    let t = shore();
    arrive(&t, 1, "Mira");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    summon(&t, 1, "furrow_cart");
    stand_at(&t, 1, 50.0, 0.0, -FRAC_PI_2);
    let skiff = summon(&t, 1, "driftpine_skiff");
    let p = pose(&t, skiff);
    assert!(p.x > 52.0 && (p.y - (sim::movement::SEA_LEVEL_M + Hull::Skiff.data().deck_height_m)).abs() < 1e-5, "{p:?}");
    let skiff2 = give(&t, 1, "driftpine_skiff");
    stand_at(&t, 1, 50.0, 20.0, -FRAC_PI_2);
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, skiff2)), Err("you already have a boat out".into()));
}

#[test]
fn stowing_returns_the_item_and_puts_riders_off() {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 50.0, 0.0, -FRAC_PI_2);
    let it = give(&t, 1, "driftpine_skiff");
    call(&t, 1, |ctx| summon_vehicle(ctx, it)).unwrap();
    let v = read(&t, |ctx| ctx.db.vehicle().iter().next().unwrap().entity_id);
    board(&t, 2, v, 0.0, 1.0);
    assert_eq!(presence_of(&t, 2).parent, v);
    assert_eq!(call(&t, 2, |ctx| stow_vehicle(ctx, v)), Err("that isn't your vehicle".into()));
    let before = presence_of(&t, 2).corrections;
    call(&t, 1, |ctx| stow_vehicle(ctx, v)).unwrap();
    let rider = presence_of(&t, 2);
    assert_eq!((rider.parent, rider.mode, rider.corrections), (0, SWIM, before + 1), "put in the water and told to snap");
    assert!(read(&t, |ctx| ctx.db.vehicle().entity_id().find(v).is_none() && ctx.db.ship_motion().entity_id().find(v).is_none()));
    let back_in = item_row(&t, it);
    assert_eq!(back_in.place, place::BAG);
    // It can come straight out again, and a full bag keeps it out.
    call(&t, 1, |ctx| summon_vehicle(ctx, it)).unwrap();
    let v = read(&t, |ctx| ctx.db.vehicle().iter().next().unwrap().entity_id);
    let c = presence_of(&t, 1).char_id;
    let free = BAG_SLOTS as u32 - read(&t, |ctx| ctx.db.item().by_slot().filter((c, place::BAG)).count() as u32);
    read(&t, |ctx| grant(ctx, c, wire("reedcutter_knife"), free)).unwrap();
    assert_eq!(call(&t, 1, |ctx| stow_vehicle(ctx, v)), Err("no room in the bag".into()));
}

#[test]
fn only_the_owner_drives_and_a_cart_follows_the_ground() {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    let v = summon(&t, 1, "furrow_cart");
    board(&t, 2, v, cart().helm_x, cart().helm_z);
    assert_eq!(call(&t, 2, take_helm), Err("only the owner can drive this".into()));
    // Tobin rides in the bed; Mira takes the reins.
    mv(&t, 2, v, [0.0, 0.0, 1.0], RUN);
    advance_ms(&t, 1000);
    board(&t, 1, v, cart().helm_x, cart().helm_z + 0.5);
    call(&t, 1, take_helm).unwrap();
    // The cart faces -z, away from the hill, so it backs up it.
    call(&t, 1, |ctx| helm(ctx, -100, 0)).unwrap();
    drive(&t, 20_000);
    let up = pose(&t, v);
    assert!(up.z > 10.0 && up.y > 3.0 + cart().deck_height_m + 1.0, "backed up the hill: {up:?}");
    // The rider takes a step in the bed: judged in the cart's frame, with no correction.
    let before = presence_of(&t, 2).corrections;
    mv(&t, 2, v, [0.3, 0.0, 0.8], RUN);
    assert_eq!(presence_of(&t, 2).corrections, before);
    // Ahead (south, -z) runs down the hill and stops at the steep bank.
    call(&t, 1, |ctx| helm(ctx, 100, 0)).unwrap();
    drive(&t, 40_000);
    let stopped = pose(&t, v);
    let m = read(&t, |ctx| ctx.db.ship_motion().entity_id().find(v).unwrap());
    assert!(stopped.z < -25.0 && stopped.z > -31.0 && m.speed == 0.0, "stopped at the bank: {stopped:?} {}", m.speed);
    assert_eq!(presence_of(&t, 2).parent, v, "the rider is still aboard");
}

#[test]
fn a_cart_stops_at_the_sea() {
    let t = shore();
    arrive(&t, 1, "Mira");
    stand_at(&t, 1, 20.0, 0.0, -FRAC_PI_2);
    let v = summon(&t, 1, "furrow_cart");
    board(&t, 1, v, cart().helm_x, cart().helm_z + 0.5);
    call(&t, 1, take_helm).unwrap();
    call(&t, 1, |ctx| helm(ctx, 100, 0)).unwrap();
    drive(&t, 20_000);
    let p = pose(&t, v);
    // Its bow stops short of water deeper than the ford: 3 - 0.4 (x - 40) = -0.4 at x = 48.5, give
    // or take the heights' 1/32 m steps.
    let bow = p.to_world([0.0, 0.0, -cart().deck_half_length_m]);
    assert!(bow[0] > 46.0 && bow[0] < 48.6, "{bow:?}");
}

#[test]
fn anyone_loads_and_only_the_owner_unloads() {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    let v = summon(&t, 1, "furrow_cart");
    let slot = pose(&t, v).to_world([cart().slots[0].x, 0.0, cart().slots[0].z]);
    // Tobin loads his own pack from the ground beside the cart.
    stand_at(&t, 2, slot[0] - 2.0, slot[2], 0.0);
    assert_eq!(call(&t, 2, |ctx| load_pack(ctx, v, 0)), Err("not carrying a pack".into()));
    give_pack(&t, 2);
    let made = back(&t, 2).unwrap().made_at;
    assert_eq!(call(&t, 2, |ctx| load_pack(ctx, v, 5)), Err("no such slot".into()));
    stand_at(&t, 2, slot[0] - 4.0, slot[2], 0.0);
    assert_eq!(call(&t, 2, |ctx| load_pack(ctx, v, 0)), Err("out of reach".into()));
    stand_at(&t, 2, slot[0] - 2.0, slot[2], 0.0);
    call(&t, 2, |ctx| load_pack(ctx, v, 0)).unwrap();
    assert!(back(&t, 2).is_none());
    assert_eq!(vehicle_of(&t, v).cargo, vec![wire("grain_bale"), 0]);
    give_pack(&t, 2);
    assert_eq!(call(&t, 2, |ctx| load_pack(ctx, v, 0)), Err("that slot is full".into()));
    // He can't take it back off Mira's cart; she can, and it keeps its age.
    read(&t, |ctx| ctx.db.pack().carrier().delete(presence_of_ctx(ctx, 2)));
    assert_eq!(call(&t, 2, |ctx| unload_pack(ctx, v, 0)), Err("only the owner can unload this".into()));
    stand_at(&t, 1, slot[0] + 1.0, slot[2], 0.0);
    give_pack(&t, 1);
    assert_eq!(call(&t, 1, |ctx| unload_pack(ctx, v, 0)), Err("already carrying a pack".into()));
    call(&t, 1, |ctx| load_pack(ctx, v, 1)).unwrap();
    assert_eq!(call(&t, 1, |ctx| stow_vehicle(ctx, v)), Err("unload its cargo first".into()));
    advance_ms(&t, 60_000);
    call(&t, 1, |ctx| unload_pack(ctx, v, 0)).unwrap();
    let pk = back(&t, 1).unwrap();
    assert_eq!((pk.def, pk.owner, pk.made_at), (wire("grain_bale"), presence_of(&t, 2).char_id, made));
    assert_eq!(vehicle_of(&t, v).cargo, vec![0, wire("grain_bale")]);
    assert_eq!(call(&t, 1, |ctx| unload_pack(ctx, v, 0)), Err("already carrying a pack".into()));
    read(&t, |ctx| ctx.db.pack().carrier().delete(presence_of_ctx(ctx, 1)));
    assert_eq!(call(&t, 1, |ctx| unload_pack(ctx, v, 0)), Err("that slot is empty".into()));
    call(&t, 1, |ctx| unload_pack(ctx, v, 1)).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.cargo().iter().count()), 0);
    read(&t, |ctx| ctx.db.pack().carrier().delete(presence_of_ctx(ctx, 1)));
    call(&t, 1, |ctx| stow_vehicle(ctx, v)).unwrap();
}

fn presence_of_ctx(ctx: &ReducerContext, player: u128) -> u64 {
    ctx.db.presence().identity().find(identity(player)).unwrap().char_id
}

#[test]
fn dying_aboard_puts_the_pack_in_a_free_slot_and_drop_is_refused() {
    let t = shore();
    arrive(&t, 1, "Mira");
    stand_at(&t, 1, 50.0, 0.0, -FRAC_PI_2);
    call(&t, 0, |ctx| dev_spawn_ship(ctx, Hull::Clipper as u8, 70.0, 0.0, FRAC_PI_2)).unwrap();
    let ship = read(&t, |ctx| ctx.db.vehicle().iter().next().unwrap().entity_id);
    assert_eq!(vehicle_of(&t, ship).owner, 0);
    board(&t, 1, ship, 0.0, 4.0);
    assert_eq!(presence_of(&t, 1).parent, ship);
    give_pack(&t, 1);
    assert_eq!(call(&t, 1, drop_pack), Err("load it into a pack slot instead".into()));
    call(&t, 1, |ctx| dev_set_vitals(ctx, 0, 0)).unwrap();
    assert!(back(&t, 1).is_none());
    assert_eq!(vehicle_of(&t, ship).cargo, vec![wire("grain_bale"), 0]);
    // Anyone may unload an unowned ship.
    arrive(&t, 2, "Tobin");
    board(&t, 2, ship, 0.0, 5.0);
    call(&t, 2, |ctx| unload_pack(ctx, ship, 0)).unwrap();
    assert!(back(&t, 2).is_some());
}
