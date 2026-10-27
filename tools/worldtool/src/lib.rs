//! The world's files (`world/`, docs/design/world.md): the whole map's heightmap and land-use
//! raster as 625 region files and a manifest, read and written here, and the placeholder
//! generator that writes them until there's editor tooling.

pub mod nodes;
pub mod placeholder;
pub mod region;
pub mod stations;

use std::path::Path;

use sim::cell::{CELLS_PER_SIDE, WORLD_HALF_M};
use sim::content::{Climate, LandUse};
use sim::terrain::{CHUNK_SIDE, HeightFile, QUADS_PER_CHUNK};

use region::Region;

/// The world in the repo, relative to its root.
pub const DEFAULT_DIR: &str = "world";
pub const MANIFEST: &str = "world.toml";
pub const FORMAT: u32 = 1;
/// One cell's 16 × 16 land texels, rows of increasing z; all 0 off the map.
pub fn land_chunk(world: &World, cell: u64) -> Vec<u8> {
    let (instance, cz, cx) = sim::cell::unpack(cell);
    if instance != sim::cell::OPEN_WORLD || cx >= CELLS_PER_SIDE || cz >= CELLS_PER_SIDE {
        return vec![0; TEXELS_PER_CELL * TEXELS_PER_CELL];
    }
    let (t0, u0) = (cx as usize * TEXELS_PER_CELL, cz as usize * TEXELS_PER_CELL);
    let mut out = Vec::with_capacity(TEXELS_PER_CELL * TEXELS_PER_CELL);
    for j in 0..TEXELS_PER_CELL {
        out.extend_from_slice(&world.land[(u0 + j) * LAND_SIDE + t0..][..TEXELS_PER_CELL]);
    }
    out
}

/// Cells along a region's side: 640 m, 25 regions along the map's.
pub const REGION_CELLS: u16 = 10;
pub const REGIONS_PER_SIDE: u16 = CELLS_PER_SIDE / REGION_CELLS;
/// Land use is stored per 4 m texel, 2 × 2 quads.
pub const TEXEL_M: f32 = 4.0;
pub const TEXELS_PER_CELL: usize = QUADS_PER_CHUNK / 2;
/// Texels along the map's side.
pub const LAND_SIDE: usize = CELLS_PER_SIDE as usize * TEXELS_PER_CELL;

/// A land-use texel: the use class in the low 4 bits, the climate in bits 4–6.
pub fn texel(land_use: LandUse, climate: Climate) -> u8 {
    land_use as u8 | (climate as u8) << 4
}

pub fn texel_use(t: u8) -> Option<LandUse> {
    LandUse::from_u8(t & 0x0f)
}

pub fn texel_climate(t: u8) -> Option<Climate> {
    Climate::from_u8((t >> 4) & 0x07)
}

/// The whole map.
pub struct World {
    /// All 250 × 250 cells, from cell (0, 0).
    pub heights: HeightFile,
    /// `LAND_SIDE²` texels, rows of increasing z.
    pub land: Vec<u8>,
}

impl World {
    /// A world of flat ground at `raw`, all wild and temperate.
    pub fn flat(raw: u16) -> Self {
        let mut heights = HeightFile { cell_x: 0, cell_z: 0, cells_x: CELLS_PER_SIDE, cells_z: CELLS_PER_SIDE, samples: Vec::new() };
        let (w, h) = heights.size();
        heights.samples = vec![raw; w * h];
        Self { heights, land: vec![texel(LandUse::Wild, Climate::Temperate); LAND_SIDE * LAND_SIDE] }
    }

    /// The land-use texel under a world position, if it's on the map.
    pub fn land_at(&self, x: f32, z: f32) -> Option<u8> {
        let (i, j) = ((x + WORLD_HALF_M) / TEXEL_M, (z + WORLD_HALF_M) / TEXEL_M);
        let (i, j) = (i.floor(), j.floor());
        let ok = |v: f32| (0.0..LAND_SIDE as f32).contains(&v);
        (ok(i) && ok(j)).then(|| self.land[j as usize * LAND_SIDE + i as usize])
    }

    /// `sim::terrain::terrain_hash` over every chunk of the map, without splitting it all at once.
    pub fn terrain_hash(&self) -> u64 {
        let (w, _) = self.heights.size();
        let mut chunk = Vec::with_capacity(CHUNK_SIDE * CHUNK_SIDE);
        let mut sum = 0u64;
        for cz in 0..CELLS_PER_SIDE {
            for cx in 0..CELLS_PER_SIDE {
                chunk.clear();
                for r in 0..CHUNK_SIDE {
                    let start = (cz as usize * QUADS_PER_CHUNK + r) * w + cx as usize * QUADS_PER_CHUNK;
                    chunk.extend_from_slice(&self.heights.samples[start..start + CHUNK_SIDE]);
                }
                let key = sim::cell::pack(sim::cell::OPEN_WORLD, cz, cx);
                sum = sum.wrapping_add(sim::terrain::chunk_hash(key, &chunk));
            }
        }
        sum
    }

