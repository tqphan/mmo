//! Land (docs/design/land.md).

use super::*;
use crate::items::{grant, item, place};
use crate::land::{
    LandChunk, Plot, PlotDemolition, abandon_plot, access, admin_clear_land, admin_put_land, admin_remove_land, admin_remove_plots,
    demolish_plot, land_chunk, pay_tax, place_plot, plot, plot_demolition, set_plot_access,
};
use crate::terrain::{TerrainChunk, admin_put_terrain, terrain_info};
use sim::cell::{CELL_SIZE_M, OPEN_WORLD, cell_of};
use sim::content::{BAG_SLOTS, item_by_id};
use sim::land::{LAND_CHUNK_LEN, PROPERTY, Standing, demolish_at, land_chunk_hash, week_micros};
use sim::terrain::{CHUNK_LEN, encode};

const ADMIN: u128 = 99;
const HOUSING: u8 = 1;
const TOWN: u8 = 2;

pub(super) fn wire(id: &str) -> u16 {
    item_by_id(id).unwrap().wire
}

pub(super) fn as_admin(t: &TestContext) {
    call(t, ADMIN, |ctx| {
        on_connect(ctx);
        crate::session::record_owner(ctx);
        Ok(())
    })
    .unwrap();
}

/// Flat ground 10 m up over the 3 × 3 cells round spawn, all housing except a town texel at (20, 20).
pub(super) fn ground(t: &TestContext) {
    as_admin(t);
    let mut chunks = Vec::new();
    let mut land = Vec::new();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let cell = cell_of(OPEN_WORLD, dx as f32 * CELL_SIZE_M + 1.0, dz as f32 * CELL_SIZE_M + 1.0);
            chunks.push(TerrainChunk { cell, samples: vec![encode(10.0); CHUNK_LEN] });
            let mut texels = vec![HOUSING; LAND_CHUNK_LEN];
            if (dx, dz) == (0, 0) {
                texels[5 * 16 + 5] = TOWN;
            }
            land.push(LandChunk { cell, texels });
        }
    }
    call(t, ADMIN, |ctx| admin_put_terrain(ctx, chunks)).unwrap();
    call(t, ADMIN, |ctx| admin_put_land(ctx, land)).unwrap();
}

pub(super) fn give(t: &TestContext, char_id: u64, id: &str, count: u32) {
    read(t, |ctx| grant(ctx, char_id, wire(id), count)).unwrap();
}

pub(super) fn held(t: &TestContext, char_id: u64, id: &str) -> u32 {
    let w = wire(id);
    read(t, |ctx| ctx.db.item().by_slot().filter((char_id, place::BAG)).filter(|i| i.def == w).map(|i| i.count).sum())
}

/// The id of a stack of `id` in the bag.
pub(super) fn stack(t: &TestContext, char_id: u64, id: &str) -> u64 {
    let w = wire(id);
    read(t, |ctx| ctx.db.item().by_slot().filter((char_id, place::BAG)).find(|i| i.def == w).unwrap().id)
}

pub(super) fn plots(t: &TestContext) -> Vec<Plot> {
    read(t, |ctx| ctx.db.plot().iter().collect())
}

/// Arrive with a garden deed, a farm deed and 100 seals.
pub(super) fn settler(t: &TestContext, player: u128, name: &str) -> Presence {
    let p = arrive(t, player, name);
    give(t, p.char_id, "garden_deed", 3);
    give(t, p.char_id, "farm_deed", 1);
    give(t, p.char_id, "levy_seal", 100);
    p
}

pub(super) fn place(t: &TestContext, player: u128, deed: &str, x: f32, z: f32) -> Result<(), String> {
    let char_id = presence_of(t, player).char_id;
    let item_id = stack(t, char_id, deed);
    call(t, player, |ctx| place_plot(ctx, item_id, x, z, 1))
}

