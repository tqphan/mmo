//! Farming (docs/design/farming.md): when a planting is ripe or withered, where one may go, and
//! what a harvest gives, all from timestamps. Nothing is ticked. The client ports this for the
//! planting preview and the crop labels, and the server decides.

pub use crate::content::{CROPS, CropData, CropKind, FARM_IDEAL_PCT, FARM_REACH_M, crop, crop_by_id, crop_by_seed};
use crate::cell::WORLD_HALF_M;
use crate::content::LandUse;
use crate::land::{LandChunks, MIN_GROUND_M, Rect, TEXEL_M, texel};
use crate::terrain::{Chunks, SPACING_M};

const MICROS_PER_S: i64 = 1_000_000;

/// How long `secs` of growth takes, in µs: `FARM_IDEAL_PCT` of it in the ideal climate.
pub fn growth_micros(secs: u32, ideal: bool) -> i64 {
    let us = i64::from(secs) * MICROS_PER_S;
    if ideal { us * i64::from(FARM_IDEAL_PCT) / 100 } else { us }
}

/// When a planting of `kind` planted at `from` is first ripe.
pub fn ripe_at(kind: &CropData, from: i64, ideal: bool) -> i64 {
    from.saturating_add(growth_micros(kind.grow_s, ideal))
}

/// When a planting harvested at `from` is ripe again, if it regrows at all.
pub fn regrown_at(kind: &CropData, from: i64, ideal: bool) -> Option<i64> {
    (kind.regrow_s > 0).then(|| from.saturating_add(growth_micros(kind.regrow_s, ideal)))
}

/// When a planting ripe at `ripe_at` withers, if its kind does.
pub fn withers_at(kind: &CropData, ripe_at: i64) -> Option<i64> {
    (kind.wither_s > 0).then(|| ripe_at.saturating_add(i64::from(kind.wither_s) * MICROS_PER_S))
}

/// A planting's stage. The discriminant is the code in the client's vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Growing = 0,
    Ripe = 1,
    /// Left ripe too long: it gives nothing and only waits to be cleared.
    Withered = 2,
}

pub fn stage(kind: &CropData, ripe_at: i64, now: i64) -> Stage {
    if now < ripe_at {
        Stage::Growing
    } else if withers_at(kind, ripe_at).is_some_and(|w| now >= w) {
        Stage::Withered
    } else {
        Stage::Ripe
    }
}

/// How far a planting growing from `grown_from` to `ripe_at` has got at `now`, from 0 to 1.
pub fn progress(grown_from: i64, ripe_at: i64, now: i64) -> f32 {
    let total = ripe_at.saturating_sub(grown_from);
    if total <= 0 {
        return 1.0;
    }
    (now.saturating_sub(grown_from) as f64 / total as f64).clamp(0.0, 1.0) as f32
}

/// A character at `(x, z)` can plant, harvest or uproot at `(px, pz)`: horizontally within reach.
pub fn in_reach(px: f32, pz: f32, x: f32, z: f32) -> bool {
    let (dx, dz) = (x - px, z - pz);
    dx * dx + dz * dz <= FARM_REACH_M * FARM_REACH_M
}

/// The climate wire in a land-use texel (bits 4–6).
pub fn climate_of(texel: u8) -> u8 {
    (texel >> 4) & 0x07
}

/// Whether `(x, z)` is on the plot covering `r`, by the centre: west and north edges in, east and
/// south out, so a spot on a shared edge belongs to one plot.
pub fn on_plot(r: &Rect, x: f32, z: f32) -> bool {
    (r.x0..r.x1).contains(&x) && (r.z0..r.z1).contains(&z)
}

/// A standing plot near a spot, and whether the planter may tend plantings on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotSpot {
    pub rect: Rect,
    pub may_tend: bool,
}

/// Another planting that isn't gone: its centre and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planting {
    pub x: f32,
    pub z: f32,
    pub space_m: f32,
}

/// Where a planting may go: its ground, the index of the plot it's on in the list given, and
/// whether it's in its ideal climate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub y: f32,
    pub plot: Option<usize>,
    pub ideal: bool,
}

/// Why a planting can't go somewhere. The discriminant is the code in the client's vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpotError {
    OffMap = 1,
    NotPlantable = 2,
    NoGround = 3,
    UnderWater = 4,
    NoAccess = 5,
    PlotOnly = 6,
    TooClose = 7,
}

