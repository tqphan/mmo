//! Owned vehicles (docs/design/vehicles.md): summoned from a vehicle item in the bag and stowed back
//! into it, packs loaded into and unloaded from their slots, wrecks and their mending, and repairs.
//! Driving, decks and damage are `ship`'s and `movement`'s; the rules are `sim::ship`'s.

use spacetimedb::{ReducerContext, Table, TimeDuration, Timestamp, ViewContext};
use sim::content::{HullData, ItemKind, VEHICLES, hull_of_item, item as def_of};
use sim::ship::{Hull, Motion};

use crate::items::{Item, item, place};
use crate::session::{Presence, character_private, character_private__view, presence};
use crate::ship::{Deck, Vehicle, deck, vehicle};
use crate::shipyard::shipyard;
use crate::terrain::Terrain;
use crate::time::now;
use crate::trade::Pack;

/// Private: a pack in a vehicle's slot, everything about it that a back would hold. `vehicle.cargo`
/// shows its item to everyone.
#[spacetimedb::table(accessor = cargo)]
#[derive(Clone, Debug, PartialEq)]
pub struct Cargo {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub vehicle: u64,
    pub slot: u8,
    pub def: u16,
    pub owner: u64,
    pub origin_x: f32,
    pub origin_z: f32,
    pub made_at: Timestamp,
}

/// Private: a vehicle item whose vehicle was wrecked. It can't be summoned until it's mended and
/// `ready_at` has passed. Read through `my_wrecks`.
#[spacetimedb::table(accessor = wreck)]
#[derive(Clone, Debug, PartialEq)]
pub struct Wreck {
    #[primary_key]
    pub item_id: u64,
    #[index(btree)]
    pub holder: u64,
    /// Kits have gone in, and it can be summoned from `ready_at`.
    pub mending: bool,
    pub ready_at: Timestamp,
}

/// The wrecks of all the caller's characters.
#[spacetimedb::view(accessor = my_wrecks, public)]
pub(crate) fn my_wrecks(ctx: &ViewContext) -> Vec<Wreck> {
    ctx.db
        .character_private()
        .owner()
        .filter(ctx.sender())
        .flat_map(|c| ctx.db.wreck().holder().filter(c.char_id).collect::<Vec<_>>())
        .collect()
}

/// Whether the caller's account owns character `char_id`.
pub fn same_account(ctx: &ReducerContext, char_id: u64) -> bool {
    ctx.db.character_private().char_id().find(char_id).is_some_and(|c| c.owner == ctx.sender())
}

/// The caller in the world and alive.
fn alive(ctx: &ReducerContext) -> Result<Presence, String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    Ok(p)
}

/// The vehicles `char_id` has out.
fn out_of(ctx: &ReducerContext, char_id: u64) -> impl Iterator<Item = Item> {
    ctx.db.item().by_slot().filter((char_id, place::OUT)).collect::<Vec<_>>().into_iter()
}

/// Summon the vehicle item `item_id` from the caller's bag, ahead of it.
#[spacetimedb::reducer]
pub fn summon_vehicle(ctx: &ReducerContext, item_id: u64) -> Result<(), String> {
    let p = alive(ctx)?;
    if p.parent != 0 {
        return Err("can't summon a vehicle while aboard one".into());
    }
    let it = crate::items::held(ctx, p.char_id, item_id).ok_or("no such item")?;
    let hull = def_of(it.def).filter(|d| d.kind == ItemKind::Vehicle).and_then(|d| hull_of_item(d.wire)).ok_or("that isn't a vehicle")?;
    if let Some(w) = ctx.db.wreck().item_id().find(it.id) {
        if !w.mending {
            return Err("it's wrecked: mend it first".into());
        }
        if now(ctx) < w.ready_at {
            return Err("it's still being mended".into());
        }
        ctx.db.wreck().item_id().delete(it.id);
    }
    let same_medium = out_of(ctx, p.char_id).any(|o| Hull::from_u8(o.slot as u8).is_some_and(|h| h.data().land == hull.land));
    if same_medium {
        return Err(if hull.land { "you already have a land vehicle out" } else { "you already have a boat out" }.into());
    }
    let m = sim::ship::summon_motion(hull, [p.x, p.y, p.z], sim::ship::yaw_from_u16(p.yaw));
    let mut terrain = Terrain::new(ctx);
    let y = sim::ship::stand(hull, &m, |x, z| terrain.ground(x, z))
        .ok_or(if hull.land { "a cart needs firm, gentle ground there" } else { "a boat needs deep enough water there" })?;
    if crowded(ctx, hull, &m) {
        return Err("no room for it there".into());
    }
    ctx.db.item().id().update(Item { place: place::OUT, slot: u16::from(hull.hull as u8), ..it });
    crate::ship::launch(ctx, hull.hull, m, y, p.char_id);
    Ok(())
}

