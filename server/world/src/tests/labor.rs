//! Labor pools (docs/design/labor.md).

use super::*;
use crate::labor::{Labor, labor, my_labor, spend};
use sim::labor::LABOR;

const PERIOD_MS: i64 = LABOR.regen_period_s as i64 * 1000;

fn connect(t: &TestContext, player: u128) {
    call(t, player, |ctx| {
        on_connect(ctx);
        Ok(())
    })
    .unwrap();
}

fn pool_of(t: &TestContext, player: u128) -> Labor {
    read(t, |ctx| ctx.db.labor().identity().find(identity(player)).unwrap())
}

fn spend_as(t: &TestContext, player: u128, cost: i32) -> Result<i32, String> {
    call(t, player, |ctx| spend(ctx, identity(player), cost))
}

#[test]
fn a_new_account_gets_one_full_pool() {
    let t = world();
    connect(&t, 1);
    connect(&t, 1);
    assert_eq!(read(&t, |ctx| ctx.db.labor().count()), 1);
    let p = pool_of(&t, 1);
    assert_eq!((p.points, p.at.to_micros_since_unix_epoch()), (LABOR.start_points, T0));
}

#[test]
fn an_account_from_before_labor_gets_a_pool_on_connect() {
    let t = world();
    connect(&t, 1);
    read(&t, |ctx| ctx.db.labor().identity().delete(identity(1)));
    assert!(spend_as(&t, 1, 1).is_err());
    connect(&t, 1);
    assert_eq!(pool_of(&t, 1).points, LABOR.start_points);
}

#[test]
fn every_character_on_an_account_shares_the_pool() {
    let t = world();
    arrive(&t, 1, "Mira");
    call(&t, 1, |ctx| create_character(ctx, "Tobin".into())).unwrap();
    spend_as(&t, 1, 40).unwrap();
    let chars = read(&t, |ctx| ctx.db.character_private().owner().filter(identity(1)).count());
    assert_eq!((chars, read(&t, |ctx| ctx.db.labor().count())), (2, 1));
    assert_eq!(pool_of(&t, 1).points, LABOR.start_points - 40);
}

#[test]
fn spending_settles_first_and_keeps_progress() {
    let t = world();
    connect(&t, 1);
    // From a full pool the next points come one whole period after the spend.
    advance_ms(&t, 1000);
    assert_eq!(spend_as(&t, 1, 100), Ok(LABOR.cap - 100));
    let spent_at = T0 + 1_000_000;
    assert_eq!(pool_of(&t, 1).at.to_micros_since_unix_epoch(), spent_at);
    // Two and a half periods later: two lots of regen, and half a period of progress kept.
    advance_ms(&t, PERIOD_MS * 5 / 2);
    assert_eq!(spend_as(&t, 1, 1), Ok(LABOR.cap - 100 + 2 * LABOR.regen_points - 1));
    let p = pool_of(&t, 1);
    assert_eq!(p.at.to_micros_since_unix_epoch(), spent_at + 2 * PERIOD_MS * 1000);
    advance_ms(&t, PERIOD_MS / 2);
    assert_eq!(spend_as(&t, 1, 1), Ok(p.points + LABOR.regen_points - 1));
}

#[test]
fn regen_stops_at_the_cap() {
    let t = world();
    connect(&t, 1);
    spend_as(&t, 1, 30).unwrap();
    advance_ms(&t, PERIOD_MS * 1000);
    assert_eq!(spend_as(&t, 1, 1), Ok(LABOR.cap - 1));
}

#[test]
fn a_refused_spend_changes_nothing() {
    let t = world();
    connect(&t, 1);
    let before = pool_of(&t, 1);
    advance_ms(&t, 5000);
    let e = spend_as(&t, 1, LABOR.cap + 1).unwrap_err();
    assert_eq!(e, format!("not enough labor: have {}, need {}", LABOR.cap, LABOR.cap + 1));
    assert!(spend_as(&t, 1, 0).unwrap_err().contains("above 0"));
    assert!(spend_as(&t, 1, -3).unwrap_err().contains("above 0"));
    let after = pool_of(&t, 1);
    assert_eq!((after.points, after.at), (before.points, before.at));
}

#[test]
fn the_view_shows_only_your_own_pool() {
    let t = world();
    connect(&t, 1);
    connect(&t, 2);
    spend_as(&t, 2, 7).unwrap();
    assert_eq!(my_labor(&t.view_context(auth(1))).unwrap().points, LABOR.start_points);
    assert_eq!(my_labor(&t.view_context(auth(2))).unwrap().points, LABOR.start_points - 7);
    assert!(my_labor(&t.view_context(auth(3))).is_none());
}

#[cfg(feature = "dev")]
#[test]
fn the_dev_clock_drives_regen() {
    use crate::time::dev_advance_clock;
    let t = world();
    connect(&t, 1);
    spend_as(&t, 1, 500).unwrap();
    call(&t, 1, |ctx| dev_advance_clock(ctx, 3 * PERIOD_MS * 1000)).unwrap();
    assert_eq!(spend_as(&t, 1, 1), Ok(LABOR.cap - 500 + 3 * LABOR.regen_points - 1));
}

#[cfg(feature = "dev")]
#[test]
fn dev_reducers_set_and_spend() {
    use crate::labor::{dev_set_labor, dev_spend_labor};
    let t = world();
    connect(&t, 1);
    advance_ms(&t, 2000);
    call(&t, 1, |ctx| dev_set_labor(ctx, 1000)).unwrap();
    let p = pool_of(&t, 1);
    assert_eq!((p.points, p.at.to_micros_since_unix_epoch()), (1000, T0 + 2_000_000));
    assert!(call(&t, 1, |ctx| dev_set_labor(ctx, -1)).is_err());
    assert!(call(&t, 1, |ctx| dev_set_labor(ctx, LABOR.cap + 1)).is_err());
    call(&t, 1, |ctx| dev_spend_labor(ctx, 1000)).unwrap();
    assert_eq!(pool_of(&t, 1).points, 0);
    assert!(call(&t, 1, |ctx| dev_spend_labor(ctx, 1)).unwrap_err().contains("not enough labor"));
    assert!(call(&t, 9, |ctx| dev_set_labor(ctx, 5)).unwrap_err().contains("no labor pool"));
}
