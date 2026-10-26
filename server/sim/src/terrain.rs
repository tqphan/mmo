//! Server-side terrain: a 2 m heightmap in 64 m chunks, ground height, slope and line of sight
//! (PLAN §5.4, docs/design/terrain.md).
//!
//! The client collides with triangles, and the server shouldn't depend on which diagonal it splits
//! each quad along, so every reading reports both triangulations and the checks take the kinder one.

use crate::cell::{CELL_SIZE_M, CELLS_PER_SIDE, OPEN_WORLD, WORLD_HALF_M, WORLD_SIZE_M, pack, unpack};

pub const SPACING_M: f32 = 2.0;
/// Quads along one side of a chunk; a chunk holds one more sample than this per side.
pub const QUADS_PER_CHUNK: usize = (CELL_SIZE_M / SPACING_M) as usize;
pub const CHUNK_SIDE: usize = QUADS_PER_CHUNK + 1;
pub const CHUNK_LEN: usize = CHUNK_SIDE * CHUNK_SIDE;
/// Quads along one side of the map.
const QUADS_PER_SIDE: usize = QUADS_PER_CHUNK * CELLS_PER_SIDE as usize;

/// Height = raw × `HEIGHT_STEP_M` + `HEIGHT_BASE_M`: −512 m to +1536 m in 3.125 cm steps.
pub const HEIGHT_STEP_M: f32 = 1.0 / 32.0;
pub const HEIGHT_BASE_M: f32 = -512.0;

/// Line of sight is sampled at least this often along a segment.
pub const LOS_STEP_M: f32 = 1.0;

pub fn decode(raw: u16) -> f32 {
    raw as f32 * HEIGHT_STEP_M + HEIGHT_BASE_M
}

/// The nearest sample to `height_m`, clamped to the range.
pub fn encode(height_m: f32) -> u16 {
    ((height_m - HEIGHT_BASE_M) / HEIGHT_STEP_M).round().clamp(0.0, u16::MAX as f32) as u16
}

/// Where chunks come from: the database in the module, a map in tests and tools.
pub trait Chunks {
    /// The chunk for an open-world cell key, `CHUNK_LEN` samples, if that cell has terrain.
    fn chunk(&mut self, cell: u64) -> Option<&[u16]>;
}

impl Chunks for std::collections::HashMap<u64, Vec<u16>> {
    fn chunk(&mut self, cell: u64) -> Option<&[u16]> {
        self.get(&cell).map(Vec::as_slice)
    }
}

/// Which way a quad is split into two triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagonal {
    /// From the quad's (−x, −z) corner to its (+x, +z) corner.
    Main,
    /// From (+x, −z) to (−x, +z).
    Anti,
}

/// The four corner heights of the 2 m quad around a point, and where the point lies in it (0 to 1).
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    /// `h[z][x]`: `h[0][0]` at (−x, −z), `h[0][1]` at (+x, −z), `h[1][0]` at (−x, +z).
    pub h: [[f32; 2]; 2],
    pub u: f32,
    pub v: f32,
}

impl Quad {
    /// Height and gradient (rise over run) of the triangle under the point.
    pub fn triangle(&self, diag: Diagonal) -> (f32, f32) {
        let [[h00, h10], [h01, h11]] = self.h;
        let (u, v, s) = (self.u, self.v, SPACING_M);
        let (height, dx, dz) = match diag {
            Diagonal::Main if u >= v => (h00 + u * (h10 - h00) + v * (h11 - h10), h10 - h00, h11 - h10),
            Diagonal::Main => (h00 + v * (h01 - h00) + u * (h11 - h01), h11 - h01, h01 - h00),
            Diagonal::Anti if u + v <= 1.0 => (h00 + u * (h10 - h00) + v * (h01 - h00), h10 - h00, h01 - h00),
            Diagonal::Anti => (h11 + (1.0 - u) * (h01 - h11) + (1.0 - v) * (h10 - h11), h11 - h01, h11 - h10),
        };
        (height, (dx * dx + dz * dz).sqrt() / s)
    }
}

