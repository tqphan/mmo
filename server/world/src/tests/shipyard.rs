//! Shipyards, damage, wrecks, mending and repairs (docs/design/vehicles.md §2B). Dev builds, for
//! `dev_teleport` and `dev_damage_vehicle`. The ground is `vehicles::shore`'s: dry for x < 40,
//! then shelving into the sea, 5 m deep from x = 60.

use super::vehicles::{ADMIN, SWIM, back, board, give, shore, stand_at, summon, vehicle_of, wire};
use super::*;
use crate::dev::dev_damage_vehicle;
use crate::items::{Item, grant, item, place};
use crate::labor::{dev_set_labor, labor};
use crate::ship::{VehicleWreck, my_vehicles, take_helm, vehicle, vehicle_wreck, wreck_vehicle};
use crate::shipyard::{Shipyard, abandon_shipyard, place_shipyard, shipyard, work_shipyard};
use crate::trade::{dropped_pack, pack, pick_up_pack};
use crate::vehicle::{load_pack, mend_vehicle, repair_vehicle, stow_vehicle, summon_vehicle, wreck};
use sim::content::{BAG_SLOTS, SHIPYARDS, VEHICLES};
use sim::ship::Hull;
use std::f32::consts::FRAC_PI_2;

/// Facing +x, out to sea.
const SEAWARD: f32 = -FRAC_PI_2;

fn yard(t: &TestContext) -> Option<Shipyard> {
    read(t, |ctx| ctx.db.shipyard().iter().next())
}

fn labor_of(t: &TestContext, player: u128) -> i32 {
    read(t, |ctx| ctx.db.labor().identity().find(identity(player)).unwrap().points)
}

/// Put a fresh pack of `id` on `player`'s back.
fn carry(t: &TestContext, player: u128, id: &str) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        crate::trade::made(ctx, &p, wire(id), [p.x, p.y, p.z]);
    });
}

fn count_of(t: &TestContext, player: u128, id: &str) -> u32 {
    let c = presence_of(t, player).char_id;
    read(t, |ctx| ctx.db.item().by_slot().filter((c, place::BAG)).filter(|i| i.def == wire(id)).map(|i| i.count).sum())
}

fn grant_n(t: &TestContext, player: u128, id: &str, n: u32) {
    let c = presence_of(t, player).char_id;
    read(t, |ctx| grant(ctx, c, wire(id), n)).unwrap();
}

/// Fill every free slot of `player`'s bag with Saltreed.
fn fill_bag(t: &TestContext, player: u128) {
    let c = presence_of(t, player).char_id;
    read(t, |ctx| {
        for slot in 0..BAG_SLOTS {
            if ctx.db.item().by_slot().filter((c, place::BAG, slot)).next().is_none() {
                ctx.db.item().insert(Item { id: 0, holder: c, place: place::BAG, slot, def: wire("saltreed"), count: 1 });
            }
        }
    });
}

fn empty_bag_of_saltreed(t: &TestContext, player: u128) {
    let c = presence_of(t, player).char_id;
    read(t, |ctx| {
        let ids: Vec<u64> = ctx.db.item().by_slot().filter((c, place::BAG)).filter(|i| i.def == wire("saltreed")).map(|i| i.id).collect();
        for id in ids {
            ctx.db.item().id().delete(id);
        }
    });
}

/// Mira (1) in the water at x = 52 facing the sea, with a yard placed ahead of her; Tobin (2) beside her.
fn placed() -> (TestContext, Shipyard) {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 52.0, 0.0, SEAWARD);
    stand_at(&t, 2, 52.0, 2.0, SEAWARD);
    let it = give(&t, 1, "swiftgull_design");
    call(&t, 1, |ctx| place_shipyard(ctx, it)).unwrap();
    let y = yard(&t).unwrap();
    (t, y)
}

