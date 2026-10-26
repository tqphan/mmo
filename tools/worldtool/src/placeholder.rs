//! A placeholder for the whole 16 km map, laid out as PLAN §9 sketches it: two continents either
//! side of a central sea with three islands, a band of land along the north edge, and ocean round
//! the outside, with placeholder land use (docs/design/world.md). It writes `world/` until there is
//! editor tooling; after that, regenerating would overwrite hand edits.

use sim::cell::WORLD_HALF_M;
use sim::content::{Climate, LandUse};
use sim::terrain::{SPACING_M, decode, encode};

use crate::{LAND_SIDE, TEXEL_M, World, texel};

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

/// Housing areas, three on each continent's lowlands: centre and radius in metres.
const HOUSING: [(f32, f32, f32); 6] = [
    (-5600.0, -2000.0, 600.0),
    (-4300.0, 900.0, 500.0),
    (-5300.0, 4300.0, 500.0),
    (5600.0, -2000.0, 600.0),
    (4300.0, 900.0, 500.0),
    (5300.0, 4300.0, 500.0),
];
/// The trade island's town, round spawn.
const TOWN_RADIUS_M: f32 = 250.0;

/// The placeholder climate at a position.
pub fn climate(x: f32, z: f32) -> Climate {
    if z < -4400.0 {
        Climate::Subarctic
    } else if z > 1500.0 && x < -2500.0 {
        Climate::Tropical
    } else if z > 1500.0 && x > 2500.0 {
        Climate::Arid
    } else {
        Climate::Temperate
    }
}

/// The placeholder use of a texel centred on `(x, z)`, from its mean height and steepest slope.
fn land_use(x: f32, z: f32, height: f32, slope: f32) -> LandUse {
    if height > 0.0 && x.hypot(z) < TOWN_RADIUS_M {
        LandUse::Town
    } else if (2.0..60.0).contains(&height) && slope < 0.25 && HOUSING.iter().any(|&(hx, hz, r)| (x - hx).hypot(z - hz) < r) {
        LandUse::Housing
    } else {
        LandUse::Wild
    }
}

/// The whole map: heights on every core, a band of rows per thread, then land use from them.
pub fn generate() -> World {
    let mut w = World::flat(0);
    let (width, rows) = w.heights.size();
    let (x0, z0) = (-WORLD_HALF_M, -WORLD_HALF_M);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let band = rows.div_ceil(threads);
    std::thread::scope(|s| {
        for (b, part) in w.heights.samples.chunks_mut(band * width).enumerate() {
            s.spawn(move || {
                for (i, v) in part.iter_mut().enumerate() {
                    let (r, c) = (b * band + i / width, i % width);
                    *v = encode(world_height(x0 + c as f32 * SPACING_M, z0 + r as f32 * SPACING_M));
                }
            });
        }
    });
    let samples = &w.heights.samples;
    for (j, row) in w.land.chunks_mut(LAND_SIDE).enumerate() {
        for (i, t) in row.iter_mut().enumerate() {
            // A texel covers 2 × 2 quads: 3 × 3 samples from (2i, 2j).
            let at = |di: usize, dj: usize| decode(samples[(2 * j + dj) * width + 2 * i + di]);
            let (mut sum, mut lo, mut hi) = (0.0, f32::MAX, f32::MIN);
            for dj in 0..3 {
                for di in 0..3 {
                    let h = at(di, dj);
                    (sum, lo, hi) = (sum + h, lo.min(h), hi.max(h));
                }
            }
            let (x, z) = (x0 + (i as f32 + 0.5) * TEXEL_M, z0 + (j as f32 + 0.5) * TEXEL_M);
            *t = texel(land_use(x, z, sum / 9.0, (hi - lo) / TEXEL_M), climate(x, z));
        }
    }
    w
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
    fn climates_by_region() {
        assert_eq!(climate(0.0, -6000.0), Climate::Subarctic);
        assert_eq!(climate(-5200.0, 3000.0), Climate::Tropical);
        assert_eq!(climate(5200.0, 3000.0), Climate::Arid);
        assert_eq!(climate(-5200.0, -1000.0), Climate::Temperate);
        assert_eq!(climate(0.0, 0.0), Climate::Temperate);
    }

    #[test]
    fn land_use_rules() {
        assert_eq!(land_use(10.0, 10.0, 13.0, 0.1), LandUse::Town);
        assert_eq!(land_use(10.0, 10.0, -1.0, 0.0), LandUse::Wild, "the sea round spawn");
        assert_eq!(land_use(-5600.0, -2000.0, 20.0, 0.1), LandUse::Housing);
        assert_eq!(land_use(-5600.0, -2000.0, 20.0, 0.3), LandUse::Wild, "too steep");
        assert_eq!(land_use(-5600.0, -2000.0, 80.0, 0.1), LandUse::Wild, "too high");
        assert_eq!(land_use(-5600.0, -2000.0, 1.0, 0.1), LandUse::Wild, "the beach");
        assert_eq!(land_use(-3000.0, -2000.0, 20.0, 0.1), LandUse::Wild, "outside the housing areas");
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