pub(super) fn set_position(t: &TestContext, player: u128, x: f32, z: f32) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        ctx.db.presence().identity().update(Presence { x, z, ..p });
    });
}

pub(super) fn advance_us(t: &TestContext, us: i64) {
    t.clock.advance(TimeDuration::from_micros(us));
}

pub(super) fn now_us(t: &TestContext) -> i64 {
    read(t, |ctx| crate::time::now(ctx).to_micros_since_unix_epoch())
}

fn job_of(t: &TestContext, plot_id: u64) -> PlotDemolition {
    read(t, |ctx| ctx.db.plot_demolition().plot().find(plot_id).unwrap())
}

/// Run a plot's demolition job as the scheduler would.
pub(super) fn run_job(t: &TestContext, plot_id: u64) {
    let job = job_of(t, plot_id);
    let id = job.scheduled_id;
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| demolish_plot(ctx, job)).unwrap();
    read(t, |ctx| ctx.db.plot_demolition().scheduled_id().delete(id));
}

fn due_in_us(t: &TestContext, plot_id: u64) -> i64 {
    let at = match job_of(t, plot_id).scheduled_at {
        spacetimedb::ScheduleAt::Time(at) => at.to_micros_since_unix_epoch(),
        other => panic!("{other:?}"),
    };
    at - read(t, |ctx| ctx.timestamp.to_micros_since_unix_epoch())
}

#[test]
fn placing_takes_the_deed_and_a_week_of_tax() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 2.9, -1.1).unwrap();
    let ps = plots(&t);
    assert_eq!(ps.len(), 1);
    let p = &ps[0];
    assert_eq!((p.x, p.z, p.y, p.yaw, p.owner, p.access), (2.0, -2.0, 10.0, 1, me.char_id, access::PRIVATE));
    assert_eq!(p.tax_paid_until.to_micros_since_unix_epoch() - p.placed_at.to_micros_since_unix_epoch(), week_micros());
    assert_eq!(held(&t, me.char_id, "garden_deed"), 2);
    assert_eq!(held(&t, me.char_id, "levy_seal"), 95);
    assert_eq!(due_in_us(&t, p.id), demolish_at(p.tax_paid_until.to_micros_since_unix_epoch()) - now_us(&t));
    // A farm beside it, sharing an edge.
    place(&t, 1, "farm_deed", 2.0 + 12.0, -2.0).unwrap();
    assert_eq!(plots(&t).len(), 2);
}

#[test]
fn refusals_change_nothing() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    let other = settler(&t, 2, "Tobin");
    let before = (held(&t, me.char_id, "garden_deed"), held(&t, me.char_id, "levy_seal"));
    let err = |r: Result<(), String>| r.unwrap_err();
    assert_eq!(err(place(&t, 1, "levy_seal", 0.0, 0.0)), "that isn't a deed");
    let theirs = stack(&t, other.char_id, "garden_deed");
    assert_eq!(err(call(&t, 1, |ctx| place_plot(ctx, theirs, 0.0, 0.0, 0))), "no such item");
    assert_eq!(err(call(&t, 1, |ctx| place_plot(ctx, theirs + 1000, 0.0, 0.0, 0))), "no such item");
    assert_eq!(err(place(&t, 1, "garden_deed", f32::NAN, 0.0)), "non-finite position");
    assert_eq!(err(place(&t, 1, "garden_deed", 4.0 + PROPERTY.reach_m + 2.0, 0.0)), "too far from the plot");
    set_position(&t, 1, 20.0, 20.0);
    assert_eq!(err(place(&t, 1, "garden_deed", 20.0, 20.0)), "plots can only go on housing land");
    as_admin(&t);
    let bare: Vec<LandChunk> =
        [-4.0, 4.0].iter().map(|&z| LandChunk { cell: cell_of(OPEN_WORLD, 160.0, z), texels: vec![HOUSING; LAND_CHUNK_LEN] }).collect();
    call(&t, ADMIN, |ctx| admin_put_land(ctx, bare)).unwrap();
    set_position(&t, 1, 160.0, 0.0);
    assert_eq!(err(place(&t, 1, "garden_deed", 160.0, 0.0)), "no ground here");
    set_position(&t, 1, 0.0, 0.0);
    place(&t, 2, "farm_deed", 0.0, 0.0).unwrap();
    assert_eq!(err(place(&t, 1, "garden_deed", 4.0, 4.0)), "the land is taken");
    assert_eq!(plots(&t).len(), 1);
    assert_eq!((held(&t, me.char_id, "garden_deed"), held(&t, me.char_id, "levy_seal")), before);
    // Without seals.
    let poor = arrive(&t, 3, "Cass");
    give(&t, poor.char_id, "garden_deed", 1);
    give(&t, poor.char_id, "levy_seal", 4);
    let err3 = place(&t, 3, "garden_deed", -12.0, 0.0).unwrap_err();
    assert!(err3.contains("not enough Levy Seal"), "{err3}");
    assert_eq!(held(&t, poor.char_id, "garden_deed"), 1);
}