/// The ground under a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    /// The lower and higher of the two triangulations' heights.
    pub lo: f32,
    pub hi: f32,
    /// The gentler of the two triangles' slopes, rise over run.
    pub gradient: f32,
}

/// The quad around `(x, z)`, or `None` off the map or where the cell has no terrain.
pub fn quad(src: &mut impl Chunks, x: f32, z: f32) -> Option<Quad> {
    let axis = |c: f32| {
        let g = (c + WORLD_HALF_M) / SPACING_M;
        if !(0.0..=WORLD_SIZE_M / SPACING_M).contains(&g) {
            return None;
        }
        let q = (g.floor() as usize).min(QUADS_PER_SIDE - 1);
        Some((q, g - q as f32))
    };
    let ((qx, u), (qz, v)) = (axis(x)?, axis(z)?);
    let cell = pack(OPEN_WORLD, (qz / QUADS_PER_CHUNK) as u16, (qx / QUADS_PER_CHUNK) as u16);
    let samples = src.chunk(cell)?;
    let (lx, lz) = (qx % QUADS_PER_CHUNK, qz % QUADS_PER_CHUNK);
    let at = |dx: usize, dz: usize| decode(samples[(lz + dz) * CHUNK_SIDE + lx + dx]);
    Some(Quad { h: [[at(0, 0), at(1, 0)], [at(0, 1), at(1, 1)]], u, v })
}

pub fn ground(src: &mut impl Chunks, x: f32, z: f32) -> Option<Ground> {
    let q = quad(src, x, z)?;
    let (a, ga) = q.triangle(Diagonal::Main);
    let (b, gb) = q.triangle(Diagonal::Anti);
    Some(Ground { lo: a.min(b), hi: a.max(b), gradient: ga.min(gb) })
}

/// Whether nothing in the terrain blocks the segment from `a` to `b` (both `[x, y, z]`). The ground
/// is sampled at least every `LOS_STEP_M` along it, and only blocks where the segment dips below
/// both triangulations. Unknown terrain blocks nothing.
pub fn line_of_sight(src: &mut impl Chunks, a: [f32; 3], b: [f32; 3]) -> bool {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (d[0] * d[0] + d[2] * d[2]).sqrt();
    if !len.is_finite() {
        return false;
    }
    let steps = (len / LOS_STEP_M).ceil().max(1.0) as usize;
    (1..steps).all(|k| {
        let t = k as f32 / steps as f32;
        let p = [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t];
        ground(src, p[0], p[2]).is_none_or(|g| p[1] >= g.lo)
    })
}

/// A rectangle of whole cells of terrain, uploaded by an admin: test terrain, or the world put
/// together from its region files (`tools/worldtool`)
/// (`.mmoh`, docs/design/terrain.md).
#[derive(Clone, Debug, PartialEq)]
pub struct HeightFile {
    /// The first cell along x and along z.
    pub cell_x: u16,
    pub cell_z: u16,
    /// Cells across (x) and down (z).
    pub cells_x: u16,
    pub cells_z: u16,
    /// `(cells_z·32+1)` rows of `(cells_x·32+1)` samples, z increasing.
    pub samples: Vec<u16>,
}

const MAGIC: &[u8; 4] = b"MMOH";
const VERSION: u16 = 1;

impl HeightFile {
    /// A file covering `cells_x × cells_z` cells from `(cell_x, cell_z)`, with heights from `height(x, z)`.
    pub fn generate(cell_x: u16, cell_z: u16, cells_x: u16, cells_z: u16, height: impl Fn(f32, f32) -> f32) -> Self {
        let mut f = Self { cell_x, cell_z, cells_x, cells_z, samples: Vec::new() };
        let (w, h) = f.size();
        let (x0, z0) = f.origin();
        f.samples = (0..h)
            .flat_map(|r| (0..w).map(move |c| (c, r)))
            .map(|(c, r)| encode(height(x0 + c as f32 * SPACING_M, z0 + r as f32 * SPACING_M)))
            .collect();
        f
    }