    /// Every cell's land chunk that isn't all wild and temperate (texel 0), as the server stores
    /// them (docs/design/land.md): `(cell key, 256 texels)`, rows of increasing z.
    pub fn land_chunks(&self) -> Vec<(u64, Vec<u8>)> {
        let mut out = Vec::new();
        for cz in 0..CELLS_PER_SIDE {
            for cx in 0..CELLS_PER_SIDE {
                let key = sim::cell::pack(sim::cell::OPEN_WORLD, cz, cx);
                let texels = land_chunk(self, key);
                if texels.iter().any(|&t| t != 0) {
                    out.push((key, texels));
                }
            }
        }
        out
    }

    /// The land hash the server should hold after uploading `land_chunks`.
    pub fn land_hash(&self) -> u64 {
        self.land_chunks().iter().fold(0u64, |h, (cell, t)| h.wrapping_add(sim::land::land_chunk_hash(*cell, t)))
    }

    /// Region `(rx, rz)` cut out of the map.
    fn region(&self, rx: u16, rz: u16) -> Region {
        let (w, _) = self.heights.size();
        let mut r = Region { cell_x: rx * REGION_CELLS, cell_z: rz * REGION_CELLS, cells: REGION_CELLS, heights: Vec::new(), land: Vec::new() };
        let (side, texels) = (r.side(), r.texels());
        let (i0, j0) = (r.cell_x as usize * QUADS_PER_CHUNK, r.cell_z as usize * QUADS_PER_CHUNK);
        for j in 0..side {
            r.heights.extend_from_slice(&self.heights.samples[(j0 + j) * w + i0..][..side]);
        }
        let (t0, u0) = (r.cell_x as usize * TEXELS_PER_CELL, r.cell_z as usize * TEXELS_PER_CELL);
        for j in 0..texels {
            r.land.extend_from_slice(&self.land[(u0 + j) * LAND_SIDE + t0..][..texels]);
        }
        r
    }

    /// Write the manifest and every region to `dir`, replacing what's there.
    pub fn save(&self, dir: &Path) -> Result<u64, String> {
        let regions = dir.join("regions");
        std::fs::create_dir_all(&regions).map_err(|e| format!("{}: {e}", regions.display()))?;
        let coords: Vec<(u16, u16)> = (0..REGIONS_PER_SIDE).flat_map(|rz| (0..REGIONS_PER_SIDE).map(move |rx| (rx, rz))).collect();
        let written = parallel(&coords, |&(rx, rz)| {
            let path = regions.join(region_name(rx, rz));
            std::fs::write(&path, self.region(rx, rz).to_bytes()).map_err(|e| format!("{}: {e}", path.display()))
        });
        written.into_iter().collect::<Result<Vec<()>, String>>()?;
        let hash = self.terrain_hash();
        let manifest = Manifest { format: FORMAT, region_cells: REGION_CELLS, terrain_hash: hash };
        let path = dir.join(MANIFEST);
        std::fs::write(&path, manifest.to_text()).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(hash)
    }

