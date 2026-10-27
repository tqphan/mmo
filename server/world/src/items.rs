//! Items and the bag (docs/design/items.md): one private row per stack, read through
//! `my_inventory`, changed only by the reducers here and by work that grants items.

use spacetimedb::{ReducerContext, Table, ViewContext};
use sim::content::{BAG_SLOTS, item as def_of};

use crate::session::{character_private__view, presence};

/// `Item::place` values: what kind of container holds a stack, and so what `holder` is.
pub mod place {
    /// A character's bag; the holder is its `char_id`.
    pub const BAG: u8 = 1;
}

/// Private: one stack. At most one per slot, and `1 ≤ count ≤` the item's stack size; the
/// reducers keep both rules.
#[spacetimedb::table(accessor = item, index(accessor = by_slot, btree(columns = [holder, place, slot])))]
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub holder: u64,
    pub place: u8,
    pub slot: u16,
    /// The item's `wire` in `content/items.toml`.
    pub def: u16,
    pub count: u32,
}

/// The bag stacks of all the caller's characters. It reads `character_private` rather than
/// `presence`, which changes on every move.
#[spacetimedb::view(accessor = my_inventory, public)]
pub(crate) fn my_inventory(ctx: &ViewContext) -> Vec<Item> {
    ctx.db
        .character_private()
        .owner()
        .filter(ctx.sender())
        .flat_map(|c| ctx.db.item().by_slot().filter((c.char_id, place::BAG)).collect::<Vec<_>>())
        .collect()
}

/// The most of `def` one slot holds. A stack whose item was removed from content holds what it has.
fn stack_of(def: u16, count: u32) -> u32 {
    def_of(def).map_or(count, |d| d.stack)
}

fn at(ctx: &ReducerContext, char_id: u64, slot: u16) -> Option<Item> {
    ctx.db.item().by_slot().filter((char_id, place::BAG, slot)).next()
}

/// The caller's character in the world.
fn in_world(ctx: &ReducerContext) -> Result<u64, String> {
    ctx.db.presence().identity().find(ctx.sender()).map(|p| p.char_id).ok_or_else(|| "not in the world".into())
}

/// A stack in the caller's bag. Anyone else's, or none, reads the same, so ids aren't revealed.
fn mine(ctx: &ReducerContext, item_id: u64) -> Result<Item, String> {
    let char_id = in_world(ctx)?;
    ctx.db
        .item()
        .id()
        .find(item_id)
        .filter(|i| i.holder == char_id && i.place == place::BAG)
        .ok_or_else(|| "no such item".into())
}

fn check_slot(slot: u16) -> Result<(), String> {
    if slot < BAG_SLOTS { Ok(()) } else { Err(format!("no slot {slot}; the bag has {BAG_SLOTS}")) }
}

/// Set a stack's count, deleting it at 0.
fn set_count(ctx: &ReducerContext, it: Item, count: u32) {
    if count == 0 {
        ctx.db.item().id().delete(it.id);
    } else {
        ctx.db.item().id().update(Item { count, ..it });
    }
}

/// Move a stack to `to_slot`: into an empty slot, merging into a stack of the same item as far as
/// it has room, or otherwise swapping places with what's there.
#[spacetimedb::reducer]
pub fn move_item(ctx: &ReducerContext, item_id: u64, to_slot: u16) -> Result<(), String> {
    let it = mine(ctx, item_id)?;
    check_slot(to_slot)?;
    if to_slot == it.slot {
        return Ok(());
    }
    match at(ctx, it.holder, to_slot) {
        None => {
            ctx.db.item().id().update(Item { slot: to_slot, ..it });
        }
        Some(there) if there.def == it.def && there.count < stack_of(there.def, there.count) => {
            let moved = it.count.min(stack_of(there.def, there.count) - there.count);
            let left = it.count - moved;
            ctx.db.item().id().update(Item { count: there.count + moved, ..there });
            set_count(ctx, it, left);
        }
        Some(there) => {
            let from = it.slot;
            ctx.db.item().id().update(Item { slot: to_slot, ..it });
            ctx.db.item().id().update(Item { slot: from, ..there });
        }
    }
    Ok(())
}

/// Take `count` off a stack into an empty slot, or onto a stack of the same item with room for all of it.
#[spacetimedb::reducer]
pub fn split_item(ctx: &ReducerContext, item_id: u64, count: u32, to_slot: u16) -> Result<(), String> {
    let it = mine(ctx, item_id)?;
    check_slot(to_slot)?;
    if count == 0 || count >= it.count {
        return Err(format!("split 1 to {} of this stack", it.count.saturating_sub(1)));
    }
    match at(ctx, it.holder, to_slot) {
        None => {
            ctx.db.item().insert(Item { id: 0, slot: to_slot, count, ..it.clone() });
        }
        Some(there) if there.def == it.def => {
            let room = stack_of(there.def, there.count) - there.count;
            if count > room {
                return Err(format!("no room for {count} in slot {to_slot}"));
            }
            ctx.db.item().id().update(Item { count: there.count + count, ..there });
        }
        Some(_) => return Err(format!("slot {to_slot} is taken")),
    }
    let left = it.count - count;
    set_count(ctx, it, left);
    Ok(())
}

/// Destroy `count` of a stack, all of it if `count` is its whole count.
#[spacetimedb::reducer]
pub fn destroy_item(ctx: &ReducerContext, item_id: u64, count: u32) -> Result<(), String> {
    let it = mine(ctx, item_id)?;
    if count == 0 || count > it.count {
        return Err(format!("destroy 1 to {} of this stack", it.count));
    }
    let left = it.count - count;
    set_count(ctx, it, left);
    Ok(())
}

/// Put `count` of item `def` in `char_id`'s bag: topping up its stacks of that item, lowest slot
/// first, then filling empty slots from the lowest. All or nothing: if it doesn't all fit, nothing changes.
#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only `dev_grant_item` grants until gathering (M1 task 3)
pub fn grant(ctx: &ReducerContext, char_id: u64, def: u16, count: u32) -> Result<(), String> {
    let d = def_of(def).ok_or_else(|| format!("no item {def}"))?;
    if count == 0 {
        return Err("grant at least 1".into());
    }
    let bag: Vec<Item> = ctx.db.item().by_slot().filter((char_id, place::BAG)).collect();
    let mut tops = Vec::new();
    let mut left = count;
    for s in bag.iter().filter(|s| s.def == def && s.count < d.stack) {
        if left == 0 {
            break;
        }
        let add = left.min(d.stack - s.count);
        tops.push((s.clone(), s.count + add));
        left -= add;
    }
    let mut news = Vec::new();
    let mut slot = 0;
    while left > 0 && slot < BAG_SLOTS {
        if !bag.iter().any(|s| s.slot == slot) {
            let n = left.min(d.stack);
            news.push((slot, n));
            left -= n;
        }
        slot += 1;
    }
    if left > 0 {
        return Err(format!("bag is full: room for {} of {count}", count - left));
    }
    for (s, n) in tops {
        ctx.db.item().id().update(Item { count: n, ..s });
    }
    for (slot, n) in news {
        ctx.db.item().insert(Item { id: 0, holder: char_id, place: place::BAG, slot, def, count: n });
    }
    Ok(())
}

/// Dev builds only: grant `count` of the item with content id `item` to the caller's character in the world.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_grant_item(ctx: &ReducerContext, item: String, count: u32) -> Result<(), String> {
    let d = sim::content::item_by_id(&item).ok_or_else(|| format!("no item {item:?}"))?;
    let char_id = in_world(ctx)?;
    grant(ctx, char_id, d.wire, count)
}
