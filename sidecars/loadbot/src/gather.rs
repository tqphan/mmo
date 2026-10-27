//! The `gather` scenario (docs/design/gathering.md): two bots by a node the scenario places. Out of
//! reach and no labor are refused; A gathers while B is refused as taken; A's bag and labor change
//! by the yield and the cost; the node is depleted until the dev clock regrows it; moving away
//! and `cancel_gather` both stop a gather and cost nothing. It removes its nodes and restores
//! A's bag and labor at the end.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sim::content::item_by_id;
use sim::gather::{NodeData, node_by_id};

use crate::module_bindings::*;
use crate::terrain::{flatten, outcome};
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

const REED: &str = "saltreed_clump";
/// In the town round spawn, where the world places no nodes of its own.
const SPOT: (f32, f32) = (60.0, -60.0);

pub fn scenario(args: &Args) -> Check {
    let kind = node_by_id(REED).ok_or("no saltreed clump in content")?;
    let a = Bot::connect(args, "A")?;
    let b = Bot::connect(args, "B")?;
    for bot in [&a, &b] {
        bot.subscribe_self()?;
        subscribe(bot, &["SELECT * FROM my_labor", "SELECT * FROM my_inventory", "SELECT * FROM game_clock"])?;
        bot.arrive()?;
    }
    call(&a, |tx| a.conn.reducers.dev_set_gm_then(3, tx))?;
    a.teleport(SPOT.0, SPOT.1)?;
    b.teleport(SPOT.0 + 1.0, SPOT.1)?;
    let me = a.presence().ok_or("A left the world")?;
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, me.x, me.z);
    subscribe(&a, &[&format!("SELECT * FROM resource_node WHERE cell = {cell}")])?;

    // Our own nodes, both in the cell we watch: one in reach and one 8 m off.
    let near = place(&a, kind, me.x + 1.5, me.y, me.z)?;
    let far = place(&a, kind, me.x - 8.0, me.y, me.z)?;
    println!("placed nodes {near} (in reach) and {far} (8 m away) at ({:.1}, {:.1})", me.x, me.z);
    let reeds = |bot: &Bot| held(bot, me.char_id, "saltreed");
    let start_reeds = reeds(&a);

    refused(&a, "a node out of reach", "out of reach", far)?;
    set_labor(&a, 0)?;
    refused(&a, "a gather without labor", "not enough labor", near)?;
    set_labor(&a, 5000)?;

    // A gathers; B can't take it meanwhile.
    gather(&a, near)?;
    let busy = node(&a, near).ok_or("node gone")?;
    if busy.gatherer != me.entity_id {
        return Err(format!("node {near} shows gatherer {}, not A ({})", busy.gatherer, me.entity_id));
    }
    refused(&b, "a node someone is gathering", "someone else is gathering this", near)?;
    wait_done(&a, kind, near)?;
    let got = reeds(&a) - start_reeds;
    if !(kind.yield_min..=kind.yield_max).contains(&got) {
        return Err(format!("gathered {got} saltreed, expected {} to {}", kind.yield_min, kind.yield_max));
    }
    let left = labor(&a);
    if left != 5000 - kind.labor {
        return Err(format!("labor is {left} after a gather, expected {}", 5000 - kind.labor));
    }
    let n = node(&a, near).ok_or("node gone")?;
    let ahead_s = (n.ready_at.to_micros_since_unix_epoch() - game_now(&a)) as f32 / 1e6;
    if (ahead_s - kind.regrow_s as f32).abs() > 5.0 {
        return Err(format!("the node regrows in {ahead_s:.0} s, expected {}", kind.regrow_s));
    }
    println!("A gathered {got} saltreed for {} labor; the node regrows in {ahead_s:.0} s", kind.labor);
    refused(&a, "a node that hasn't regrown", "nothing to gather here yet", near)?;

    // The dev clock regrows it, and nothing else does.
    advance(&a, i64::from(kind.regrow_s) * 1_000_000)?;
    let before = reeds(&a);
    gather(&a, near)?;
    wait_done(&a, kind, near)?;
    if reeds(&a) <= before {
        return Err("the second gather yielded nothing".into());
    }
    println!("after the dev clock moved {} s, A gathered again: {} saltreed in all", kind.regrow_s, reeds(&a) - start_reeds);

    // Moving away cancels, for free.
    advance(&a, i64::from(kind.regrow_s) * 1_000_000)?;
    let (before, spent) = (reeds(&a), labor(&a));
    gather(&a, near)?;
    std::thread::sleep(Duration::from_millis(300));
    step(&a, 0.8)?;
    wait_for("the node freed by a move", || node(&a, near).is_some_and(|n| n.gatherer == 0))?;
    std::thread::sleep(Duration::from_secs_f32(kind.gather_s + 0.5));
    if reeds(&a) != before || labor(&a) != spent {
        return Err(format!("a gather cancelled by moving changed the bag ({} → {}) or labor ({spent} → {})", before, reeds(&a), labor(&a)));
    }
    println!("moving 0.8 m cancelled a gather at no cost");

    // So does cancel_gather.
    step(&a, 0.0)?;
    gather(&a, near)?;
    call(&a, |tx| a.conn.reducers.cancel_gather_then(tx))?;
    wait_for("the node freed by cancel_gather", || node(&a, near).is_some_and(|n| n.gatherer == 0))?;
    println!("cancel_gather freed the node");

    // Clean up: our nodes, A's saltreed and A's labor.
    call(&a, |tx| a.conn.reducers.admin_remove_nodes_then(vec![near, far], tx))?;
    wait_for("our nodes removed", || node(&a, near).is_none() && node(&a, far).is_none())?;
    let wire = item_by_id("saltreed").map(|i| i.wire).unwrap_or_default();
    let stacks: Vec<(u64, u32)> = a.conn.db.my_inventory().iter().filter(|i| i.holder == me.char_id && i.def == wire).map(|i| (i.id, i.count)).collect();
    for (id, count) in stacks {
        call(&a, |tx| a.conn.reducers.destroy_item_then(id, count, tx))?;
    }
    set_labor(&a, 5000)?;
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A reducer's `_then` callback.
pub(crate) type Then = Box<dyn FnOnce(&ReducerEventContext, Result<Result<(), String>, spacetimedb_sdk::__codegen::InternalError>) + Send>;

