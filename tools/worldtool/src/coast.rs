//! Hearthshore, the M1 coast zone, laid out by hand (docs/design/coast.md): about 3 × 3 km of the
//! west continent's east coast, with the island town of Gullhaven as its outpost across the water.
//! Everything here is placed on purpose; the rest of the map stays the placeholder's. North is −z.
//!
//! The generator shapes the ground with `LANDFORMS` after the placeholder's noise, and takes the
//! zone's towns, roads, NPC camps, node fields and named areas from here. Changing this file and
//! running `cargo xtask world --gen` rewrites `world/`.

use crate::placeholder::{fbm, smoothstep};

/// The zone's bounds (x0, z0, x1, z1). Inside them, nodes come only from `NODE_FIELDS`.
pub const ZONE: (f32, f32, f32, f32) = (-5900.0, -1500.0, -2900.0, 1800.0);

pub fn in_zone(x: f32, z: f32) -> bool {
    (ZONE.0..ZONE.2).contains(&x) && (ZONE.1..ZONE.3).contains(&z)
}

/// A change to the ground, applied in order after the placeholder's heights.
pub enum Landform {
    /// Raise the ground by up to `h` m within `r` of the centre, with a smooth fall-off, roughened by
    /// `rough` (0 smooth, 1 craggy).
    Hill { x: f32, z: f32, r: f32, h: f32, rough: f32 },
    /// The same along a line from (x0, z0) to (x1, z1), `w` m either side.
    Ridge { x0: f32, z0: f32, x1: f32, z1: f32, w: f32, h: f32, rough: f32 },
    /// Bring the ground to `to` m, fully within 60% of `r` and blending out to `r`, give or take
    /// `wobble` m of noise: town pads, the fen's pools, the tarn and the cove.
    Level { x: f32, z: f32, r: f32, to: f32, wobble: f32 },
}

pub const LANDFORMS: [Landform; 12] = [
    // Bristle Downs: rolling hills north-west of Furrowdale.
    Landform::Hill { x: -3750.0, z: -200.0, r: 380.0, h: 30.0, rough: 0.3 },
    Landform::Hill { x: -3480.0, z: -520.0, r: 220.0, h: 18.0, rough: 0.3 },
    // Greymane Wood's low rise.
    Landform::Hill { x: -4250.0, z: -450.0, r: 350.0, h: 12.0, rough: 0.2 },
    // The Cairn Crags in the north-west, above Cairnwick.
    Landform::Hill { x: -5050.0, z: -1250.0, r: 520.0, h: 85.0, rough: 0.8 },
    // Ashwing Scarp, the ridge along the zone's west side.
    Landform::Ridge { x0: -5480.0, z0: -350.0, x1: -5560.0, z1: 1150.0, w: 360.0, h: 85.0, rough: 0.4 },
    // Wrackline Point, a low headland on the north shore.
    Landform::Hill { x: -3170.0, z: -950.0, r: 260.0, h: 20.0, rough: 0.4 },
    // Croaker Fen: wet flats with pools, between Furrowdale and Redfen.
    Landform::Level { x: -3560.0, z: 1020.0, r: 330.0, to: 0.6, wobble: 1.4 },
    // Hexer's Tarn, a small dark lake in the south-west.
    Landform::Level { x: -4750.0, z: 1450.0, r: 240.0, to: -3.0, wobble: 0.5 },
    // Wreckers' Cove, a bay cut into the south shore.
    Landform::Level { x: -3330.0, z: 1660.0, r: 280.0, to: -7.0, wobble: 1.0 },
    // Town pads.
    Landform::Level { x: -3330.0, z: 380.0, r: 110.0, to: 5.0, wobble: 0.3 },
    Landform::Level { x: -4600.0, z: -1050.0, r: 80.0, to: 27.0, wobble: 0.3 },
    Landform::Level { x: -3930.0, z: 1250.0, r: 80.0, to: 22.0, wobble: 0.3 },
];

