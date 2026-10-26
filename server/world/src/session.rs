//! Accounts, characters, entering and leaving the world (PLAN §5.3, docs/design/movement.md).

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext};

use crate::time::now;

pub const MAX_CHARACTERS_PER_ACCOUNT: usize = 4;
pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 16;

/// Where new arrivals appear until the world has real spawn points.
pub const SPAWN: [f32; 3] = [0.0, 0.0, 0.0];

pub mod entity_kind {
    pub const CHARACTER: u8 = 1;
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
    /// For characters, the `char_id`.
    pub ref_id: u64,
}

/// Hot: where it is. Public, keyed by cell. Kept small.
#[spacetimedb::table(accessor = transform, public)]
pub struct Transform {
    #[primary_key]
    pub entity_id: u64,
    #[index(btree)]
    pub cell: u64,
    /// 0 = the world, otherwise the vehicle the entity stands on.
    pub parent: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    /// Horizontal speed in cm/s, for extrapolation.
    pub speed: i16,
    pub mode: u8,
}

/// Private: who is in the world as which character, and their movement bookkeeping.
#[spacetimedb::table(accessor = presence)]
pub struct Presence {
    #[primary_key]
    pub identity: Identity,
    #[unique]
    pub char_id: u64,
    #[unique]
    pub entity_id: u64,
    pub entered_at: Timestamp,
    /// The last accepted movement event.
    pub last_seq: u32,
    pub last_move_at: Timestamp,
    /// Bumped whenever the server overrules the client's position; the client snaps to its
    /// `transform` row when this changes.
    pub corrections: u32,
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
}

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) {
    despawn(ctx, ctx.sender());
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

#[spacetimedb::reducer]
pub fn enter_world(ctx: &ReducerContext, char_id: u64) -> Result<(), String> {
    let c = ctx
        .db
        .character_private()
        .char_id()
        .find(char_id)
        .filter(|c| c.owner == ctx.sender())
        .ok_or("no such character")?;
    // Re-entering, with the same or another character, replaces the old presence.
    despawn(ctx, ctx.sender());
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, c.x, c.z);
    let e = ctx.db.entity().insert(Entity { id: 0, cell, kind: entity_kind::CHARACTER, ref_id: char_id });
    ctx.db.transform().insert(Transform {
        entity_id: e.id,
        cell,
        parent: 0,
        x: c.x,
        y: c.y,
        z: c.z,
        yaw: 0,
        speed: 0,
        mode: sim::movement::MoveMode::Run as u8,
    });
    let t = now(ctx);
    ctx.db.presence().insert(Presence {
        identity: ctx.sender(),
        char_id,
        entity_id: e.id,
        entered_at: t,
        last_seq: 0,
        last_move_at: t,
        corrections: 0,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn leave_world(ctx: &ReducerContext) {
    despawn(ctx, ctx.sender());
}

/// Remove `who`'s character from the world, saving where it stood.
fn despawn(ctx: &ReducerContext, who: Identity) {
    let Some(p) = ctx.db.presence().identity().find(who) else { return };
    if let Some(t) = ctx.db.transform().entity_id().find(p.entity_id)
        && let Some(c) = ctx.db.character_private().char_id().find(p.char_id)
    {
        ctx.db.character_private().char_id().update(CharacterPrivate { x: t.x, y: t.y, z: t.z, ..c });
    }
    ctx.db.transform().entity_id().delete(p.entity_id);
    ctx.db.entity().id().delete(p.entity_id);
    ctx.db.presence().identity().delete(who);
}
