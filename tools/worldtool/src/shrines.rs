//! Wayshrine placements (`world/shrines.tsv`, docs/design/combat.md), in the same format as
//! `posts.tsv`: `kind x y z yaw-degrees`, kinds by content id. For now there is one of each kind in
//! each placeholder town, set on the ground; the coast zone will be placed by hand (M1, the coast
//! task).

use std::path::Path;

use sim::combat::{ShrineData, shrine_by_id};
use sim::terrain::FileChunks;

use crate::World;
use crate::nodes::{parse_lines, round2};

pub const FILE: &str = "shrines.tsv";

/// Where the Wayshrines stand: (kind, x, z, yaw in degrees), a short walk from each town's post or
/// bench (`placeholder::TOWNS`).
pub const SHRINES: [(&str, f32, f32, u16); 4] = [
    ("gullhaven_wayshrine", 0.0, -15.0, 0),
    ("furrowdale_wayshrine", -3812.0, 588.0, 90),
    ("cairnwick_wayshrine", -4488.0, -1412.0, 90),
    ("redfen_wayshrine", -3888.0, 1788.0, 0),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: &'static ShrineData,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw_deg: u16,
}

/// The placeholder placement: each Wayshrine set on the ground where it stands.
pub fn place(world: &World) -> Vec<Placement> {
    let mut chunks = FileChunks::new(&world.heights);
    SHRINES
        .iter()
        .filter_map(|&(id, x, z, yaw_deg)| {
            let kind = shrine_by_id(id)?;
            let g = sim::terrain::ground(&mut chunks, x, z)?;
            Some(Placement { kind, x, y: round2(g.lo), z, yaw_deg })
        })
        .collect()
}

pub fn to_text(shrines: &[Placement]) -> String {
    let mut s = String::from("# Wayshrines (docs/design/combat.md): kind, x, y, z, yaw in degrees. Written by worldtool: don't edit by hand.\n");
    for p in shrines {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\t{}\n", p.kind.id, p.x, p.y, p.z, p.yaw_deg);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Placement>, String> {
    parse_lines(text, "shrine", shrine_by_id).map(|rows| rows.into_iter().map(|(kind, x, y, z, yaw_deg)| Placement { kind, x, y, z, yaw_deg }).collect())
}

pub fn load(dir: &Path) -> Result<Vec<Placement>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, shrines: &[Placement]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(shrines)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texel_use;
    use sim::content::LandUse;

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let kind = shrine_by_id("redfen_wayshrine").unwrap();
        let rows = vec![Placement { kind, x: -3888.0, y: 25.5, z: 1788.0, yaw_deg: 0 }];
        assert_eq!(parse(&to_text(&rows)).unwrap(), rows);
        assert!(parse("altar\t0\t0\t0\t0").unwrap_err().contains("no shrine kind"));
        assert!(parse("redfen_wayshrine\t0\t0\t0").unwrap_err().contains("5 tab-separated"));
        assert!(parse("redfen_wayshrine\t0\t0\t9000\t0").unwrap_err().contains("off the map"));
    }

    /// The committed Wayshrines load, stand on gentle town ground on the committed world, and
    /// place each kind once.
    #[test]
    fn committed_shrines_stand_in_towns() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        let world = World::load(&dir).unwrap();
        let shrines = load(&dir).unwrap();
        let mut chunks = FileChunks::new(&world.heights);
        for p in &shrines {
            assert_eq!(world.land_at(p.x, p.z).and_then(texel_use), Some(LandUse::Town), "{p:?}");
            let g = sim::terrain::ground(&mut chunks, p.x, p.z).unwrap();
            assert!((p.y - g.lo).abs() < 0.01 && p.y > 1.0, "{p:?} on {g:?}");
            assert!(g.gradient < 0.3, "{p:?} stands on a slope of {}", g.gradient);
        }
        for k in &sim::combat::SHRINES {
            assert_eq!(shrines.iter().filter(|p| p.kind == k).count(), 1, "{}", k.id);
        }
        assert_eq!(place(&world), shrines, "the committed shrines are stale; run `cargo run -p worldtool -- shrines`");
    }
}