#[test]
fn a_shipyard_is_placed_on_the_water_from_a_design() {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 10.0, 0.0, SEAWARD);
    let cart = give(&t, 1, "furrow_cart");
    assert_eq!(call(&t, 1, |ctx| place_shipyard(ctx, cart)), Err("that isn't a design".into()));
    let it = give(&t, 1, "swiftgull_design");
    assert_eq!(call(&t, 1, |ctx| place_shipyard(ctx, it)), Err("a shipyard needs deep enough water there".into()));
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { parent: 7, ..p });
    });
    assert_eq!(call(&t, 1, |ctx| place_shipyard(ctx, it)), Err("can't place a shipyard while aboard".into()));
    assert_eq!(call(&t, 2, |ctx| place_shipyard(ctx, it)), Err("no such item".into()), "not Tobin's");

    stand_at(&t, 1, 52.0, 0.0, SEAWARD);
    let before = labor_of(&t, 1);
    call(&t, 1, |ctx| place_shipyard(ctx, it)).unwrap();
    let y = yard(&t).unwrap();
    let hull = Hull::Clipper.data();
    assert_eq!((y.kind, y.owner, y.stage), (0, presence_of(&t, 1).char_id, 0));
    assert!((y.x - (52.0 + hull.deck_half_length_m + sim::ship::SUMMON_GAP_M)).abs() < 1e-3 && y.z.abs() < 1e-3, "{y:?}");
    assert_eq!(read(&t, |ctx| ctx.db.item().id().find(it)), None, "the design is used up");
    assert_eq!(before - labor_of(&t, 1), SHIPYARDS[0].place_labor);

    let again = give(&t, 1, "swiftgull_design");
    assert_eq!(call(&t, 1, |ctx| place_shipyard(ctx, again)), Err("you already have a shipyard".into()));
    stand_at(&t, 2, 52.0, 3.0, SEAWARD);
    let theirs = give(&t, 2, "swiftgull_design");
    assert_eq!(call(&t, 2, |ctx| place_shipyard(ctx, theirs)), Err("no room for it there".into()));
    // Nor can a boat be summoned into it.
    let skiff = give(&t, 2, "driftpine_skiff");
    assert_eq!(call(&t, 2, |ctx| summon_vehicle(ctx, skiff)), Err("no room for it there".into()));
}

#[test]
fn stages_take_their_packs_and_labor_in_order_and_only_the_owner_launches() {
    let (t, y) = placed();
    let id = y.id;
    assert_eq!(call(&t, 1, |ctx| work_shipyard(ctx, id)), Err("this stage needs a Timber Bundle".into()));
    carry(&t, 1, "grain_bale");
    assert_eq!(call(&t, 1, |ctx| work_shipyard(ctx, id)), Err("this stage needs a Timber Bundle".into()), "the wrong pack");
    assert!(back(&t, 1).is_some(), "and keeps it");

    // Tobin, a friend, brings the timber, but not from the beach.
    stand_at(&t, 2, 40.0, 0.0, SEAWARD);
    carry(&t, 2, "timber_bundle");
    assert_eq!(call(&t, 2, |ctx| work_shipyard(ctx, id)), Err("out of reach".into()));
    stand_at(&t, 2, 52.0, 2.0, SEAWARD);
    call(&t, 2, |ctx| dev_set_labor(ctx, 10)).unwrap();
    assert!(call(&t, 2, |ctx| work_shipyard(ctx, id)).unwrap_err().contains("not enough labor"));
    assert!(back(&t, 2).is_some(), "nothing is taken when refused");
    call(&t, 2, |ctx| dev_set_labor(ctx, 1000)).unwrap();
    call(&t, 2, |ctx| work_shipyard(ctx, id)).unwrap();
    assert_eq!((yard(&t).unwrap().stage, back(&t, 2), labor_of(&t, 2)), (1, None, 975));

    // A spoiled pack won't do.
    read(&t, |ctx| {
        let c = ctx.db.presence().identity().find(identity(1)).unwrap().char_id;
        ctx.db.pack().carrier().delete(c);
    });
    carry(&t, 1, "ironwork_crate");
    read(&t, |ctx| {
        let c = ctx.db.presence().identity().find(identity(1)).unwrap().char_id;
        let pk = ctx.db.pack().carrier().find(c).unwrap();
        let old = Timestamp::from_micros_since_unix_epoch(T0 - 30 * 86_400 * 1_000_000);
        ctx.db.pack().carrier().update(crate::trade::Pack { made_at: old, ..pk });
    });
    assert_eq!(call(&t, 1, |ctx| work_shipyard(ctx, id)), Err("this pack has spoiled".into()));
    read(&t, |ctx| {
        let c = ctx.db.presence().identity().find(identity(1)).unwrap().char_id;
        ctx.db.pack().carrier().delete(c);
    });
    carry(&t, 1, "ironwork_crate");
    call(&t, 1, |ctx| work_shipyard(ctx, id)).unwrap();
    carry(&t, 1, "sailcloth_bale");
    call(&t, 1, |ctx| work_shipyard(ctx, id)).unwrap();
    // Fitting out takes no pack; Tobin may do it, but not the launch.
    call(&t, 2, |ctx| work_shipyard(ctx, id)).unwrap();
    call(&t, 1, |ctx| work_shipyard(ctx, id)).unwrap();
    assert_eq!(yard(&t).unwrap().stage, 5);
    assert_eq!(call(&t, 2, |ctx| work_shipyard(ctx, id)), Err("only the owner can launch it".into()));
    fill_bag(&t, 1);
    assert_eq!(call(&t, 1, |ctx| work_shipyard(ctx, id)), Err("no room in the owner's bag".into()));
    empty_bag_of_saltreed(&t, 1);
    let before = labor_of(&t, 1);
    call(&t, 1, |ctx| work_shipyard(ctx, id)).unwrap();
    assert_eq!(yard(&t), None, "launched");
    assert_eq!((count_of(&t, 1, "swiftgull"), before - labor_of(&t, 1)), (1, 25));
    assert_eq!(call(&t, 1, |ctx| work_shipyard(ctx, id)), Err("no such shipyard".into()));
}

