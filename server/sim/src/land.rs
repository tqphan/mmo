//! Land (docs/design/land.md): where a plot may stand, what its tax costs, and whether it's paid,
//! overdue or demolished, all from timestamps. The client ports this for the placement preview
//! and the plot sign, and the server decides.

pub use crate::content::{PLOTS, PROPERTY, PlotData, PropertyRules, plot, plot_by_deed};
use crate::cell::{CELL_SIZE_M, CELLS_PER_SIDE, OPEN_WORLD, WORLD_HALF_M, pack};
use crate::content::LandUse;
use crate::movement::SEA_LEVEL_M;
use crate::terrain::{Chunks, SPACING_M};

/// Land use is stored per 4 m texel (docs/design/world.md).
pub const TEXEL_M: f32 = 4.0;
pub const TEXELS_PER_CELL: usize = (CELL_SIZE_M / TEXEL_M) as usize;
/// Texels in one cell's land chunk: 16 × 16, rows of increasing z.
pub const LAND_CHUNK_LEN: usize = TEXELS_PER_CELL * TEXELS_PER_CELL;
/// A plot's ground must be at least this far above the sea everywhere.
pub const MIN_GROUND_M: f32 = SEA_LEVEL_M + 0.5;

const MICROS_PER_S: i64 = 1_000_000;
const DAY_MICROS: i64 = 86_400 * MICROS_PER_S;

/// Where land chunks come from: the database in the module, a map in tests and tools.
pub trait LandChunks {
    /// The chunk for an open-world cell key, `LAND_CHUNK_LEN` texels, if the cell has one. A cell
    /// without one is all wild and temperate.
    fn land(&mut self, cell: u64) -> Option<&[u8]>;
}

impl LandChunks for std::collections::HashMap<u64, Vec<u8>> {
    fn land(&mut self, cell: u64) -> Option<&[u8]> {
        self.get(&cell).map(Vec::as_slice)
    }
}

/// The land-use texel at texel `(i, j)`, counted from the map's (−x, −z) corner; 0 (wild,
/// temperate) where there's no chunk.
pub fn texel(src: &mut impl LandChunks, i: usize, j: usize) -> u8 {
    let (cx, cz) = (i / TEXELS_PER_CELL, j / TEXELS_PER_CELL);
    if cx >= usize::from(CELLS_PER_SIDE) || cz >= usize::from(CELLS_PER_SIDE) {
        return 0;
    }
    src.land(pack(OPEN_WORLD, cz as u16, cx as u16))
        .and_then(|t| t.get((j % TEXELS_PER_CELL) * TEXELS_PER_CELL + i % TEXELS_PER_CELL).copied())
        .unwrap_or(0)
}

/// A plot centre on the 2 m height grid, so a square of any multiple of 4 m has its corners on samples.
pub fn snap(v: f32) -> f32 {
    (v / SPACING_M).round() * SPACING_M
}

/// An axis-aligned rectangle on the ground, `x0 < x1`, `z0 < z1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
}

impl Rect {
    /// The square a plot of `kind` covers around centre `(x, z)`.
    pub fn plot(kind: &PlotData, x: f32, z: f32) -> Self {
        let h = kind.size_m as f32 / 2.0;
        Self { x0: x - h, z0: z - h, x1: x + h, z1: z + h }
    }

    /// The two share some area; sharing only an edge or a corner doesn't count.
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.z0 < o.z1 && o.z0 < self.z1
    }

    /// Horizontal distance from `(x, z)` to the rectangle, 0 inside it.
    pub fn distance(&self, x: f32, z: f32) -> f32 {
        let dx = (self.x0 - x).max(x - self.x1).max(0.0);
        let dz = (self.z0 - z).max(z - self.z1).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }
}

/// A character at `(x, z)` can place, pay or change a plot covering `r`.
pub fn in_reach(r: &Rect, x: f32, z: f32) -> bool {
    r.distance(x, z) <= PROPERTY.reach_m
}

