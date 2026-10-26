//! The `world` SpacetimeDB module: the whole game world in one database (PLAN §4, §5).

mod movement;
mod session;
mod tick;
mod time;

#[cfg(test)]
mod tests;

#[spacetimedb::reducer(init)]
pub fn init(ctx: &spacetimedb::ReducerContext) {
    tick::start(ctx);
    log::info!("world module initialised");
}