impl SpotError {
    pub fn message(self) -> &'static str {
        match self {
            SpotError::OffMap => "off the map",
            SpotError::NotPlantable => "nothing can be planted here",
            SpotError::NoGround => "no ground here",
            SpotError::UnderWater => "too close to the water",
            SpotError::NoAccess => "this plot isn't yours to plant",
            SpotError::PlotOnly => "this can only be kept on a plot",
            SpotError::TooClose => "too close to another planting",
        }
    }
}

/// Whether a planting of `kind` may go at `(x, z)`, given the standing plots near it and the
/// other plantings that aren't gone. The checks run in the spec's order, so the first failure is
/// the one reported.
pub fn check_spot(
    land: &mut impl LandChunks,
    heights: &mut impl Chunks,
    kind: &CropData,
    x: f32,
    z: f32,
    plots: &[PlotSpot],
    others: impl IntoIterator<Item = Planting>,
) -> Result<Spot, SpotError> {
    let lo = -WORLD_HALF_M;
    if !(x.is_finite() && z.is_finite() && (lo..WORLD_HALF_M).contains(&x) && (lo..WORLD_HALF_M).contains(&z)) {
        return Err(SpotError::OffMap);
    }
    let t = texel(land, ((x - lo) / TEXEL_M) as usize, ((z - lo) / TEXEL_M) as usize);
    if !LandUse::from_u8(t & 0x0f).is_some_and(|u| u.data().plantable) {
        return Err(SpotError::NotPlantable);
    }
    let (i, j) = (((x - lo) / SPACING_M) as usize, ((z - lo) / SPACING_M) as usize);
    let mut y = f32::INFINITY;
    for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        y = y.min(crate::terrain::sample(heights, i + di, j + dj).ok_or(SpotError::NoGround)?);
    }
    if y < MIN_GROUND_M {
        return Err(SpotError::UnderWater);
    }
    let plot = plots.iter().position(|p| on_plot(&p.rect, x, z));
    match plot {
        Some(k) if !plots[k].may_tend => return Err(SpotError::NoAccess),
        None if kind.plot_only => return Err(SpotError::PlotOnly),
        _ => {}
    }
    let close = |o: &Planting| {
        let (dx, dz, r) = (o.x - x, o.z - z, o.space_m + kind.space_m);
        dx * dx + dz * dz < r * r
    };
    if others.into_iter().any(|o| close(&o)) {
        return Err(SpotError::TooClose);
    }
    Ok(Spot { y, plot, ideal: climate_of(t) == kind.climate })
}

/// The yield for a random number `r`, from `yield_min` to `yield_max` inclusive.
pub fn yield_for(kind: &CropData, r: u32) -> u32 {
    kind.yield_min + r % (kind.yield_max - kind.yield_min + 1)
}

