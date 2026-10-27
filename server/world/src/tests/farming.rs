//! Farming (docs/design/farming.md).

use super::land::{advance_us, ground, give, held, now_us, place, plots, set_position, settler, stack};
use super::*;
use crate::farming::{Crop, admin_remove_crops, crop, harvest, plant, uproot};
use crate::land::{abandon_plot, set_plot_access};
use crate::items::item;
use sim::content::item_by_id;
use sim::farm::{crop_by_id, growth_micros, withers_at};
use sim::land::demolish_at;
use sim::movement::MoveMode;

fn crops(t: &TestContext) -> Vec<Crop> {
    read(t, |ctx| ctx.db.crop().iter().collect())
}

fn sow(t: &TestContext, player: u128, seed: &str, x: f32, z: f32) -> Result<(), String> {
    let char_id = presence_of(t, player).char_id;
    let item_id = stack(t, char_id, seed);
    call(t, player, |ctx| plant(ctx, item_id, x, z))
}

/// Arrive with plot deeds and seals, and seeds of every kind.
fn farmer(t: &TestContext, player: u128, name: &str) -> Presence {
    let p = settler(t, player, name);
    for seed in ["hearthgrain_seed", "dustbean_seed", "amberpear_sapling", "woolback_lamb"] {
        give(t, p.char_id, seed, 5);
    }
    p
}

fn labor(t: &TestContext, player: u128) -> i32 {
    read(t, |ctx| crate::labor::available(ctx, identity(player))).unwrap()
}

fn only_crop(t: &TestContext) -> Crop {
    let cs = crops(t);
    assert_eq!(cs.len(), 1, "{cs:?}");
    cs[0].clone()
}

/// Move the clock to just past `c` being ripe.
fn ripen(t: &TestContext, c: &Crop) {
    advance_us(t, c.ripe_at.to_micros_since_unix_epoch() - now_us(t) + 1);
}

#[test]
fn planting_takes_a_seed_and_times_growth_by_climate() {
    let t = world();
    ground(&t);
    let me = farmer(&t, 1, "Mira");
    set_position(&t, 1, 1.0, 1.0);
    sow(&t, 1, "hearthgrain_seed", 2.5, 2.0).unwrap();
    let c = only_crop(&t);
    let grain = crop_by_id("hearthgrain").unwrap();
    assert_eq!((c.kind, c.owner, c.plot, c.x, c.y, c.z, c.ideal, c.harvests), (grain.wire, me.char_id, 0, 2.5, 10.0, 2.0, true, 0));
    assert_eq!(c.ripe_at.to_micros_since_unix_epoch() - c.planted_at.to_micros_since_unix_epoch(), growth_micros(grain.grow_s, true));
    assert_eq!((c.grown_from, c.cell), (c.planted_at, sim::cell::cell_of(sim::cell::OPEN_WORLD, 2.5, 2.0)));
    assert_eq!(held(&t, me.char_id, "hearthgrain_seed"), 4);
    // Dustbean wants an arid climate; the test ground is temperate.
    sow(&t, 1, "dustbean_seed", 0.0, 0.0).unwrap();
    let bean = crops(&t).into_iter().find(|c| c.kind == crop_by_id("dustbean").unwrap().wire).unwrap();
    assert!(!bean.ideal);
    assert_eq!(bean.ripe_at.to_micros_since_unix_epoch() - bean.planted_at.to_micros_since_unix_epoch(), growth_micros(crop_by_id("dustbean").unwrap().grow_s, false));
}

#[test]
fn refusals_change_nothing() {
    let t = world();
    ground(&t);
    let me = farmer(&t, 1, "Mira");
    let other = farmer(&t, 2, "Tobin");
    let err = |r: Result<(), String>| r.unwrap_err();
    set_position(&t, 1, 0.0, 0.0);
    assert_eq!(err(sow(&t, 1, "levy_seal", 1.0, 0.0)), "that isn't a seed");
    let theirs = stack(&t, other.char_id, "hearthgrain_seed");
    assert_eq!(err(call(&t, 1, |ctx| plant(ctx, theirs, 1.0, 0.0))), "no such item");
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", f32::NAN, 0.0)), "non-finite position");
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", 3.1, 0.0)), "too far away");
    assert_eq!(err(sow(&t, 1, "woolback_lamb", 1.0, 0.0)), "this can only be kept on a plot");
    set_position(&t, 1, 22.0, 22.0);
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", 22.0, 22.0)), "nothing can be planted here", "the town texel");
    set_position(&t, 1, 200.0, 0.0);
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", 200.0, 0.0)), "no ground here");
    set_position(&t, 1, 0.0, 0.0);
    sow(&t, 1, "hearthgrain_seed", 0.0, 0.0).unwrap();
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", 0.9, 0.0)), "too close to another planting");
    assert_eq!(err(sow(&t, 1, "amberpear_sapling", 2.4, 0.0)), "too close to another planting", "a tree needs 2.5 m from a crop");
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { mode: MoveMode::Swim as u8, ..p });
    });
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", 2.0, 0.0)), "can't plant while swimming");
    read(&t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(1)).unwrap();
        ctx.db.presence().identity().update(Presence { mode: 1, parent: 7, ..p });
    });
    assert_eq!(err(sow(&t, 1, "hearthgrain_seed", 2.0, 0.0)), "can't plant from a ship");
    assert_eq!(crops(&t).len(), 2);
    assert_eq!(held(&t, me.char_id, "hearthgrain_seed"), 3);
    assert_eq!(err(call(&t, 3, |ctx| plant(ctx, theirs, 0.0, 0.0))), "not in the world");
}