/// Whether a vehicle or shipyard is too close to put `hull` at `m`: their centres nearer than the
/// two half-lengths added.
pub fn crowded(ctx: &ReducerContext, hull: &HullData, m: &Motion) -> bool {
    let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, m.x, m.z);
    let near = |x: f32, z: f32, half: f32| ((x - m.x).powi(2) + (z - m.z).powi(2)).sqrt() < half + hull.deck_half_length_m;
    sim::cell::window_cells(cell, 1).into_iter().any(|c| {
        ctx.db.vehicle().cell().filter(c).filter_map(|v| deck(ctx, v.entity_id)).any(|d| near(d.motion.x, d.motion.z, d.hull.deck_half_length_m))
            || ctx.db.shipyard().cell().filter(c).any(|y| crate::shipyard::hull_of(&y).is_some_and(|h| near(y.x, y.z, h.deck_half_length_m)))
    })
}

/// Put the caller's vehicle back into its bag, from anywhere. Anyone aboard is put off.
#[spacetimedb::reducer]
pub fn stow_vehicle(ctx: &ReducerContext, vehicle_id: u64) -> Result<(), String> {
    let v = ctx.db.vehicle().entity_id().find(vehicle_id).ok_or("no such vehicle")?;
    if v.owner == 0 || !same_account(ctx, v.owner) {
        return Err("that isn't your vehicle".into());
    }
    if v.hp <= 0 {
        return Err(Hull::from_u8(v.hull).map_or("it's sinking", |h| crate::ship::sinking(h.data())).into());
    }
    if crate::ship::under_fire(ctx, &v) {
        return Err("it's under fire".into());
    }
    if v.cargo.iter().any(|&c| c != 0) {
        return Err("unload its cargo first".into());
    }
    let it = ctx.db.item().by_slot().filter((v.owner, place::OUT, u16::from(v.hull))).next().ok_or("its item is missing")?;
    let slot = crate::items::free_slot(ctx, v.owner).ok_or("no room in the bag")?;
    let d = deck(ctx, vehicle_id).ok_or("no such vehicle")?;
    crate::ship::remove(ctx, &d);
    ctx.db.item().id().update(Item { place: place::BAG, slot, ..it });
    Ok(())
}

/// The vehicle, its deck and the world point of `slot`, if the caller at `p` can reach it.
fn slot_in_reach(ctx: &ReducerContext, p: &Presence, vehicle_id: u64, slot: u8) -> Result<(Vehicle, Deck), String> {
    let v = ctx.db.vehicle().entity_id().find(vehicle_id).ok_or("no such vehicle")?;
    let d = deck(ctx, vehicle_id).ok_or("no such vehicle")?;
    let point = sim::ship::slot_point(d.hull, &d.pose(), usize::from(slot)).ok_or("no such slot")?;
    if !sim::ship::cargo_reach(point, crate::ship::world_position(ctx, p)) {
        return Err("out of reach".into());
    }
    Ok((v, d))
}

/// Show `def` (0 for none) in a vehicle's slot.
fn show(ctx: &ReducerContext, v: Vehicle, slot: u8, def: u16) {
    let mut cargo = v.cargo.clone();
    if let Some(c) = cargo.get_mut(usize::from(slot)) {
        *c = def;
    }
    ctx.db.vehicle().entity_id().update(Vehicle { cargo, ..v });
}

/// Put `pk` into `slot` of `v`. The caller checked the slot is empty.
fn stow_pack(ctx: &ReducerContext, v: Vehicle, slot: u8, pk: &Pack) {
    ctx.db.cargo().insert(Cargo {
        id: 0,
        vehicle: v.entity_id,
        slot,
        def: pk.def,
        owner: pk.owner,
        origin_x: pk.origin_x,
        origin_z: pk.origin_z,
        made_at: pk.made_at,
    });
    show(ctx, v, slot, pk.def);
}

