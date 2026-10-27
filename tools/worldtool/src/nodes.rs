//! Resource node placements (`world/nodes.tsv`, docs/design/gathering.md): where nodes stand, one
//! per line as `kind x y z yaw-degrees`, kinds by content id. The placeholder placement scatters
//! them over wild land; the coast zone will be placed by hand (M1 task 11).

use std::path::Path;

use sim::cell::{CELL_SIZE_M, CELLS_PER_SIDE, WORLD_HALF_M};
use sim::content::{Climate, LandUse};
use sim::gather::{NodeData, node_by_id};
use sim::terrain::FileChunks;

use crate::{World, texel_climate, texel_use};

pub const FILE: &str = "nodes.tsv";
/// Kept candidates, one per 64 m cell at most.
const KEEP: f32 = 0.5;
/// Below this the ground is beach or tidal.
const MIN_HEIGHT_M: f32 = 1.0;
/// Steeper than this, nothing is placed: a gatherer has to stand beside it.
const MAX_GRADIENT: f32 = 0.7;

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: &'static NodeData,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw_deg: u16,
}

/// What grows on ground of this height, slope and climate, for a roll from 0 to 1.
pub fn kind_for(height: f32, gradient: f32, climate: Climate, roll: f32) -> Option<&'static NodeData> {
    if height < MIN_HEIGHT_M || gradient > MAX_GRADIENT {
        return None;
    }
    let id = if height > 80.0 {
        "rustvein_outcrop"
    } else if gradient > 0.25 || (height > 30.0 && roll > 0.85) {
        "tinstone_vein"
    } else if height < 6.0 {
        "saltreed_clump"
    } else if height < 15.0 || roll < 0.4 {
        match climate {
            Climate::Tropical => "duskberry_bush",
            Climate::Arid | Climate::Subarctic => "emberleaf_patch",
            Climate::Temperate if roll < 0.2 => "duskberry_bush",
            Climate::Temperate => "emberleaf_patch",
        }
    } else if height < 25.0 {
        "driftpine"
    } else {
        "hollowoak"
    };
    node_by_id(id)
}

/// A deterministic number from 0 to 1 for a cell and a salt (splitmix64).
fn unit(cx: u16, cz: u16, salt: u64) -> f32 {
    let mut v = (u64::from(cx) << 32 | u64::from(cz)) ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    v ^= v >> 31;
    (v >> 40) as f32 / (1u64 << 24) as f32
}

/// The placeholder placement: one candidate per cell at a random spot, about half kept, on wild
/// land only, the kind following the ground.
pub fn place(world: &World) -> Vec<Placement> {
    let mut out = Vec::new();
    for cz in 0..CELLS_PER_SIDE {
        // One row of cells at a time, so the cut chunks don't pile up.
        let mut chunks = FileChunks::new(&world.heights);
        for cx in 0..CELLS_PER_SIDE {
            if unit(cx, cz, 1) >= KEEP {
                continue;
            }
            let x = -WORLD_HALF_M + (f32::from(cx) + 0.1 + 0.8 * unit(cx, cz, 2)) * CELL_SIZE_M;
            let z = -WORLD_HALF_M + (f32::from(cz) + 0.1 + 0.8 * unit(cx, cz, 3)) * CELL_SIZE_M;
            let Some(t) = world.land_at(x, z) else { continue };
            if texel_use(t) != Some(LandUse::Wild) {
                continue;
            }
            let Some(g) = sim::terrain::ground(&mut chunks, x, z) else { continue };
            let climate = texel_climate(t).unwrap_or(Climate::Temperate);
            if let Some(kind) = kind_for(g.lo, g.gradient, climate, unit(cx, cz, 4)) {
                let (x, z) = (round2(x), round2(z));
                let y = sim::terrain::ground(&mut chunks, x, z).map_or(g.lo, |g| g.lo);
                out.push(Placement { kind, x, y: round2(y), z, yaw_deg: (unit(cx, cz, 5) * 360.0) as u16 % 360 });
            }
        }
    }
    out
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

pub fn to_text(nodes: &[Placement]) -> String {
    let mut s = String::from("# Resource nodes (docs/design/gathering.md): kind, x, y, z, yaw in degrees. Written by worldtool: don't edit by hand.\n");
    for n in nodes {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\t{}\n", n.kind.id, n.x, n.y, n.z, n.yaw_deg);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Placement>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", i + 1);
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            return Err(at(format!("expected 5 tab-separated fields, found {}", f.len())));
        }
        let kind = node_by_id(f[0]).ok_or_else(|| at(format!("no node kind {:?}", f[0])))?;
        let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| at(format!("bad number {s:?}")));
        let (x, y, z) = (num(f[1])?, num(f[2])?, num(f[3])?);
        if x.abs() >= WORLD_HALF_M || z.abs() >= WORLD_HALF_M {
            return Err(at("off the map".into()));
        }
        let yaw_deg = f[4].parse::<u16>().ok().filter(|&d| d < 360).ok_or_else(|| at(format!("bad yaw {:?}", f[4])))?;
        out.push(Placement { kind, x, y, z, yaw_deg });
    }
    Ok(out)
}