#[test]
fn plots_decide_who_plants_and_keep_livestock() {
    let t = world();
    ground(&t);
    let me = farmer(&t, 1, "Mira");
    farmer(&t, 2, "Tobin");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let garden = plots(&t)[0].clone();
    set_position(&t, 1, 1.0, 1.0);
    sow(&t, 1, "woolback_lamb", 2.0, 2.0).unwrap();
    assert_eq!(only_crop(&t).plot, garden.id);
    set_position(&t, 2, -2.0, -2.0);
    assert_eq!(sow(&t, 2, "hearthgrain_seed", -2.0, -2.0).unwrap_err(), "this plot isn't yours to plant");
    sow(&t, 2, "hearthgrain_seed", -2.0, -4.5).unwrap();
    assert_eq!(crops(&t).iter().find(|c| c.z == -4.5).unwrap().plot, 0, "just off the plot");
    call(&t, 1, |ctx| set_plot_access(ctx, garden.id, 1)).unwrap();
    sow(&t, 2, "hearthgrain_seed", -2.0, -2.0).unwrap();
    assert_eq!(crops(&t).len(), 3);
    // Giving the plot up takes what grows on it; the crop beside it stays.
    call(&t, 1, |ctx| abandon_plot(ctx, garden.id)).unwrap();
    let left = crops(&t);
    assert_eq!((left.len(), left[0].plot), (1, 0));
    assert_eq!(held(&t, me.char_id, "woolback_lamb"), 4, "the lamb is gone with the plot");
}

#[test]
fn harvesting_by_the_clock() {
    let t = world();
    ground(&t);
    let me = farmer(&t, 1, "Mira");
    set_position(&t, 1, 0.0, 0.0);
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    let c = only_crop(&t);
    assert_eq!(call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap_err(), "it isn't ripe yet");
    ripen(&t, &c);
    let before = labor(&t, 1);
    call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap();
    let grain = crop_by_id("hearthgrain").unwrap();
    assert!((grain.yield_min..=grain.yield_max).contains(&held(&t, me.char_id, "hearthgrain")));
    assert_eq!(labor(&t, 1), before - grain.labor);
    assert!(crops(&t).is_empty(), "a crop is gone after its harvest");
    assert_eq!(call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap_err(), "no such crop");

    // A tree fruits again, sooner in its climate.
    sow(&t, 1, "amberpear_sapling", 2.0, 0.0).unwrap();
    let tree = only_crop(&t);
    ripen(&t, &tree);
    call(&t, 1, |ctx| harvest(ctx, tree.id)).unwrap();
    let again = only_crop(&t);
    let pear = crop_by_id("amberpear_tree").unwrap();
    assert_eq!(again.harvests, 1);
    assert_eq!(again.grown_from.to_micros_since_unix_epoch(), now_us(&t));
    assert_eq!(again.ripe_at.to_micros_since_unix_epoch() - now_us(&t), growth_micros(pear.regrow_s, true));
    assert!(held(&t, me.char_id, "amberpear") >= pear.yield_min);
    assert_eq!(call(&t, 1, |ctx| harvest(ctx, tree.id)).unwrap_err(), "it isn't ripe yet");
    set_position(&t, 1, 6.0, 0.0);
    ripen(&t, &again);
    assert_eq!(call(&t, 1, |ctx| harvest(ctx, tree.id)).unwrap_err(), "too far away");
}

#[test]
fn a_harvest_needs_room_and_labor() {
    let t = world();
    ground(&t);
    let me = farmer(&t, 1, "Mira");
    set_position(&t, 1, 0.0, 0.0);
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    let c = only_crop(&t);
    ripen(&t, &c);
    // A bag full of knives has no room for grain.
    let used = read(&t, |ctx| ctx.db.item().by_slot().filter((me.char_id, crate::items::place::BAG)).count() as u32);
    give(&t, me.char_id, "reedcutter_knife", u32::from(sim::content::BAG_SLOTS) - used);
    let have = labor(&t, 1);
    let e = call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap_err();
    assert!(e.starts_with("bag is full"), "{e}");
    assert_eq!((labor(&t, 1), crops(&t).len()), (have, 1), "nothing spent, nothing taken");
    let knife = item_by_id("reedcutter_knife").unwrap().wire;
    read(&t, |ctx| {
        for i in ctx.db.item().iter().filter(|i| i.def == knife).collect::<Vec<_>>() {
            ctx.db.item().id().delete(i.id);
        }
    });
    read(&t, |ctx| crate::labor::spend(ctx, identity(1), have - 2)).unwrap();
    let e = call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap_err();
    assert!(e.starts_with("not enough labor"), "{e}");
    assert_eq!(crops(&t).len(), 1);
}

