//! `worldtool gen [--dir D]` writes the placeholder world to `world/` (or D), resource nodes and
//! crafting stations included; `worldtool nodes [--dir D]` and `worldtool stations [--dir D]`
//! rewrite only those, from the world there; `worldtool info [--dir D]` loads it, checks it and
//! prints what's in it. Paths are relative to the repo root,
//! which `cargo xtask` runs from (docs/design/world.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use sim::content::{CLIMATE_IDS, LAND_USES};
use worldtool::{DEFAULT_DIR, World, texel_climate, texel_use};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(args.iter().position(|a| a == "--dir").and_then(|i| args.get(i + 1)).map_or(DEFAULT_DIR, String::as_str));
    let result = match args.first().map(String::as_str) {
        Some("gen") => generate(&dir),
        Some("nodes") => World::load(&dir).and_then(|w| write_nodes(&dir, &w)),
        Some("stations") => World::load(&dir).and_then(|w| write_stations(&dir, &w)),
        Some("info") => info(&dir),
        _ => {
            eprintln!("usage: worldtool gen|nodes|stations|info [--dir DIR]");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("worldtool: {e}");
            ExitCode::FAILURE
        }
    }
}

fn generate(dir: &std::path::Path) -> Result<(), String> {
    let started = Instant::now();
    let world = worldtool::placeholder::generate();
    let hash = world.save(dir)?;
    println!("wrote the placeholder world to {} in {:.1} s, terrain hash {hash:016x}", dir.display(), started.elapsed().as_secs_f32());
    summary(&world);
    write_nodes(dir, &world)?;
    write_stations(dir, &world)
}

/// Place crafting stations on `world` and write them to `dir` (docs/design/crafting.md).
fn write_stations(dir: &std::path::Path, world: &World) -> Result<(), String> {
    let stations = worldtool::stations::place(world);
    worldtool::stations::save(dir, &stations)?;
    println!("wrote {} crafting stations to {}", stations.len(), dir.join(worldtool::stations::FILE).display());
    Ok(())
}

/// Place resource nodes on `world` and write them to `dir` (docs/design/gathering.md).
fn write_nodes(dir: &std::path::Path, world: &World) -> Result<(), String> {
    let nodes = worldtool::nodes::place(world);
    worldtool::nodes::save(dir, &nodes)?;
    println!("wrote {} resource nodes to {}", nodes.len(), dir.join(worldtool::nodes::FILE).display());
    node_summary(&nodes);
    Ok(())
}

fn node_summary(nodes: &[worldtool::nodes::Placement]) {
    for k in &sim::gather::NODES {
        println!("  node {:<18} {}", k.id, nodes.iter().filter(|n| n.kind == k).count());
    }
}

fn info(dir: &std::path::Path) -> Result<(), String> {
    let started = Instant::now();
    let world = World::load(dir)?;
    println!("loaded {} in {:.1} s, terrain hash {:016x}", dir.display(), started.elapsed().as_secs_f32(), world.terrain_hash());
    summary(&world);
    let nodes = worldtool::nodes::load(dir)?;
    println!("{} resource nodes", nodes.len());
    node_summary(&nodes);
    let stations = worldtool::stations::load(dir)?;
    println!("{} crafting stations", stations.len());
    for s in &stations {
        println!("  {:<14} ({:.1}, {:.1}, {:.1})", s.kind.id, s.x, s.y, s.z);
    }
    Ok(())
}

/// How much of the map is land, and the share of each use and climate.
fn summary(w: &World) {
    let sea = sim::terrain::encode(0.0);
    let land = w.heights.samples.iter().filter(|&&s| s > sea).count();
    println!("{:.0}% land", 100.0 * land as f64 / w.heights.samples.len() as f64);
    let share = |n: usize| 100.0 * n as f64 / w.land.len() as f64;
    for u in &LAND_USES {
        let n = w.land.iter().filter(|&&t| texel_use(t) == Some(u.land_use)).count();
        println!("  use {:<10} {:6.2}% of texels ({:.1} km²)", u.id, share(n), n as f64 * 16.0 / 1e6);
    }
    for (wire, id) in CLIMATE_IDS.iter().enumerate() {
        let n = w.land.iter().filter(|&&t| texel_climate(t).map(|c| c as usize) == Some(wire)).count();
        println!("  climate {id:<10} {:6.2}%", share(n));
    }
}