#[test]
fn the_ground_must_be_flat_and_dry() {
    let t = world();
    ground(&t);
    settler(&t, 1, "Mira");
    as_admin(&t);
    let cell = cell_of(OPEN_WORLD, 1.0, 1.0);
    let mut samples = vec![encode(10.0); CHUNK_LEN];
    samples[2 * 33 + 2] = encode(13.0); // (4, 4)
    samples[2 * 33 + 6] = encode(0.2); // (12, 4)
    call(&t, ADMIN, |ctx| admin_put_terrain(ctx, vec![TerrainChunk { cell, samples }])).unwrap();
    assert_eq!(place(&t, 1, "garden_deed", 2.0, 2.0).unwrap_err(), "the ground is too steep");
    assert_eq!(place(&t, 1, "garden_deed", 10.0, 2.0).unwrap_err(), "too close to the water");
    assert_eq!(place(&t, 1, "garden_deed", -6.0, -6.0), Ok(()));
}

#[test]
fn the_land_rush_goes_to_whoever_calls_first() {
    let t = world();
    ground(&t);
    settler(&t, 1, "Mira");
    let b = settler(&t, 2, "Tobin");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    assert_eq!(place(&t, 2, "garden_deed", 1.0, 1.0).unwrap_err(), "the land is taken");
    assert_eq!(held(&t, b.char_id, "levy_seal"), 100);
    let ps = plots(&t);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].owner, presence_of(&t, 1).char_id);
}

#[test]
fn tax_rises_with_plots_owned_and_has_a_limit() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    give(&t, me.char_id, "garden_deed", 5);
    let mut seals = held(&t, me.char_id, "levy_seal");
    for (n, x) in [-24.0, -12.0, 0.0, 12.0, 24.0].into_iter().enumerate() {
        set_position(&t, 1, x, 0.0);
        place(&t, 1, "garden_deed", x, 0.0).unwrap();
        let tax = sim::land::weekly_tax(&sim::land::PLOTS[0], n + 1).unwrap();
        assert_eq!(held(&t, me.char_id, "levy_seal"), seals - tax, "plot {}", n + 1);
        seals -= tax;
    }
    set_position(&t, 1, 36.0, 0.0);
    let e = place(&t, 1, "garden_deed", 36.0, 0.0).unwrap_err();
    assert!(e.contains("most plots allowed (5)"), "{e}");
    // Paying one now costs the five-plot rate.
    let id = plots(&t).iter().find(|p| p.x == 24.0).unwrap().id;
    set_position(&t, 1, 24.0, 0.0);
    call(&t, 1, |ctx| pay_tax(ctx, id)).unwrap();
    assert_eq!(held(&t, me.char_id, "levy_seal"), seals - sim::land::weekly_tax(&sim::land::PLOTS[0], 5).unwrap());
}