/// Whether a harvest with random number `r` also gives a seed: `seed_pct` in 100.
pub fn seed_back(kind: &CropData, r: u32) -> bool {
    r % 100 < kind.seed_pct
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{CELL_SIZE_M, OPEN_WORLD, cell_of, pack};
    use crate::content::{Climate, item};
    use crate::land::{LAND_CHUNK_LEN, TEXELS_PER_CELL};
    use crate::terrain::{CHUNK_LEN, encode};
    use std::collections::HashMap;

    const HOUSING: u8 = 1;
    const TOWN: u8 = 2;
    const T: i64 = 1_800_000_000_000_000;

    fn grain() -> &'static CropData {
        crop_by_id("hearthgrain").unwrap()
    }

    fn pear() -> &'static CropData {
        crop_by_id("amberpear_tree").unwrap()
    }

    fn woolback() -> &'static CropData {
        crop_by_id("woolback").unwrap()
    }

    /// Ground 10 m up over the cell at the origin and its neighbours, wild and temperate except
    /// housing in the origin's cell.
    fn world() -> (HashMap<u64, Vec<u8>>, HashMap<u64, Vec<u16>>) {
        let (mut land, mut heights) = (HashMap::new(), HashMap::new());
        for dz in -1..=1 {
            for dx in -1..=1 {
                let cell = cell_of(OPEN_WORLD, dx as f32 * CELL_SIZE_M + 1.0, dz as f32 * CELL_SIZE_M + 1.0);
                land.insert(cell, vec![if (dx, dz) == (0, 0) { HOUSING } else { 0 }; LAND_CHUNK_LEN]);
                heights.insert(cell, vec![encode(10.0); CHUNK_LEN]);
            }
        }
        (land, heights)
    }

    fn set_texel(land: &mut HashMap<u64, Vec<u8>>, x: f32, z: f32, t: u8) {
        let (i, j) = (((x + WORLD_HALF_M) / TEXEL_M) as usize, ((z + WORLD_HALF_M) / TEXEL_M) as usize);
        let cell = pack(OPEN_WORLD, (j / TEXELS_PER_CELL) as u16, (i / TEXELS_PER_CELL) as u16);
        land.get_mut(&cell).unwrap()[(j % TEXELS_PER_CELL) * TEXELS_PER_CELL + i % TEXELS_PER_CELL] = t;
    }

    fn spot(land: &mut HashMap<u64, Vec<u8>>, heights: &mut HashMap<u64, Vec<u16>>, kind: &CropData, x: f32, z: f32) -> Result<Spot, SpotError> {
        check_spot(land, heights, kind, x, z, &[], [])
    }

    #[test]
    fn growth_is_shorter_in_the_ideal_climate() {
        let k = grain();
        assert_eq!(ripe_at(k, T, false) - T, i64::from(k.grow_s) * 1_000_000);
        assert_eq!(ripe_at(k, T, true) - T, i64::from(k.grow_s) * 1_000_000 * i64::from(FARM_IDEAL_PCT) / 100);
        // The barley example: 43 minutes elsewhere, 30 in its climate.
        assert_eq!((ripe_at(k, T, true) - T) / 60_000_000, 30);
        assert_eq!(regrown_at(k, T, false), None, "a crop is gone after one harvest");
        assert_eq!(regrown_at(pear(), T, true), Some(T + growth_micros(pear().regrow_s, true)));
        assert_eq!(ripe_at(k, i64::MAX, false), i64::MAX, "saturates");
    }

    #[test]
    fn stages_follow_the_clock() {
        let k = grain();
        let ripe = ripe_at(k, T, false);
        let gone = withers_at(k, ripe).unwrap();
        assert_eq!(stage(k, ripe, ripe - 1), Stage::Growing);
        assert_eq!(stage(k, ripe, ripe), Stage::Ripe);
        assert_eq!(stage(k, ripe, gone - 1), Stage::Ripe);
        assert_eq!(stage(k, ripe, gone), Stage::Withered);
        assert_eq!(withers_at(pear(), ripe), None);
        assert_eq!(stage(pear(), ripe, ripe + 365 * 86_400_000_000), Stage::Ripe, "trees never wither");
    }

    #[test]
    fn progress_runs_from_zero_to_one() {
        let ripe = T + 1_000_000;
        assert_eq!(progress(T, ripe, T - 5), 0.0);
        assert_eq!(progress(T, ripe, T), 0.0);
        assert!((progress(T, ripe, T + 250_000) - 0.25).abs() < 1e-6);
        assert_eq!(progress(T, ripe, ripe), 1.0);
        assert_eq!(progress(T, ripe, ripe + 9), 1.0);
        assert_eq!(progress(T, T, T), 1.0, "no growth time is grown");
    }

    #[test]
    fn reach_is_horizontal() {
        assert!(in_reach(0.0, 0.0, FARM_REACH_M, 0.0));
        assert!(!in_reach(0.0, 0.0, FARM_REACH_M + 0.01, 0.0));
        assert!(in_reach(5.0, 5.0, 7.0, 7.0), "2.83 m on the diagonal");
    }

    #[test]
    fn open_ground_takes_any_planting_but_livestock() {
        let (mut land, mut heights) = world();
        let s = spot(&mut land, &mut heights, grain(), 100.0, 5.0).unwrap();
        assert_eq!(s, Spot { y: 10.0, plot: None, ideal: true }, "wild and temperate");
        assert_eq!(spot(&mut land, &mut heights, grain(), 5.0, 5.0).map(|s| s.plot), Ok(None), "housing land off a plot");
        assert_eq!(spot(&mut land, &mut heights, woolback(), 5.0, 5.0), Err(SpotError::PlotOnly));
    }

    #[test]
    fn towns_roads_and_the_sea_refuse() {
        let (mut land, mut heights) = world();
        set_texel(&mut land, 5.0, 5.0, TOWN);
        assert_eq!(spot(&mut land, &mut heights, grain(), 5.0, 5.0), Err(SpotError::NotPlantable));
        assert_eq!(spot(&mut land, &mut heights, grain(), 8.5, 5.0).map(|s| s.y), Ok(10.0), "the next texel");
        set_texel(&mut land, 5.0, 5.0, 3);
        assert_eq!(spot(&mut land, &mut heights, grain(), 5.0, 5.0), Err(SpotError::NotPlantable), "a road");
        let far = 3.0 * CELL_SIZE_M;
        assert_eq!(spot(&mut land, &mut heights, grain(), far, far), Err(SpotError::NoGround), "no terrain, though wild");
        assert_eq!(spot(&mut land, &mut heights, grain(), WORLD_HALF_M, 0.0), Err(SpotError::OffMap));
        assert_eq!(spot(&mut land, &mut heights, grain(), f32::NAN, 0.0), Err(SpotError::OffMap));
        let cell = cell_of(OPEN_WORLD, 1.0, 1.0);
        heights.get_mut(&cell).unwrap()[0] = encode(0.2);
        assert_eq!(spot(&mut land, &mut heights, grain(), 1.0, 1.0), Err(SpotError::UnderWater), "one low corner is enough");
        assert_eq!(spot(&mut land, &mut heights, grain(), 2.5, 2.5).map(|s| s.y), Ok(10.0));
    }

    #[test]
    fn the_climate_bits_decide_ideal() {
        let (mut land, mut heights) = world();
        set_texel(&mut land, 5.0, 5.0, HOUSING | (Climate::Arid as u8) << 4);
        assert!(!spot(&mut land, &mut heights, grain(), 5.0, 5.0).unwrap().ideal);
        assert!(spot(&mut land, &mut heights, crop_by_id("dustbean").unwrap(), 5.0, 5.0).unwrap().ideal);
        assert_eq!(climate_of(0x35), 3);
    }

    #[test]
    fn plots_need_access() {
        let (mut land, mut heights) = world();
        let farm = Rect { x0: 0.0, z0: 0.0, x1: 16.0, z1: 16.0 };
        let theirs = [PlotSpot { rect: Rect { x0: -8.0, z0: 0.0, x1: 0.0, z1: 8.0 }, may_tend: false }, PlotSpot { rect: farm, may_tend: true }];
        let at = |x, z, k| check_spot(&mut land.clone(), &mut heights.clone(), k, x, z, &theirs, []);
        assert_eq!(at(4.0, 4.0, woolback()).map(|s| s.plot), Ok(Some(1)));
        assert_eq!(at(-4.0, 4.0, grain()), Err(SpotError::NoAccess));
        assert_eq!(at(0.0, 4.0, grain()).map(|s| s.plot), Ok(Some(1)), "the west edge belongs to the plot east of it");
        assert_eq!(at(16.0, 4.0, grain()).map(|s| s.plot), Ok(None), "the east edge doesn't");
        land.clear();
        assert_eq!(check_spot(&mut land, &mut heights, grain(), 4.0, 4.0, &theirs, []).map(|s| s.plot), Ok(Some(1)), "wild under a plot");
    }

    #[test]
    fn plantings_keep_their_distance() {
        let (mut land, mut heights) = world();
        let other = Planting { x: 5.0, z: 5.0, space_m: 0.5 };
        let at = |x: f32, k| check_spot(&mut land.clone(), &mut heights.clone(), k, x, 5.0, &[], [other]);
        assert_eq!(at(5.99, grain()), Err(SpotError::TooClose));
        assert!(at(6.0, grain()).is_ok(), "touching is fine");
        assert_eq!(at(7.4, pear()), Err(SpotError::TooClose), "a tree needs 2 m");
        assert!(at(7.5, pear()).is_ok());
        assert!(check_spot(&mut land, &mut heights, grain(), 5.0, 5.0, &[], []).is_ok(), "gone plantings aren't passed in");
    }

    #[test]
    fn rolls_cover_their_ranges() {
        for k in &CROPS {
            let got: Vec<u32> = (0..20).map(|r| yield_for(k, r)).collect();
            assert_eq!(got.iter().min(), Some(&k.yield_min), "{}", k.id);
            assert_eq!(got.iter().max(), Some(&k.yield_max), "{}", k.id);
            assert_eq!((0..100).filter(|&r| seed_back(k, r)).count() as u32, k.seed_pct, "{}", k.id);
        }
    }

    #[test]
    fn every_crop_has_its_seed_and_product() {
        for k in &CROPS {
            assert_eq!(crop_by_seed(k.seed), Some(k));
            assert_eq!(crop(k.wire), Some(k));
            assert!(item(k.product).is_some() && item(k.seed).is_some(), "{}", k.id);
        }
        assert!(CROPS.iter().all(|k| k.plot_only == (k.kind == CropKind::Animal)));
    }
}