    /// Read the manifest and every region in `dir`. Shared region edges must agree, and the heights
    /// must match the manifest's terrain hash.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest = Manifest::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if manifest.format != FORMAT || manifest.region_cells != REGION_CELLS {
            return Err(format!("{}: format {} with {}-cell regions, expected {FORMAT} and {REGION_CELLS}", path.display(), manifest.format, manifest.region_cells));
        }
        let coords: Vec<(u16, u16)> = (0..REGIONS_PER_SIDE).flat_map(|rz| (0..REGIONS_PER_SIDE).map(move |rx| (rx, rz))).collect();
        let regions = parallel(&coords, |&(rx, rz)| {
            let path = dir.join("regions").join(region_name(rx, rz));
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let r = Region::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            if (r.cell_x, r.cell_z, r.cells) != (rx * REGION_CELLS, rz * REGION_CELLS, REGION_CELLS) {
                return Err(format!("{}: covers the wrong cells", path.display()));
            }
            Ok(r)
        });
        let mut world = World::flat(0);
        let (w, _) = world.heights.size();
        for (&(rx, rz), r) in coords.iter().zip(regions) {
            let r = r?;
            let (side, texels) = (r.side(), r.texels());
            let (i0, j0) = (r.cell_x as usize * QUADS_PER_CHUNK, r.cell_z as usize * QUADS_PER_CHUNK);
            // Regions are copied in order, so the west and north neighbours have written their side
            // of a shared edge already.
            for j in 0..side {
                for i in 0..side {
                    let (dst, v) = ((j0 + j) * w + i0 + i, r.heights[j * side + i]);
                    let shared = (i == 0 && rx > 0) || (j == 0 && rz > 0);
                    if shared && world.heights.samples[dst] != v {
                        return Err(format!("region {}: its edge differs from its neighbour's at sample ({}, {})", region_name(rx, rz), i0 + i, j0 + j));
                    }
                    world.heights.samples[dst] = v;
                }
            }
            let (t0, u0) = (r.cell_x as usize * TEXELS_PER_CELL, r.cell_z as usize * TEXELS_PER_CELL);
            for j in 0..texels {
                world.land[(u0 + j) * LAND_SIDE + t0..][..texels].copy_from_slice(&r.land[j * texels..][..texels]);
            }
        }
        let hash = world.terrain_hash();
        if hash != manifest.terrain_hash {
            return Err(format!("{}: terrain hash {:016x}, but the regions hash to {hash:016x}", path.display(), manifest.terrain_hash));
        }
        Ok(world)
    }
}

pub fn region_name(rx: u16, rz: u16) -> String {
    format!("x{rx:02}z{rz:02}.mmor")
}

/// `world/world.toml`: `key = value` lines and `#` comments, simple enough that the client reads it
/// without a TOML library.
#[derive(Debug, PartialEq)]
pub struct Manifest {
    pub format: u32,
    pub region_cells: u16,
    pub terrain_hash: u64,
}

impl Manifest {
    pub fn to_text(&self) -> String {
        format!(
            "# The world's files (docs/design/world.md). Written by worldtool: don't edit by hand.\n\
             format = {}\n\
             region_cells = {}\n\
             # sim::terrain::terrain_hash of the heights; enter_world compares the client's with the server's.\n\
             terrain_hash = \"{:016x}\"\n",
            self.format, self.region_cells, self.terrain_hash
        )
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let (mut format, mut region_cells, mut terrain_hash) = (None, None, None);
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || format!("line {}: expected `key = value`", n + 1);
            let (key, value) = line.split_once('=').ok_or_else(bad)?;
            let value = value.trim();
            match key.trim() {
                "format" => format = Some(value.parse().map_err(|_| bad())?),
                "region_cells" => region_cells = Some(value.parse().map_err(|_| bad())?),
                "terrain_hash" => {
                    let hex = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).ok_or_else(bad)?;
                    terrain_hash = Some(u64::from_str_radix(hex, 16).map_err(|_| bad())?);
                }
                k => return Err(format!("line {}: unknown key `{k}`", n + 1)),
            }
        }
        match (format, region_cells, terrain_hash) {
            (Some(format), Some(region_cells), Some(terrain_hash)) => Ok(Self { format, region_cells, terrain_hash }),
            _ => Err("needs format, region_cells and terrain_hash".into()),
        }
    }
}