#[test]
fn paying_adds_a_week_up_to_the_limit() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let p = plots(&t)[0].clone();
    let paid = |t: &TestContext| plots(t)[0].tax_paid_until.to_micros_since_unix_epoch();
    let start = paid(&t);
    for k in 1..PROPERTY.prepay_weeks {
        call(&t, 1, |ctx| pay_tax(ctx, p.id)).unwrap();
        assert_eq!(paid(&t), start + i64::from(k) * week_micros());
    }
    let e = call(&t, 1, |ctx| pay_tax(ctx, p.id)).unwrap_err();
    assert!(e.contains("at most 4 weeks ahead"), "{e}");
    assert_eq!(held(&t, me.char_id, "levy_seal"), 100 - 5 * PROPERTY.prepay_weeks);
    let job = due_in_us(&t, p.id);
    assert_eq!(job, demolish_at(paid(&t)) - now_us(&t), "rescheduled");
    // Others can't pay it, and the owner must stand near it.
    settler(&t, 2, "Tobin");
    assert_eq!(call(&t, 2, |ctx| pay_tax(ctx, p.id)).unwrap_err(), "that isn't your plot");
    set_position(&t, 1, 30.0, 0.0);
    advance_us(&t, week_micros());
    assert_eq!(call(&t, 1, |ctx| pay_tax(ctx, p.id)).unwrap_err(), "too far from the plot");
}

#[test]
fn an_unpaid_plot_goes_overdue_then_comes_down_and_the_deed_returns() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let p = plots(&t)[0].clone();
    let paid = p.tax_paid_until.to_micros_since_unix_epoch();
    let standing = |t: &TestContext| {
        let p = plots(t)[0].clone();
        read(t, |ctx| crate::land::standing(ctx, &p))
    };
    assert_eq!(standing(&t), Standing::Paid);
    advance_us(&t, paid - now_us(&t));
    assert_eq!(standing(&t), Standing::Overdue);
    // A job that fires early (the host's clock ran ahead) only reschedules.
    run_job(&t, p.id);
    assert_eq!(plots(&t).len(), 1);
    advance_us(&t, demolish_at(paid) - now_us(&t));
    assert_eq!(standing(&t), Standing::Demolished);
    run_job(&t, p.id);
    assert!(plots(&t).is_empty());
    assert_eq!(read(&t, |ctx| ctx.db.plot_demolition().count()), 0);
    assert_eq!(held(&t, me.char_id, "garden_deed"), 3);
}

#[test]
fn paying_while_overdue_pays_the_arrears_and_saves_the_plot() {
    let t = world();
    ground(&t);
    settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let p = plots(&t)[0].clone();
    let paid = p.tax_paid_until.to_micros_since_unix_epoch();
    advance_us(&t, demolish_at(paid) - now_us(&t) - 1);
    call(&t, 1, |ctx| pay_tax(ctx, p.id)).unwrap();
    assert_eq!(plots(&t)[0].tax_paid_until.to_micros_since_unix_epoch(), paid + week_micros());
    advance_us(&t, 1);
    run_job(&t, p.id);
    assert_eq!(plots(&t).len(), 1, "saved");
    assert_eq!(due_in_us(&t, p.id), demolish_at(paid + week_micros()) - now_us(&t));
}

#[test]
fn a_demolished_plot_is_free_land_before_its_job_runs() {
    let t = world();
    ground(&t);
    let a = settler(&t, 1, "Mira");
    let b = settler(&t, 2, "Tobin");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let paid = plots(&t)[0].tax_paid_until.to_micros_since_unix_epoch();
    advance_us(&t, demolish_at(paid) - now_us(&t));
    place(&t, 2, "garden_deed", 2.0, 0.0).unwrap();
    let ps = plots(&t);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].owner, b.char_id);
    assert_eq!(held(&t, a.char_id, "garden_deed"), 3, "the old owner's deed came back");
    assert_eq!(read(&t, |ctx| ctx.db.plot_demolition().count()), 1);
}

