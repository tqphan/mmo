//! Named areas (`world/areas.tsv`, docs/design/coast.md): `name x z radius`, one per line, from
//! `coast::AREAS`. The client loads them with the world and shows the smallest area the player
//! stands in; the server doesn't need them.

use std::path::Path;

use sim::cell::WORLD_HALF_M;

pub const FILE: &str = "areas.tsv";

#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub name: String,
    pub x: f32,
    pub z: f32,
    pub radius: f32,
}

/// The areas laid out in `coast::AREAS`.
pub fn place() -> Vec<Area> {
    crate::coast::AREAS.iter().map(|&(name, x, z, radius)| Area { name: name.into(), x, z, radius }).collect()
}

/// The smallest area round (x, z), if any.
pub fn at(areas: &[Area], x: f32, z: f32) -> Option<&Area> {
    areas.iter().filter(|a| (a.x - x).hypot(a.z - z) <= a.radius).min_by(|a, b| a.radius.total_cmp(&b.radius))
}

pub fn to_text(areas: &[Area]) -> String {
    let mut s = String::from("# Named areas (docs/design/coast.md): name, x, z, radius. Written by worldtool: don't edit by hand.\n");
    for a in areas {
        s += &format!("{}\t{:.2}\t{:.2}\t{:.2}\n", a.name, a.x, a.z, a.radius);
    }
    s
}

pub fn parse(text: &str) -> Result<Vec<Area>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", i + 1);
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 4 {
            return Err(at(format!("expected 4 tab-separated fields, found {}", f.len())));
        }
        if f[0].trim().is_empty() {
            return Err(at("no name".into()));
        }
        let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| at(format!("bad number {s:?}")));
        let (x, z, radius) = (num(f[1])?, num(f[2])?, num(f[3])?);
        if x.abs() >= WORLD_HALF_M || z.abs() >= WORLD_HALF_M {
            return Err(at("off the map".into()));
        }
        if radius <= 0.0 {
            return Err(at(format!("bad radius {radius}")));
        }
        out.push(Area { name: f[0].into(), x, z, radius });
    }
    Ok(out)
}

pub fn load(dir: &Path) -> Result<Vec<Area>, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(dir: &Path, areas: &[Area]) -> Result<(), String> {
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(areas)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips_and_bad_lines_are_refused() {
        let rows = vec![Area { name: "Hexer's Tarn".into(), x: -4750.0, z: 1450.0, radius: 320.0 }];
        assert_eq!(parse(&to_text(&rows)).unwrap(), rows);
        assert!(parse("Tarn\t0\t0").unwrap_err().contains("4 tab-separated"));
        assert!(parse("Tarn\t0\t0\t0").unwrap_err().contains("bad radius"));
        assert!(parse("Tarn\t9000\t0\t10").unwrap_err().contains("off the map"));
        assert!(parse("\t0\t0\t10").unwrap_err().contains("no name"));
    }

    #[test]
    fn the_smallest_area_wins() {
        let areas = place();
        assert_eq!(at(&areas, -3330.0, 380.0).unwrap().name, "Furrowdale");
        assert_eq!(at(&areas, -4100.0, 200.0).unwrap().name, "Hearthshore");
        assert_eq!(at(&areas, 0.0, 0.0).unwrap().name, "Gullhaven");
        assert_eq!(at(&areas, 300.0, 300.0).unwrap().name, "Gullhaven Isle");
        assert!(at(&areas, 0.0, 4000.0).is_none(), "the open sea");
    }

    /// The committed areas are current, and every name is in the world bible.
    #[test]
    fn committed_areas_are_current_and_named_in_the_bible() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        let areas = load(&root.join(crate::DEFAULT_DIR)).unwrap();
        assert_eq!(areas, place(), "the committed areas are stale; run `cargo run -p worldtool -- areas`");
        let bible = std::fs::read_to_string(root.join("docs/world/README.md")).unwrap();
        for a in &areas {
            assert!(bible.contains(&a.name), "{} isn't in docs/world/README.md", a.name);
        }
    }
}
