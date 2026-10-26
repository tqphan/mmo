//! The 10 Hz world tick (PLAN §4.4, docs/design/tick.md). It steps whatever moves continuously
//! (vehicles, NPCs near players, due buffs) by a measured `dt`. Nothing that can be computed from
//! timestamps belongs here.
//!
//! Timing metrics come from the host, not from this module: reducers have no clock to time
//! themselves with (ADR 0010).

use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp};

use crate::time::now;

#[spacetimedb::table(accessor = world_tick_timer, scheduled(world_tick))]
pub struct WorldTickTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

/// Single-row, private: the tick's bookkeeping.
#[spacetimedb::table(accessor = tick_state)]
pub struct TickState {
    #[primary_key]
    pub id: u8,
    /// Ticks run since the database was created.
    pub ticks: u64,
    /// Game time of the last tick.
    pub last_at: Timestamp,
    /// How far the last tick stepped the world, in seconds.
    pub last_dt: f32,
}

/// Start the tick unless it is already running. Called from `init`, and from `on_connect` so a
/// database published before the tick existed picks it up when the first client connects.
pub fn start(ctx: &ReducerContext) {
    if ctx.db.world_tick_timer().count() == 0 {
        let every = TimeDuration::from_micros(sim::tick::TICK_MICROS);
        ctx.db.world_tick_timer().insert(WorldTickTimer { scheduled_id: 0, scheduled_at: every.into() });
    }
}

#[spacetimedb::reducer]
pub fn world_tick(ctx: &ReducerContext, _timer: WorldTickTimer) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("world_tick runs on the server's schedule only".into());
    }
    let at = now(ctx);
    let last = ctx.db.tick_state().id().find(0);
    // The first tick has nothing to measure from, so it steps nothing.
    let dt = last.as_ref().map_or(0.0, |s| {
        sim::tick::step_dt(at.to_micros_since_unix_epoch() - s.last_at.to_micros_since_unix_epoch())
    });

    // Systems that step by `dt` go here as they arrive (ships in M0 spike 3).

    let ticks = last.map_or(0, |s| s.ticks) + 1;
    ctx.db.tick_state().id().delete(0);
    ctx.db.tick_state().insert(TickState { id: 0, ticks, last_at: at, last_dt: dt });
    Ok(())
}

/// Dev builds only: the tick's bookkeeping, so bots can check it is running at 10 Hz.
#[cfg(feature = "dev")]
#[spacetimedb::view(accessor = dev_tick, public)]
pub(crate) fn dev_tick(ctx: &spacetimedb::ViewContext) -> Option<TickState> {
    ctx.db.tick_state().id().find(0)
}
