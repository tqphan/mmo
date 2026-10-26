//! The `world` SpacetimeDB module: the whole game world in one database (PLAN §4, §5).

mod movement;
mod session;
mod time;

#[cfg(test)]
mod tests;

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &spacetimedb::ReducerContext) {
    log::info!("world module initialised");
}
