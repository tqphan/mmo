//! `worldtool gen [--dir D]` writes the placeholder world to `world/` (or D), resource nodes,
//! crafting stations, trade posts, Wayshrines, quest givers, NPC spawn points and named areas
//! included; `worldtool nodes`, `stations`, `posts`, `shrines`, `givers`, `spawns` and `areas` (each `[--dir D]`)
//! rewrite only those, from the world there; `worldtool info [--dir D]` loads it, checks it and
//! prints what's in it. `worldtool probe X,Z...` prints the ground at points, and `worldtool map
//! --area X0,Z0,X1,Z1 [--px N] [--out F]` draws part of the world as a PNG (docs/design/coast.md).
//! `worldtool apply-terrain3d [--from D]` writes the client's Terrain3D export (`heights.mmoh` and
//! `uses.raw` in D, default `target/terrain3d/export`) back into the world (ADR 0032).
//! Paths are relative to the repo root, which `cargo xtask` runs from (docs/design/world.md).

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
        Some("posts") => World::load(&dir).and_then(|w| write_posts(&dir, &w)),
        Some("shrines") => World::load(&dir).and_then(|w| write_shrines(&dir, &w)),
        Some("givers") => World::load(&dir).and_then(|w| write_givers(&dir, &w)),
        Some("spawns") => World::load(&dir).and_then(|w| write_spawns(&dir, &w)),
        Some("areas") => write_areas(&dir),
        Some("info") => info(&dir),
        Some("apply-terrain3d") => apply_terrain3d(&dir, &args),
        Some("map") => map(&dir, &args),
        Some("probe") => probe(&dir, &args[1..]),
        _ => {
            eprintln!("usage: worldtool gen|nodes|stations|posts|shrines|givers|spawns|areas|info [--dir DIR]\n       worldtool apply-terrain3d [--from DIR] [--dir DIR]\n       worldtool probe X,Z...\n       worldtool map --area X0,Z0,X1,Z1 [--px N] [--out FILE] [--dir DIR]");
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
    write_stations(dir, &world)?;
    write_posts(dir, &world)?;
    write_shrines(dir, &world)?;
    write_givers(dir, &world)?;
    write_spawns(dir, &world)?;
    write_areas(dir)
}

/// Write the named areas to `dir` (docs/design/coast.md).
fn write_areas(dir: &std::path::Path) -> Result<(), String> {
    let areas = worldtool::areas::place();
    worldtool::areas::save(dir, &areas)?;
    println!("wrote {} named areas to {}", areas.len(), dir.join(worldtool::areas::FILE).display());
    Ok(())
}

/// Place NPC spawn points on `world` and write them to `dir` (docs/design/npcs.md).
fn write_spawns(dir: &std::path::Path, world: &World) -> Result<(), String> {
    let spawns = worldtool::spawns::place(world);
    worldtool::spawns::save(dir, &spawns)?;
    println!("wrote {} spawn points to {}", spawns.len(), dir.join(worldtool::spawns::FILE).display());
    for (id, x, z, r, want) in worldtool::spawns::CAMPS {
        let got = spawns.iter().filter(|p| p.kind.id == id && (p.x - x).hypot(p.z - z) <= r).count();
        println!("  {id} at ({x:.0}, {z:.0}): {got} of {want}");
    }
    Ok(())
}

/// Place Wayshrines on `world` and write them to `dir` (docs/design/combat.md).
fn write_shrines(dir: &std::path::Path, world: &World) -> Result<(), String> {
    let shrines = worldtool::shrines::place(world);
    worldtool::shrines::save(dir, &shrines)?;
    println!("wrote {} Wayshrines to {}", shrines.len(), dir.join(worldtool::shrines::FILE).display());
    Ok(())
}

/// Place quest givers on `world` and write them to `dir` (docs/design/quests.md).
fn write_givers(dir: &std::path::Path, world: &World) -> Result<(), String> {
    let givers = worldtool::givers::place(world);
    worldtool::givers::save(dir, &givers)?;
    println!("wrote {} quest givers to {}", givers.len(), dir.join(worldtool::givers::FILE).display());
    Ok(())
}

