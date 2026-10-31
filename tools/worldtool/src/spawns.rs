//! NPC spawn points (`world/spawns.tsv`, docs/design/npcs.md), in the same format as `nodes.tsv`:
//! `kind x y z yaw-degrees`, kinds by content id. The placeholder placement puts a camp of each kind
//! on dry wild land, rising in level with the distance from Gullhaven; the coast task lays them out
//! by hand.

use std::path::Path;

use sim::content::LandUse;
use sim::npc::{NpcKindData, npc_kind_by_id};
use sim::terrain::FileChunks;

use crate::nodes::{parse_lines, round2};
use crate::{World, texel_use};

pub const FILE: &str = "spawns.tsv";
/// Lower than this the ground is beach or tidal: only Skitters live there.
const MIN_HEIGHT_M: f32 = 1.0;
/// Steeper than this, nothing is placed.
const MAX_GRADIENT: f32 = 0.6;
/// Spawn points of one camp stand at least this far apart.
const SPACING_M: f32 = 8.0;
/// Candidates tried per camp.
const TRIES: u64 = 400;

/// The camps: (kind, centre x, z, radius, spawn points). Skitters on Gullhaven's beaches, Boars and
/// Croakers on its hills, the rest round Furrowdale across the sea, further out as they get stronger.
pub const CAMPS: [(&str, f32, f32, f32, usize); 12] = [
    ("shore_skitter", 330.0, 0.0, 90.0, 6),
    ("shore_skitter", -320.0, 80.0, 90.0, 4),
    ("bristleback_boar", 380.0, 220.0, 120.0, 5),
    ("fen_croaker", 360.0, -260.0, 120.0, 5),
    ("greymane_wolf", -3560.0, 820.0, 140.0, 5),
    ("wreck_brigand", -3620.0, 260.0, 120.0, 5),
    ("brine_hexer", -4400.0, 1300.0, 140.0, 4),
    ("cliff_stalker", -4300.0, -900.0, 160.0, 4),
    ("mossback_tortoise", -3450.0, 1400.0, 140.0, 4),
    ("ashwing_drone", -4750.0, -500.0, 160.0, 5),
    ("wreck_captain", -4050.0, 2150.0, 160.0, 1),
    ("wreck_brigand", -4050.0, 2150.0, 160.0, 4),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: &'static NpcKindData,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw_deg: u16,
}

/// A deterministic number from 0 to 1 (splitmix64).
fn unit(seed: u64) -> f32 {
    let mut v = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    v ^= v >> 31;
    (v >> 40) as f32 / (1u64 << 24) as f32
}

/// The placeholder placement: for each camp, deterministic candidates round its centre, kept on
/// dry, gentle wild land and apart from each other, until the camp has its count.
pub fn place(world: &World) -> Vec<Placement> {
    let mut chunks = FileChunks::new(&world.heights);
    let mut out: Vec<Placement> = Vec::new();
    for (c, &(id, cx, cz, radius, count)) in CAMPS.iter().enumerate() {
        let Some(kind) = npc_kind_by_id(id) else { continue };
        let mut placed = 0;
        for i in 0..TRIES {
            if placed == count {
                break;
            }
            let seed = (c as u64) << 32 | i;
            let r = radius * unit(seed * 3).sqrt();
            let a = unit(seed * 3 + 1) * std::f32::consts::TAU;
            let (x, z) = (round2(cx + r * a.cos()), round2(cz + r * a.sin()));
            if world.land_at(x, z).and_then(texel_use) != Some(LandUse::Wild) {
                continue;
            }
            let Some(g) = sim::terrain::ground(&mut chunks, x, z) else { continue };
            if g.lo < MIN_HEIGHT_M || g.gradient > MAX_GRADIENT || out.iter().any(|p| (p.x - x).hypot(p.z - z) < SPACING_M) {
                continue;
            }
            out.push(Placement { kind, x, y: round2(g.lo), z, yaw_deg: (unit(seed * 3 + 2) * 360.0) as u16 % 360 });
            placed += 1;
        }
    }
    out
}

pub fn to_text(spawns: &[Placement]) -> String {
    let mut s = String::from("# NPC spawn points (docs/design/npcs.md): kind, x, y, z, yaw in degrees. Written by worldtool: don't edit by hand.\n");
    for p in spawns {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\t{}\n", p.kind.id, p.x, p.y, p.z, p.yaw_deg);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Placement>, String> {
    parse_lines(text, "NPC", npc_kind_by_id).map(|rows| rows.into_iter().map(|(kind, x, y, z, yaw_deg)| Placement { kind, x, y, z, yaw_deg }).collect())
}

pub fn load(dir: &Path) -> Result<Vec<Placement>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, spawns: &[Placement]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(spawns)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let kind = npc_kind_by_id("greymane_wolf").unwrap();
        let rows = vec![Placement { kind, x: -3560.0, y: 12.5, z: 820.0, yaw_deg: 90 }];
        assert_eq!(parse(&to_text(&rows)).unwrap(), rows);
        assert!(parse("dragon\t0\t0\t0\t0").unwrap_err().contains("no NPC kind"));
        assert!(parse("greymane_wolf\t0\t0\t0").unwrap_err().contains("5 tab-separated"));
        assert!(parse("greymane_wolf\t9000\t0\t0\t0").unwrap_err().contains("off the map"));
    }

    /// The committed spawn points load, stand on dry, gentle wild land on the committed world,
    /// place every kind, and are what the placement makes.
    #[test]
    fn committed_spawns_stand_on_dry_wild_land() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        let world = World::load(&dir).unwrap();
        let spawns = load(&dir).unwrap();
        let mut chunks = FileChunks::new(&world.heights);
        for p in &spawns {
            assert_eq!(world.land_at(p.x, p.z).and_then(texel_use), Some(LandUse::Wild), "{p:?}");
            let g = sim::terrain::ground(&mut chunks, p.x, p.z).unwrap();
            assert!((p.y - g.lo).abs() < 0.01 && p.y >= MIN_HEIGHT_M, "{p:?} on {g:?}");
        }
        for k in &sim::npc::NPC_KINDS {
            assert!(spawns.iter().any(|p| p.kind == k), "{} has no spawn point", k.id);
        }
        let wanted: usize = CAMPS.iter().map(|c| c.4).sum();
        assert_eq!(spawns.len(), wanted, "every camp is full");
        assert_eq!(place(&world), spawns, "the committed spawns are stale; run `cargo run -p worldtool -- spawns`");
    }
}
