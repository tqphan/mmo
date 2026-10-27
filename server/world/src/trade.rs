//! Trade packs (docs/design/trade.md): packs carried on the back, dropped on the ground and picked
//! up, sold at posts for copper. Prices, demand and freshness come from timestamps by the rules in
//! `sim::trade`; nothing is ticked.

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, ViewContext};
use sim::movement::MoveMode;
use sim::trade::{FULL, Quote, after_sale, demand_key, demand_now, fresh_permille, in_reach, post, quote};

use crate::session::{Entity, Presence, character_private, character_private__view, entity, presence, require_admin};
use crate::time::now;

/// Private: a pack on a character's back, one each at most. The carrier reads it through `my_pack`.
#[spacetimedb::table(accessor = pack)]
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    #[primary_key]
    pub carrier: u64,
    /// The pack item's wire.
    pub def: u16,
    /// The character that made it.
    pub owner: u64,
    /// Where it was made.
    pub origin_x: f32,
    pub origin_z: f32,
    pub made_at: Timestamp,
    /// When it went on this back. A move whose interval began before this is judged at full speed.
    pub on_at: Timestamp,
}

/// Static and public: a pack lying on the ground.
#[spacetimedb::table(accessor = dropped_pack, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct DroppedPack {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    pub def: u16,
    pub owner: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
    pub origin_x: f32,
    pub origin_z: f32,
    pub made_at: Timestamp,
    pub dropped_at: Timestamp,
}

/// Static and public: a trade post. Written only when admins place or remove it.
#[spacetimedb::table(accessor = trade_post, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct TradePost {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The post kind's wire (`content/trade.toml`).
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Where an admin puts a post.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct PostPlacement {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Public: a post kind's demand for a pack kind, as it was at `at`; it recovers from there
/// (`sim::trade::demand_now`). No row is full demand.
#[spacetimedb::table(accessor = trade_demand, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct TradeDemand {
    /// `sim::trade::demand_key(post kind, pack item)`.
    #[primary_key]
    pub key: u32,
    /// Tenths of a percent.
    pub permille: u16,
    pub at: Timestamp,
}

/// Private: a character's coins, in copper. No row is none. The owner reads it through `my_purse`.
#[spacetimedb::table(accessor = purse)]
#[derive(Clone, Debug, PartialEq)]
pub struct Purse {
    #[primary_key]
    pub char_id: u64,
    pub copper: u64,
}

/// The packs on the backs of the caller's characters.
#[spacetimedb::view(accessor = my_pack, public)]
pub(crate) fn my_pack(ctx: &ViewContext) -> Vec<Pack> {
    ctx.db.character_private().owner().filter(ctx.sender()).filter_map(|c| ctx.db.pack().carrier().find(c.char_id)).collect()
}

/// The purses of the caller's characters.
#[spacetimedb::view(accessor = my_purse, public)]
pub(crate) fn my_purse(ctx: &ViewContext) -> Vec<Purse> {
    ctx.db.character_private().owner().filter(ctx.sender()).filter_map(|c| ctx.db.purse().char_id().find(c.char_id)).collect()
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

/// The caller in the world, on land: not aboard a ship and not swimming.
fn on_foot(ctx: &ReducerContext, ship: &str, water: &str) -> Result<Presence, String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if p.parent != 0 {
        return Err(ship.into());
    }
    if p.mode == MoveMode::Swim as u8 {
        return Err(water.into());
    }
    Ok(p)
}

/// The pack `char_id` carries, if any.
pub fn carried(ctx: &ReducerContext, char_id: u64) -> Option<Pack> {
    ctx.db.pack().carrier().find(char_id)
}

/// Put a pack on `p`'s back and show it on the entity. The caller checked the back was free.
#[allow(clippy::too_many_arguments)] // the pack's fields, wherever it comes from
fn put_on(ctx: &ReducerContext, p: &Presence, def: u16, owner: u64, origin_x: f32, origin_z: f32, made_at: Timestamp) {
    ctx.db.pack().insert(Pack { carrier: p.char_id, def, owner, origin_x, origin_z, made_at, on_at: now(ctx) });
    show(ctx, p.entity_id, def);
}

/// Take `p`'s pack off its back.
fn take_off(ctx: &ReducerContext, p: &Presence) {
    ctx.db.pack().carrier().delete(p.char_id);
    show(ctx, p.entity_id, 0);
}