#[test]
fn a_deed_that_does_not_fit_is_held_for_mail() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    give(&t, me.char_id, "garden_deed", 7); // the stack of 10 is full
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    give(&t, me.char_id, "garden_deed", 1);
    let free = BAG_SLOTS as u32 - read(&t, |ctx| ctx.db.item().by_slot().filter((me.char_id, place::BAG)).count() as u32);
    give(&t, me.char_id, "saltreed", free * 1000);
    let p = plots(&t)[0].clone();
    advance_us(&t, demolish_at(p.tax_paid_until.to_micros_since_unix_epoch()) - now_us(&t));
    run_job(&t, p.id);
    assert!(plots(&t).is_empty());
    let held_back: Vec<(u16, u32)> =
        read(&t, |ctx| ctx.db.item().by_slot().filter((me.char_id, place::RETURNS)).map(|i| (i.def, i.count)).collect());
    assert_eq!(held_back, [(wire("garden_deed"), 1)]);
    // Nor can a full bag abandon a plot.
    read(&t, |ctx| ctx.db.item().by_slot().filter((me.char_id, place::BAG)).filter(|i| i.def == wire("saltreed")).take(1).for_each(|i| {
        ctx.db.item().id().delete(i.id);
    }));
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    give(&t, me.char_id, "garden_deed", 1);
    give(&t, me.char_id, "saltreed", 1000);
    let id = plots(&t)[0].id;
    let e = call(&t, 1, |ctx| abandon_plot(ctx, id)).unwrap_err();
    assert!(e.contains("bag is full"), "{e}");
}

#[test]
fn owners_set_access_and_abandon() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    settler(&t, 2, "Tobin");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let id = plots(&t)[0].id;
    call(&t, 1, |ctx| set_plot_access(ctx, id, access::PUBLIC)).unwrap();
    assert_eq!(plots(&t)[0].access, access::PUBLIC);
    assert!(call(&t, 1, |ctx| set_plot_access(ctx, id, 7)).is_err());
    assert_eq!(call(&t, 2, |ctx| set_plot_access(ctx, id, access::PRIVATE)).unwrap_err(), "that isn't your plot");
    assert_eq!(call(&t, 2, |ctx| abandon_plot(ctx, id)).unwrap_err(), "that isn't your plot");
    call(&t, 1, |ctx| abandon_plot(ctx, id)).unwrap();
    assert!(plots(&t).is_empty());
    assert_eq!(read(&t, |ctx| ctx.db.plot_demolition().count()), 0);
    assert_eq!(held(&t, me.char_id, "garden_deed"), 3);
    assert_eq!(held(&t, me.char_id, "levy_seal"), 95, "no refund");
}

#[test]
fn another_character_on_the_account_counts_as_the_owner() {
    let t = world();
    ground(&t);
    let first = settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    let second = arrive(&t, 1, "Brin");
    assert_ne!(second.char_id, first.char_id);
    give(&t, second.char_id, "levy_seal", 5);
    let id = plots(&t)[0].id;
    call(&t, 1, |ctx| pay_tax(ctx, id)).unwrap();
    assert_eq!(held(&t, second.char_id, "levy_seal"), 0);
}

#[test]
fn only_the_schedule_demolishes() {
    let t = world();
    ground(&t);
    settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let job = job_of(&t, plots(&t)[0].id);
    assert!(call(&t, 1, |ctx| demolish_plot(ctx, job)).is_err());
    assert_eq!(plots(&t).len(), 1);
}