pub fn load(dir: &Path) -> Result<Vec<Placement>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, nodes: &[Placement]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(nodes)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::content::NodeKind;

    fn id(k: Option<&NodeData>) -> &str {
        k.map_or("none", |k| k.id)
    }

    #[test]
    fn kinds_follow_the_ground() {
        let t = Climate::Temperate;
        assert_eq!(id(kind_for(0.5, 0.0, t, 0.5)), "none", "the beach");
        assert_eq!(id(kind_for(30.0, 0.8, t, 0.5)), "none", "too steep to stand by");
        assert_eq!(id(kind_for(3.0, 0.1, t, 0.9)), "saltreed_clump");
        assert_eq!(id(kind_for(10.0, 0.1, Climate::Tropical, 0.9)), "duskberry_bush");
        assert_eq!(id(kind_for(10.0, 0.1, Climate::Arid, 0.9)), "emberleaf_patch");
        assert_eq!(id(kind_for(20.0, 0.1, t, 0.7)), "driftpine");
        assert_eq!(id(kind_for(40.0, 0.1, t, 0.7)), "hollowoak");
        assert_eq!(id(kind_for(40.0, 0.1, t, 0.1)), "duskberry_bush", "some plants inland too");
        assert_eq!(id(kind_for(40.0, 0.5, t, 0.5)), "tinstone_vein");
        assert_eq!(id(kind_for(40.0, 0.1, t, 0.9)), "tinstone_vein", "some ore in the hills");
        assert_eq!(id(kind_for(120.0, 0.1, t, 0.9)), "rustvein_outcrop");
    }

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let reed = node_by_id("saltreed_clump").unwrap();
        let oak = node_by_id("hollowoak").unwrap();
        let nodes = vec![
            Placement { kind: reed, x: -12.5, y: 3.25, z: 7.0, yaw_deg: 0 },
            Placement { kind: oak, x: 7999.0, y: 41.0, z: -7999.0, yaw_deg: 359 },
        ];
        assert_eq!(parse(&to_text(&nodes)).unwrap(), nodes);
        assert!(parse("pearl_bed\t0\t0\t0\t0").unwrap_err().contains("no node kind"));
        assert!(parse("hollowoak\t0\t0\t0").unwrap_err().contains("5 tab-separated"));
        assert!(parse("hollowoak\t0\tNaN\t0\t0").unwrap_err().contains("bad number"));
        assert!(parse("hollowoak\t8000\t0\t0\t0").unwrap_err().contains("off the map"));
        assert!(parse("hollowoak\t0\t0\t0\t360").unwrap_err().contains("bad yaw"));
        assert!(parse("\n# a comment\n").unwrap().is_empty());
    }

    /// The committed placements load, stand on wild land above the sea on the committed world, and
    /// cover every kind.
    #[test]
    fn committed_nodes_stand_on_the_committed_world() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        let world = World::load(&dir).unwrap();
        let nodes = load(&dir).unwrap();
        assert!(nodes.len() > 1000, "{} nodes", nodes.len());
        let mut chunks = FileChunks::new(&world.heights);
        for n in &nodes {
            assert_eq!(world.land_at(n.x, n.z).and_then(texel_use), Some(LandUse::Wild), "{n:?}");
            let g = sim::terrain::ground(&mut chunks, n.x, n.z).unwrap();
            assert!((n.y - g.lo).abs() < 0.01 && n.y >= MIN_HEIGHT_M, "{n:?} on {g:?}");
        }
        for k in &sim::gather::NODES {
            assert!(nodes.iter().any(|n| n.kind == k), "no {}", k.id);
        }
        for kind in [NodeKind::Plant, NodeKind::Ore, NodeKind::Tree] {
            assert!(nodes.iter().filter(|n| n.kind.kind == kind).count() > 100, "{kind:?}");
        }
        assert_eq!(place(&world), nodes, "the committed placements are stale; run `cargo run -p worldtool -- nodes`");
    }
}