#[test]
fn only_the_owner_abandons_a_shipyard() {
    let (t, y) = placed();
    assert_eq!(call(&t, 2, |ctx| abandon_shipyard(ctx, y.id)), Err("that isn't your shipyard".into()));
    call(&t, 1, |ctx| abandon_shipyard(ctx, y.id)).unwrap();
    assert_eq!(yard(&t), None);
}

/// Mira's Swiftgull out at sea with her Grain Bale in slot 0 and Tobin on deck. Returns its id.
fn sailing() -> (TestContext, u64) {
    let t = shore();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    stand_at(&t, 1, 52.0, 0.0, SEAWARD);
    let v = summon(&t, 1, "swiftgull");
    carry(&t, 1, "grain_bale");
    board(&t, 1, v, -1.0, 3.0);
    call(&t, 1, |ctx| load_pack(ctx, v, 0)).unwrap();
    board(&t, 2, v, 0.0, 0.0);
    (t, v)
}

fn wrecks(t: &TestContext) -> Vec<VehicleWreck> {
    read(t, |ctx| ctx.db.vehicle_wreck().iter().collect())
}

fn run_wreck(t: &TestContext, row: VehicleWreck) {
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| wreck_vehicle(ctx, row)).unwrap();
}

#[test]
fn a_hit_puts_a_ship_under_fire_and_repairs_wait_it_out() {
    let (t, v) = sailing();
    let hp = Hull::Clipper.data().hp;
    call(&t, 1, |ctx| dev_damage_vehicle(ctx, v, 2000)).unwrap();
    assert_eq!(vehicle_of(&t, v).hp, hp - 2000);
    assert_eq!(call(&t, 1, |ctx| stow_vehicle(ctx, v)), Err("it's under fire".into()));
    grant_n(&t, 2, "hullmenders_kit", 1);
    assert_eq!(call(&t, 2, |ctx| repair_vehicle(ctx, v)), Err("can't repair it under fire".into()));
    advance_ms(&t, i64::from(VEHICLES.under_fire_s) * 1000);
    assert_eq!(call(&t, 1, |ctx| repair_vehicle(ctx, v)), Err("repairs take a Hullmender's Kit".into()), "Mira has none");
    let before = labor_of(&t, 2);
    call(&t, 2, |ctx| repair_vehicle(ctx, v)).unwrap();
    assert_eq!((vehicle_of(&t, v).hp, count_of(&t, 2, "hullmenders_kit")), (hp - 2000 + hp / 10, 0));
    assert_eq!(before - labor_of(&t, 2), VEHICLES.repair_labor);
    grant_n(&t, 2, "hullmenders_kit", 2);
    call(&t, 2, |ctx| repair_vehicle(ctx, v)).unwrap();
    assert_eq!(vehicle_of(&t, v).hp, hp, "capped at full");
    assert_eq!(call(&t, 2, |ctx| repair_vehicle(ctx, v)), Err("it isn't damaged".into()));
    // From the water beside it, but not from far off.
    call(&t, 1, |ctx| dev_damage_vehicle(ctx, v, 100)).unwrap();
    advance_ms(&t, i64::from(VEHICLES.under_fire_s) * 1000);
    grant_n(&t, 1, "hullmenders_kit", 1);
    let c = read(&t, |ctx| crate::ship::deck(ctx, v).unwrap().motion);
    stand_at(&t, 1, c.x, c.z + 30.0, SEAWARD);
    assert_eq!(call(&t, 1, |ctx| repair_vehicle(ctx, v)), Err("out of reach".into()));
    stand_at(&t, 1, c.x, c.z + 4.0, SEAWARD);
    call(&t, 1, |ctx| repair_vehicle(ctx, v)).unwrap();
}