/// Load the caller's pack into an empty slot it can reach. Anyone may.
#[spacetimedb::reducer]
pub fn load_pack(ctx: &ReducerContext, vehicle_id: u64, slot: u8) -> Result<(), String> {
    let p = alive(ctx)?;
    let pk = crate::trade::carried(ctx, p.char_id).ok_or("not carrying a pack")?;
    let (v, _) = slot_in_reach(ctx, &p, vehicle_id, slot)?;
    if v.cargo.get(usize::from(slot)).is_some_and(|&c| c != 0) {
        return Err("that slot is full".into());
    }
    crate::trade::take_off(ctx, &p);
    stow_pack(ctx, v, slot, &pk);
    Ok(())
}

/// Take the pack in a slot onto the caller's back. Only the vehicle's owner may, or anyone for an
/// unowned one. A spoiled pack falls apart instead.
#[spacetimedb::reducer]
pub fn unload_pack(ctx: &ReducerContext, vehicle_id: u64, slot: u8) -> Result<(), String> {
    let p = alive(ctx)?;
    if crate::trade::carried(ctx, p.char_id).is_some() {
        return Err("already carrying a pack".into());
    }
    let (v, _) = slot_in_reach(ctx, &p, vehicle_id, slot)?;
    if v.owner != 0 && !same_account(ctx, v.owner) {
        return Err("only the owner can unload this".into());
    }
    let c = ctx.db.cargo().vehicle().filter(vehicle_id).find(|c| c.slot == slot).ok_or("that slot is empty")?;
    ctx.db.cargo().id().delete(c.id);
    show(ctx, v, slot, 0);
    if sim::trade::fresh_permille(c.made_at.to_micros_since_unix_epoch(), now(ctx).to_micros_since_unix_epoch()).is_none() {
        log::info!("char {}'s spoiled pack fell apart as it was unloaded", p.char_id);
        return Ok(());
    }
    if !same_account(ctx, c.owner) {
        log::info!("crime: char {} took pack {} of char {} off vehicle {vehicle_id}", p.char_id, c.def, c.owner);
        crate::crime::theft(ctx, p.char_id, c.owner, crate::ship::world_position(ctx, &p));
    }
    crate::trade::put_on(ctx, &p, c.def, c.owner, c.origin_x, c.origin_z, c.made_at);
    Ok(())
}

/// Called when `p` dies aboard a vehicle: its pack goes into the first free slot, if any. Returns
/// whether it did.
pub fn stow_on_death(ctx: &ReducerContext, p: &Presence, pk: &Pack) -> bool {
    let Some(v) = ctx.db.vehicle().entity_id().find(p.parent) else { return false };
    let Some(slot) = v.cargo.iter().position(|&c| c == 0) else { return false };
    crate::trade::take_off(ctx, p);
    stow_pack(ctx, v, slot as u8, pk);
    true
}

/// Delete a vehicle's cargo rows, when the vehicle goes.
pub fn clear_cargo(ctx: &ReducerContext, vehicle_id: u64) {
    let ids: Vec<u64> = ctx.db.cargo().vehicle().filter(vehicle_id).map(|c| c.id).collect();
    for id in ids {
        ctx.db.cargo().id().delete(id);
    }
}