/// Call a reducer through its `_then` form and wait for its outcome.
pub(crate) fn call(_bot: &Bot, send: impl FnOnce(Then) -> spacetimedb_sdk::Result<()>) -> Check {
    outcome(|tx| send(Box::new(move |_, r| drop(tx.send(flatten(r))))))
}

pub(crate) fn subscribe(bot: &Bot, queries: &[&str]) -> Check {
    let applied = Arc::new(AtomicBool::new(false));
    let flag = applied.clone();
    let queries: Vec<String> = queries.iter().map(|q| q.to_string()).collect();
    bot.conn
        .subscription_builder()
        .on_applied(move |_| flag.store(true, Ordering::SeqCst))
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(queries);
    wait_for("subscription", || applied.load(Ordering::SeqCst))
}

/// Place a node of `kind` as an admin; returns its id once we see it.
fn place(bot: &Bot, kind: &NodeData, x: f32, y: f32, z: f32) -> Result<u64, String> {
    let before: Vec<u64> = bot.conn.db.resource_node().iter().map(|n| n.id).collect();
    let p = NodePlacement { kind: kind.wire, x, y, z, yaw: 0 };
    call(bot, |tx| bot.conn.reducers.admin_put_nodes_then(vec![p], tx))?;
    let fresh = || bot.conn.db.resource_node().iter().find(|n| !before.contains(&n.id) && n.x == x && n.z == z).map(|n| n.id);
    wait_for("the new node", || fresh().is_some())?;
    fresh().ok_or_else(|| "the new node vanished".into())
}

fn node(bot: &Bot, id: u64) -> Option<ResourceNode> {
    bot.conn.db.resource_node().id().find(&id)
}

fn gather(bot: &Bot, id: u64) -> Check {
    call(bot, |tx| bot.conn.reducers.gather_then(id, tx)).map_err(|e| format!("{}: gather {id}: {e}", bot.label))
}

fn refused(bot: &Bot, what: &str, want: &str, id: u64) -> Check {
    match call(bot, |tx| bot.conn.reducers.gather_then(id, tx)) {
        Ok(()) => Err(format!("{what} was accepted")),
        Err(e) if e.contains(want) => {
            println!("{}: refused {what}: {e}", bot.label);
            Ok(())
        }
        Err(e) => Err(format!("{what}: unexpected refusal {e:?}")),
    }
}

/// Wait out the gather's time; the server's timer frees the node when it's done.
fn wait_done(bot: &Bot, kind: &NodeData, id: u64) -> Check {
    std::thread::sleep(Duration::from_secs_f32(kind.gather_s));
    wait_for("the gather to finish", || node(bot, id).is_some_and(|n| n.gatherer == 0))
}

/// Move `dx` metres east of the spot, at the ground's height where we stand.
fn step(bot: &Bot, dx: f32) -> Check {
    let p = bot.presence().ok_or("left the world")?;
    let x = SPOT.0 + dx;
    call(bot, |tx| bot.conn.reducers.move_event_then(p.last_seq + 1, 0, x, p.y, p.z, 0, 0, 1, tx))?;
    wait_for("the move", || bot.presence().is_some_and(|q| q.x == x))
}

pub(crate) fn held(bot: &Bot, char_id: u64, item: &str) -> u32 {
    let wire = item_by_id(item).map(|i| i.wire).unwrap_or_default();
    bot.conn.db.my_inventory().iter().filter(|i| i.holder == char_id && i.def == wire).map(|i| i.count).sum()
}

pub(crate) fn game_now(bot: &Bot) -> i64 {
    let offset = bot.conn.db.game_clock().iter().next().map_or(0, |c| c.offset_micros);
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_micros() as i64) + offset
}

/// The pool as of game time now.
pub(crate) fn labor(bot: &Bot) -> i32 {
    bot.conn.db.my_labor().iter().next().map_or(-1, |l| {
        sim::labor::Pool { points: l.points, at: l.at.to_micros_since_unix_epoch() }.current(&sim::labor::LABOR, game_now(bot))
    })
}

pub(crate) fn set_labor(bot: &Bot, points: i32) -> Check {
    call(bot, |tx| bot.conn.reducers.dev_set_labor_then(points, tx))?;
    wait_for("the new pool", || bot.conn.db.my_labor().iter().next().is_some_and(|l| l.points == points))
}

fn advance(bot: &Bot, micros: i64) -> Check {
    let offset = || bot.conn.db.game_clock().iter().next().map_or(0, |c| c.offset_micros);
    let before = offset();
    call(bot, |tx| bot.conn.reducers.dev_advance_clock_then(micros, tx))?;
    wait_for("the game clock", || offset() == before + micros)
}