#[test]
fn a_sunk_ship_floats_its_packs_and_comes_back_as_a_wreck_to_mend() {
    let (t, v) = sailing();
    call(&t, 2, |ctx| {
        // Tobin can't drive Mira's ship anyway; Mira takes the helm from the deck.
        let _ = take_helm(ctx);
        Ok(())
    })
    .unwrap();
    board(&t, 1, v, 0.0, 7.5);
    call(&t, 1, take_helm).unwrap();
    call(&t, 1, |ctx| dev_damage_vehicle(ctx, v, 20_000)).unwrap();
    let sunk = vehicle_of(&t, v);
    assert_eq!((sunk.hp, sunk.helm, sunk.throttle), (0, 0, 0), "it lets go of the helm");
    assert_eq!(call(&t, 1, take_helm), Err("it's sinking".into()));
    assert_eq!(call(&t, 1, |ctx| stow_vehicle(ctx, v)), Err("it's sinking".into()));
    grant_n(&t, 1, "hullmenders_kit", 1);
    assert!(call(&t, 1, |ctx| repair_vehicle(ctx, v)).is_err());
    call(&t, 1, |ctx| dev_damage_vehicle(ctx, v, 5)).unwrap();
    let rows = wrecks(&t);
    assert_eq!(rows.len(), 1, "it goes down once");
    let slot = read(&t, |ctx| crate::ship::deck(ctx, v).unwrap().pose()).to_world([-1.0, 0.0, 4.5]);

    run_wreck(&t, rows.into_iter().next().unwrap());
    assert!(read(&t, |ctx| ctx.db.vehicle().entity_id().find(v)).is_none());
    let tobin = presence_of(&t, 2);
    assert_eq!((tobin.parent, tobin.mode), (0, SWIM), "put in the water");
    let d = read(&t, |ctx| ctx.db.dropped_pack().iter().next().unwrap());
    assert_eq!((d.def, d.owner, d.y), (wire("grain_bale"), presence_of(&t, 1).char_id, sim::movement::SEA_LEVEL_M), "floating");
    assert!((d.x - slot[0]).abs() < 1e-3 && (d.z - slot[2]).abs() < 1e-3);
    // Tobin swims to it and takes it: theft.
    stand_at(&t, 2, d.x, d.z, SEAWARD);
    assert_eq!(presence_of(&t, 2).mode, SWIM);
    call(&t, 2, |ctx| pick_up_pack(ctx, d.id)).unwrap();
    assert_eq!(back(&t, 2).map(|p| p.def), Some(wire("grain_bale")));

    // The Swiftgull is Mira's still, wrecked.
    let c = presence_of(&t, 1).char_id;
    let it = read(&t, |ctx| ctx.db.item().by_slot().filter((c, place::BAG)).find(|i| i.def == wire("swiftgull")).unwrap());
    let w = read(&t, |ctx| ctx.db.wreck().item_id().find(it.id).unwrap());
    assert!(!w.mending);
    assert_eq!(read(&t, |ctx| ctx.db.item().by_slot().filter((c, place::OUT)).count()), 0);
    stand_at(&t, 1, 52.0, 0.0, SEAWARD);
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, it.id)), Err("it's wrecked: mend it first".into()));
    assert_eq!(call(&t, 1, |ctx| mend_vehicle(ctx, it.id)), Err("mending it takes 5 Hullmender's Kit".into()));
    grant_n(&t, 1, "hullmenders_kit", 4);
    call(&t, 1, |ctx| mend_vehicle(ctx, it.id)).unwrap();
    assert_eq!(count_of(&t, 1, "hullmenders_kit"), 0);
    assert_eq!(call(&t, 1, |ctx| mend_vehicle(ctx, it.id)), Err("it's already being mended".into()));
    assert_eq!(call(&t, 1, |ctx| summon_vehicle(ctx, it.id)), Err("it's still being mended".into()));
    advance_ms(&t, i64::from(Hull::Clipper.data().mend_s) * 1000);
    call(&t, 1, |ctx| summon_vehicle(ctx, it.id)).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.wreck().item_id().find(it.id)), None);
    let fresh = read(&t, |ctx| ctx.db.vehicle().iter().next().unwrap());
    assert_eq!(fresh.hp, Hull::Clipper.data().hp, "back at full HP");
    let small = give(&t, 2, "driftpine_skiff");
    assert_eq!(call(&t, 2, |ctx| mend_vehicle(ctx, small)), Err("it isn't wrecked".into()));
}

