//! The `items` scenario (docs/design/items.md): a bot fills its bag with dev grants, then moves,
//! merges, splits, swaps and destroys stacks, checking the bag it sees after each step, and has
//! bad requests refused. It empties its bag at the end.

use sim::content::{BAG_SLOTS, item, item_by_id};

use crate::module_bindings::*;
use crate::terrain::{flatten, outcome};
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

const REED: &str = "saltreed"; // stacks to 1000
const TART: &str = "duskberry_tart"; // stacks to 100
const KNIFE: &str = "reedcutter_knife"; // doesn't stack

type Bag = Vec<(u16, &'static str, u32)>;
/// A reducer's `_then` callback.
type Then = Box<dyn FnOnce(&ReducerEventContext, Result<Result<(), String>, spacetimedb_sdk::__codegen::InternalError>) + Send>;

pub fn scenario(args: &Args) -> Check {
    let bot = Bot::connect(args, "I")?;
    bot.subscribe_self()?;
    let applied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = applied.clone();
    bot.conn
        .subscription_builder()
        .on_applied(move |_| flag.store(true, std::sync::atomic::Ordering::SeqCst))
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(["SELECT * FROM my_inventory"]);
    wait_for("inventory subscription", || applied.load(std::sync::atomic::Ordering::SeqCst))?;
    let me = bot.arrive()?.char_id;
    let b = Items { bot: &bot, me };
    b.expect("an empty bag", &[])?;

    // 1,500 saltreed takes two stacks; the tarts and the knife follow in order.
    b.grant(REED, 1500)?;
    b.grant(TART, 30)?;
    b.grant(KNIFE, 1)?;
    b.expect("the grants", &[(0, REED, 1000), (1, REED, 500), (2, TART, 30), (3, KNIFE, 1)])?;
    println!("granted: {:?}", b.bag());

    // Move the knife to the far corner, then merge 500 onto a stack with room for only part of it.
    b.call(|tx| bot.conn.reducers.move_item_then(b.id(3), BAG_SLOTS - 1, tx))?;
    b.call(|tx| bot.conn.reducers.destroy_item_then(b.id(0), 300, tx))?;
    b.expect("a move and a destroy", &[(0, REED, 700), (1, REED, 500), (2, TART, 30), (BAG_SLOTS - 1, KNIFE, 1)])?;
    b.call(|tx| bot.conn.reducers.move_item_then(b.id(1), 0, tx))?;
    b.expect("a merge with some left over", &[(0, REED, 1000), (1, REED, 200), (2, TART, 30), (BAG_SLOTS - 1, KNIFE, 1)])?;
    println!("moved and merged: {:?}", b.bag());

    // Split ten tarts off, then swap the tarts with the saltreed.
    b.call(|tx| bot.conn.reducers.split_item_then(b.id(2), 10, 5, tx))?;
    b.call(|tx| bot.conn.reducers.move_item_then(b.id(2), 1, tx))?;
    b.expect(
        "a split and a swap",
        &[(0, REED, 1000), (1, TART, 20), (2, REED, 200), (5, TART, 10), (BAG_SLOTS - 1, KNIFE, 1)],
    )?;
    println!("split and swapped: {:?}", b.bag());

    // Refusals change nothing.
    let before = b.bag();
    for (what, want, r) in [
        ("an unknown item", "no such item", b.call(|tx| bot.conn.reducers.move_item_then(u64::MAX, 3, tx))),
        ("a slot past the bag", "no slot", b.call(|tx| bot.conn.reducers.move_item_then(b.id(0), BAG_SLOTS, tx))),
        ("a split onto another item", "is taken", b.call(|tx| bot.conn.reducers.split_item_then(b.id(1), 5, 0, tx))),
        ("a whole-stack split", "split 1 to", b.call(|tx| bot.conn.reducers.split_item_then(b.id(5), 10, 7, tx))),
        ("a grant that doesn't fit", "bag is full", b.grant(KNIFE, u32::from(BAG_SLOTS))),
    ] {
        let e = r.err().ok_or_else(|| format!("{what} was accepted"))?;
        if !e.contains(want) {
            return Err(format!("{what}: unexpected refusal {e:?}"));
        }
        println!("refused {what}: {e}");
    }
    if b.bag() != before {
        return Err(format!("a refusal changed the bag: {:?}", b.bag()));
    }

    // Empty the bag, so a rerun with this identity starts clean.
    for slot in before.iter().map(|s| s.0) {
        let (id, count) = (b.id(slot), before.iter().find(|s| s.0 == slot).unwrap().2);
        b.call(|tx| bot.conn.reducers.destroy_item_then(id, count, tx))?;
    }
    b.expect("an empty bag again", &[])?;
    bot.conn.disconnect().map_err(|e| e.to_string())
}

struct Items<'a> {
    bot: &'a Bot,
    me: u64,
}

impl Items<'_> {
    fn rows(&self) -> Vec<Item> {
        self.bot.conn.db.my_inventory().iter().filter(|i| i.holder == self.me).collect()
    }

    fn bag(&self) -> Bag {
        let mut b: Bag = self.rows().iter().map(|i| (i.slot, item(i.def).map_or("?", |d| d.id), i.count)).collect();
        b.sort();
        b
    }

    /// The id of the stack in `slot`; 0 (no such item) if it's empty.
    fn id(&self, slot: u16) -> u64 {
        self.rows().iter().find(|i| i.slot == slot).map_or(0, |i| i.id)
    }

    fn expect(&self, what: &str, want: &[(u16, &'static str, u32)]) -> Check {
        wait_for(what, || self.bag() == want).map_err(|e| format!("{e}: have {:?}, want {want:?}", self.bag()))
    }

    /// Call a reducer through its `_then` form and wait for its outcome.
    fn call(&self, send: impl FnOnce(Then) -> spacetimedb_sdk::Result<()>) -> Check {
        outcome(|tx| send(Box::new(move |_, r| drop(tx.send(flatten(r))))))
    }

    fn grant(&self, id: &str, count: u32) -> Check {
        item_by_id(id).ok_or_else(|| format!("no item {id}"))?;
        let id = id.to_string();
        self.call(|tx| self.bot.conn.reducers.dev_grant_item_then(id, count, tx))
    }
}
