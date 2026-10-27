//! Accounts, characters, entering and leaving the world (PLAN §5.3, docs/design/movement.md).

use spacetimedb::{ConnectionId, Identity, ReducerContext, Table, Timestamp, ViewContext};

use crate::movement::moved;
use crate::terrain::{Terrain, terrain_info};
use crate::time::now;

pub const MAX_CHARACTERS_PER_ACCOUNT: usize = 4;
pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 16;

/// Where new arrivals appear until the world has real spawn points.
pub const SPAWN: [f32; 3] = [0.0, 0.0, 0.0];

pub mod entity_kind {
    pub const CHARACTER: u8 = 1;
    /// `ref_id` is the hull's wire id (docs/design/ships.md).
    #[cfg_attr(not(feature = "dev"), allow(dead_code))] // only `dev_spawn_ship` makes ships until shipyards (M1)
    pub const SHIP: u8 = 2;
}

/// `account.gm_level` values.
pub mod gm {
    /// May change the world itself: upload terrain and the like.
    pub const ADMIN: u8 = 3;
}

/// Single row, private: the identity that published the database, which is always an admin.
#[spacetimedb::table(accessor = owner)]
pub struct Owner {
    #[primary_key]
    pub id: u8,
    pub identity: Identity,
}

#[spacetimedb::table(accessor = account)]
pub struct Account {
    #[primary_key]
    pub identity: Identity,
    pub created_at: Timestamp,
    pub gm_level: u8,
}

/// What other players may see about a character.
#[spacetimedb::table(accessor = character_public, public)]
pub struct CharacterPublic {
    #[primary_key]
    pub char_id: u64,
    #[unique]
    pub name: String,
}

/// Owner-only character data, read through `my_characters`.
#[spacetimedb::table(accessor = character_private)]
pub struct CharacterPrivate {
    #[primary_key]
    #[auto_inc]
    pub char_id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub created_at: Timestamp,
    /// Where the character was when it last left the world.
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Cold: what to spawn. Public, keyed by cell.
#[spacetimedb::table(accessor = entity, public)]
pub struct Entity {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    pub kind: u8,
    /// For characters, the `char_id`; for ships, the hull.
    pub ref_id: u64,
}

/// Hot: where it is, as others see it. Public, keyed by cell. Kept small. For characters this
/// lags the authoritative position in `presence` by up to one tick: `world_tick` publishes moves
/// in one batch every 100 ms (docs/spikes/mass-fight.md).
#[spacetimedb::table(accessor = transform, public)]
#[derive(Debug, PartialEq)]
pub struct Transform {
    #[primary_key]
    pub entity_id: u64,
    #[index(btree)]
    pub cell: u64,
    /// 0 = the world, otherwise the ship the entity stands on; the position is then ship-local, and
    /// the cell is the ship's (docs/design/ships.md).
    pub parent: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    /// Horizontal speed in cm/s, for extrapolation.
    pub speed: i16,
    pub mode: u8,
}

/// Private: who is in the world as which character, where it really is, and its movement
/// bookkeeping. The owner reads it through `my_presence`.
#[spacetimedb::table(accessor = presence)]
pub struct Presence {
    #[primary_key]
    pub identity: Identity,
    #[unique]
    pub char_id: u64,
    #[unique]
    pub entity_id: u64,
    pub entered_at: Timestamp,
    /// The connection that entered. Only its own disconnect despawns the character: a client that
    /// lost its connection may reconnect and enter again before the server notices the old one has
    /// gone (docs/design/client.md).
    pub connection_id: ConnectionId,
    /// 0 in the world, or the ship whose deck the character is on. The position and anchor below
    /// are in this frame (docs/design/ships.md).
    #[index(btree)]
    pub parent: u64,
    /// The last accepted movement event.
    pub last_seq: u32,
    pub last_move_at: Timestamp,
    /// Bumped whenever the server overrules the client's position; the client snaps to the
    /// position below when this changes.
    pub corrections: u32,
    /// The authoritative position and motion, as of the last accepted move.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    pub speed: i16,
    pub mode: u8,
    /// The last solid footing, which vertical moves are measured from, and when the character last
    /// touched the ground (docs/design/terrain.md).
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub anchor_z: f32,
    pub contact_at: Timestamp,
}

#[spacetimedb::view(accessor = my_characters, public)]
pub(crate) fn my_characters(ctx: &ViewContext) -> Vec<CharacterPrivate> {
    ctx.db.character_private().owner().filter(ctx.sender()).collect()
}

#[spacetimedb::view(accessor = my_presence, public)]
pub(crate) fn my_presence(ctx: &ViewContext) -> Option<Presence> {
    ctx.db.presence().identity().find(ctx.sender())
}

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) {
    // SpacetimeDB 2.13 has no lifecycle reducer for republishing, so a database created before
    // the tick existed starts it here (ADR 0010).
    crate::tick::start(ctx);
    if ctx.db.account().identity().find(ctx.sender()).is_none() {
        ctx.db.account().insert(Account { identity: ctx.sender(), created_at: now(ctx), gm_level: 0 });
    }
    crate::labor::open(ctx, ctx.sender());
}

/// Called by `init`: the publisher becomes the owner.
pub fn record_owner(ctx: &ReducerContext) {
    if ctx.db.owner().id().find(0).is_none() {
        ctx.db.owner().insert(Owner { id: 0, identity: ctx.sender() });
    }
}

