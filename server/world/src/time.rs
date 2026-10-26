//! All game time goes through `now(ctx)` (CLAUDE.md). Dev builds add a test-clock offset that
//! an admin reducer can push forward; production builds compile the offset out.

use spacetimedb::{ReducerContext, Timestamp};

#[cfg(feature = "dev")]
use spacetimedb::{Table, TimeDuration};

/// Single-row, private: how far the dev clock runs ahead of real time.
#[cfg(feature = "dev")]
#[spacetimedb::table(accessor = dev_clock)]
pub struct DevClock {
    #[primary_key]
    pub id: u8,
    pub offset_micros: i64,
}

#[cfg(feature = "dev")]
pub fn now(ctx: &ReducerContext) -> Timestamp {
    let offset = ctx.db.dev_clock().id().find(0).map_or(0, |c| c.offset_micros);
    ctx.timestamp + TimeDuration::from_micros(offset)
}

#[cfg(not(feature = "dev"))]
pub fn now(ctx: &ReducerContext) -> Timestamp {
    ctx.timestamp
}

/// Dev builds only: move game time forward. Any caller may use it; dev builds never run in
/// production (ADR 0006).
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_advance_clock(ctx: &ReducerContext, micros: i64) -> Result<(), String> {
    if micros < 0 {
        return Err("the dev clock only moves forward".into());
    }
    let offset = ctx.db.dev_clock().id().find(0).map_or(0, |c| c.offset_micros);
    let offset_micros = offset.checked_add(micros).ok_or("dev clock offset overflow")?;
    ctx.db.dev_clock().id().delete(0);
    ctx.db.dev_clock().insert(DevClock { id: 0, offset_micros });
    log::info!("dev clock now {offset_micros} µs ahead");
    Ok(())
}

/// Seconds from `earlier` to `later`, negative if `later` is before `earlier`.
pub fn secs_between(earlier: Timestamp, later: Timestamp) -> f32 {
    let micros = later.to_micros_since_unix_epoch() - earlier.to_micros_since_unix_epoch();
    micros as f32 / 1_000_000.0
}