/// Why a plot can't stand somewhere. The discriminant is the code in the client's vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SiteError {
    OffMap = 1,
    NotHousing = 2,
    NoGround = 3,
    UnderWater = 4,
    TooSteep = 5,
    Taken = 6,
}

impl SiteError {
    pub fn message(self) -> &'static str {
        match self {
            SiteError::OffMap => "off the map",
            SiteError::NotHousing => "plots can only go on housing land",
            SiteError::NoGround => "no ground here",
            SiteError::UnderWater => "too close to the water",
            SiteError::TooSteep => "the ground is too steep",
            SiteError::Taken => "the land is taken",
        }
    }
}

/// Whether a plot of `kind` may stand centred on `(x, z)` (already snapped), beside plots covering
/// `others`. On success, the lowest ground under it. The checks run in the spec's order, so the
/// first failure is the one reported.
pub fn check_site(
    land: &mut impl LandChunks,
    heights: &mut impl Chunks,
    kind: &PlotData,
    x: f32,
    z: f32,
    others: impl IntoIterator<Item = Rect>,
) -> Result<f32, SiteError> {
    let r = Rect::plot(kind, x, z);
    let lo = -WORLD_HALF_M;
    if !(r.x0 >= lo && r.z0 >= lo && r.x1 <= WORLD_HALF_M && r.z1 <= WORLD_HALF_M) {
        return Err(SiteError::OffMap);
    }
    // Texels the square covers: any it reaches into, not just touches.
    let texels = |a: f32, b: f32| ((a - lo) / TEXEL_M).floor() as usize..((b - lo) / TEXEL_M).ceil() as usize;
    for j in texels(r.z0, r.z1) {
        for i in texels(r.x0, r.x1) {
            let use_class = LandUse::from_u8(texel(land, i, j) & 0x0f);
            if !use_class.is_some_and(|u| u.data().buildable) {
                return Err(SiteError::NotHousing);
            }
        }
    }
    // Height samples on and inside the square; the corners are on the grid after `snap`.
    let samples = |a: f32, b: f32| ((a - lo) / SPACING_M).round() as usize..=((b - lo) / SPACING_M).round() as usize;
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for j in samples(r.z0, r.z1) {
        for i in samples(r.x0, r.x1) {
            let h = crate::terrain::sample(heights, i, j).ok_or(SiteError::NoGround)?;
            min = min.min(h);
            max = max.max(h);
        }
    }
    if min < MIN_GROUND_M {
        return Err(SiteError::UnderWater);
    }
    if max - min > kind.max_rise_m {
        return Err(SiteError::TooSteep);
    }
    if others.into_iter().any(|o| o.overlaps(&r)) {
        return Err(SiteError::Taken);
    }
    Ok(min)
}

/// Levy Seals a week for one plot of `kind` when the account owns `owned` plots (this one
/// included), rounded up; `None` past the most an account may own.
pub fn weekly_tax(kind: &PlotData, owned: usize) -> Option<u32> {
    let pct = *PROPERTY.heavy_tax_pct.get(owned.checked_sub(1)?)?;
    Some((kind.tax * pct).div_ceil(100))
}

/// The most plots one account may own.
pub fn max_plots() -> usize {
    PROPERTY.heavy_tax_pct.len()
}

pub fn week_micros() -> i64 {
    i64::from(PROPERTY.week_s) * MICROS_PER_S
}

/// When a plot whose tax ran out at `paid_until` is demolished: the first `demolish_hour_utc` at
/// least the grace later (all in game time, microseconds since the Unix epoch).
pub fn demolish_at(paid_until: i64) -> i64 {
    let t = paid_until.saturating_add(i64::from(PROPERTY.grace_s) * MICROS_PER_S);
    let hour = i64::from(PROPERTY.demolish_hour_utc) * 3_600 * MICROS_PER_S;
    let past = (t - hour).rem_euclid(DAY_MICROS);
    if past == 0 { t } else { t.saturating_add(DAY_MICROS - past) }
}