/// Distance from (x, z) to the segment from a to b.
fn to_segment(x: f32, z: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * dx + (z - a.1) * dz) / (dx * dx + dz * dz)).clamp(0.0, 1.0);
    (x - a.0 - t * dx).hypot(z - a.1 - t * dz)
}

/// How far outlines wander from a circle or a straight line, as a share of the distance.
const WANDER: f32 = 0.6;

/// The ground at (x, z) after the zone's landforms, from the placeholder's `h`. Distances are
/// stretched and shrunk by noise so no outline is a true circle.
pub fn shape(x: f32, z: f32, mut h: f32) -> f32 {
    let warp = 1.0 + WANDER * fbm(x, z, 250.0, 3, 83);
    for f in &LANDFORMS {
        match *f {
            Landform::Hill { x: cx, z: cz, r, h: up, rough } => {
                let t = smoothstep(r, 0.0, (x - cx).hypot(z - cz) * warp);
                if t > 0.0 {
                    h += up * t * (1.0 + rough * 2.0 * fbm(x, z, 140.0, 4, 71));
                }
            }
            Landform::Ridge { x0, z0, x1, z1, w, h: up, rough } => {
                let t = smoothstep(w, 0.0, to_segment(x, z, (x0, z0), (x1, z1)) * warp);
                if t > 0.0 {
                    h += up * t * (1.0 + rough * 2.0 * fbm(x, z, 160.0, 4, 73));
                }
            }
            Landform::Level { x: cx, z: cz, r, to, wobble } => {
                let t = smoothstep(r, 0.6 * r, (x - cx).hypot(z - cz) * warp);
                if t > 0.0 {
                    let target = to + wobble * 2.0 * fbm(x, z, 60.0, 3, 79);
                    h += (target - h) * t;
                }
            }
        }
    }
    h
}

/// The zone's towns and the island's: centre and radius. Furrowdale is where characters start.
pub const TOWNS: [(f32, f32, f32); 4] = [(0.0, 0.0, 250.0), (-3330.0, 380.0, 90.0), (-4600.0, -1050.0, 55.0), (-3930.0, 1250.0, 55.0)];

/// Roads, as polylines; land within `ROAD_HALF_M` of one is road (outside towns).
pub const ROADS: [&[(f32, f32)]; 2] = [
    // Furrowdale to Cairnwick, over the downs and through Greymane Wood.
    &[(-3330.0, 380.0), (-3640.0, 240.0), (-3980.0, -120.0), (-4330.0, -620.0), (-4600.0, -1050.0)],
    // Furrowdale to Redfen, round the fen's west side.
    &[(-3330.0, 380.0), (-3700.0, 600.0), (-3960.0, 980.0), (-3930.0, 1250.0)],
];
pub const ROAD_HALF_M: f32 = 4.0;

pub fn on_road(x: f32, z: f32) -> bool {
    ROADS.iter().any(|r| r.windows(2).any(|s| to_segment(x, z, s[0], s[1]) < ROAD_HALF_M))
}

/// Crafting stations: (kind, x, z, yaw in degrees). Gullhaven's three stand a few metres east of
/// the island's spawn; Furrowdale has one of each, the Packwright's Bench included.
pub const STATIONS: [(&str, f32, f32, u16); 7] = [
    ("joiners_bench", 9.0, -5.0, 270),
    ("bloomery", 12.0, 2.0, 270),
    ("cookhearth", 8.0, 8.0, 0),
    ("joiners_bench", -3345.0, 352.0, 0),
    ("bloomery", -3360.0, 360.0, 90),
    ("cookhearth", -3358.0, 400.0, 90),
    ("packwright_bench", -3305.0, 410.0, 270),
];

/// Trade posts: (kind, x, z, yaw in degrees). Two inland hamlets on the coast and one across the sea.
pub const POSTS: [(&str, f32, f32, u16); 3] =
    [("cairnwick_post", -4590.0, -1040.0, 90), ("redfen_post", -3920.0, 1240.0, 0), ("gullhaven_post", -10.0, 4.0, 90)];