pub fn require_admin(ctx: &ReducerContext) -> Result<(), String> {
    let owner = ctx.db.owner().id().find(0).is_some_and(|o| o.identity == ctx.sender());
    let gm = ctx.db.account().identity().find(ctx.sender()).is_some_and(|a| a.gm_level >= gm::ADMIN);
    if owner || gm { Ok(()) } else { Err("admins only".into()) }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) {
    let entered_here = ctx.db.presence().identity().find(ctx.sender()).is_some_and(|p| Some(p.connection_id) == ctx.connection_id());
    if entered_here {
        despawn(ctx, ctx.sender());
    }
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let len = name.chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&len) {
        return Err(format!("names are {NAME_MIN} to {NAME_MAX} letters"));
    }
    if !name.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err("names use the letters A to Z only".into());
    }
    Ok(())
}

/// `Abcd` form, so uniqueness ignores case.
fn normalise_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut chars = lower.chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

#[spacetimedb::reducer]
pub fn create_character(ctx: &ReducerContext, name: String) -> Result<(), String> {
    validate_name(&name)?;
    let name = normalise_name(&name);
    if ctx.db.account().identity().find(ctx.sender()).is_none() {
        return Err("no account".into());
    }
    if ctx.db.character_private().owner().filter(ctx.sender()).count() >= MAX_CHARACTERS_PER_ACCOUNT {
        return Err(format!("at most {MAX_CHARACTERS_PER_ACCOUNT} characters per account"));
    }
    if ctx.db.character_public().name().find(&name).is_some() {
        return Err("that name is taken".into());
    }
    let [x, y, z] = SPAWN;
    let c = ctx.db.character_private().insert(CharacterPrivate {
        char_id: 0,
        owner: ctx.sender(),
        created_at: now(ctx),
        x,
        y,
        z,
    });
    ctx.db.character_public().insert(CharacterPublic { char_id: c.char_id, name });
    Ok(())
}

/// `content_hash` is the client's `CONTENT_HASH`, and `terrain_hash` the terrain hash of the world
/// it loaded. A client built from other content would predict with other rules, and one on other
/// terrain would walk on other ground, so both are refused (docs/design/content.md, world.md).
#[spacetimedb::reducer]
pub fn enter_world(ctx: &ReducerContext, char_id: u64, content_hash: u64, terrain_hash: u64) -> Result<(), String> {
    if content_hash != sim::content::CONTENT_HASH {
        return Err(format!(
            "client content {content_hash:016x} does not match the server's {:016x}; update the client",
            sim::content::CONTENT_HASH
        ));
    }
    let terrain = terrain_info(ctx);
    if terrain_hash != terrain.hash {
        return Err(format!(
            "client terrain {terrain_hash:016x} does not match the server's {:016x} ({} chunks); run `cargo xtask world`",
            terrain.hash, terrain.chunks
        ));
    }
    let c = ctx
        .db
        .character_private()
        .char_id()
        .find(char_id)
        .filter(|c| c.owner == ctx.sender())
        .ok_or("no such character")?;
    // Re-entering, with the same or another character, replaces the old presence.
    despawn(ctx, ctx.sender());
    let mode = sim::movement::MoveMode::Run;
    // Saved before the terrain there existed, or under it since: start on the ground.
    let y = sim::movement::settle(Terrain::new(ctx).ground(c.x, c.z), [c.x, c.y, c.z], mode);
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, c.x, c.z);
    let e = ctx.db.entity().insert(Entity { id: 0, cell, kind: entity_kind::CHARACTER, ref_id: char_id });
    ctx.db.transform().insert(Transform {
        entity_id: e.id,
        cell,
        parent: 0,
        x: c.x,
        y,
        z: c.z,
        yaw: 0,
        speed: 0,
        mode: mode as u8,
    });
    let t = now(ctx);
    ctx.db.presence().insert(Presence {
        identity: ctx.sender(),
        char_id,
        entity_id: e.id,
        entered_at: t,
        connection_id: ctx.connection_id().unwrap_or(ConnectionId::ZERO),
        parent: 0,
        last_seq: 0,
        last_move_at: t,
        corrections: 0,
        x: c.x,
        y,
        z: c.z,
        yaw: 0,
        speed: 0,
        mode: mode as u8,
        anchor_x: c.x,
        anchor_y: y,
        anchor_z: c.z,
        contact_at: t,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn leave_world(ctx: &ReducerContext) {
    despawn(ctx, ctx.sender());
}

/// Remove `who`'s character from the world, saving where it stood in world coordinates.
fn despawn(ctx: &ReducerContext, who: Identity) {
    let Some(p) = ctx.db.presence().identity().find(who) else { return };
    let [x, y, z] = crate::ship::world_position(ctx, &p);
    crate::ship::let_go(ctx, p.parent, p.entity_id);
    crate::gathering::cancel(ctx, p.char_id);
    crate::crafting::cancel(ctx, p.char_id);
    if let Some(c) = ctx.db.character_private().char_id().find(p.char_id) {
        ctx.db.character_private().char_id().update(CharacterPrivate { x, y, z, ..c });
    }
    ctx.db.moved().entity_id().delete(p.entity_id);
    ctx.db.transform().entity_id().delete(p.entity_id);
    ctx.db.entity().id().delete(p.entity_id);
    ctx.db.presence().identity().delete(who);
}
