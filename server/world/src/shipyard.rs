//! Shipyards (docs/design/vehicles.md §2B): a design placed on the water and built stage by stage,
//! with packs and labor, into a hull's vehicle item in the owner's bag. Anyone may do a stage but
//! the last, which only the owner launches.

use spacetimedb::{ReducerContext, Table, Timestamp};
use sim::content::{HullData, ShipyardData, item as def_of, shipyard as kind_of, shipyard_by_design};
use sim::ship::Motion;

use crate::session::presence;
use crate::time::now;
use crate::vehicle::same_account;

/// Static and public: a shipyard and how far it has got. Written when a stage is done.
#[spacetimedb::table(accessor = shipyard, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Shipyard {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// The shipyard kind's wire (`content/ships.toml`).
    pub kind: u8,
    /// The character that placed it, which the launch puts the ship's item with.
    #[index(btree)]
    pub owner: u64,
    /// The footprint: its hull's deck, centred here and facing `yaw`.
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    /// Stages done.
    pub stage: u8,
    pub placed_at: Timestamp,
}

/// The hull a yard builds.
pub fn hull_of(y: &Shipyard) -> Option<&'static HullData> {
    kind_of(y.kind).map(|k| k.hull.data())
}

fn motion_of(y: &Shipyard) -> Motion {
    Motion { x: y.x, z: y.z, yaw: y.yaw, speed: 0.0 }
}

/// Place a shipyard from the design `item_id` in the caller's bag, on the water ahead of it where
/// the hull would be summoned.
#[spacetimedb::reducer]
pub fn place_shipyard(ctx: &ReducerContext, item_id: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    if p.parent != 0 {
        return Err("can't place a shipyard while aboard".into());
    }
    let design = crate::items::held(ctx, p.char_id, item_id).ok_or("no such item")?;
    let kind = shipyard_by_design(design.def).ok_or("that isn't a design")?;
    if ctx.db.shipyard().owner().filter(p.char_id).next().is_some() {
        return Err("you already have a shipyard".into());
    }
    let hull = kind.hull.data();
    let m = sim::ship::summon_motion(hull, [p.x, p.y, p.z], sim::ship::yaw_from_u16(p.yaw));
    let mut terrain = crate::terrain::Terrain::new(ctx);
    sim::ship::stand(hull, &m, |x, z| terrain.ground(x, z)).ok_or("a shipyard needs deep enough water there")?;
    if crate::vehicle::crowded(ctx, hull, &m) {
        return Err("no room for it there".into());
    }
    if kind.place_labor > 0 {
        crate::labor::spend(ctx, ctx.sender(), kind.place_labor)?;
    }
    crate::items::take(ctx, p.char_id, &[(design.def, 1)])?;
    let y = ctx.db.shipyard().insert(Shipyard {
        id: 0,
        cell: sim::cell::cell_of(sim::cell::OPEN_WORLD, m.x, m.z),
        kind: kind.wire,
        owner: p.char_id,
        x: m.x,
        z: m.z,
        yaw: m.yaw,
        stage: 0,
        placed_at: now(ctx),
    });
    log::info!("char {} placed {} {} at ({:.1}, {:.1})", p.char_id, kind.id, y.id, m.x, m.z);
    Ok(())
}

/// Do the next stage of a shipyard in reach: deliver the pack it needs, if any, and spend its
/// labor. The last stage launches the ship's item into the owner's bag, and only the owner may.
#[spacetimedb::reducer]
pub fn work_shipyard(ctx: &ReducerContext, yard_id: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    let y = ctx.db.shipyard().id().find(yard_id).ok_or("no such shipyard")?;
    let kind: &ShipyardData = kind_of(y.kind).ok_or("no such shipyard")?;
    let hull = kind.hull.data();
    if !sim::ship::yard_reach(hull, &motion_of(&y), crate::ship::world_position(ctx, &p)) {
        return Err("out of reach".into());
    }
    let stage = kind.stages.get(usize::from(y.stage)).ok_or("it's finished")?;
    let launch = usize::from(y.stage) + 1 == kind.stages.len();
    if launch && !same_account(ctx, y.owner) {
        return Err("only the owner can launch it".into());
    }
    if stage.pack != 0 {
        let needs = || {
            let name = def_of(stage.pack).map_or("pack", |d| d.name);
            let a = if name.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
            format!("this stage needs {a} {name}")
        };
        let pk = crate::trade::carried(ctx, p.char_id).filter(|pk| pk.def == stage.pack).ok_or_else(needs)?;
        if sim::trade::fresh_permille(pk.made_at.to_micros_since_unix_epoch(), now(ctx).to_micros_since_unix_epoch()).is_none() {
            return Err("this pack has spoiled".into());
        }
        crate::trade::take_off(ctx, &p);
    }
    if stage.labor > 0 {
        crate::labor::spend(ctx, ctx.sender(), stage.labor)?;
    }
    if launch {
        crate::items::grant(ctx, y.owner, hull.item, 1).map_err(|_| "no room in the owner's bag")?;
        ctx.db.shipyard().id().delete(y.id);
        log::info!("char {} launched a {} from shipyard {}", p.char_id, hull.name, y.id);
    } else {
        log::info!("char {} did stage {} ({}) of shipyard {}", p.char_id, y.stage, stage.name, y.id);
        ctx.db.shipyard().id().update(Shipyard { stage: y.stage + 1, ..y });
    }
    Ok(())
}

/// The owner gives up its shipyard, and everything put into it.
#[spacetimedb::reducer]
pub fn abandon_shipyard(ctx: &ReducerContext, yard_id: u64) -> Result<(), String> {
    let y = ctx.db.shipyard().id().find(yard_id).ok_or("no such shipyard")?;
    if !same_account(ctx, y.owner) {
        return Err("that isn't your shipyard".into());
    }
    ctx.db.shipyard().id().delete(y.id);
    log::info!("shipyard {} of char {} abandoned at stage {}", y.id, y.owner, y.stage);
    Ok(())
}
