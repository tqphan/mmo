//! Trade post placements (`world/posts.tsv`, docs/design/trade.md), in the same format as
//! `stations.tsv`: `kind x y z yaw-degrees`, kinds by content id. For now there is one of each post
//! kind at fixed spots in the placeholder towns, set on the ground; the coast zone will be placed by
//! hand (M1, the coast task).

use std::path::Path;

use sim::terrain::FileChunks;
use sim::trade::{PostData, post_by_id};

use crate::World;
use crate::nodes::{parse_lines, round2};

pub const FILE: &str = "posts.tsv";

/// Where the posts stand: (kind, x, z, yaw in degrees). Cairnwick and Redfen are hamlets on the
/// west continent near Furrowdale's Packwright's Bench; Gullhaven is the trade island's town round
/// spawn, across the sea (`placeholder::TOWNS`).
pub const POSTS: [(&str, f32, f32, u16); 3] =
    [("cairnwick_post", -4500.0, -1400.0, 90), ("redfen_post", -3900.0, 1800.0, 0), ("gullhaven_post", -10.0, 4.0, 90)];

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: &'static PostData,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw_deg: u16,
}

/// The placeholder placement: each post set on the ground where it stands.
pub fn place(world: &World) -> Vec<Placement> {
    let mut chunks = FileChunks::new(&world.heights);
    POSTS
        .iter()
        .filter_map(|&(id, x, z, yaw_deg)| {
            let kind = post_by_id(id)?;
            let g = sim::terrain::ground(&mut chunks, x, z)?;
            Some(Placement { kind, x, y: round2(g.lo), z, yaw_deg })
        })
        .collect()
}

pub fn to_text(posts: &[Placement]) -> String {
    let mut s = String::from("# Trade posts (docs/design/trade.md): kind, x, y, z, yaw in degrees. Written by worldtool: don't edit by hand.\n");
    for p in posts {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\t{}\n", p.kind.id, p.x, p.y, p.z, p.yaw_deg);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Placement>, String> {
    parse_lines(text, "post", post_by_id).map(|rows| rows.into_iter().map(|(kind, x, y, z, yaw_deg)| Placement { kind, x, y, z, yaw_deg }).collect())
}

pub fn load(dir: &Path) -> Result<Vec<Placement>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, posts: &[Placement]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(posts)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texel_use;
    use sim::content::LandUse;

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let post = post_by_id("redfen_post").unwrap();
        let rows = vec![Placement { kind: post, x: -3900.0, y: 25.5, z: 1800.0, yaw_deg: 0 }];
        assert_eq!(parse(&to_text(&rows)).unwrap(), rows);
        assert!(parse("market\t0\t0\t0\t0").unwrap_err().contains("no post kind"));
        assert!(parse("redfen_post\t0\t0\t0").unwrap_err().contains("5 tab-separated"));
        assert!(parse("redfen_post\t0\t0\t9000\t0").unwrap_err().contains("off the map"));
    }

    /// The committed posts load, stand on gentle town ground on the committed world, and place each
    /// kind once, far enough from the Packwright's Bench to buy its packs.
    #[test]
    fn committed_posts_stand_in_towns() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(crate::DEFAULT_DIR);
        let world = World::load(&dir).unwrap();
        let posts = load(&dir).unwrap();
        let mut chunks = FileChunks::new(&world.heights);
        for p in &posts {
            assert_eq!(world.land_at(p.x, p.z).and_then(texel_use), Some(LandUse::Town), "{p:?}");
            let g = sim::terrain::ground(&mut chunks, p.x, p.z).unwrap();
            assert!((p.y - g.lo).abs() < 0.01 && p.y > 1.0, "{p:?} on {g:?}");
            assert!(g.gradient < 0.3, "{p:?} stands on a slope of {}", g.gradient);
        }
        for k in &sim::trade::POSTS {
            assert_eq!(posts.iter().filter(|p| p.kind == k).count(), 1, "{}", k.id);
        }
        let stations = crate::stations::load(&dir).unwrap();
        let bench = stations.iter().find(|s| s.kind.id == "packwright_bench").expect("a Packwright's Bench");
        for p in &posts {
            assert!(sim::trade::route_pct([bench.x, bench.z], [p.x, p.z], 0).is_some(), "{} is too close to the bench", p.kind.id);
        }
        assert_eq!(place(&world), posts, "the committed posts are stale; run `cargo run -p worldtool -- posts`");
    }
}
