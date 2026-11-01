//! Quest giver placements (`world/givers.tsv`, docs/design/quests.md), in the same format as
//! `shrines.tsv`: `giver x y z yaw-degrees`, givers by content id. One stands in each town, where
//! the coast layout puts it (`coast::GIVERS`), set on the ground.

use std::path::Path;

use sim::quest::{GiverData, giver_by_id};
use sim::terrain::FileChunks;

use crate::World;
use crate::nodes::{parse_lines, round2};

pub const FILE: &str = "givers.tsv";

/// Where they stand (`coast::GIVERS`).
pub use crate::coast::GIVERS;

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: &'static GiverData,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw_deg: u16,
}

/// The placement: each giver set on the ground where it stands.
pub fn place(world: &World) -> Vec<Placement> {
    let mut chunks = FileChunks::new(&world.heights);
    GIVERS
        .iter()
        .filter_map(|&(id, x, z, yaw_deg)| {
            let kind = giver_by_id(id)?;
            let g = sim::terrain::ground(&mut chunks, x, z)?;
            Some(Placement { kind, x, y: round2(g.lo), z, yaw_deg })
        })
        .collect()
}

pub fn to_text(givers: &[Placement]) -> String {
    let mut s = String::from("# Quest givers (docs/design/quests.md): giver, x, y, z, yaw in degrees. Written by worldtool: don't edit by hand.\n");
    for p in givers {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\t{}\n", p.kind.id, p.x, p.y, p.z, p.yaw_deg);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Placement>, String> {
    parse_lines(text, "giver", giver_by_id).map(|rows| rows.into_iter().map(|(kind, x, y, z, yaw_deg)| Placement { kind, x, y, z, yaw_deg }).collect())
}

pub fn load(dir: &Path) -> Result<Vec<Placement>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, givers: &[Placement]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(givers)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texel_use;
    use sim::content::LandUse;

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let kind = giver_by_id("yeva_mossgrove").unwrap();
        let rows = vec![Placement { kind, x: -3888.0, y: 25.5, z: 1788.0, yaw_deg: 0 }];
        assert_eq!(parse(&to_text(&rows)).unwrap(), rows);
        assert!(parse("hermit\t0\t0\t0\t0").unwrap_err().contains("no giver kind"));
        assert!(parse("yeva_mossgrove\t0\t0\t0").unwrap_err().contains("5 tab-separated"));
        assert!(parse("yeva_mossgrove\t0\t0\t9000\t0").unwrap_err().contains("off the map"));
    }

    /// The committed givers load, stand on gentle town ground on the committed world beside a
    /// Wayshrine, and place each giver once.
    #[test]
    fn committed_givers_stand_in_towns() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        let world = World::load(&dir).unwrap();
        let givers = load(&dir).unwrap();
        let shrines = crate::shrines::load(&dir).unwrap();
        let mut chunks = FileChunks::new(&world.heights);
        for p in &givers {
            assert!(shrines.iter().any(|s| (s.x - p.x).hypot(s.z - p.z) < 20.0), "{p:?} is by no Wayshrine");
            assert_eq!(world.land_at(p.x, p.z).and_then(texel_use), Some(LandUse::Town), "{p:?}");
            let g = sim::terrain::ground(&mut chunks, p.x, p.z).unwrap();
            assert!((p.y - g.lo).abs() < 0.01 && p.y > 1.0, "{p:?} on {g:?}");
            assert!(g.gradient < 0.3, "{p:?} stands on a slope of {}", g.gradient);
        }
        for k in &sim::quest::GIVERS {
            assert_eq!(givers.iter().filter(|p| p.kind == k).count(), 1, "{}", k.id);
        }
        assert_eq!(place(&world), givers, "the committed givers are stale; run `cargo run -p worldtool -- givers`");
    }
}
