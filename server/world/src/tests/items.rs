//! Items and the bag (docs/design/items.md).

use super::*;
use crate::items::{Item, destroy_item, grant, item, move_item, my_inventory, place, split_item};
use sim::content::{BAG_SLOTS, item_by_id};

fn wire(id: &str) -> u16 {
    item_by_id(id).unwrap().wire
}

const REED: &str = "saltreed"; // material, stacks to 1000
const TART: &str = "duskberry_tart"; // consumable, stacks to 100
const KNIFE: &str = "reedcutter_knife"; // gear, doesn't stack

fn give(t: &TestContext, char_id: u64, id: &str, count: u32) -> Result<(), String> {
    read(t, |ctx| grant(ctx, char_id, wire(id), count))
}

/// The bag as (slot, item id, count), by slot.
fn bag(t: &TestContext, char_id: u64) -> Vec<(u16, &'static str, u32)> {
    read(t, |ctx| {
        let mut rows: Vec<_> = ctx
            .db
            .item()
            .by_slot()
            .filter((char_id, place::BAG))
            .map(|i| (i.slot, sim::content::item(i.def).unwrap().id, i.count))
            .collect();
        rows.sort();
        rows
    })
}

fn at(t: &TestContext, char_id: u64, slot: u16) -> Item {
    read(t, |ctx| ctx.db.item().by_slot().filter((char_id, place::BAG, slot)).next().unwrap())
}

#[test]
fn grant_tops_up_stacks_then_fills_empty_slots_in_order() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, REED, 900).unwrap();
    give(&t, me, TART, 5).unwrap();
    give(&t, me, REED, 1300).unwrap();
    assert_eq!(bag(&t, me), [(0, REED, 1000), (1, TART, 5), (2, REED, 1000), (3, REED, 200)]);
    give(&t, me, KNIFE, 2).unwrap();
    assert_eq!(bag(&t, me)[4..], [(4, KNIFE, 1), (5, KNIFE, 1)]);
}

#[test]
fn grant_is_all_or_nothing() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, KNIFE, u32::from(BAG_SLOTS) - 1).unwrap();
    give(&t, me, TART, 60).unwrap();
    let before = bag(&t, me);
    // 40 top up the tarts, 100 fill nothing: there's no empty slot left.
    assert_eq!(give(&t, me, TART, 141), Err("bag is full: room for 40 of 141".into()));
    assert_eq!(give(&t, me, KNIFE, 1), Err("bag is full: room for 0 of 1".into()));
    assert_eq!(bag(&t, me), before);
    give(&t, me, TART, 40).unwrap();
    assert!(read(&t, |ctx| grant(ctx, me, 0, 1)).unwrap_err().contains("no item"));
    assert!(give(&t, me, REED, 0).is_err());
}

#[test]
fn moving_to_an_empty_slot() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, REED, 10).unwrap();
    let id = at(&t, me, 0).id;
    call(&t, 1, |ctx| move_item(ctx, id, 7)).unwrap();
    assert_eq!(bag(&t, me), [(7, REED, 10)]);
    assert_eq!(at(&t, me, 7).id, id);
    call(&t, 1, |ctx| move_item(ctx, id, 7)).unwrap();
    assert_eq!(bag(&t, me), [(7, REED, 10)]);
    assert!(call(&t, 1, |ctx| move_item(ctx, id, BAG_SLOTS)).unwrap_err().contains("no slot 50"));
}

#[test]
fn moving_onto_the_same_item_merges_as_far_as_it_fits() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, REED, 1000).unwrap();
    give(&t, me, REED, 1000).unwrap();
    let (a, b) = (at(&t, me, 0).id, at(&t, me, 1).id);
    call(&t, 1, |ctx| destroy_item(ctx, a, 300)).unwrap();
    call(&t, 1, |ctx| destroy_item(ctx, b, 600)).unwrap();
    // 700 and 400: moving the 400 onto the 700 fills it, leaving 100 behind.
    call(&t, 1, |ctx| move_item(ctx, b, 0)).unwrap();
    assert_eq!(bag(&t, me), [(0, REED, 1000), (1, REED, 100)]);
    call(&t, 1, |ctx| destroy_item(ctx, a, 500)).unwrap();
    // The 100 all fits into the 500, and its row goes.
    call(&t, 1, |ctx| move_item(ctx, b, 0)).unwrap();
    assert_eq!(bag(&t, me), [(0, REED, 600)]);
    assert!(read(&t, |ctx| ctx.db.item().id().find(b)).is_none());
}

#[test]
fn moving_onto_another_item_or_a_full_stack_swaps() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, REED, 1000).unwrap();
    give(&t, me, TART, 3).unwrap();
    give(&t, me, REED, 20).unwrap();
    let (reed, tart, small) = (at(&t, me, 0).id, at(&t, me, 1).id, at(&t, me, 2).id);
    call(&t, 1, |ctx| move_item(ctx, reed, 1)).unwrap();
    assert_eq!(bag(&t, me), [(0, TART, 3), (1, REED, 1000), (2, REED, 20)]);
    assert_eq!((at(&t, me, 0).id, at(&t, me, 1).id), (tart, reed));
    // Onto a full stack of the same item: nothing fits, so they swap.
    call(&t, 1, |ctx| move_item(ctx, small, 1)).unwrap();
    assert_eq!(bag(&t, me), [(0, TART, 3), (1, REED, 20), (2, REED, 1000)]);
    assert_eq!(at(&t, me, 1).id, small);
}