/// Where a plot stands, from its tax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Paid = 0,
    /// The tax ran out; it comes down at `demolish_at`.
    Overdue = 1,
    /// It's gone, whether or not its row has been removed yet.
    Demolished = 2,
}

pub fn standing(paid_until: i64, now: i64) -> Standing {
    if now < paid_until {
        Standing::Paid
    } else if now < demolish_at(paid_until) {
        Standing::Overdue
    } else {
        Standing::Demolished
    }
}

/// Whether one more week may be paid at `now`: after it, the tax may run at most `prepay_weeks`
/// ahead. A demolished plot can't be paid.
pub fn can_pay(paid_until: i64, now: i64) -> bool {
    let ahead = paid_until.saturating_add(week_micros()).saturating_sub(now);
    standing(paid_until, now) != Standing::Demolished && ahead <= i64::from(PROPERTY.prepay_weeks) * week_micros()
}

/// One land chunk's part of the land hash (docs/design/land.md): FNV-1a 64 over the cell key and
/// the texels, finished like `terrain::chunk_hash`.
pub fn land_chunk_hash(cell: u64, texels: &[u8]) -> u64 {
    let fnv = |h: u64, b: u8| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    let h = cell.to_le_bytes().into_iter().fold(0xcbf2_9ce4_8422_2325, fnv);
    crate::terrain::finish(texels.iter().copied().fold(h, fnv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::cell_of;
    use crate::content::item;
    use crate::terrain::{CHUNK_LEN, encode};
    use std::collections::HashMap;

    const HOUSING: u8 = 1;
    const TOWN: u8 = 2;

    fn garden() -> &'static PlotData {
        &PLOTS[0]
    }

    fn farm() -> &'static PlotData {
        &PLOTS[1]
    }

    /// Ground 10 m up over the cell at the origin and its neighbours, and housing land on all of it.
    fn world() -> (HashMap<u64, Vec<u8>>, HashMap<u64, Vec<u16>>) {
        let (mut land, mut heights) = (HashMap::new(), HashMap::new());
        for dz in -1..=1 {
            for dx in -1..=1 {
                let cell = cell_of(OPEN_WORLD, dx as f32 * CELL_SIZE_M + 1.0, dz as f32 * CELL_SIZE_M + 1.0);
                land.insert(cell, vec![HOUSING; LAND_CHUNK_LEN]);
                heights.insert(cell, vec![encode(10.0); CHUNK_LEN]);
            }
        }
        (land, heights)
    }

    /// Set the texel under `(x, z)`.
    fn set_texel(land: &mut HashMap<u64, Vec<u8>>, x: f32, z: f32, t: u8) {
        let (i, j) = (((x + WORLD_HALF_M) / TEXEL_M) as usize, ((z + WORLD_HALF_M) / TEXEL_M) as usize);
        let cell = pack(OPEN_WORLD, (j / TEXELS_PER_CELL) as u16, (i / TEXELS_PER_CELL) as u16);
        land.get_mut(&cell).unwrap()[(j % TEXELS_PER_CELL) * TEXELS_PER_CELL + i % TEXELS_PER_CELL] = t;
    }

    /// Set the height sample at `(x, z)`, on the grid.
    fn set_height(heights: &mut HashMap<u64, Vec<u16>>, x: f32, z: f32, h: f32) {
        let (i, j) = (((x + WORLD_HALF_M) / SPACING_M) as usize, ((z + WORLD_HALF_M) / SPACING_M) as usize);
        let q = crate::terrain::QUADS_PER_CHUNK;
        // Every chunk holding the point keeps it, as neighbours repeat their shared edges.
        for (cx, cz) in [(i / q, j / q), (i.wrapping_sub(1) / q, j / q), (i / q, j.wrapping_sub(1) / q), (i.wrapping_sub(1) / q, j.wrapping_sub(1) / q)] {
            let (lx, lz) = (i.wrapping_sub(cx * q), j.wrapping_sub(cz * q));
            if let Some(c) = heights.get_mut(&pack(OPEN_WORLD, cz as u16, cx as u16)).filter(|_| lx <= q && lz <= q) {
                c[lz * (q + 1) + lx] = encode(h);
            }
        }
    }

    #[test]
    fn centres_snap_to_the_height_grid() {
        assert_eq!(snap(0.9), 0.0);
        assert_eq!(snap(1.1), 2.0);
        assert_eq!(snap(-3.2), -4.0);
        assert_eq!(snap(7.0), 8.0, "halves round away from zero");
    }

    #[test]
    fn rectangles_overlap_only_by_area() {
        let a = Rect::plot(garden(), 0.0, 0.0);
        assert_eq!(a, Rect { x0: -4.0, z0: -4.0, x1: 4.0, z1: 4.0 });
        assert!(a.overlaps(&Rect::plot(garden(), 6.0, 2.0)));
        assert!(!a.overlaps(&Rect::plot(garden(), 8.0, 0.0)), "sharing an edge");
        assert!(!a.overlaps(&Rect::plot(garden(), 8.0, 8.0)), "sharing a corner");
        assert!(Rect::plot(farm(), 10.0, 0.0).overlaps(&a), "a farm reaches 8 m");
        assert_eq!(a.distance(0.0, 0.0), 0.0);
        assert_eq!(a.distance(7.0, 0.0), 3.0);
        assert_eq!(a.distance(7.0, 8.0), 5.0);
        assert!(in_reach(&a, 4.0 + PROPERTY.reach_m, 0.0) && !in_reach(&a, 4.01 + PROPERTY.reach_m, 0.0));
    }

    #[test]
    fn a_flat_housing_site_passes() {
        let (mut land, mut heights) = world();
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, []), Ok(10.0));
        assert_eq!(check_site(&mut land, &mut heights, farm(), 30.0, -30.0, []), Ok(10.0), "across a cell edge");
    }

    #[test]
    fn every_texel_under_it_must_be_housing() {
        let (mut land, mut heights) = world();
        set_texel(&mut land, 3.0, 3.0, TOWN);
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, []), Err(SiteError::NotHousing));
        assert_eq!(check_site(&mut land, &mut heights, garden(), 8.0, 0.0, []), Ok(10.0), "the texel is outside");
        set_texel(&mut land, 3.0, 3.0, HOUSING | 3 << 4);
        assert!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, []).is_ok(), "any climate");
        // A square off the texel grid reaches into the texels on both sides.
        set_texel(&mut land, -5.0, 0.0, 0);
        assert_eq!(check_site(&mut land, &mut heights, garden(), -2.0, 0.0, []), Err(SiteError::NotHousing));
        assert_eq!(check_site(&mut land, &mut heights, garden(), 4.0, 0.0, []), Ok(10.0));
        let far = 3.0 * CELL_SIZE_M;
        assert_eq!(check_site(&mut land, &mut heights, garden(), far, far, []), Err(SiteError::NotHousing), "no land data is wild");
    }

    #[test]
    fn the_ground_must_be_known_dry_and_flat_enough() {
        let (mut land, mut heights) = world();
        let edge = 2.0 * CELL_SIZE_M;
        for z in [-4.0, 4.0] {
            land.insert(cell_of(OPEN_WORLD, edge + 4.0, z), vec![HOUSING; LAND_CHUNK_LEN]);
        }
        assert_eq!(check_site(&mut land, &mut heights, farm(), edge, 0.0, []), Err(SiteError::NoGround));
        set_height(&mut heights, 2.0, 2.0, 0.4);
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, []), Err(SiteError::UnderWater));
        set_height(&mut heights, 2.0, 2.0, 10.0 + garden().max_rise_m);
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, []), Ok(10.0), "the rise may reach the limit");
        set_height(&mut heights, 2.0, 2.0, 10.1 + garden().max_rise_m);
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, []), Err(SiteError::TooSteep));
        assert_eq!(check_site(&mut land, &mut heights, garden(), 8.0, 0.0, []), Ok(10.0), "the sample is outside");
        assert_eq!(check_site(&mut land, &mut heights, garden(), 6.0, 0.0, []), Err(SiteError::TooSteep), "the edge counts");
        let off = WORLD_HALF_M - 2.0;
        assert_eq!(check_site(&mut land, &mut heights, garden(), off, 0.0, []), Err(SiteError::OffMap));
    }

    #[test]
    fn other_plots_take_the_land() {
        let (mut land, mut heights) = world();
        let near = [Rect::plot(farm(), 10.0, 0.0)];
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, near), Err(SiteError::Taken));
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, [Rect::plot(farm(), 12.0, 12.0)]), Ok(10.0), "a corner");
        assert_eq!(check_site(&mut land, &mut heights, garden(), 0.0, 0.0, [Rect::plot(farm(), 12.0, 0.0)]), Ok(10.0), "an edge");
    }

    #[test]
    fn tax_rises_with_plots_owned() {
        assert_eq!(weekly_tax(garden(), 1), Some(garden().tax));
        let steps = PROPERTY.heavy_tax_pct;
        for (n, pct) in steps.iter().enumerate() {
            assert_eq!(weekly_tax(farm(), n + 1), Some((farm().tax * pct).div_ceil(100)));
        }
        assert_eq!(weekly_tax(garden(), 0), None);
        assert_eq!(weekly_tax(garden(), max_plots() + 1), None);
        assert!(steps.windows(2).all(|w| w[0] <= w[1]));
        assert!(item(PROPERTY.tax_item).is_some());
    }

    #[test]
    fn demolition_comes_on_the_hour_after_the_grace() {
        let hour = i64::from(PROPERTY.demolish_hour_utc) * 3_600 * MICROS_PER_S;
        let grace = i64::from(PROPERTY.grace_s) * MICROS_PER_S;
        // 2026-10-26 00:00 UTC.
        let midnight = 1_792_972_800 * MICROS_PER_S;
        assert_eq!(midnight % DAY_MICROS, 0);
        let on_hour = midnight + hour - grace;
        assert_eq!(demolish_at(on_hour), midnight + hour, "exactly on the hour");
        assert_eq!(demolish_at(on_hour + 1), midnight + hour + DAY_MICROS, "just past it waits a day");
        assert_eq!(demolish_at(on_hour - 1), midnight + hour);
        let d = demolish_at(midnight);
        assert!(d - midnight >= grace && d - midnight < grace + DAY_MICROS && (d - hour) % DAY_MICROS == 0);
    }

    #[test]
    fn standing_follows_the_clock() {
        let paid = 1_800_000_000 * MICROS_PER_S;
        let gone = demolish_at(paid);
        assert_eq!(standing(paid, paid - 1), Standing::Paid);
        assert_eq!(standing(paid, paid), Standing::Overdue);
        assert_eq!(standing(paid, gone - 1), Standing::Overdue);
        assert_eq!(standing(paid, gone), Standing::Demolished);
        let week = week_micros();
        let limit = i64::from(PROPERTY.prepay_weeks) * week;
        assert!(can_pay(paid, paid - limit + week), "paid to the limit");
        assert!(!can_pay(paid, paid - limit + week - 1));
        assert!(can_pay(paid, gone - 1), "overdue can still pay");
        assert!(!can_pay(paid, gone));
    }

    #[test]
    fn land_hash_depends_on_the_cell_and_every_texel() {
        let t = vec![1u8; LAND_CHUNK_LEN];
        let h = land_chunk_hash(5, &t);
        assert_ne!(h, land_chunk_hash(6, &t));
        for k in [0, 100, LAND_CHUNK_LEN - 1] {
            let mut u = t.clone();
            u[k] = 2;
            assert_ne!(h, land_chunk_hash(5, &u), "texel {k}");
        }
    }

    #[test]
    fn every_plot_has_a_deed() {
        for p in &PLOTS {
            assert_eq!(plot_by_deed(p.deed), Some(p));
            assert_eq!(plot(p.wire), Some(p));
            assert_eq!(p.size_m % TEXEL_M as u32, 0);
        }
    }
}
