//! The private server heightmap, its upload by admins, and readings for the movement and sight
//! checks (docs/design/terrain.md). The rules themselves live in `sim::terrain` and `sim::movement`.

use std::collections::HashMap;

use spacetimedb::{ReducerContext, Table};
use sim::terrain::{CHUNK_LEN, Chunks, Ground, valid_chunk_cell};

use crate::session::require_admin;

/// Private: one 64 m cell of terrain, 33 × 33 height samples including the far edge.
#[spacetimedb::table(accessor = terrain_chunk)]
pub struct TerrainChunk {
    #[primary_key]
    pub cell: u64,
    pub samples: Vec<u16>,
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
    for c in chunks {
        ctx.db.terrain_chunk().cell().delete(c.cell);
        ctx.db.terrain_chunk().insert(c);
    }
    log::info!("{n} terrain chunks uploaded");
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_terrain(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let cells: Vec<u64> = ctx.db.terrain_chunk().iter().map(|c| c.cell).collect();
    for &cell in &cells {
        ctx.db.terrain_chunk().cell().delete(cell);
    }
    log::info!("{} terrain chunks cleared", cells.len());
    Ok(())
}