/// Wayshrines: (kind, x, z, yaw in degrees), one in each town. New characters start in front of
/// Furrowdale's (content: `start = true`).
pub const SHRINES: [(&str, f32, f32, u16); 4] = [
    ("gullhaven_wayshrine", 0.0, -15.0, 0),
    ("furrowdale_wayshrine", -3320.0, 380.0, 90),
    ("cairnwick_wayshrine", -4610.0, -1060.0, 90),
    ("redfen_wayshrine", -3940.0, 1260.0, 0),
];

/// Quest givers: (giver, x, z, yaw in degrees), a few steps from each town's Wayshrine
/// (docs/design/quests.md).
pub const GIVERS: [(&str, f32, f32, u16); 4] = [
    ("odda_fernwright", -3326.0, 372.0, 90),
    ("brannoch_hale", -4616.0, -1052.0, 90),
    ("yeva_mossgrove", -3946.0, 1252.0, 0),
    ("quill_ambry", 6.0, -12.0, 270),
];

/// NPC camps: (kind, centre x, z, radius, spawn points), rising in level away from Furrowdale.
pub const CAMPS: [(&str, f32, f32, f32, usize); 13] = [
    // Level 1, on the beaches either side of Furrowdale, and on Gullhaven's.
    ("shore_skitter", -3235.0, 130.0, 70.0, 6),
    ("shore_skitter", -3235.0, 640.0, 70.0, 5),
    ("shore_skitter", 330.0, 0.0, 90.0, 5),
    // 2: Bristle Downs. 3: Croaker Fen. 4: Greymane Wood.
    ("bristleback_boar", -3720.0, -230.0, 150.0, 7),
    ("fen_croaker", -3560.0, 1020.0, 160.0, 7),
    ("greymane_wolf", -4250.0, -450.0, 170.0, 6),
    // 6: brigands' camp on Wrackline Point. 7: Hexer's Tarn.
    ("wreck_brigand", -3260.0, -950.0, 110.0, 6),
    ("brine_hexer", -4750.0, 1450.0, 300.0, 5),
    // 9: the Cairn Crags. 10: Mossback Meadow. 11: Ashwing Scarp.
    ("cliff_stalker", -5050.0, -1250.0, 260.0, 5),
    ("mossback_tortoise", -5020.0, 320.0, 200.0, 4),
    ("ashwing_drone", -5520.0, 450.0, 260.0, 6),
    // 15: the Wreck Captain and a crew at Wreckers' Cove.
    ("wreck_captain", -3600.0, 1690.0, 50.0, 1),
    ("wreck_brigand", -3600.0, 1690.0, 130.0, 4),
];

/// Node fields: (kind, centre x, z, radius, nodes). The placeholder's scattering stops at the zone's
/// bounds; inside them, these are all there is.
pub const NODE_FIELDS: [(&str, f32, f32, f32, usize); 16] = [
    // Saltreed on the strand north of Furrowdale and in the fen.
    ("saltreed_clump", -3230.0, -150.0, 160.0, 18),
    ("saltreed_clump", -3560.0, 1020.0, 260.0, 18),
    // Herbs and berries round Furrowdale and on the downs.
    ("emberleaf_patch", -3540.0, 280.0, 140.0, 12),
    ("emberleaf_patch", -3780.0, -300.0, 220.0, 12),
    ("duskberry_bush", -3520.0, 640.0, 140.0, 12),
    ("duskberry_bush", -4150.0, 1450.0, 180.0, 10),
    // Driftpines along the coast, Hollowoaks in Greymane Wood and on the meadow.
    ("driftpine", -3380.0, -560.0, 200.0, 18),
    ("driftpine", -3480.0, 1450.0, 150.0, 10),
    ("hollowoak", -4250.0, -450.0, 320.0, 26),
    ("hollowoak", -5000.0, 120.0, 220.0, 12),
    // Ore: Tinstone at the foot of the crags and on the downs, Rustvein high on the crags and
    // the scarp.
    ("tinstone_vein", -4880.0, -980.0, 200.0, 12),
    ("tinstone_vein", -3820.0, -380.0, 120.0, 6),
    ("rustvein_outcrop", -5150.0, -1350.0, 220.0, 10),
    ("rustvein_outcrop", -5520.0, 300.0, 300.0, 12),
    // A few of each on Gullhaven's hills, so the island isn't bare.
    ("emberleaf_patch", 250.0, -250.0, 120.0, 6),
    ("driftpine", -250.0, -300.0, 150.0, 8),
];