#[test]
fn a_broken_cart_drops_its_packs_on_the_ground_and_a_full_bag_holds_the_wreck_aside() {
    let t = shore();
    arrive(&t, 1, "Mira");
    stand_at(&t, 1, 10.0, 0.0, 0.0);
    let v = summon(&t, 1, "furrow_cart");
    carry(&t, 1, "grain_bale");
    let slot = read(&t, |ctx| crate::ship::deck(ctx, v).unwrap().pose()).to_world([-0.45, 0.0, 0.6]);
    call(&t, 1, |ctx| load_pack(ctx, v, 0)).unwrap();
    fill_bag(&t, 1);
    call(&t, 1, |ctx| dev_damage_vehicle(ctx, v, 99_999)).unwrap();
    assert_eq!(call(&t, 1, |ctx| stow_vehicle(ctx, v)), Err("it's broken down".into()));
    read(&t, |ctx| wreck(ctx, v));
    let d = read(&t, |ctx| ctx.db.dropped_pack().iter().next().unwrap());
    assert!((d.y - 3.0).abs() < 1e-3 && (d.x - slot[0]).abs() < 1e-3, "on the ground: {d:?}");
    let c = presence_of(&t, 1).char_id;
    let held: Vec<Item> = read(&t, |ctx| ctx.db.item().by_slot().filter((c, place::RETURNS)).collect());
    assert_eq!(held.iter().map(|i| i.def).collect::<Vec<_>>(), [wire("furrow_cart")]);
    assert!(read(&t, |ctx| ctx.db.wreck().item_id().find(held[0].id)).is_some());
}

#[test]
fn destroying_a_wreck_forgets_it_and_my_vehicles_lists_only_ones_own() {
    let (t, v) = sailing();
    let mine: Vec<u64> = my_vehicles(&t.view_context(auth(1))).into_iter().map(|v| v.entity_id).collect();
    assert_eq!(mine, [v]);
    assert!(my_vehicles(&t.view_context(auth(2))).is_empty());
    call(&t, 1, |ctx| dev_damage_vehicle(ctx, v, 99_999)).unwrap();
    run_wreck(&t, wrecks(&t).pop().unwrap());
    let c = presence_of(&t, 1).char_id;
    let it = read(&t, |ctx| ctx.db.item().by_slot().filter((c, place::BAG)).find(|i| i.def == wire("swiftgull")).unwrap());
    let theirs = crate::vehicle::my_wrecks(&t.view_context(auth(1)));
    assert_eq!(theirs.iter().map(|w| w.item_id).collect::<Vec<_>>(), [it.id]);
    call(&t, 1, |ctx| crate::items::destroy_item(ctx, it.id, 1)).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.wreck().item_id().find(it.id)), None);
    let _ = ADMIN;
}
