//! Crafting station placements (`world/stations.tsv`, docs/design/crafting.md), in the same format
//! as `nodes.tsv`: `kind x y z yaw-degrees`, kinds by content id. For now there is one of each
//! station in the town round spawn, at fixed spots set on the ground; the coast zone will be placed
//! by hand (M1, the coast task).

use std::path::Path;

use sim::craft::{StationData, station_by_id};
use sim::terrain::FileChunks;

use crate::World;
use crate::nodes::{parse_lines, round2};

pub const FILE: &str = "stations.tsv";

/// Where the town's stations stand: (kind, x, z, yaw in degrees), a few metres east of spawn.
const TOWN: [(&str, f32, f32, u16); 3] = [("joiners_bench", 9.0, -5.0, 270), ("bloomery", 12.0, 2.0, 270), ("cookhearth", 8.0, 8.0, 0)];

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: &'static StationData,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw_deg: u16,
}

/// The placeholder placement: the town's stations, each set on the ground where it stands.
pub fn place(world: &World) -> Vec<Placement> {
    let mut chunks = FileChunks::new(&world.heights);
    TOWN.iter()
        .filter_map(|&(id, x, z, yaw_deg)| {
            let kind = station_by_id(id)?;
            let g = sim::terrain::ground(&mut chunks, x, z)?;
            Some(Placement { kind, x, y: round2(g.lo), z, yaw_deg })
        })
        .collect()
}

pub fn to_text(stations: &[Placement]) -> String {
    let mut s = String::from("# Crafting stations (docs/design/crafting.md): kind, x, y, z, yaw in degrees. Written by worldtool: don't edit by hand.\n");
    for p in stations {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\t{}\n", p.kind.id, p.x, p.y, p.z, p.yaw_deg);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Placement>, String> {
    parse_lines(text, "station", station_by_id).map(|rows| rows.into_iter().map(|(kind, x, y, z, yaw_deg)| Placement { kind, x, y, z, yaw_deg }).collect())
}

pub fn load(dir: &Path) -> Result<Vec<Placement>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, stations: &[Placement]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(stations)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texel_use;
    use sim::content::LandUse;

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let bench = station_by_id("joiners_bench").unwrap();
        let rows = vec![Placement { kind: bench, x: 9.0, y: 13.25, z: -5.0, yaw_deg: 270 }];
        assert_eq!(parse(&to_text(&rows)).unwrap(), rows);
        assert!(parse("kiln\t0\t0\t0\t0").unwrap_err().contains("no station kind"));
        assert!(parse("bloomery\t0\t0\t0").unwrap_err().contains("5 tab-separated"));
        assert!(parse("bloomery\t8000\t0\t0\t0").unwrap_err().contains("off the map"));
    }

    /// The committed stations load, stand on gentle town ground on the committed world, and cover
    /// every kind.
    #[test]
    fn committed_stations_stand_in_the_town() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        let world = World::load(&dir).unwrap();
        let stations = load(&dir).unwrap();
        let mut chunks = FileChunks::new(&world.heights);
        for s in &stations {
            assert_eq!(world.land_at(s.x, s.z).and_then(texel_use), Some(LandUse::Town), "{s:?}");
            let g = sim::terrain::ground(&mut chunks, s.x, s.z).unwrap();
            assert!((s.y - g.lo).abs() < 0.01 && s.y > 1.0, "{s:?} on {g:?}");
            assert!(g.gradient < 0.3, "{s:?} stands on a slope of {}", g.gradient);
        }
        for k in &sim::craft::STATIONS {
            assert!(stations.iter().any(|s| s.kind == k), "no {}", k.id);
        }
        assert_eq!(place(&world), stations, "the committed stations are stale; run `cargo run -p worldtool -- stations`");
    }
}