#[test]
fn land_uploads_keep_the_hash() {
    let t = world();
    as_admin(&t);
    let a = LandChunk { cell: cell_of(OPEN_WORLD, 1.0, 1.0), texels: vec![HOUSING; LAND_CHUNK_LEN] };
    let b = LandChunk { cell: cell_of(OPEN_WORLD, 100.0, 1.0), texels: vec![TOWN; LAND_CHUNK_LEN] };
    let sum = |cs: &[&LandChunk]| cs.iter().fold(0u64, |h, c| h.wrapping_add(land_chunk_hash(c.cell, &c.texels)));
    call(&t, ADMIN, |ctx| admin_put_land(ctx, vec![LandChunk { cell: a.cell, texels: a.texels.clone() }])).unwrap();
    call(&t, ADMIN, |ctx| admin_put_land(ctx, vec![LandChunk { cell: b.cell, texels: b.texels.clone() }])).unwrap();
    let info = read(&t, terrain_info);
    assert_eq!((info.land_chunks, info.land_hash), (2, sum(&[&a, &b])));
    let a2 = LandChunk { cell: a.cell, texels: vec![0; LAND_CHUNK_LEN] };
    call(&t, ADMIN, |ctx| admin_put_land(ctx, vec![LandChunk { cell: a2.cell, texels: a2.texels.clone() }])).unwrap();
    let info = read(&t, terrain_info);
    assert_eq!((info.land_chunks, info.land_hash), (2, sum(&[&a2, &b])));
    call(&t, ADMIN, |ctx| admin_remove_land(ctx, vec![b.cell, 12345])).unwrap();
    let info = read(&t, terrain_info);
    assert_eq!((info.land_chunks, info.land_hash), (1, sum(&[&a2])));
    // The row rebuilt from the chunks agrees.
    read(&t, |ctx| ctx.db.terrain_info().id().delete(0));
    let info = read(&t, terrain_info);
    assert_eq!((info.land_chunks, info.land_hash), (1, sum(&[&a2])));
    call(&t, ADMIN, admin_clear_land).unwrap();
    let info = read(&t, terrain_info);
    assert_eq!((info.land_chunks, info.land_hash), (0, 0));
    assert_eq!(read(&t, |ctx| ctx.db.land_chunk().count()), 0);
    // Bad chunks and non-admins are refused.
    assert!(call(&t, ADMIN, |ctx| admin_put_land(ctx, vec![LandChunk { cell: a.cell, texels: vec![1; 10] }])).is_err());
    assert!(call(&t, ADMIN, |ctx| admin_put_land(ctx, vec![LandChunk { cell: a.cell, texels: vec![0x81; LAND_CHUNK_LEN] }])).is_err());
    assert!(call(&t, 1, |ctx| admin_put_land(ctx, vec![LandChunk { cell: a.cell, texels: a.texels.clone() }])).is_err());
    assert!(call(&t, 1, admin_clear_land).is_err());
}

#[test]
fn admins_remove_plots_and_the_deed_returns() {
    let t = world();
    ground(&t);
    let me = settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let id = plots(&t)[0].id;
    assert!(call(&t, 1, |ctx| admin_remove_plots(ctx, vec![id])).is_err());
    call(&t, ADMIN, |ctx| admin_remove_plots(ctx, vec![id, 999])).unwrap();
    assert!(plots(&t).is_empty());
    assert_eq!(held(&t, me.char_id, "garden_deed"), 3);
}

#[cfg(feature = "dev")]
#[test]
fn the_dev_clock_reschedules_demolitions() {
    use crate::time::dev_advance_clock;
    let t = world();
    ground(&t);
    settler(&t, 1, "Mira");
    place(&t, 1, "garden_deed", 0.0, 0.0).unwrap();
    let id = plots(&t)[0].id;
    let before = due_in_us(&t, id);
    call(&t, 1, |ctx| dev_advance_clock(ctx, week_micros())).unwrap();
    assert_eq!(due_in_us(&t, id), before - week_micros());
    call(&t, 1, |ctx| dev_advance_clock(ctx, 2 * week_micros())).unwrap();
    assert_eq!(due_in_us(&t, id), 0, "overdue jobs run at once");
}
