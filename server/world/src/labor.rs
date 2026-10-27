//! Labor (docs/design/labor.md): one pool per account, computed from a timestamp by
//! `sim::labor` and written only when it's spent.

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext};
use sim::labor::{LABOR, Pool};

use crate::time::now;

/// Private: an account's pool, `points` as of `at`. The owner reads it through `my_labor`.
#[spacetimedb::table(accessor = labor)]
pub struct Labor {
    #[primary_key]
    pub identity: Identity,
    pub points: i32,
    pub at: Timestamp,
}

impl Labor {
    fn pool(&self) -> Pool {
        Pool { points: self.points, at: self.at.to_micros_since_unix_epoch() }
    }

    fn with(identity: Identity, p: Pool) -> Self {
        Self { identity, points: p.points, at: Timestamp::from_micros_since_unix_epoch(p.at) }
    }
}

#[spacetimedb::view(accessor = my_labor, public)]
pub(crate) fn my_labor(ctx: &ViewContext) -> Option<Labor> {
    ctx.db.labor().identity().find(ctx.sender())
}

/// Give `who` a new pool unless it has one. Called on connect, so accounts made before labor
/// existed get theirs too.
pub fn open(ctx: &ReducerContext, who: Identity) {
    if ctx.db.labor().identity().find(who).is_none() {
        let p = Pool::new(&LABOR, now(ctx).to_micros_since_unix_epoch());
        ctx.db.labor().insert(Labor::with(who, p));
    }
}

/// What `who`'s pool holds as of now, without writing it.
pub fn available(ctx: &ReducerContext, who: Identity) -> Result<i32, String> {
    let row = ctx.db.labor().identity().find(who).ok_or("no labor pool")?;
    Ok(row.pool().current(&LABOR, now(ctx).to_micros_since_unix_epoch()))
}

/// Take `cost` off `who`'s pool as of now, for work done in the same transaction. Returns what's
/// left; on `Err` nothing changes.
pub fn spend(ctx: &ReducerContext, who: Identity, cost: i32) -> Result<i32, String> {
    let row = ctx.db.labor().identity().find(who).ok_or("no labor pool")?;
    let t = now(ctx).to_micros_since_unix_epoch();
    let p = row.pool().spend(&LABOR, t, cost).map_err(|s| {
        if cost <= 0 { format!("labor cost must be above 0, not {cost}") } else { format!("not enough labor: have {}, need {}", s.have, s.need) }
    })?;
    ctx.db.labor().identity().update(Labor::with(who, p));
    Ok(p.points)
}

/// Dev builds only: spend labor as work would. Bots and tests use it until there is work to do.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_spend_labor(ctx: &ReducerContext, cost: i32) -> Result<(), String> {
    spend(ctx, ctx.sender(), cost).map(|_| ())
}

/// Dev builds only: set the caller's pool to `points` (0 to the cap) as of now.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_set_labor(ctx: &ReducerContext, points: i32) -> Result<(), String> {
    if !(0..=LABOR.cap).contains(&points) {
        return Err(format!("labor is from 0 to {}", LABOR.cap));
    }
    ctx.db.labor().identity().find(ctx.sender()).ok_or("no labor pool")?;
    let p = Pool { points, at: now(ctx).to_micros_since_unix_epoch() };
    ctx.db.labor().identity().update(Labor::with(ctx.sender(), p));
    Ok(())
}