    /// Samples across and down.
    pub fn size(&self) -> (usize, usize) {
        (self.cells_x as usize * QUADS_PER_CHUNK + 1, self.cells_z as usize * QUADS_PER_CHUNK + 1)
    }

    /// World position of the first sample.
    pub fn origin(&self) -> (f32, f32) {
        let at = |c: u16| c as f32 * CELL_SIZE_M - WORLD_HALF_M;
        (at(self.cell_x), at(self.cell_z))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for v in [VERSION, self.cell_x, self.cell_z, self.cells_x, self.cells_z] {
            out.extend(v.to_le_bytes());
        }
        out.extend(self.samples.iter().flat_map(|s| s.to_le_bytes()));
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 14 || &bytes[..4] != MAGIC {
            return Err("not a .mmoh height file".into());
        }
        let word = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        if word(4) != VERSION {
            return Err(format!("height file version {}, expected {VERSION}", word(4)));
        }
        let mut f = Self { cell_x: word(6), cell_z: word(8), cells_x: word(10), cells_z: word(12), samples: Vec::new() };
        if f.cells_x == 0
            || f.cells_z == 0
            || f.cell_x as u32 + f.cells_x as u32 > CELLS_PER_SIDE as u32
            || f.cell_z as u32 + f.cells_z as u32 > CELLS_PER_SIDE as u32
        {
            return Err("height file region is empty or off the map".into());
        }
        let (w, h) = f.size();
        if bytes.len() != 14 + 2 * w * h {
            return Err(format!("height file has {} bytes, expected {}", bytes.len(), 14 + 2 * w * h));
        }
        f.samples = bytes[14..].as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        Ok(f)
    }

    /// One `(cell key, CHUNK_LEN samples)` per cell, the shared edges repeated.
    pub fn chunks(&self) -> Vec<(u64, Vec<u16>)> {
        let mut out = Vec::new();
        for cz in 0..self.cells_z as usize {
            for cx in 0..self.cells_x as usize {
                let key = pack(OPEN_WORLD, self.cell_z + cz as u16, self.cell_x + cx as u16);
                out.push((key, self.chunk_at(cx, cz)));
            }
        }
        out
    }

    /// The samples of the file's cell `(cx, cz)`, counted from its first cell.
    fn chunk_at(&self, cx: usize, cz: usize) -> Vec<u16> {
        let (w, _) = self.size();
        (0..CHUNK_SIDE)
            .flat_map(|r| {
                let start = (cz * QUADS_PER_CHUNK + r) * w + cx * QUADS_PER_CHUNK;
                self.samples[start..start + CHUNK_SIDE].iter().copied()
            })
            .collect()
    }
}

/// A height file read as chunks, each cut out the first time it's asked for. For tools that need
/// the ground in a few places on a big map without splitting all of it.
pub struct FileChunks<'a> {
    file: &'a HeightFile,
    cut: std::collections::HashMap<u64, Vec<u16>>,
}

impl<'a> FileChunks<'a> {
    pub fn new(file: &'a HeightFile) -> Self {
        Self { file, cut: Default::default() }
    }
}

impl Chunks for FileChunks<'_> {
    fn chunk(&mut self, cell: u64) -> Option<&[u16]> {
        let (instance, cz, cx) = unpack(cell);
        let f = self.file;
        let (dx, dz) = (cx.checked_sub(f.cell_x)?, cz.checked_sub(f.cell_z)?);
        if instance != OPEN_WORLD || dx >= f.cells_x || dz >= f.cells_z {
            return None;
        }
        Some(self.cut.entry(cell).or_insert_with(|| f.chunk_at(dx as usize, dz as usize)))
    }
}