/// Set what an entity shows on its back.
pub fn show(ctx: &ReducerContext, entity_id: u64, pack: u16) {
    if let Some(e) = ctx.db.entity().id().find(entity_id)
        && e.pack != pack
    {
        ctx.db.entity().id().update(Entity { pack, ..e });
    }
}

/// Called by crafting when a pack recipe finishes: the pack goes onto the crafter's back, made at
/// the station.
pub fn made(ctx: &ReducerContext, p: &Presence, def: u16, station: [f32; 3]) {
    put_on(ctx, p, def, p.char_id, station[0], station[2], now(ctx));
}

/// The speed limit factor for `char_id`'s move at `now`, given the last accepted move was at
/// `last_move_at`: slower while it carries a pack, unless the pack went on since the last move.
pub fn speed_scale(ctx: &ReducerContext, char_id: u64, last_move_at: Timestamp) -> f32 {
    match carried(ctx, char_id) {
        Some(pk) if pk.on_at < last_move_at => sim::trade::carry_limit(1.0),
        _ => 1.0,
    }
}

/// The account a character belongs to.
fn account_of(ctx: &ReducerContext, char_id: u64) -> Option<Identity> {
    ctx.db.character_private().char_id().find(char_id).map(|c| c.owner)
}

/// Crime hook: the crime task (BACKLOG M1) turns this into evidence and crime points.
fn theft(thief: u64, d: &DroppedPack) {
    log::info!("crime: char {thief} took pack {} of char {} at ({}, {})", d.id, d.owner, d.x, d.z);
}

/// Put the carried pack on the ground at the carrier's feet. A spoiled pack falls apart instead.
#[spacetimedb::reducer]
pub fn drop_pack(ctx: &ReducerContext) -> Result<(), String> {
    let p = on_foot(ctx, "can't drop a pack on a ship yet", "can't drop a pack in the water")?;
    let pk = carried(ctx, p.char_id).ok_or("not carrying a pack")?;
    take_off(ctx, &p);
    let t = now(ctx);
    if fresh_permille(micros(pk.made_at), micros(t)).is_none() {
        log::info!("char {}'s spoiled pack fell apart", p.char_id);
        return Ok(());
    }
    ctx.db.dropped_pack().insert(DroppedPack {
        id: 0,
        cell: sim::cell::cell_of(sim::cell::OPEN_WORLD, p.x, p.z),
        def: pk.def,
        owner: pk.owner,
        x: p.x,
        y: p.y,
        z: p.z,
        yaw: p.yaw,
        origin_x: pk.origin_x,
        origin_z: pk.origin_z,
        made_at: pk.made_at,
        dropped_at: t,
    });
    Ok(())
}

/// Put a dropped pack on the caller's back. Taking one made by another account's character is theft.
#[spacetimedb::reducer]
pub fn pick_up_pack(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let p = on_foot(ctx, "can't pick up a pack from a ship", "can't pick up a pack while swimming")?;
    if carried(ctx, p.char_id).is_some() {
        return Err("already carrying a pack".into());
    }
    let d = ctx.db.dropped_pack().id().find(id).ok_or("no such pack")?;
    if !in_reach([d.x, d.y, d.z], [p.x, p.y, p.z]) {
        return Err("out of reach".into());
    }
    if fresh_permille(micros(d.made_at), micros(now(ctx))).is_none() {
        return Err("this pack has spoiled".into());
    }
    if account_of(ctx, d.owner) != Some(ctx.sender()) {
        theft(p.char_id, &d);
    }
    ctx.db.dropped_pack().id().delete(d.id);
    put_on(ctx, &p, d.def, d.owner, d.origin_x, d.origin_z, d.made_at);
    Ok(())
}

/// What the post `post_id` would pay `p` for the pack it carries now.
fn offer(ctx: &ReducerContext, p: &Presence, post_id: u64) -> Result<(Pack, TradePost, Quote), String> {
    let pk = carried(ctx, p.char_id).ok_or("not carrying a pack")?;
    let tp = ctx.db.trade_post().id().find(post_id).ok_or("no such post")?;
    let kind = post(tp.kind).ok_or("no such post")?;
    if !in_reach([tp.x, tp.y, tp.z], [p.x, p.y, p.z]) {
        return Err("out of reach".into());
    }
    let t = micros(now(ctx));
    let demand = demand_of(ctx, tp.kind, pk.def, t);
    let q = quote(pk.def, [pk.origin_x, pk.origin_z], micros(pk.made_at), [tp.x, tp.z], kind.bonus_pct, demand, t).map_err(|r| r.message().to_string())?;
    Ok((pk, tp, q))
}

