//! World partitioning: 64 m cells over a 16×16 km map centred on the origin (PLAN §4.3).
//!
//! A cell key packs `(instance, cy, cx)` into one `u64`, laid out row by row. Clients subscribe to a
//! window cell by cell (`window_cells`), since SpacetimeDB prunes equality queries but not ranges.

pub const CELL_SIZE_M: f32 = 64.0;
pub const WORLD_SIZE_M: f32 = 16_000.0;
pub const WORLD_HALF_M: f32 = WORLD_SIZE_M / 2.0;
pub const CELLS_PER_SIDE: u16 = 250;

/// The open world. Dungeons, arenas, the prison and so on get their own instance ids.
pub const OPEN_WORLD: u32 = 0;

/// Grid index along one axis, clamped to the map edge.
pub fn axis_index(coord_m: f32) -> u16 {
    let i = ((coord_m + WORLD_HALF_M) / CELL_SIZE_M).floor();
    if i.is_nan() || i < 0.0 {
        0
    } else {
        (i as u32).min(CELLS_PER_SIDE as u32 - 1) as u16
    }
}

pub fn pack(instance: u32, cy: u16, cx: u16) -> u64 {
    ((instance as u64) << 32) | ((cy as u64) << 16) | cx as u64
}

pub fn unpack(key: u64) -> (u32, u16, u16) {
    ((key >> 32) as u32, (key >> 16) as u16, key as u16)
}

/// The cell containing world position `(x, z)`. `y` is up and plays no part.
pub fn cell_of(instance: u32, x: f32, z: f32) -> u64 {
    pack(instance, axis_index(z), axis_index(x))
}

/// The inclusive key range for row `cy` of a window `radius` cells either side of `cx`.
pub fn row_range(instance: u32, cy: u16, cx: u16, radius: u16) -> (u64, u64) {
    let lo = cx.saturating_sub(radius);
    let hi = cx.saturating_add(radius).min(CELLS_PER_SIDE - 1);
    (pack(instance, cy, lo), pack(instance, cy, hi))
}

/// One key range per row of the `(2·radius+1)²` window around `center`, clipped to the map.
pub fn window_rows(center: u64, radius: u16) -> Vec<(u64, u64)> {
    let (instance, cy, cx) = unpack(center);
    let lo = cy.saturating_sub(radius);
    let hi = cy.saturating_add(radius).min(CELLS_PER_SIDE - 1);
    (lo..=hi).map(|y| row_range(instance, y, cx, radius)).collect()
}

/// Every cell key of the window around `center`, clipped to the map. Clients subscribe with one
/// `cell = key` query per cell: SpacetimeDB prunes equality queries when it works out who gets a
/// change, but checks range queries against every change (docs/spikes/throughput.md).
pub fn window_cells(center: u64, radius: u16) -> Vec<u64> {
    window_rows(center, radius).into_iter().flat_map(|(lo, hi)| lo..=hi).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_in_the_middle_cells() {
        // 8000 / 64 = 125, so the origin sits at the corner of cells 124 and 125.
        assert_eq!(unpack(cell_of(OPEN_WORLD, 0.0, 0.0)), (0, 125, 125));
        assert_eq!(unpack(cell_of(OPEN_WORLD, -0.01, -0.01)), (0, 124, 124));
    }

    #[test]
    fn edges_clamp_to_the_map() {
        assert_eq!(axis_index(-WORLD_HALF_M), 0);
        assert_eq!(axis_index(-1.0e9), 0);
        assert_eq!(axis_index(WORLD_HALF_M - 0.001), CELLS_PER_SIDE - 1);
        assert_eq!(axis_index(WORLD_HALF_M), CELLS_PER_SIDE - 1);
        assert_eq!(axis_index(1.0e9), CELLS_PER_SIDE - 1);
        assert_eq!(axis_index(f32::NAN), 0);
    }

    #[test]
    fn pack_roundtrips_and_keeps_instances_apart() {
        let k = pack(7, 200, 3);
        assert_eq!(unpack(k), (7, 200, 3));
        assert!(pack(1, 0, 0) > pack(0, CELLS_PER_SIDE - 1, CELLS_PER_SIDE - 1));
    }

    #[test]
    fn x_is_the_fast_axis() {
        // Within a row, keys are contiguous in x, so a row of a window is one range.
        assert_eq!(pack(0, 10, 11), pack(0, 10, 10) + 1);
        assert_eq!(cell_of(0, 64.0, 0.0), cell_of(0, 0.0, 0.0) + 1);
    }

    #[test]
    fn window_rows_cover_a_5x5_and_clip_at_edges() {
        let rows = window_rows(pack(0, 100, 100), 2);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], (pack(0, 98, 98), pack(0, 98, 102)));
        assert_eq!(rows[4], (pack(0, 102, 98), pack(0, 102, 102)));

        let corner = window_rows(pack(0, 0, CELLS_PER_SIDE - 1), 2);
        assert_eq!(corner.len(), 3);
        assert_eq!(corner[0], (pack(0, 0, CELLS_PER_SIDE - 3), pack(0, 0, CELLS_PER_SIDE - 1)));
    }

    #[test]
    fn window_cells_list_every_cell_once() {
        let cells = window_cells(pack(0, 100, 100), 2);
        assert_eq!(cells.len(), 25);
        assert_eq!(cells[0], pack(0, 98, 98));
        assert_eq!(cells[24], pack(0, 102, 102));
        assert!(cells.contains(&pack(0, 100, 100)));
        assert_eq!(window_cells(pack(0, 0, 0), 2).len(), 9);
    }
}
