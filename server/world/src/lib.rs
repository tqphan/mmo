//! The `world` SpacetimeDB module: the whole game world in one database (PLAN §4, §5).

mod combat;
mod crafting;
#[cfg(feature = "dev")]
mod dev;
mod farming;
mod gathering;
mod items;
mod labor;
mod land;
mod movement;
mod session;
mod ship;
mod terrain;
mod tick;
mod time;
mod trade;

#[cfg(test)]
mod tests;

#[spacetimedb::reducer(init)]
pub fn init(ctx: &spacetimedb::ReducerContext) {
    session::record_owner(ctx);
    tick::start(ctx);
    log::info!("world module initialised");
}
