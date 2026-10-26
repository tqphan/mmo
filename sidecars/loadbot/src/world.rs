//! A placeholder for the whole 16 km map, laid out as PLAN §9 sketches it, for the world-scale
//! spike (docs/spikes/world-scale.md): two continents either side of a central sea with three
//! islands, a band of land along the north edge, and ocean round the outside. Written as one
//! `.mmoh` height file that the client draws and walks on, and the server checks moves against.
//! The real world comes from the editor through the world exporter.

use sim::cell::{CELLS_PER_SIDE, WORLD_HALF_M};
use sim::terrain::{HeightFile, SPACING_M, encode};

use crate::Check;

pub const DEFAULT_FILE: &str = "target/world.mmoh";

/// A landmass: a rounded rectangle (`hx`, `hz` half-sizes, corner radius `r`) and how high its
/// mountains get. North is -z.
struct Land {
    x: f32,
    z: f32,
    hx: f32,
    hz: f32,
    r: f32,
    peaks: f32,
}

const LANDS: [Land; 6] = [
    // West and east continents, 4.4 × 9 km.
    Land { x: -5200.0, z: 1200.0, hx: 2200.0, hz: 4500.0, r: 1200.0, peaks: 320.0 },
    Land { x: 5200.0, z: 1200.0, hx: 2200.0, hz: 4500.0, r: 1200.0, peaks: 320.0 },
    // The north, 10 × 3 km along the top edge.
    Land { x: 0.0, z: -5900.0, hx: 5000.0, hz: 1400.0, r: 900.0, peaks: 420.0 },
    // Central sea islands: trade, battleground, sanctum. Spawn (0, 0) is on the trade island, about
    // 13 m up, until the world has real spawn points.
    Land { x: 0.0, z: -100.0, hx: 650.0, hz: 550.0, r: 450.0, peaks: 60.0 },
    Land { x: -900.0, z: 4300.0, hx: 500.0, hz: 400.0, r: 350.0, peaks: 40.0 },
    Land { x: 1100.0, z: -2300.0, hx: 380.0, hz: 380.0, r: 300.0, peaks: 90.0 },
];

fn hash(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Smooth value noise in 0..1 with features about `scale` metres across.
fn noise(x: f32, z: f32, scale: f32, seed: u32) -> f32 {
    let (fx, fz) = (x / scale, z / scale);
    let (ix, iz) = (fx.floor(), fz.floor());
    let (tx, tz) = (fx - ix, fz - iz);
    let (sx, sz) = (tx * tx * (3.0 - 2.0 * tx), tz * tz * (3.0 - 2.0 * tz));
    let (ix, iz) = (ix as i32, iz as i32);
    let a = hash(ix, iz, seed) + (hash(ix + 1, iz, seed) - hash(ix, iz, seed)) * sx;
    let b = hash(ix, iz + 1, seed) + (hash(ix + 1, iz + 1, seed) - hash(ix, iz + 1, seed)) * sx;
    a + (b - a) * sz
}

/// Fractal noise in about -0.5..0.5.
fn fbm(x: f32, z: f32, scale: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut s) = (0.0, 0.5, scale);
    for o in 0..octaves {
        sum += (noise(x, z, s, seed + o) - 0.5) * amp;
        amp *= 0.5;
        s *= 0.5;
    }
    sum
}

/// Ridged noise in 0..1: sharp crests for mountain ranges.
fn ridged(x: f32, z: f32, scale: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut s, mut norm) = (0.0, 0.5, scale, 0.0);
    for o in 0..octaves {
        let n = 1.0 - (noise(x, z, s, seed + o) * 2.0 - 1.0).abs();
        sum += n * n * amp;
        norm += amp;
        amp *= 0.5;
        s *= 0.5;
    }
    sum / norm
}