#[test]
fn splitting_into_an_empty_slot_or_onto_the_same_item() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, TART, 50).unwrap();
    let id = at(&t, me, 0).id;
    call(&t, 1, |ctx| split_item(ctx, id, 20, 4)).unwrap();
    assert_eq!(bag(&t, me), [(0, TART, 30), (4, TART, 20)]);
    assert_ne!(at(&t, me, 4).id, id);
    call(&t, 1, |ctx| split_item(ctx, id, 29, 4)).unwrap();
    assert_eq!(bag(&t, me), [(0, TART, 1), (4, TART, 49)]);
}

#[test]
fn refused_splits_change_nothing() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, TART, 100).unwrap();
    give(&t, me, TART, 90).unwrap();
    give(&t, me, REED, 5).unwrap();
    let (full, ninety) = (at(&t, me, 0).id, at(&t, me, 1).id);
    let before = bag(&t, me);
    let split = |id, n, to| call(&t, 1, |ctx| split_item(ctx, id, n, to));
    assert_eq!(split(full, 11, 1), Err("no room for 11 in slot 1".into()));
    assert_eq!(split(full, 5, 2), Err("slot 2 is taken".into()));
    assert_eq!(split(ninety, 0, 9), Err("split 1 to 89 of this stack".into()));
    assert_eq!(split(ninety, 90, 9), Err("split 1 to 89 of this stack".into()));
    assert!(split(ninety, 1, BAG_SLOTS).unwrap_err().contains("no slot"));
    assert_eq!(bag(&t, me), before);
    split(full, 10, 1).unwrap();
    assert_eq!(bag(&t, me)[..2], [(0, TART, 90), (1, TART, 100)]);
}

#[test]
fn destroying_some_or_all_of_a_stack() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, REED, 30).unwrap();
    let id = at(&t, me, 0).id;
    assert_eq!(call(&t, 1, |ctx| destroy_item(ctx, id, 31)), Err("destroy 1 to 30 of this stack".into()));
    assert!(call(&t, 1, |ctx| destroy_item(ctx, id, 0)).is_err());
    call(&t, 1, |ctx| destroy_item(ctx, id, 12)).unwrap();
    assert_eq!(bag(&t, me), [(0, REED, 18)]);
    call(&t, 1, |ctx| destroy_item(ctx, id, 18)).unwrap();
    assert_eq!(bag(&t, me), []);
}

#[test]
fn other_players_items_are_no_such_item() {
    let t = world();
    let mira = arrive(&t, 1, "Mira").char_id;
    arrive(&t, 2, "Tobin");
    give(&t, mira, REED, 5).unwrap();
    let id = at(&t, mira, 0).id;
    for r in [
        call(&t, 2, |ctx| move_item(ctx, id, 3)),
        call(&t, 2, |ctx| split_item(ctx, id, 1, 3)),
        call(&t, 2, |ctx| destroy_item(ctx, id, 1)),
        call(&t, 2, |ctx| move_item(ctx, id + 1000, 3)),
    ] {
        assert_eq!(r, Err("no such item".into()));
    }
    assert_eq!(bag(&t, mira), [(0, REED, 5)]);
}

#[test]
fn bag_work_needs_a_character_in_the_world() {
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    give(&t, me, REED, 5).unwrap();
    let id = at(&t, me, 0).id;
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(call(&t, 1, |ctx| move_item(ctx, id, 3)), Err("not in the world".into()));
    // The items stay with the character.
    assert_eq!(bag(&t, me), [(0, REED, 5)]);
}

#[test]
fn the_view_shows_your_own_characters_bags_only() {
    let t = world();
    let mira = arrive(&t, 1, "Mira").char_id;
    call(&t, 1, |ctx| create_character(ctx, "Tobin".into())).unwrap();
    let tobin = read(&t, |ctx| ctx.db.character_private().owner().filter(identity(1)).last().unwrap().char_id);
    let ash = arrive(&t, 2, "Ash").char_id;
    give(&t, mira, REED, 5).unwrap();
    give(&t, tobin, TART, 2).unwrap();
    give(&t, ash, KNIFE, 1).unwrap();
    let mut mine: Vec<_> = my_inventory(&t.view_context(auth(1))).into_iter().map(|i| (i.holder, i.count)).collect();
    mine.sort();
    assert_eq!(mine, [(mira, 5), (tobin, 2)]);
    let theirs: Vec<_> = my_inventory(&t.view_context(auth(2))).into_iter().map(|i| i.holder).collect();
    assert_eq!(theirs, [ash]);
    assert!(my_inventory(&t.view_context(auth(3))).is_empty());
}

#[cfg(feature = "dev")]
#[test]
fn dev_grant_goes_by_content_id() {
    use crate::items::dev_grant_item;
    let t = world();
    let me = arrive(&t, 1, "Mira").char_id;
    call(&t, 1, |ctx| dev_grant_item(ctx, "tinstone".into(), 3)).unwrap();
    assert_eq!(bag(&t, me), [(0, "tinstone", 3)]);
    assert_eq!(call(&t, 1, |ctx| dev_grant_item(ctx, "gold".into(), 1)), Err("no item \"gold\"".into()));
    assert_eq!(call(&t, 3, |ctx| dev_grant_item(ctx, "tinstone".into(), 1)), Err("not in the world".into()));
}