/// A vehicle at 0 HP goes: its packs fall where their slots were, on the ground off a cart or
/// floating off a ship, for anyone to take; riders are put off; and an owner's item comes back as a
/// wreck, into the bag or, if that's full, held in `RETURNS`.
pub fn wreck(ctx: &ReducerContext, vehicle_id: u64) {
    let (Some(v), Some(d)) = (ctx.db.vehicle().entity_id().find(vehicle_id), deck(ctx, vehicle_id)) else { return };
    let pose = d.pose();
    let mut terrain = Terrain::new(ctx);
    let yaw = sim::ship::yaw_to_u16(d.motion.yaw);
    let cargo: Vec<Cargo> = ctx.db.cargo().vehicle().filter(vehicle_id).collect();
    for c in cargo {
        let Some([x, deck_y, z]) = sim::ship::slot_point(d.hull, &pose, usize::from(c.slot)) else { continue };
        let y = if d.hull.land { terrain.ground(x, z).map_or(deck_y - d.hull.deck_height_m, |g| g.hi) } else { sim::movement::SEA_LEVEL_M };
        if !crate::trade::lay(ctx, c.def, c.owner, c.origin_x, c.origin_z, c.made_at, [x, y, z], yaw) {
            log::info!("a spoiled pack fell apart as vehicle {vehicle_id} was wrecked");
        }
    }
    if v.owner != 0
        && let Some(it) = ctx.db.item().by_slot().filter((v.owner, place::OUT, u16::from(v.hull))).next()
    {
        let (at, slot) = match crate::items::free_slot(ctx, v.owner) {
            Some(slot) => (place::BAG, slot),
            None => (place::RETURNS, ctx.db.item().by_slot().filter((v.owner, place::RETURNS)).map(|i| i.slot + 1).max().unwrap_or(0)),
        };
        ctx.db.item().id().update(Item { place: at, slot, ..it });
        ctx.db.wreck().insert(Wreck { item_id: it.id, holder: v.owner, mending: false, ready_at: now(ctx) });
    }
    log::info!("vehicle {vehicle_id} ({}) of char {} wrecked", d.hull.id, v.owner);
    crate::ship::remove(ctx, &d);
}

/// Mend the wrecked vehicle item `item_id` in the caller's bag: its hull's kits go in now, and it
/// can be summoned once `mend_s` has passed.
#[spacetimedb::reducer]
pub fn mend_vehicle(ctx: &ReducerContext, item_id: u64) -> Result<(), String> {
    let p = alive(ctx)?;
    let it = crate::items::held(ctx, p.char_id, item_id).ok_or("no such item")?;
    let w = ctx.db.wreck().item_id().find(it.id).ok_or("it isn't wrecked")?;
    if w.mending {
        return Err("it's already being mended".into());
    }
    let hull = hull_of_item(it.def).ok_or("that isn't a vehicle")?;
    if hull.mend_kits > 0 {
        crate::items::take(ctx, p.char_id, &[(VEHICLES.mend_item, hull.mend_kits as u32)])
            .map_err(|_| format!("mending it takes {} {}", hull.mend_kits, def_of(VEHICLES.mend_item).map_or("kits", |d| d.name)))?;
    }
    let ready_at = now(ctx) + TimeDuration::from_micros(i64::from(hull.mend_s) * 1_000_000);
    ctx.db.wreck().item_id().update(Wreck { mending: true, ready_at, ..w });
    Ok(())
}

/// Patch a damaged vehicle from its deck or beside it: one kit and some labor for a share of its
/// HP. Anyone may, but not while it's under fire.
#[spacetimedb::reducer]
pub fn repair_vehicle(ctx: &ReducerContext, vehicle_id: u64) -> Result<(), String> {
    let p = alive(ctx)?;
    let v = ctx.db.vehicle().entity_id().find(vehicle_id).ok_or("no such vehicle")?;
    let d = deck(ctx, vehicle_id).ok_or("no such vehicle")?;
    if p.parent != vehicle_id && !sim::ship::repair_reach(d.hull, &d.motion, crate::ship::world_position(ctx, &p)) {
        return Err("out of reach".into());
    }
    if v.hp <= 0 {
        return Err(crate::ship::sinking(d.hull).into());
    }
    if v.hp >= d.hull.hp {
        return Err("it isn't damaged".into());
    }
    if crate::ship::under_fire(ctx, &v) {
        return Err("can't repair it under fire".into());
    }
    crate::items::take(ctx, p.char_id, &[(VEHICLES.mend_item, 1)])
        .map_err(|_| format!("repairs take a {}", def_of(VEHICLES.mend_item).map_or("kit", |d| d.name)))?;
    if VEHICLES.repair_labor > 0 {
        crate::labor::spend(ctx, ctx.sender(), VEHICLES.repair_labor)?;
    }
    let hp = sim::ship::repaired_hp(d.hull, v.hp);
    ctx.db.vehicle().entity_id().update(Vehicle { hp, ..v });
    Ok(())
}
