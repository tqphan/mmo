//! The `labor` scenario (docs/design/labor.md): a fresh account starts with a full pool, and the
//! pool regenerates by whole periods with the dev clock, stops at the cap, and refuses spends it
//! can't cover. The bot also computes the pool itself from `my_labor` and `game_clock`, as the
//! client does, and must agree with the server.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sim::labor::{LABOR, Pool};

use crate::module_bindings::*;
use crate::terrain::{flatten, outcome};
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

const PERIOD_US: i64 = LABOR.regen_period_s as i64 * 1_000_000;

pub fn scenario(args: &Args) -> Check {
    let bot = Bot::connect(args, "L")?;
    let applied = Arc::new(AtomicBool::new(false));
    let flag = applied.clone();
    bot.conn
        .subscription_builder()
        .on_applied(move |_| flag.store(true, Ordering::SeqCst))
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(["SELECT * FROM my_labor", "SELECT * FROM game_clock"]);
    wait_for("subscription", || applied.load(Ordering::SeqCst))?;
    wait_for("a labor pool", || stored(&bot).is_some())?;

    let first = stored(&bot).unwrap();
    if first.points != LABOR.start_points {
        return Err(format!("a new account has {} labor, expected {}", first.points, LABOR.start_points));
    }
    println!("new account: {} / {} labor", first.points, LABOR.cap);

    // A pool set as of now is anchored where our own game clock says now is.
    set(&bot, 1000)?;
    let p = stored(&bot).unwrap();
    let skew_ms = (p.at - game_now(&bot)).abs() / 1000;
    if skew_ms > 2000 {
        return Err(format!("the server's game time is {skew_ms} ms from ours"));
    }
    println!("set to 1000; clocks agree within {skew_ms} ms");

    // Two whole periods and a bit: +2 × regen, which we must compute too.
    advance(&bot, 2 * PERIOD_US + 10_000_000)?;
    let want = 1000 + 2 * LABOR.regen_points;
    let ours = p.current(&LABOR, game_now(&bot));
    if ours != want {
        return Err(format!("computed {ours} labor after two periods, expected {want}"));
    }
    spend(&bot, 1)?;
    wait_for("the spend", || stored(&bot).is_some_and(|s| s.points == want - 1))?;
    let after = stored(&bot).unwrap();
    if after.at != p.at + 2 * PERIOD_US {
        return Err(format!("progress lost: anchor moved {} µs, expected {}", after.at - p.at, 2 * PERIOD_US));
    }
    println!("after two periods: computed {ours}, spent 1, server has {}", after.points);

    // Near the cap a period fills it, and a spend from full starts a whole new period.
    set(&bot, LABOR.cap - LABOR.regen_points / 2)?;
    advance(&bot, PERIOD_US + 10_000_000)?;
    let full = stored(&bot).unwrap();
    if full.current(&LABOR, game_now(&bot)) != LABOR.cap || full.next_regen_at(&LABOR, game_now(&bot)).is_some() {
        return Err("the pool did not fill to the cap".into());
    }
    spend(&bot, 1)?;
    wait_for("the spend from full", || stored(&bot).is_some_and(|s| s.points == LABOR.cap - 1))?;
    let s = stored(&bot).unwrap();
    if (s.at - game_now(&bot)).abs() > 2_000_000 {
        return Err("a spend from a full pool did not start a new period".into());
    }
    println!("filled to {}; a spend from full anchors at now", LABOR.cap);

    // Spends that can't be covered are refused and change nothing.
    let e = spend(&bot, LABOR.cap).err().ok_or("a spend beyond the pool was accepted")?;
    if !e.contains("not enough labor") {
        return Err(format!("unexpected refusal: {e}"));
    }
    if spend(&bot, 0).is_ok() {
        return Err("a spend of 0 was accepted".into());
    }
    if stored(&bot).unwrap() != s {
        return Err("a refused spend changed the pool".into());
    }
    println!("refused: {e}");
    bot.conn.disconnect().map_err(|e| e.to_string())
}

fn stored(bot: &Bot) -> Option<Pool> {
    bot.conn.db.my_labor().iter().next().map(|l| Pool { points: l.points, at: l.at.to_micros_since_unix_epoch() })
}

/// Our clock plus the server's dev offset, as the client computes it.
fn game_now(bot: &Bot) -> i64 {
    let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as i64;
    wall + offset(bot)
}

fn offset(bot: &Bot) -> i64 {
    bot.conn.db.game_clock().iter().next().map_or(0, |c| c.offset_micros)
}

fn set(bot: &Bot, points: i32) -> Check {
    outcome(|tx| bot.conn.reducers.dev_set_labor_then(points, move |_, r| drop(tx.send(flatten(r)))))?;
    wait_for("the new pool", || stored(bot).is_some_and(|p| p.points == points))
}

fn spend(bot: &Bot, cost: i32) -> Check {
    outcome(|tx| bot.conn.reducers.dev_spend_labor_then(cost, move |_, r| drop(tx.send(flatten(r)))))
}

fn advance(bot: &Bot, micros: i64) -> Check {
    let before = offset(bot);
    outcome(|tx| bot.conn.reducers.dev_advance_clock_then(micros, move |_, r| drop(tx.send(flatten(r)))))?;
    wait_for("the game clock", || offset(bot) == before + micros)
}