/// `f` over `items` on every core, results in order.
fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let per = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(per).map(|part| s.spawn(|| part.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker panicked")).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::terrain::encode;

    #[test]
    fn land_chunks_skip_wild_temperate_cells_and_match_the_hash() {
        let mut w = World::flat(0);
        assert!(w.land_chunks().is_empty());
        assert_eq!(w.land_hash(), 0);
        // The texel at (x 5, z 70) m: cell (125, 126), texel (1, 1) in it.
        let (i, j) = (((5.0 + WORLD_HALF_M) / TEXEL_M) as usize, ((70.0 + WORLD_HALF_M) / TEXEL_M) as usize);
        w.land[j * LAND_SIDE + i] = texel(LandUse::Housing, Climate::Arid);
        let chunks = w.land_chunks();
        assert_eq!(chunks.len(), 1);
        let (cell, t) = &chunks[0];
        assert_eq!(*cell, sim::cell::cell_of(sim::cell::OPEN_WORLD, 5.0, 70.0));
        assert_eq!(t.len(), sim::land::LAND_CHUNK_LEN);
        assert_eq!(t.iter().position(|&v| v != 0), Some(TEXELS_PER_CELL + 1));
        assert_eq!(w.land_hash(), sim::land::land_chunk_hash(*cell, t));
        assert_eq!(&land_chunk(&w, *cell), t);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("worldtool-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A world with a ramp and a few marked texels, to tell regions apart.
    fn sample_world() -> World {
        let mut w = World::flat(encode(-20.0));
        let (width, _) = w.heights.size();
        for (k, s) in w.heights.samples.iter_mut().enumerate() {
            *s = encode(((k % width) as f32 * 0.01).sin() * 40.0 + (k / width) as f32 * 0.003);
        }
        w.land[0] = texel(LandUse::Town, Climate::Arid);
        w.land[LAND_SIDE * LAND_SIDE - 1] = texel(LandUse::Housing, Climate::Subarctic);
        w.land[1234 * LAND_SIDE + 2345] = texel(LandUse::Road, Climate::Tropical);
        w
    }

    #[test]
    fn worlds_round_trip_through_a_directory() {
        let dir = scratch("round-trip");
        let w = sample_world();
        let hash = w.save(&dir).unwrap();
        assert_eq!(std::fs::read_dir(dir.join("regions")).unwrap().count(), 625);
        let back = World::load(&dir).unwrap();
        assert_eq!(back.heights, w.heights);
        assert!(back.land == w.land);
        assert_eq!(back.terrain_hash(), hash);
        assert_eq!(back.land_at(-WORLD_HALF_M + 1.0, -WORLD_HALF_M + 1.0), Some(texel(LandUse::Town, Climate::Arid)));
        assert_eq!(back.land_at(2345.0 * TEXEL_M - WORLD_HALF_M + 3.9, 1234.0 * TEXEL_M - WORLD_HALF_M), Some(texel(LandUse::Road, Climate::Tropical)));
        assert_eq!(back.land_at(WORLD_HALF_M + 0.1, 0.0), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_terrain_hash_matches_the_chunks_the_server_gets() {
        let w = sample_world();
        let chunks = w.heights.chunks();
        assert_eq!(w.terrain_hash(), sim::terrain::terrain_hash(chunks.iter().map(|(c, s)| (*c, s.as_slice()))));
    }

    #[test]
    fn a_changed_region_or_manifest_fails_to_load() {
        let dir = scratch("tamper");
        sample_world().save(&dir).unwrap();
        // Heights changed inside one region: the hash no longer matches.
        let path = dir.join("regions").join(region_name(3, 4));
        let mut r = Region::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        let side = r.side();
        r.heights[side * 5 + 5] ^= 1;
        std::fs::write(&path, r.to_bytes()).unwrap();
        assert!(World::load(&dir).err().expect("should fail").contains("hash"));
        // A shared edge that disagrees with the neighbour's.
        r.heights[side * 5 + 5] ^= 1;
        r.heights[side * 5] ^= 1;
        std::fs::write(&path, r.to_bytes()).unwrap();
        assert!(World::load(&dir).err().expect("should fail").contains("neighbour"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(World::load(&dir).is_err());
    }

    #[test]
    fn manifests_round_trip_and_bad_ones_fail() {
        let m = Manifest { format: 1, region_cells: 10, terrain_hash: 0x0123_4567_89ab_cdef };
        assert_eq!(Manifest::parse(&m.to_text()).unwrap(), m);
        assert!(Manifest::parse("format = 1\nregion_cells = 10\n").is_err());
        assert!(Manifest::parse("format = 1\nregion_cells = 10\nterrain_hash = 12\n").is_err());
        assert!(Manifest::parse("format = 1\nsize = 3\n").unwrap_err().contains("unknown key"));
    }

    #[test]
    fn texels_pack_use_and_climate() {
        let t = texel(LandUse::Road, Climate::Subarctic);
        assert_eq!((texel_use(t), texel_climate(t)), (Some(LandUse::Road), Some(Climate::Subarctic)));
        assert_eq!(texel_use(0x0f), None);
    }

    /// The world in the repo loads, matches its manifest, and has what spawn expects.
    #[test]
    fn the_committed_world_is_consistent() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(DEFAULT_DIR);
        let w = World::load(&dir).unwrap();
        assert!(sim::terrain::decode(w.heights.samples[w.heights.size().0 * 4000 + 4000]) > 5.0, "spawn is on land");
        assert_eq!(w.land_at(0.0, 0.0).and_then(texel_use), Some(LandUse::Town));
        // Housing on both continents (at least 1 km² each), and every climate somewhere.
        let mut housing = [0usize; 2];
        let mut climates = [false; 4];
        for (k, &t) in w.land.iter().enumerate() {
            if texel_use(t) == Some(LandUse::Housing) {
                housing[usize::from(k % LAND_SIDE >= LAND_SIDE / 2)] += 1;
            }
            climates[texel_climate(t).unwrap() as usize] = true;
        }
        assert!(housing.iter().all(|&n| n as f32 * TEXEL_M * TEXEL_M > 1e6), "{housing:?}");
        assert_eq!(climates, [true; 4]);
    }
}