/// Place trade posts on `world` and write them to `dir` (docs/design/trade.md).
fn write_posts(dir: &std::path::Path, world: &World) -> Result<(), String> {
    let posts = worldtool::posts::place(world);
    worldtool::posts::save(dir, &posts)?;
    println!("wrote {} trade posts to {}", posts.len(), dir.join(worldtool::posts::FILE).display());
    Ok(())
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

/// Print the ground and land use at each `X,Z` given.
fn probe(dir: &std::path::Path, points: &[String]) -> Result<(), String> {
    let world = World::load(dir)?;
    let mut chunks = sim::terrain::FileChunks::new(&world.heights);
    for p in points.iter().filter(|p| p.contains(',')) {
        let v: Vec<f32> = p.split(',').map(|v| v.parse::<f32>().map_err(|e| format!("{p}: {e}"))).collect::<Result<_, _>>()?;
        let [x, z] = v[..] else { return Err(format!("{p}: want X,Z")) };
        let g = sim::terrain::ground(&mut chunks, x, z).ok_or(format!("{p}: off the map"))?;
        let t = world.land_at(x, z).unwrap_or(0);
        println!("({x:.0}, {z:.0}): {:.1} m, gradient {:.2}, {:?}, {:?}", g.lo, g.gradient, texel_use(t), texel_climate(t));
    }
    Ok(())
}

/// Draw part of the world with everything placed on it (`worldtool::map`).
fn map(dir: &std::path::Path, args: &[String]) -> Result<(), String> {
    use worldtool::map::{Area, Mark};
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str);
    let area = Area::parse(arg("--area").ok_or("map needs --area X0,Z0,X1,Z1")?)?;
    let px: usize = arg("--px").map_or(Ok(1200), str::parse).map_err(|e| format!("--px: {e}"))?;
    let out = std::path::PathBuf::from(arg("--out").unwrap_or("target/maps/map.png"));
    let world = World::load(dir)?;
    let mark = |x: f32, z: f32, rgb: [u8; 3], r: i32| Mark { x, z, rgb, r };
    let mut marks = Vec::new();
    for n in worldtool::nodes::load(dir)? {
        let rgb = match n.kind.id {
            id if id.ends_with("_vein") || id.ends_with("_outcrop") => [90, 90, 110],
            "driftpine" | "hollowoak" => [20, 70, 30],
            _ => [60, 220, 60],
        };
        marks.push(mark(n.x, n.z, rgb, 1));
    }
    marks.extend(worldtool::spawns::load(dir)?.iter().map(|p| mark(p.x, p.z, [230, 30, 30], 2)));
    marks.extend(worldtool::stations::load(dir)?.iter().map(|p| mark(p.x, p.z, [255, 140, 0], 3)));
    marks.extend(worldtool::posts::load(dir)?.iter().map(|p| mark(p.x, p.z, [255, 230, 0], 4)));
    marks.extend(worldtool::shrines::load(dir)?.iter().map(|p| mark(p.x, p.z, [255, 255, 255], 3)));
    marks.extend(worldtool::givers::load(dir)?.iter().map(|p| mark(p.x, p.z, [80, 200, 255], 3)));
    let pic = worldtool::map::render(&world, area, px, &marks);
    worldtool::map::save(&out, &pic)?;
    println!("wrote {} ({} × {} px, {:.1} m a pixel)", out.display(), pic.w, pic.h, (area.x1 - area.x0) / px as f32);
    Ok(())
}

/// Write the client's Terrain3D export back into `world/`, as one save of the whole world (ADR 0032).
/// The placements (nodes, stations, …) aren't moved; rewrite them with their own commands if the
/// ground under them changed.
fn apply_terrain3d(dir: &std::path::Path, args: &[String]) -> Result<(), String> {
    let from = PathBuf::from(args.iter().position(|a| a == "--from").and_then(|i| args.get(i + 1)).map_or("target/terrain3d/export", String::as_str));
    let read = |name: &str| std::fs::read(from.join(name)).map_err(|e| format!("{}: {e}", from.join(name).display()));
    let heights = sim::terrain::HeightFile::from_bytes(&read("heights.mmoh")?)?;
    let uses = read("uses.raw")?;
    let mut world = World::load(dir)?;
    let (samples, texels) = world.apply_terrain3d(heights, &uses)?;
    let hash = world.save(dir)?;
    println!("applied {}: {samples} height samples and {texels} land texels changed, terrain hash {hash:016x}", from.display());
    if samples > 0 {
        println!("heights changed: check what stands on them (worldtool nodes, stations, posts, shrines, givers, spawns)");
    }
    Ok(())
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
        println!("  {:<16} ({:.1}, {:.1}, {:.1})", s.kind.id, s.x, s.y, s.z);
    }
    let posts = worldtool::posts::load(dir)?;
    println!("{} trade posts", posts.len());
    for p in &posts {
        println!("  {:<16} ({:.1}, {:.1}, {:.1})", p.kind.id, p.x, p.y, p.z);
    }
    let spawns = worldtool::spawns::load(dir)?;
    println!("{} NPC spawn points", spawns.len());
    for k in &sim::npc::NPC_KINDS {
        println!("  {:<20} {}", k.id, spawns.iter().filter(|p| p.kind == k).count());
    }
    let shrines = worldtool::shrines::load(dir)?;
    println!("{} Wayshrines", shrines.len());
    for p in &shrines {
        println!("  {:<20} ({:.1}, {:.1}, {:.1})", p.kind.id, p.x, p.y, p.z);
    }
    let givers = worldtool::givers::load(dir)?;
    println!("{} quest givers", givers.len());
    for p in &givers {
        println!("  {:<20} ({:.1}, {:.1}, {:.1})", p.kind.id, p.x, p.y, p.z);
    }
    let areas = worldtool::areas::load(dir)?;
    println!("{} named areas", areas.len());
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