#[test]
fn left_ripe_a_crop_withers_and_anyone_may_clear_it() {
    let t = world();
    ground(&t);
    farmer(&t, 1, "Mira");
    farmer(&t, 2, "Tobin");
    set_position(&t, 1, 0.0, 0.0);
    set_position(&t, 2, 0.0, 1.0);
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    let c = only_crop(&t);
    let grain = crop_by_id("hearthgrain").unwrap();
    advance_us(&t, withers_at(grain, c.ripe_at.to_micros_since_unix_epoch()).unwrap() - now_us(&t));
    assert_eq!(call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap_err(), "it has withered");
    // Planting over it clears it.
    sow(&t, 2, "hearthgrain_seed", 1.0, 0.0).unwrap();
    let fresh = only_crop(&t);
    assert_ne!(fresh.id, c.id);
    advance_us(&t, i64::from(grain.wither_s) * 2_000_000 + growth_micros(grain.grow_s, false));
    call(&t, 1, |ctx| uproot(ctx, fresh.id)).unwrap();
    assert!(crops(&t).is_empty(), "a stranger clears a withered crop");
}

#[test]
fn off_a_plot_a_ripe_crop_can_be_stolen() {
    let t = world();
    ground(&t);
    farmer(&t, 1, "Mira");
    let thief = farmer(&t, 2, "Tobin");
    set_position(&t, 1, 0.0, 0.0);
    set_position(&t, 2, 0.0, 1.0);
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    sow(&t, 1, "hearthgrain_seed", -1.0, 0.0).unwrap();
    let (a, b) = (crops(&t)[0].clone(), crops(&t)[1].clone());
    assert_eq!(call(&t, 2, |ctx| harvest(ctx, a.id)).unwrap_err(), "it isn't ripe yet");
    assert_eq!(call(&t, 2, |ctx| uproot(ctx, a.id)).unwrap_err(), "this isn't yours to uproot");
    ripen(&t, &a);
    call(&t, 2, |ctx| harvest(ctx, a.id)).unwrap();
    assert!(held(&t, thief.char_id, "hearthgrain") > 0);
    call(&t, 2, |ctx| uproot(ctx, b.id)).unwrap();
    assert!(crops(&t).is_empty());
}

#[test]
fn on_a_private_plot_nothing_can_be_taken() {
    let t = world();
    ground(&t);
    farmer(&t, 1, "Mira");
    farmer(&t, 2, "Tobin");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    set_position(&t, 1, 1.0, 1.0);
    set_position(&t, 2, 1.0, 2.0);
    sow(&t, 1, "woolback_lamb", 1.0, 1.0).unwrap();
    let c = only_crop(&t);
    ripen(&t, &c);
    assert_eq!(call(&t, 2, |ctx| harvest(ctx, c.id)).unwrap_err(), "this isn't yours to harvest");
    assert_eq!(call(&t, 2, |ctx| uproot(ctx, c.id)).unwrap_err(), "this isn't yours to uproot");
    call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap();
    assert_eq!(only_crop(&t).harvests, 1, "livestock is shorn again and again");
}

#[test]
fn a_demolished_plot_takes_its_plantings() {
    let t = world();
    ground(&t);
    farmer(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let garden = plots(&t)[0].clone();
    set_position(&t, 1, 1.0, 1.0);
    sow(&t, 1, "amberpear_sapling", 1.0, 1.0).unwrap();
    let c = only_crop(&t);
    // Past the demolition time, but before the job runs: the tree is gone with its plot.
    advance_us(&t, demolish_at(garden.tax_paid_until.to_micros_since_unix_epoch()) - now_us(&t));
    assert_eq!(call(&t, 1, |ctx| harvest(ctx, c.id)).unwrap_err(), "its plot is gone");
    super::land::run_job(&t, garden.id);
    assert!(crops(&t).is_empty() && plots(&t).is_empty());
}

#[test]
fn only_admins_remove_crops() {
    let t = world();
    ground(&t);
    farmer(&t, 1, "Mira");
    set_position(&t, 1, 0.0, 0.0);
    sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    let c = only_crop(&t);
    assert!(call(&t, 1, |ctx| admin_remove_crops(ctx, vec![c.id])).is_err());
    call(&t, 99, |ctx| admin_remove_crops(ctx, vec![c.id, 12345])).unwrap();
    assert!(crops(&t).is_empty());
}