/// Named areas, shown in the HUD: (name, x, z, radius). The zone itself and the island are the
/// largest; a position takes the smallest area it's in.
pub const AREAS: [(&str, f32, f32, f32); 17] = [
    ("Hearthshore", -4400.0, 150.0, 1900.0),
    ("Gullhaven Isle", 0.0, -100.0, 800.0),
    ("Gullhaven", 0.0, 0.0, 250.0),
    ("Furrowdale", -3330.0, 380.0, 90.0),
    ("Cairnwick", -4600.0, -1050.0, 55.0),
    ("Redfen", -3930.0, 1250.0, 55.0),
    ("Saltreed Strand", -3230.0, -150.0, 220.0),
    ("Bristle Downs", -3720.0, -280.0, 380.0),
    ("Croaker Fen", -3560.0, 1020.0, 330.0),
    ("Greymane Wood", -4250.0, -450.0, 340.0),
    ("Wrackline Point", -3200.0, -950.0, 260.0),
    ("Hexer's Tarn", -4750.0, 1450.0, 320.0),
    ("Cairn Crags", -5050.0, -1250.0, 480.0),
    ("Mossback Meadow", -5020.0, 250.0, 280.0),
    ("Ashwing Scarp", -5520.0, 400.0, 420.0),
    ("Wreckers' Cove", -3400.0, 1660.0, 300.0),
    ("Furrowdale Commons", -4300.0, 900.0, 500.0),
];

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sim::content::LandUse;
    use sim::terrain::FileChunks;

    use super::*;
    use crate::{World, texel_use};

    fn committed() -> (World, std::path::PathBuf) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        (World::load(&dir).unwrap(), dir)
    }

    #[test]
    fn landforms_only_touch_the_zone() {
        assert_eq!(shape(0.0, 0.0, 13.0), 13.0, "Gullhaven");
        assert_eq!(shape(-5200.0, 4000.0, 30.0), 30.0, "the west continent's south");
        assert!(shape(-5050.0, -1250.0, 30.0) > 60.0, "the Cairn Crags");
        assert!(shape(-4750.0, 1450.0, 25.0) < 0.0, "Hexer's Tarn is under water");
        assert!((shape(-3330.0, 380.0, 12.0) - 5.0).abs() < 1.0, "Furrowdale's pad");
    }

    /// The towns are dry and flat, the roads are roads, and the Wayshrine new characters start at
    /// stands in Furrowdale.
    #[test]
    fn towns_roads_and_the_start_on_the_committed_world() {
        let (world, dir) = committed();
        let mut chunks = FileChunks::new(&world.heights);
        for &(x, z, _) in &TOWNS {
            let g = sim::terrain::ground(&mut chunks, x, z).unwrap();
            assert!(g.lo > 1.0 && g.gradient < 0.15, "town at ({x}, {z}): {g:?}");
            assert_eq!(world.land_at(x, z).and_then(texel_use), Some(LandUse::Town));
        }
        for road in ROADS {
            for s in road.windows(2) {
                for k in 1..10 {
                    let t = k as f32 / 10.0;
                    let (x, z) = (s[0].0 + (s[1].0 - s[0].0) * t, s[0].1 + (s[1].1 - s[0].1) * t);
                    let u = world.land_at(x, z).and_then(texel_use);
                    assert!(matches!(u, Some(LandUse::Road | LandUse::Town)), "({x}, {z}) is {u:?}");
                }
            }
        }
        let start = sim::combat::SHRINES[usize::from(sim::content::START_SHRINE)].id;
        let shrine = crate::shrines::load(&dir).unwrap().into_iter().find(|s| s.kind.id == start).unwrap();
        let (fx, fz, fr) = TOWNS[1];
        assert!((shrine.x - fx).hypot(shrine.z - fz) < fr, "{start} is in Furrowdale");
    }

    /// Inside the zone every node belongs to a field of its kind, and every field is full.
    #[test]
    fn the_zone_has_only_its_fields_and_they_are_full() {
        let (_, dir) = committed();
        let nodes = crate::nodes::load(&dir).unwrap();
        for n in nodes.iter().filter(|n| in_zone(n.x, n.z)) {
            assert!(NODE_FIELDS.iter().any(|&(id, x, z, r, _)| id == n.kind.id && (n.x - x).hypot(n.z - z) <= r), "{n:?} is in no field");
        }
        for &(id, x, z, r, want) in &NODE_FIELDS {
            let got = nodes.iter().filter(|n| n.kind.id == id && (n.x - x).hypot(n.z - z) <= r).count();
            assert!(got >= want, "{id} at ({x}, {z}): {got} of {want}");
        }
    }

    /// Every NPC kind lives in the zone, and the camps stand in it or on Gullhaven's isle; the
    /// gentle kinds are nearest Furrowdale.
    #[test]
    fn camps_rise_in_level_away_from_furrowdale() {
        for k in &sim::npc::NPC_KINDS {
            assert!(CAMPS.iter().any(|c| c.0 == k.id && in_zone(c.1, c.2)), "no {} in the zone", k.id);
        }
        let (fx, fz, _) = TOWNS[1];
        let level = |id: &str| sim::npc::npc_kind_by_id(id).unwrap().level;
        let far = |c: &(&str, f32, f32, f32, usize)| (c.1 - fx).hypot(c.2 - fz);
        for c in CAMPS.iter().filter(|c| in_zone(c.1, c.2)) {
            assert!(level(c.0) > 1 || far(c) < 400.0, "{} is level 1 but {:.0} m out", c.0, far(c));
            assert!(level(c.0) < 4 || far(c) > 1000.0, "{} is level {} but only {:.0} m out", c.0, level(c.0), far(c));
        }
        for c in CAMPS.iter().filter(|c| !in_zone(c.1, c.2)) {
            assert!(c.1.hypot(c.2) < 800.0, "{} at ({}, {}) is neither in the zone nor on Gullhaven's isle", c.0, c.1, c.2);
        }
    }

    /// The trade loop's distances: Redfen and Cairnwick a walk away, Gullhaven a sail across open
    /// water.
    #[test]
    fn posts_are_a_walk_and_a_sail_from_the_bench() {
        let (world, _) = committed();
        let bench = STATIONS.iter().find(|s| s.0 == "packwright_bench").unwrap();
        let mut chunks = FileChunks::new(&world.heights);
        for &(id, x, z, _) in &POSTS {
            let d = (x - bench.1).hypot(z - bench.2);
            let wet = (1..100).filter(|k| {
                let t = *k as f32 / 100.0;
                sim::terrain::ground(&mut chunks, bench.1 + (x - bench.1) * t, bench.2 + (z - bench.2) * t).unwrap().lo < 0.0
            });
            match id {
                "gullhaven_post" => assert!(d > 2500.0 && wet.count() > 60, "{id}: {d:.0} m"),
                _ => assert!((800.0..2500.0).contains(&d) && wet.count() < 10, "{id}: {d:.0} m"),
            }
        }
    }
}