fn smoothstep(edge0: f32, edge1: f32, v: f32) -> f32 {
    let t = ((v - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance inside the land's outline in metres (negative outside).
fn inland(l: &Land, x: f32, z: f32) -> f32 {
    let qx = (x - l.x).abs() - (l.hx - l.r);
    let qz = (z - l.z).abs() - (l.hz - l.r);
    let outside = (qx.max(0.0).powi(2) + qz.max(0.0).powi(2)).sqrt();
    l.r - (outside + qx.max(qz).min(0.0))
}

/// Height in metres at a world position; sea level is 0.
pub fn world_height(x: f32, z: f32) -> f32 {
    // Warp the coastlines so they wander: bays, headlands and offshore rocks.
    let wx = x + 1800.0 * fbm(x, z, 3000.0, 4, 11);
    let wz = z + 1800.0 * fbm(x, z, 3000.0, 4, 23);
    let coast = 600.0 * fbm(x, z, 1100.0, 5, 37);
    let mut height = f32::MIN;
    for l in &LANDS {
        let d = inland(l, wx, wz) + coast;
        // Beaches and lowland rise to about 25 m over the first 600 m inland, then hills and
        // mountains grow with distance from the sea.
        let low = 25.0 * smoothstep(-40.0, 600.0, d) + 14.0 * fbm(x, z, 400.0, 4, 41) * smoothstep(0.0, 300.0, d);
        // Ranges only where a broad mask allows, so there are plains and valleys between them.
        let mask = smoothstep(0.0, 0.25, fbm(x, z, 3500.0, 3, 61) + 0.05);
        let range = l.peaks * mask * smoothstep(300.0, 2000.0, d.min(l.hx.min(l.hz))) * ridged(x, z, 1800.0, 5, 53);
        let land = low + range;
        // The shelf drops to 40 m within 700 m of the coast, then to a 60 m floor.
        let sea = -40.0 * smoothstep(0.0, -700.0, d) - 20.0 * smoothstep(-700.0, -3000.0, d);
        let h = if d >= 0.0 { land } else { sea };
        height = height.max(h);
    }
    height
}

/// Write the whole map (250 × 250 cells, 8001² samples, 128 MB) to `file`, a band of rows per thread.
pub fn write(file: &str) -> Check {
    let mut f = HeightFile { cell_x: 0, cell_z: 0, cells_x: CELLS_PER_SIDE, cells_z: CELLS_PER_SIDE, samples: Vec::new() };
    let (w, h) = f.size();
    let (x0, z0) = (-WORLD_HALF_M, -WORLD_HALF_M);
    f.samples = vec![0; w * h];
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let band = h.div_ceil(threads);
    std::thread::scope(|s| {
        for (b, rows) in f.samples.chunks_mut(band * w).enumerate() {
            s.spawn(move || {
                for (i, v) in rows.iter_mut().enumerate() {
                    let (r, c) = (b * band + i / w, i % w);
                    *v = encode(world_height(x0 + c as f32 * SPACING_M, z0 + r as f32 * SPACING_M));
                }
            });
        }
    });
    if let Some(dir) = std::path::Path::new(file).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, f.to_bytes()).map_err(|e| format!("{file}: {e}"))?;
    let land = f.samples.iter().filter(|&&s| s > encode(0.0)).count();
    println!("wrote the {} km world to {file}: {:.0}% land", 2.0 * WORLD_HALF_M / 1000.0, 100.0 * land as f32 / f.samples.len() as f32);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continents_islands_and_ocean() {
        assert!(world_height(-5200.0, 1200.0) > 20.0, "west continent");
        assert!(world_height(5200.0, 1200.0) > 20.0, "east continent");
        assert!(world_height(0.0, -6000.0) > 20.0, "the north");
        assert!(world_height(0.0, 0.0) > 5.0, "trade island, under spawn");
        assert!(world_height(-2000.0, 3000.0) < -10.0, "central sea");
        assert!(world_height(0.0, 7900.0) < -10.0, "outer ocean");
        assert!(world_height(-7950.0, 0.0) < -2.0, "west edge");
    }

    #[test]
    fn heights_fit_the_file_format() {
        for i in 0..2000 {
            let (x, z) = ((i * 7919 % 16000) as f32 - 8000.0, (i * 104729 % 16000) as f32 - 8000.0);
            let h = world_height(x, z);
            assert!((-100.0..600.0).contains(&h), "{h} at {x},{z}");
        }
    }
}