/// A post kind's demand for a pack kind at `t`.
fn demand_of(ctx: &ReducerContext, post: u8, def: u16, t: i64) -> u32 {
    ctx.db.trade_demand().key().find(demand_key(post, def)).map_or(FULL, |d| demand_now(u32::from(d.permille), micros(d.at), t))
}

/// Sell the carried pack at a post in reach, for copper. Demand there falls.
#[spacetimedb::reducer]
pub fn sell_pack(ctx: &ReducerContext, post_id: u64) -> Result<(), String> {
    let p = on_foot(ctx, "can't sell from a ship", "can't sell while swimming")?;
    let (pk, tp, q) = offer(ctx, &p, post_id)?;
    take_off(ctx, &p);
    let at = now(ctx);
    let key = demand_key(tp.kind, pk.def);
    let row = TradeDemand { key, permille: u16::try_from(after_sale(q.demand)).unwrap_or(0), at };
    if ctx.db.trade_demand().key().find(key).is_some() {
        ctx.db.trade_demand().key().update(row);
    } else {
        ctx.db.trade_demand().insert(row);
    }
    match ctx.db.purse().char_id().find(p.char_id) {
        Some(pu) => {
            ctx.db.purse().char_id().update(Purse { copper: pu.copper.saturating_add(q.copper), ..pu });
        }
        None => {
            ctx.db.purse().insert(Purse { char_id: p.char_id, copper: q.copper });
        }
    }
    log::info!(
        "char {} sold pack {} of char {} at post {} for {} (route {}%, demand {}, fresh {})",
        p.char_id,
        pk.def,
        pk.owner,
        tp.id,
        q.copper,
        q.route_pct,
        q.demand,
        q.fresh
    );
    Ok(())
}

/// Add posts.
#[spacetimedb::reducer]
pub fn admin_put_posts(ctx: &ReducerContext, posts: Vec<PostPlacement>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = posts.iter().find(|s| post(s.kind).is_none() || ![s.x, s.y, s.z].iter().all(|v| v.is_finite())) {
        return Err(format!("bad post {bad:?}: unknown kind or a non-finite position"));
    }
    let half = sim::cell::WORLD_HALF_M;
    if let Some(bad) = posts.iter().find(|s| s.x.abs() >= half || s.z.abs() >= half) {
        return Err(format!("bad post {bad:?}: off the map"));
    }
    for s in &posts {
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, s.x, s.z);
        ctx.db.trade_post().insert(TradePost { id: 0, cell, kind: s.kind, x: s.x, y: s.y, z: s.z, yaw: s.yaw });
    }
    log::info!("{} trade posts placed", posts.len());
    Ok(())
}

/// Remove posts by id. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_posts(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        ctx.db.trade_post().id().delete(id);
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_posts(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let ids: Vec<u64> = ctx.db.trade_post().iter().map(|s| s.id).collect();
    for &id in &ids {
        ctx.db.trade_post().id().delete(id);
    }
    log::info!("{} trade posts cleared", ids.len());
    Ok(())
}

/// Remove dropped packs by id, for scenarios that clean up after themselves. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_dropped_packs(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        ctx.db.dropped_pack().id().delete(id);
    }
    Ok(())
}

/// Dev builds only: put a fresh pack of item `item` on the caller's back, made where it stands.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_give_pack(ctx: &ReducerContext, item: String) -> Result<(), String> {
    let d = sim::content::item_by_id(&item).filter(|d| sim::trade::pack(d.wire).is_some()).ok_or_else(|| format!("no pack {item:?}"))?;
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    if carried(ctx, p.char_id).is_some() {
        return Err("already carrying a pack".into());
    }
    put_on(ctx, &p, d.wire, p.char_id, p.x, p.z, now(ctx));
    Ok(())
}

/// Dev builds only: take the pack off the caller's back, wherever it is, and destroy it.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_discard_pack(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    take_off(ctx, &p);
    Ok(())
}