/// One chunk's part of the terrain hash (docs/design/world.md): FNV-1a 64 over the cell key and
/// the samples, little-endian, then the splitmix64 finaliser so that sums of hashes mix well.
pub fn chunk_hash(cell: u64, samples: &[u16]) -> u64 {
    let fnv = |h: u64, b: u8| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    let mut h = cell.to_le_bytes().into_iter().fold(0xcbf2_9ce4_8422_2325, fnv);
    for s in samples {
        h = s.to_le_bytes().into_iter().fold(h, fnv);
    }
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

/// The terrain hash: the wrapping sum of every chunk's `chunk_hash`, so it doesn't depend on order
/// and can follow uploads one chunk at a time. No terrain is 0.
pub fn terrain_hash<'a>(chunks: impl IntoIterator<Item = (u64, &'a [u16])>) -> u64 {
    chunks.into_iter().fold(0, |sum, (cell, samples)| sum.wrapping_add(chunk_hash(cell, samples)))
}

/// Whether `cell` can hold a chunk: an open-world cell on the map.
pub fn valid_chunk_cell(cell: u64) -> bool {
    let (instance, cy, cx) = unpack(cell);
    instance == OPEN_WORLD && cy < CELLS_PER_SIDE && cx < CELLS_PER_SIDE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::cell_of;
    use std::collections::HashMap;

    fn terrain(f: &HeightFile) -> HashMap<u64, Vec<u16>> {
        f.chunks().into_iter().collect()
    }

    /// 2×2 cells starting at the origin cell, heights from `h`.
    fn around_origin(h: impl Fn(f32, f32) -> f32) -> HashMap<u64, Vec<u16>> {
        terrain(&HeightFile::generate(125, 125, 2, 2, h))
    }

    #[test]
    fn heights_round_trip_to_the_step() {
        for h in [-512.0, -3.3, 0.0, 12.345, 1535.9] {
            assert!((decode(encode(h)) - h).abs() <= HEIGHT_STEP_M / 2.0, "{h}");
        }
        assert_eq!(encode(-1.0e6), 0);
        assert_eq!(encode(1.0e6), u16::MAX);
    }

    #[test]
    fn a_tilted_plane_is_exact_under_both_diagonals() {
        // 0.5 m per metre in x, 0.25 in z: every quad is planar.
        let mut t = around_origin(|x, z| 10.0 + 0.5 * x + 0.25 * z);
        for (x, z) in [(0.0, 0.0), (3.3, 7.9), (63.99, 64.01), (100.0, 1.0)] {
            let g = ground(&mut t, x, z).unwrap();
            let want = 10.0 + 0.5 * x + 0.25 * z;
            assert!((g.lo - want).abs() < 0.03 && (g.hi - want).abs() < 0.03, "({x}, {z}): {g:?}");
            assert!((g.gradient - (0.5f32 * 0.5 + 0.25 * 0.25).sqrt()).abs() < 0.01);
        }
    }

    #[test]
    fn a_twisted_quad_brackets_bilinear() {
        // A saddle: the diagonals disagree by a quarter of the twist at the quad's centre.
        let q = Quad { h: [[0.0, 4.0], [4.0, 0.0]], u: 0.5, v: 0.5 };
        let (main, _) = q.triangle(Diagonal::Main);
        let (anti, _) = q.triangle(Diagonal::Anti);
        assert_eq!((main, anti), (0.0, 4.0));
        let bilinear = 2.0; // the mean of the corners at the centre
        assert!(main.min(anti) < bilinear && bilinear < main.max(anti));
    }

    #[test]
    fn triangles_meet_at_the_corners() {
        let q = |u, v| Quad { h: [[1.0, 2.0], [3.0, 5.0]], u, v };
        for diag in [Diagonal::Main, Diagonal::Anti] {
            assert_eq!(q(0.0, 0.0).triangle(diag).0, 1.0);
            assert_eq!(q(1.0, 0.0).triangle(diag).0, 2.0);
            assert_eq!(q(0.0, 1.0).triangle(diag).0, 3.0);
            assert_eq!(q(1.0, 1.0).triangle(diag).0, 5.0);
        }
    }

    #[test]
    fn chunk_edges_agree() {
        let mut t = around_origin(|x, z| (x * 0.1).sin() * 5.0 + (z * 0.07).cos() * 3.0);
        // Just either side of the x = 64 m border, and exactly on it.
        let a = ground(&mut t, 63.999, 20.0).unwrap();
        let b = ground(&mut t, 64.0, 20.0).unwrap();
        let c = ground(&mut t, 64.001, 20.0).unwrap();
        assert!((a.hi - b.hi).abs() < 0.01 && (b.hi - c.hi).abs() < 0.01);
        assert_ne!(cell_of(OPEN_WORLD, 63.999, 20.0), cell_of(OPEN_WORLD, 64.0, 20.0));
    }

    #[test]
    fn missing_chunks_and_off_map_are_unknown() {
        let mut t = around_origin(|_, _| 5.0);
        assert!(ground(&mut t, 10.0, 10.0).is_some());
        assert!(ground(&mut t, -10.0, 10.0).is_none());
        assert!(ground(&mut t, 200.0, 10.0).is_none());
        let mut edge = terrain(&HeightFile::generate(CELLS_PER_SIDE - 1, 0, 1, 1, |_, _| 1.0));
        assert!(ground(&mut edge, WORLD_HALF_M, -WORLD_HALF_M).is_some());
        assert!(ground(&mut edge, WORLD_HALF_M + 0.1, -WORLD_HALF_M).is_none());
        assert!(ground(&mut edge, f32::NAN, 0.0).is_none());
    }

    /// Flat at 0 m with a 10 m ridge along x = 50.
    fn ridge() -> HashMap<u64, Vec<u16>> {
        around_origin(|x, _| if (48.0..=52.0).contains(&x) { 10.0 } else { 0.0 })
    }

    #[test]
    fn sight_over_flat_ground_is_clear() {
        let mut t = ridge();
        assert!(line_of_sight(&mut t, [10.0, 1.6, 10.0], [40.0, 1.0, 30.0]));
    }

    #[test]
    fn a_ridge_blocks_sight_unless_you_are_above_it() {
        let mut t = ridge();
        assert!(!line_of_sight(&mut t, [30.0, 1.6, 10.0], [70.0, 1.0, 10.0]));
        assert!(!line_of_sight(&mut t, [70.0, 1.6, 10.0], [30.0, 1.0, 10.0]));
        assert!(line_of_sight(&mut t, [30.0, 12.0, 10.0], [70.0, 11.0, 10.0]));
    }

    #[test]
    fn unknown_terrain_does_not_block() {
        let mut t = ridge();
        assert!(line_of_sight(&mut t, [-40.0, 1.0, 10.0], [-10.0, 1.0, 10.0]));
        assert!(!line_of_sight(&mut t, [0.0, f32::NAN, 0.0], [f32::INFINITY, 1.0, 10.0]));
    }

    #[test]
    fn height_files_round_trip_and_split_into_chunks() {
        let f = HeightFile::generate(10, 20, 3, 2, |x, z| x * 0.01 + z * 0.02);
        assert_eq!(f.size(), (97, 65));
        assert_eq!(HeightFile::from_bytes(&f.to_bytes()).unwrap(), f);

        let chunks = f.chunks();
        assert_eq!(chunks.len(), 6);
        assert!(chunks.iter().all(|(_, s)| s.len() == CHUNK_LEN));
        // The second chunk of the second row starts at sample (32, 32) and shares its left edge.
        let (key, s) = &chunks[4];
        assert_eq!(unpack(*key), (OPEN_WORLD, 21, 11));
        assert_eq!(s[0], f.samples[32 * 97 + 32]);
        assert_eq!(s[CHUNK_LEN - 1], f.samples[64 * 97 + 64]);
        assert_eq!(chunks[3].1[CHUNK_SIDE - 1], s[0]);
    }

    #[test]
    fn bad_height_files_are_refused() {
        let good = HeightFile::generate(0, 0, 1, 1, |_, _| 0.0).to_bytes();
        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        assert!(HeightFile::from_bytes(&bad_magic).is_err());
        assert!(HeightFile::from_bytes(&good[..good.len() - 2]).is_err());
        let mut off_map = good.clone();
        off_map[6..8].copy_from_slice(&CELLS_PER_SIDE.to_le_bytes());
        assert!(HeightFile::from_bytes(&off_map).is_err());
        assert!(HeightFile::from_bytes(b"MMO").is_err());
    }

    #[test]
    fn file_chunks_cut_the_same_chunks_on_demand() {
        let f = HeightFile::generate(5, 7, 3, 2, |x, z| x * 0.1 - z * 0.05);
        let mut lazy = FileChunks::new(&f);
        for (key, samples) in f.chunks() {
            assert_eq!(lazy.chunk(key), Some(samples.as_slice()));
        }
        assert_eq!(lazy.chunk(pack(OPEN_WORLD, 7, 4)), None, "west of the file");
        assert_eq!(lazy.chunk(pack(OPEN_WORLD, 9, 5)), None, "south of the file");
        assert_eq!(lazy.chunk(pack(1, 7, 5)), None, "another instance");
        let (x0, z0) = f.origin();
        let g = ground(&mut lazy, x0 + 100.0, z0 + 50.0).unwrap();
        assert!((g.hi - ((x0 + 100.0) * 0.1 - (z0 + 50.0) * 0.05)).abs() < 0.05);
    }

    #[test]
    fn chunk_hashes_cover_the_cell_and_every_sample() {
        let f = HeightFile::generate(0, 0, 1, 1, |x, z| x * 0.3 + z * 0.1);
        let (cell, samples) = &f.chunks()[0];
        let h = chunk_hash(*cell, samples);
        assert_ne!(chunk_hash(cell + 1, samples), h);
        for i in [0, 500, CHUNK_LEN - 1] {
            let mut changed = samples.clone();
            changed[i] ^= 1;
            assert_ne!(chunk_hash(*cell, &changed), h, "sample {i}");
        }
    }

    #[test]
    fn the_terrain_hash_ignores_order_and_follows_replacements() {
        let f = HeightFile::generate(10, 20, 3, 2, |x, z| (x * 0.05).sin() * 9.0 + z * 0.01);
        let chunks = f.chunks();
        let all = terrain_hash(chunks.iter().map(|(c, s)| (*c, s.as_slice())));
        assert_eq!(terrain_hash(chunks.iter().rev().map(|(c, s)| (*c, s.as_slice()))), all);
        assert_eq!(terrain_hash([]), 0);
        // Swap one chunk's samples by subtracting the old hash and adding the new.
        let (cell, old) = &chunks[2];
        let new = vec![7u16; CHUNK_LEN];
        let followed = all.wrapping_sub(chunk_hash(*cell, old)).wrapping_add(chunk_hash(*cell, &new));
        let recomputed =
            terrain_hash(chunks.iter().map(|(c, s)| (*c, if c == cell { new.as_slice() } else { s.as_slice() })));
        assert_eq!(followed, recomputed);
        assert_ne!(followed, all);
    }

    #[test]
    fn only_open_world_cells_on_the_map_take_chunks() {
        assert!(valid_chunk_cell(pack(OPEN_WORLD, 0, CELLS_PER_SIDE - 1)));
        assert!(!valid_chunk_cell(pack(OPEN_WORLD, CELLS_PER_SIDE, 0)));
        assert!(!valid_chunk_cell(pack(1, 0, 0)));
    }
}
