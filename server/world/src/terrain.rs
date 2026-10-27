//! The private server heightmap, its upload by admins, and readings for the movement and sight
//! checks (docs/design/terrain.md). The rules themselves live in `sim::terrain` and `sim::movement`.

use std::collections::HashMap;

use spacetimedb::{ReducerContext, Table};
use sim::terrain::{CHUNK_LEN, Chunks, Ground, chunk_hash, valid_chunk_cell};

use crate::land::land_chunk;
use crate::session::require_admin;

/// Private: one 64 m cell of terrain, 33 × 33 height samples including the far edge.
#[spacetimedb::table(accessor = terrain_chunk)]
pub struct TerrainChunk {
    #[primary_key]
    pub cell: u64,
    pub samples: Vec<u16>,
}

/// Public, one row: how many chunks there are and their terrain hash (docs/design/world.md), so
/// `enter_world` can refuse a client standing on other ground, and the same for the land-use
/// raster's chunks (docs/design/land.md). Kept up to date by the uploads.
#[spacetimedb::table(accessor = terrain_info, public)]
pub struct TerrainInfo {
    #[primary_key]
    pub id: u8,
    pub chunks: u32,
    pub hash: u64,
    pub land_chunks: u32,
    pub land_hash: u64,
}

/// The terrain's row, rebuilt from the chunks if it's missing (a database from before the table).
pub fn terrain_info(ctx: &ReducerContext) -> TerrainInfo {
    if let Some(info) = ctx.db.terrain_info().id().find(0) {
        return info;
    }
    let (mut chunks, mut hash) = (0u32, 0u64);
    for c in ctx.db.terrain_chunk().iter() {
        chunks += 1;
        hash = hash.wrapping_add(chunk_hash(c.cell, &c.samples));
    }
    let (mut land_chunks, mut land_hash) = (0u32, 0u64);
    for c in ctx.db.land_chunk().iter() {
        land_chunks += 1;
        land_hash = land_hash.wrapping_add(sim::land::land_chunk_hash(c.cell, &c.texels));
    }
    if chunks > 0 || land_chunks > 0 {
        log::info!("terrain_info rebuilt from {chunks} chunks and {land_chunks} land chunks");
    }
    ctx.db.terrain_info().insert(TerrainInfo { id: 0, chunks, hash, land_chunks, land_hash })
}

pub fn set_terrain_info(ctx: &ReducerContext, info: TerrainInfo) {
    ctx.db.terrain_info().id().delete(info.id);
    ctx.db.terrain_info().insert(info);
}

/// Reads chunks from the database once per reducer call, however many lookups need them.
pub struct Terrain<'a> {
    ctx: &'a ReducerContext,
    cache: HashMap<u64, Option<Vec<u16>>>,
}

impl<'a> Terrain<'a> {
    pub fn new(ctx: &'a ReducerContext) -> Self {
        Self { ctx, cache: HashMap::new() }
    }

    pub fn ground(&mut self, x: f32, z: f32) -> Option<Ground> {
        sim::terrain::ground(self, x, z)
    }
}

impl Chunks for Terrain<'_> {
    fn chunk(&mut self, cell: u64) -> Option<&[u16]> {
        let ctx = self.ctx;
        self.cache.entry(cell).or_insert_with(|| ctx.db.terrain_chunk().cell().find(cell).map(|c| c.samples)).as_deref()
    }
}

/// Add or replace chunks. Upload with the world empty: players already in it aren't moved.
#[spacetimedb::reducer]
pub fn admin_put_terrain(ctx: &ReducerContext, chunks: Vec<TerrainChunk>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = chunks.iter().find(|c| c.samples.len() != CHUNK_LEN || !valid_chunk_cell(c.cell)) {
        return Err(format!("chunk {:#x}: needs {CHUNK_LEN} samples and an open-world cell on the map", bad.cell));
    }
    let n = chunks.len();
    let mut info = terrain_info(ctx);
    for c in chunks {
        match ctx.db.terrain_chunk().cell().find(c.cell) {
            Some(old) => {
                info.hash = info.hash.wrapping_sub(chunk_hash(old.cell, &old.samples));
                ctx.db.terrain_chunk().cell().delete(c.cell);
            }
            None => info.chunks += 1,
        }
        info.hash = info.hash.wrapping_add(chunk_hash(c.cell, &c.samples));
        ctx.db.terrain_chunk().insert(c);
    }
    log::info!("{n} terrain chunks uploaded; {} in all, terrain hash {:016x}", info.chunks, info.hash);
    set_terrain_info(ctx, info);
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_terrain(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let cells: Vec<u64> = ctx.db.terrain_chunk().iter().map(|c| c.cell).collect();
    for &cell in &cells {
        ctx.db.terrain_chunk().cell().delete(cell);
    }
    set_terrain_info(ctx, TerrainInfo { chunks: 0, hash: 0, ..terrain_info(ctx) });
    log::info!("{} terrain chunks cleared", cells.len());
    Ok(())
}

/// Remove chunks by cell. Unknown cells are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_terrain(ctx: &ReducerContext, cells: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    let mut info = terrain_info(ctx);
    for cell in cells {
        if let Some(old) = ctx.db.terrain_chunk().cell().find(cell) {
            info.hash = info.hash.wrapping_sub(chunk_hash(old.cell, &old.samples));
            info.chunks -= 1;
            ctx.db.terrain_chunk().cell().delete(cell);
        }
    }
    set_terrain_info(ctx, info);
    Ok(())
}
