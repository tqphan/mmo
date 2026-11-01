//! The `world` SpacetimeDB module: the whole game world in one database (PLAN §4, §5).

mod chat;
mod combat;
mod crafting;
mod crime;
#[cfg(feature = "dev")]
mod dev;
mod farming;
mod gathering;
mod items;
mod labor;
mod land;
mod movement;
mod npc;
mod party;
mod quests;
mod session;
mod ship;
mod shipyard;
mod terrain;
mod tick;
mod time;
mod trade;
mod vehicle;

#[cfg(test)]
mod tests;

#[spacetimedb::reducer(init)]
pub fn init(ctx: &spacetimedb::ReducerContext) {
    session::record_owner(ctx);
    tick::start(ctx);
    log::info!("world module initialised");
}
