//! Turns `content/` into Rust statics, C# data and a content hash (PLAN §5.4,
//! docs/design/content.md). The outputs are committed; `contentgen check` fails when they are
//! stale, and `cargo xtask test` runs it.

use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

/// Where the outputs go, relative to the repo root.
pub const RUST_OUT: &str = "server/sim/src/content.rs";
pub const CSHARP_OUT: &str = "client/content/Content.g.cs";

/// Part of the hash, so a change in what the generator emits for the same data still changes it.
const FORMAT: &str = "contentgen 4";
/// Anything faster is a typo, not a design choice.
const MAX_SPEED_LIMIT_MPS: f32 = 50.0;
/// More cannons than this on one hull is a typo too.
const MAX_MOUNTS: usize = 16;
/// The land-use raster packs a use class into 4 bits and a climate into 3 (docs/design/world.md).
pub const MAX_LAND_USES: usize = 16;
pub const MAX_CLIMATES: usize = 8;

/// The raw text of every content file.
pub struct Sources {
    pub movement: String,
    pub ships: String,
    pub land: String,
    pub labor: String,
}

impl Sources {
    pub fn read(dir: &Path) -> Result<Self, Vec<String>> {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).map_err(|e| vec![format!("{name}: {e}")])
        };
        Ok(Self {
            movement: read("movement.toml")?,
            ships: read("ships.toml")?,
            land: read("land.toml")?,
            labor: read("labor.toml")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MovementFile {
    mode: Vec<MoveModeDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MoveModeDef {
    pub id: String,
    pub wire: u8,
    pub max_speed_mps: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShipsFile {
    hull: Vec<HullDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HullDef {
    pub id: String,
    pub wire: u8,
    pub max_speed_mps: f32,
    pub astern_speed_mps: f32,
    pub accel_mps2: f32,
    pub drift_decel_mps2: f32,
    pub turn_rate_dps: f32,
    pub deck_half_width_m: f32,
    pub deck_half_length_m: f32,
    pub deck_height_m: f32,
    pub draft_m: f32,
    pub hp: i32,
    /// Ship-local `[x, z]`.
    pub helm: [f32; 2],
    pub cannon: CannonDef,
    #[serde(default)]
    pub mount: Vec<MountDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CannonDef {
    pub muzzle_speed_mps: f32,
    pub muzzle_height_m: f32,
    pub arc_deg: f32,
    pub min_pitch_deg: f32,
    pub max_pitch_deg: f32,
    pub reload_s: f32,
    pub damage: i32,
    pub splash_m: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MountDef {
    pub x: f32,
    pub z: f32,
    pub facing_deg: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LandFile {
    land_use: Vec<LandUseDef>,
    climate: Vec<ClimateDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LandUseDef {
    pub id: String,
    pub wire: u8,
    pub buildable: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClimateDef {
    pub id: String,
    pub wire: u8,
}

/// `labor.toml`: the labor pool's rules (docs/design/labor.md).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaborDef {
    pub cap: i32,
    pub regen_points: i32,
    pub regen_period_s: i32,
    pub start_points: i32,
}

/// Checked content, ready to emit.
#[derive(Debug)]
pub struct Content {
    /// Sorted by `wire`, which runs 0, 1, 2, … so it doubles as the index.
    pub move_modes: Vec<MoveModeDef>,
    /// Sorted by `wire`, like the modes.
    pub hulls: Vec<HullDef>,
    /// Sorted by `wire`, like the modes.
    pub land_uses: Vec<LandUseDef>,
    pub climates: Vec<ClimateDef>,
    pub labor: LaborDef,
    pub hash: u64,
}

/// Parse and check everything, collecting every problem rather than stopping at the first.
pub fn parse(src: &Sources) -> Result<Content, Vec<String>> {
    let mut errors = Vec::new();
    let movement: Result<MovementFile, _> = from_toml("movement.toml", &src.movement);
    let ships: Result<ShipsFile, _> = from_toml("ships.toml", &src.ships);
    let land: Result<LandFile, _> = from_toml("land.toml", &src.land);
    let labor: Result<LaborDef, _> = from_toml("labor.toml", &src.labor);
    let (mut modes, mut hulls, mut land, labor) = match (movement, ships, land, labor) {
        (Ok(m), Ok(s), Ok(l), Ok(b)) => (m.mode, s.hull, l, b),
        (m, s, l, b) => {
            return Err(m.err().into_iter().chain(s.err()).chain(l.err()).chain(b.err()).flatten().collect());
        }
    };
    modes.sort_by_key(|m| m.wire);
    hulls.sort_by_key(|h| h.wire);
    land.land_use.sort_by_key(|u| u.wire);
    land.climate.sort_by_key(|c| c.wire);
    check_move_modes(&modes, &mut errors);
    check_hulls(&hulls, &mut errors);
    check_land(&land, &mut errors);
    check_labor(&labor, &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut c = Content { move_modes: modes, hulls, land_uses: land.land_use, climates: land.climate, labor, hash: 0 };
    c.hash = hash(&canonical(&c));
    Ok(c)
}

fn from_toml<T: serde::de::DeserializeOwned>(name: &str, text: &str) -> Result<T, Vec<String>> {
    toml::from_str(text).map_err(|e| {
        let line = e.span().map(|r| text[..r.start].matches('\n').count() + 1);
        let at = line.map(|l| format!(":{l}")).unwrap_or_default();
        vec![format!("{name}{at}: {}", e.message())]
    })
}

/// The rules every table keyed by `id` and `wire` shares; `rows` is sorted by wire.
fn check_ids<'a>(file: &str, what: &str, rows: impl Iterator<Item = (&'a str, u8)>, errors: &mut Vec<String>) {
    let rows: Vec<_> = rows.collect();
    if rows.is_empty() {
        errors.push(format!("{file}: no {what}s"));
    }
    for (i, &(id, wire)) in rows.iter().enumerate() {
        let at = format!("{file}: {what} {id:?}");
        if !is_id(id) {
            errors.push(format!("{at}: ids are lower-case letters, digits and _, starting with a letter"));
        }
        if rows[..i].iter().any(|o| o.0 == id) {
            errors.push(format!("{at}: duplicate id"));
        }
        if usize::from(wire) != i {
            errors.push(format!("{at}: wire values must run 0, 1, 2, … with no gaps or repeats (found {wire})"));
        }
    }
}

/// Push an error unless `v` is finite and within `lo..=hi`.
fn check_range(at: &str, name: &str, v: f32, lo: f32, hi: f32, errors: &mut Vec<String>) {
    if !(v.is_finite() && (lo..=hi).contains(&v)) {
        errors.push(format!("{at}: {name} must be from {lo} to {hi}"));
    }
}

fn check_move_modes(modes: &[MoveModeDef], errors: &mut Vec<String>) {
    check_ids("movement.toml", "mode", modes.iter().map(|m| (m.id.as_str(), m.wire)), errors);
    for m in modes {
        let at = format!("movement.toml: mode {:?}", m.id);
        if !(m.max_speed_mps.is_finite() && m.max_speed_mps > 0.0 && m.max_speed_mps <= MAX_SPEED_LIMIT_MPS) {
            errors.push(format!("{at}: max_speed_mps must be above 0 and at most {MAX_SPEED_LIMIT_MPS}"));
        }
    }
}

fn check_hulls(hulls: &[HullDef], errors: &mut Vec<String>) {
    check_ids("ships.toml", "hull", hulls.iter().map(|h| (h.id.as_str(), h.wire)), errors);
    for h in hulls {
        let at = format!("ships.toml: hull {:?}", h.id);
        let e = &mut *errors;
        // Every positive quantity starts just above 0.
        let tiny = f32::MIN_POSITIVE;
        check_range(&at, "max_speed_mps", h.max_speed_mps, tiny, MAX_SPEED_LIMIT_MPS, e);
        check_range(&at, "astern_speed_mps", h.astern_speed_mps, 0.0, h.max_speed_mps.max(0.0), e);
        check_range(&at, "accel_mps2", h.accel_mps2, tiny, 20.0, e);
        check_range(&at, "drift_decel_mps2", h.drift_decel_mps2, tiny, 20.0, e);
        check_range(&at, "turn_rate_dps", h.turn_rate_dps, tiny, 180.0, e);
        check_range(&at, "deck_half_width_m", h.deck_half_width_m, tiny, 32.0, e);
        check_range(&at, "deck_half_length_m", h.deck_half_length_m, tiny, 32.0, e);
        check_range(&at, "deck_height_m", h.deck_height_m, 0.0, 20.0, e);
        check_range(&at, "draft_m", h.draft_m, 0.0, 20.0, e);
        if h.hp <= 0 {
            e.push(format!("{at}: hp must be above 0"));
        }
        let on_deck = |x: f32, z: f32| x.abs() <= h.deck_half_width_m && z.abs() <= h.deck_half_length_m;
        if !on_deck(h.helm[0], h.helm[1]) {
            e.push(format!("{at}: the helm must be on the deck"));
        }
        let c = &h.cannon;
        check_range(&at, "cannon.muzzle_speed_mps", c.muzzle_speed_mps, tiny, 200.0, e);
        check_range(&at, "cannon.muzzle_height_m", c.muzzle_height_m, 0.0, 10.0, e);
        check_range(&at, "cannon.arc_deg", c.arc_deg, 0.0, 180.0, e);
        check_range(&at, "cannon.min_pitch_deg", c.min_pitch_deg, -45.0, 45.0, e);
        check_range(&at, "cannon.max_pitch_deg", c.max_pitch_deg, c.min_pitch_deg.max(-45.0), 45.0, e);
        check_range(&at, "cannon.reload_s", c.reload_s, tiny, 600.0, e);
        check_range(&at, "cannon.splash_m", c.splash_m, 0.0, 20.0, e);
        if c.damage < 0 {
            e.push(format!("{at}: cannon.damage must be 0 or more"));
        }
        if h.mount.len() > MAX_MOUNTS {
            e.push(format!("{at}: at most {MAX_MOUNTS} mounts"));
        }
        for (i, m) in h.mount.iter().enumerate() {
            if !on_deck(m.x, m.z) {
                e.push(format!("{at}: mount {i} must be on the deck"));
            }
            check_range(&at, &format!("mount {i} facing_deg"), m.facing_deg, -180.0, 180.0, e);
        }
    }
}

fn check_land(land: &LandFile, errors: &mut Vec<String>) {
    check_ids("land.toml", "land_use", land.land_use.iter().map(|u| (u.id.as_str(), u.wire)), errors);
    check_ids("land.toml", "climate", land.climate.iter().map(|c| (c.id.as_str(), c.wire)), errors);
    if land.land_use.len() > MAX_LAND_USES {
        errors.push(format!("land.toml: at most {MAX_LAND_USES} land uses (4 bits of the raster)"));
    }
    if land.climate.len() > MAX_CLIMATES {
        errors.push(format!("land.toml: at most {MAX_CLIMATES} climates (3 bits of the raster)"));
    }
}

fn check_labor(l: &LaborDef, errors: &mut Vec<String>) {
    let at = "labor.toml";
    if !(1..=1_000_000).contains(&l.cap) {
        errors.push(format!("{at}: cap must be from 1 to 1000000"));
    }
    if !(1..=l.cap.max(1)).contains(&l.regen_points) {
        errors.push(format!("{at}: regen_points must be from 1 to the cap"));
    }
    if !(1..=86_400).contains(&l.regen_period_s) {
        errors.push(format!("{at}: regen_period_s must be from 1 to 86400"));
    }
    if !(0..=l.cap).contains(&l.start_points) {
        errors.push(format!("{at}: start_points must be from 0 to the cap"));
    }
}

fn is_id(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The data the hash covers, independent of how the TOML is laid out or commented.
fn canonical(c: &Content) -> String {
    let mut s = format!("{FORMAT}\n");
    for m in &c.move_modes {
        writeln!(s, "move_mode\t{}\t{}\t{:08x}", m.id, m.wire, m.max_speed_mps.to_bits()).unwrap();
    }
    for h in &c.hulls {
        write!(s, "hull\t{}\t{}", h.id, h.wire).unwrap();
        for f in hull_fields(h).iter().chain(&cannon_fields(&h.cannon)) {
            write!(s, "\t{}", f.value.bits()).unwrap();
        }
        s += "\n";
        for m in &h.mount {
            s += "mount";
            for f in mount_fields(m) {
                write!(s, "\t{}", f.value.bits()).unwrap();
            }
            s += "\n";
        }
    }
    for u in &c.land_uses {
        writeln!(s, "land_use\t{}\t{}\t{}", u.id, u.wire, u.buildable).unwrap();
    }
    for cl in &c.climates {
        writeln!(s, "climate\t{}\t{}", cl.id, cl.wire).unwrap();
    }
    s += "labor";
    for f in labor_fields(&c.labor) {
        write!(s, "\t{}", f.value.bits()).unwrap();
    }
    s += "\n";
    s
}

/// One scalar field of a generated record.
struct Field {
    /// snake_case; PascalCase in C#.
    name: &'static str,
    doc: &'static str,
    value: Value,
}

#[derive(Clone, Copy)]
enum Value {
    F32(f32),
    I32(i32),
}

impl Value {
    fn bits(self) -> String {
        match self {
            Value::F32(v) => format!("{:08x}", v.to_bits()),
            Value::I32(v) => format!("{v}"),
        }
    }

    fn rust_type(self) -> &'static str {
        match self {
            Value::F32(_) => "f32",
            Value::I32(_) => "i32",
        }
    }

    fn csharp_type(self) -> &'static str {
        match self {
            Value::F32(_) => "float",
            Value::I32(_) => "int",
        }
    }

    // Rust's `{:?}` prints the shortest text that reads back as the same f32, which C# also parses
    // to that f32.
    fn rust(self) -> String {
        match self {
            Value::F32(v) => format!("{v:?}"),
            Value::I32(v) => format!("{v}"),
        }
    }

    fn csharp(self) -> String {
        match self {
            Value::F32(v) => format!("{v:?}f"),
            Value::I32(v) => format!("{v}"),
        }
    }
}

fn hull_fields(h: &HullDef) -> Vec<Field> {
    use Value::*;
    let f = |name, doc, value| Field { name, doc, value };
    vec![
        f("max_speed_mps", "Top speed ahead, m/s.", F32(h.max_speed_mps)),
        f("astern_speed_mps", "Top speed astern, m/s.", F32(h.astern_speed_mps)),
        f("accel_mps2", "How fast speed follows the throttle, m/s².", F32(h.accel_mps2)),
        f("drift_decel_mps2", "How fast the ship slows with the throttle at 0, m/s².", F32(h.drift_decel_mps2)),
        f("turn_rate_dps", "Yaw rate at full rudder and full authority, degrees/s.", F32(h.turn_rate_dps)),
        f("deck_half_width_m", "Half the deck's width (ship-local x), m.", F32(h.deck_half_width_m)),
        f("deck_half_length_m", "Half the deck's length (ship-local z), m.", F32(h.deck_half_length_m)),
        f("deck_height_m", "The deck's height above the water, m.", F32(h.deck_height_m)),
        f("draft_m", "Depth below the water, m. Shallower water stops the ship.", F32(h.draft_m)),
        f("hp", "Hit points when built.", I32(h.hp)),
        f("helm_x", "The helm's ship-local x, m.", F32(h.helm[0])),
        f("helm_z", "The helm's ship-local z, m.", F32(h.helm[1])),
    ]
}

fn cannon_fields(c: &CannonDef) -> Vec<Field> {
    use Value::*;
    let f = |name, doc, value| Field { name, doc, value };
    vec![
        f("muzzle_speed_mps", "Shot speed leaving the muzzle, m/s.", F32(c.muzzle_speed_mps)),
        f("muzzle_height_m", "The muzzle's height above the deck, m.", F32(c.muzzle_height_m)),
        f("arc_deg", "How far either side of the mount's facing it aims, degrees.", F32(c.arc_deg)),
        f("min_pitch_deg", "Lowest elevation, degrees.", F32(c.min_pitch_deg)),
        f("max_pitch_deg", "Highest elevation, degrees.", F32(c.max_pitch_deg)),
        f("reload_s", "Time between shots from one mount, s.", F32(c.reload_s)),
        f("damage", "Hull damage per hit.", I32(c.damage)),
        f("splash_m", "How far outside a hull a shot still hits it, m.", F32(c.splash_m)),
    ]
}

fn mount_fields(m: &MountDef) -> Vec<Field> {
    use Value::*;
    let f = |name, doc, value| Field { name, doc, value };
    vec![
        f("x", "Ship-local x, m.", F32(m.x)),
        f("z", "Ship-local z, m.", F32(m.z)),
        f("facing_deg", "Yaw on deck: 0 the bow, 90 port, -90 starboard.", F32(m.facing_deg)),
    ]
}

fn labor_fields(l: &LaborDef) -> Vec<Field> {
    use Value::*;
    let f = |name, doc, value| Field { name, doc, value };
    vec![
        f("cap", "The most a pool holds by regenerating.", I32(l.cap)),
        f("regen_points", "Points added at the end of each period.", I32(l.regen_points)),
        f("regen_period_s", "The regeneration period, s.", I32(l.regen_period_s)),
        f("start_points", "What a new account starts with.", I32(l.start_points)),
    ]
}

/// `pub name: type,` lines with docs, for a Rust struct.
fn rust_struct_fields(s: &mut String, fields: &[Field]) {
    for f in fields {
        writeln!(s, "    /// {}\n    pub {}: {},", f.doc, f.name, f.value.rust_type()).unwrap();
    }
}

/// `name: value` pairs for a Rust struct literal.
fn rust_values(fields: &[Field]) -> String {
    fields.iter().map(|f| format!("{}: {}", f.name, f.value.rust())).collect::<Vec<_>>().join(", ")
}

/// `<param>` docs for a C# positional record.
fn csharp_param_docs(s: &mut String, fields: &[Field]) {
    for f in fields {
        writeln!(s, "    /// <param name=\"{}\">{}</param>", pascal(f.name), f.doc).unwrap();
    }
}

fn csharp_params(fields: &[Field]) -> String {
    fields.iter().map(|f| format!("{} {}", f.value.csharp_type(), pascal(f.name))).collect::<Vec<_>>().join(", ")
}

fn csharp_values(fields: &[Field]) -> String {
    fields.iter().map(|f| f.value.csharp()).collect::<Vec<_>>().join(", ")
}

/// FNV-1a, 64-bit. It only has to tell builds apart, not resist tampering.
fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn pascal(id: &str) -> String {
    id.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

pub fn render_rust(c: &Content) -> String {
    let mut s = String::new();
    s += "// @generated by contentgen from content/. Do not edit; run `cargo xtask content`.\n";
    s += "//! Game data from `content/` (docs/design/content.md).\n\n";
    s += "/// Identifies this build's content. Clients with a different hash are refused.\n";
    writeln!(s, "pub const CONTENT_HASH: u64 = 0x{:016x};\n", c.hash).unwrap();

    s += "/// Movement modes (`content/movement.toml`). The discriminant is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum MoveMode {\n";
    for m in &c.move_modes {
        writeln!(s, "    {} = {},", pascal(&m.id), m.wire).unwrap();
    }
    s += "}\n\n";
    s += "#[derive(Debug, PartialEq)]\npub struct MoveModeData {\n";
    s += "    pub mode: MoveMode,\n    pub id: &'static str,\n";
    s += "    /// Horizontal speed limit in m/s.\n    pub max_speed_mps: f32,\n}\n\n";
    writeln!(s, "pub static MOVE_MODES: [MoveModeData; {}] = [", c.move_modes.len()).unwrap();
    for m in &c.move_modes {
        writeln!(
            s,
            "    MoveModeData {{ mode: MoveMode::{}, id: {:?}, max_speed_mps: {:?} }},",
            pascal(&m.id),
            m.id,
            m.max_speed_mps
        )
        .unwrap();
    }
    s += "];\n\n";
    s += "impl MoveMode {\n";
    s += "    pub fn from_u8(v: u8) -> Option<Self> {\n";
    s += "        MOVE_MODES.get(usize::from(v)).map(|d| d.mode)\n    }\n\n";
    s += "    pub fn data(self) -> &'static MoveModeData {\n";
    s += "        &MOVE_MODES[self as usize]\n    }\n}\n\n";

    // Every hull has the same fields, and parsing guarantees at least one.
    let first = &c.hulls[0];
    s += "/// Ship hulls (`content/ships.toml`). The discriminant is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum Hull {\n";
    for h in &c.hulls {
        writeln!(s, "    {} = {},", pascal(&h.id), h.wire).unwrap();
    }
    s += "}\n\n";
    s += "#[derive(Debug, PartialEq)]\npub struct CannonData {\n";
    rust_struct_fields(&mut s, &cannon_fields(&first.cannon));
    s += "}\n\n";
    s += "/// A cannon's place on deck.\n#[derive(Debug, PartialEq)]\npub struct Mount {\n";
    rust_struct_fields(&mut s, &mount_fields(&MountDef { x: 0.0, z: 0.0, facing_deg: 0.0 }));
    s += "}\n\n";
    s += "#[derive(Debug, PartialEq)]\npub struct HullData {\n";
    s += "    pub hull: Hull,\n    pub id: &'static str,\n";
    rust_struct_fields(&mut s, &hull_fields(first));
    s += "    pub cannon: CannonData,\n    pub mounts: &'static [Mount],\n}\n\n";
    writeln!(s, "pub static HULLS: [HullData; {}] = [", c.hulls.len()).unwrap();
    for h in &c.hulls {
        writeln!(s, "    HullData {{\n        hull: Hull::{},\n        id: {:?},", pascal(&h.id), h.id).unwrap();
        for f in hull_fields(h) {
            writeln!(s, "        {}: {},", f.name, f.value.rust()).unwrap();
        }
        writeln!(s, "        cannon: CannonData {{ {} }},", rust_values(&cannon_fields(&h.cannon))).unwrap();
        s += "        mounts: &[\n";
        for m in &h.mount {
            writeln!(s, "            Mount {{ {} }},", rust_values(&mount_fields(m))).unwrap();
        }
        s += "        ],\n    },\n";
    }
    s += "];\n\n";
    s += "impl Hull {\n";
    s += "    pub fn from_u8(v: u8) -> Option<Self> {\n";
    s += "        HULLS.get(usize::from(v)).map(|d| d.hull)\n    }\n\n";
    s += "    pub fn data(self) -> &'static HullData {\n";
    s += "        &HULLS[self as usize]\n    }\n}\n\n";

    s += "/// Land-use classes (`content/land.toml`): the low 4 bits of a land-use texel. The discriminant\n";
    s += "/// is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum LandUse {\n";
    for u in &c.land_uses {
        writeln!(s, "    {} = {},", pascal(&u.id), u.wire).unwrap();
    }
    s += "}\n\n";
    s += "#[derive(Debug, PartialEq)]\npub struct LandUseData {\n";
    s += "    pub land_use: LandUse,\n    pub id: &'static str,\n";
    s += "    /// Houses and farms may be placed here.\n    pub buildable: bool,\n}\n\n";
    writeln!(s, "pub static LAND_USES: [LandUseData; {}] = [", c.land_uses.len()).unwrap();
    for u in &c.land_uses {
        writeln!(s, "    LandUseData {{ land_use: LandUse::{}, id: {:?}, buildable: {} }},", pascal(&u.id), u.id, u.buildable)
            .unwrap();
    }
    s += "];\n\n";
    s += "impl LandUse {\n";
    s += "    pub fn from_u8(v: u8) -> Option<Self> {\n";
    s += "        LAND_USES.get(usize::from(v)).map(|d| d.land_use)\n    }\n\n";
    s += "    pub fn data(self) -> &'static LandUseData {\n";
    s += "        &LAND_USES[self as usize]\n    }\n}\n\n";

    s += "/// Climates (`content/land.toml`): bits 4–6 of a land-use texel. The discriminant is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum Climate {\n";
    for cl in &c.climates {
        writeln!(s, "    {} = {},", pascal(&cl.id), cl.wire).unwrap();
    }
    s += "}\n\n";
    let ids: Vec<String> = c.climates.iter().map(|cl| format!("{:?}", cl.id)).collect();
    writeln!(s, "pub static CLIMATE_IDS: [&str; {}] = [{}];\n", c.climates.len(), ids.join(", ")).unwrap();
    s += "impl Climate {\n";
    s += "    pub fn from_u8(v: u8) -> Option<Self> {\n        match v {\n";
    for cl in &c.climates {
        writeln!(s, "            {} => Some(Self::{}),", cl.wire, pascal(&cl.id)).unwrap();
    }
    s += "            _ => None,\n        }\n    }\n\n";
    s += "    pub fn id(self) -> &'static str {\n        CLIMATE_IDS[self as usize]\n    }\n}\n\n";

    let labor = labor_fields(&c.labor);
    s += "/// Labor rules (`content/labor.toml`, docs/design/labor.md).\n#[derive(Debug, PartialEq)]\npub struct LaborRules {\n";
    rust_struct_fields(&mut s, &labor);
    s += "}\n\n";
    writeln!(s, "pub static LABOR: LaborRules = LaborRules {{ {} }};", rust_values(&labor)).unwrap();
    s
}

pub fn render_csharp(c: &Content) -> String {
    let mut s = String::new();
    s += "// <auto-generated>\n";
    s += "// Generated by contentgen from content/. Do not edit; run `cargo xtask content`.\n";
    s += "// </auto-generated>\n\n";
    s += "#nullable enable\n\nusing System.Collections.Generic;\n\nnamespace Mmo.Content\n{\n";
    s += "    /// <summary>Movement modes (content/movement.toml). The value is the wire value.</summary>\n";
    s += "    public enum MoveMode : byte\n    {\n";
    for m in &c.move_modes {
        writeln!(s, "        {} = {},", pascal(&m.id), m.wire).unwrap();
    }
    s += "    }\n\n";
    s += "    /// <param name=\"MaxSpeedMps\">Horizontal speed limit in m/s.</param>\n";
    s += "    public sealed record MoveModeData(MoveMode Mode, string Id, float MaxSpeedMps);\n\n";

    let first = &c.hulls[0];
    s += "    /// <summary>Ship hulls (content/ships.toml). The value is the wire value.</summary>\n";
    s += "    public enum Hull : byte\n    {\n";
    for h in &c.hulls {
        writeln!(s, "        {} = {},", pascal(&h.id), h.wire).unwrap();
    }
    s += "    }\n\n";
    let cannon = cannon_fields(&first.cannon);
    csharp_param_docs(&mut s, &cannon);
    writeln!(s, "    public sealed record CannonData({});\n", csharp_params(&cannon)).unwrap();
    let mount = mount_fields(&MountDef { x: 0.0, z: 0.0, facing_deg: 0.0 });
    s += "    /// <summary>A cannon's place on deck.</summary>\n";
    csharp_param_docs(&mut s, &mount);
    writeln!(s, "    public sealed record Mount({});\n", csharp_params(&mount)).unwrap();
    s += "    /// <summary>Land-use classes (content/land.toml): the low 4 bits of a land-use texel. The value is the wire value.</summary>\n";
    s += "    public enum LandUse : byte\n    {\n";
    for u in &c.land_uses {
        writeln!(s, "        {} = {},", pascal(&u.id), u.wire).unwrap();
    }
    s += "    }\n\n";
    s += "    /// <param name=\"Buildable\">Houses and farms may be placed here.</param>\n";
    s += "    public sealed record LandUseData(LandUse Use, string Id, bool Buildable);\n\n";
    s += "    /// <summary>Climates (content/land.toml): bits 4–6 of a land-use texel. The value is the wire value.</summary>\n";
    s += "    public enum Climate : byte\n    {\n";
    for cl in &c.climates {
        writeln!(s, "        {} = {},", pascal(&cl.id), cl.wire).unwrap();
    }
    s += "    }\n\n";
    let labor = labor_fields(&c.labor);
    s += "    /// <summary>Labor rules (content/labor.toml, docs/design/labor.md).</summary>\n";
    csharp_param_docs(&mut s, &labor);
    writeln!(s, "    public sealed record LaborRules({});\n", csharp_params(&labor)).unwrap();
    let hull = hull_fields(first);
    csharp_param_docs(&mut s, &hull);
    writeln!(
        s,
        "    public sealed record HullData(Hull Hull, string Id, {}, CannonData Cannon, IReadOnlyList<Mount> Mounts);\n",
        csharp_params(&hull)
    )
    .unwrap();
    s += "    public static class GameContent\n    {\n";
    s += "        /// <summary>Must match the server's, or entering the world is refused.</summary>\n";
    writeln!(s, "        public const ulong Hash = 0x{:016x}UL;\n", c.hash).unwrap();
    s += "        public static readonly IReadOnlyList<MoveModeData> MoveModes = new MoveModeData[]\n        {\n";
    for m in &c.move_modes {
        // Rust's `{:?}` prints the shortest text that reads back as the same f32, which C# also
        // parses to that f32.
        writeln!(s, "            new(MoveMode.{}, {:?}, {:?}f),", pascal(&m.id), m.id, m.max_speed_mps).unwrap();
    }
    s += "        };\n\n";
    s += "        public static MoveModeData Data(this MoveMode mode) => MoveModes[(int)mode];\n\n";
    s += "        public static readonly IReadOnlyList<HullData> Hulls = new HullData[]\n        {\n";
    for h in &c.hulls {
        let mounts: Vec<String> = h.mount.iter().map(|m| format!("new({})", csharp_values(&mount_fields(m)))).collect();
        writeln!(
            s,
            "            new(Hull.{}, {:?}, {}, new CannonData({}), new Mount[] {{ {} }}),",
            pascal(&h.id),
            h.id,
            csharp_values(&hull_fields(h)),
            csharp_values(&cannon_fields(&h.cannon)),
            mounts.join(", ")
        )
        .unwrap();
    }
    s += "        };\n\n";
    s += "        public static HullData Data(this Hull hull) => Hulls[(int)hull];\n\n";
    s += "        public static readonly IReadOnlyList<LandUseData> LandUses = new LandUseData[]\n        {\n";
    for u in &c.land_uses {
        writeln!(s, "            new(LandUse.{}, {:?}, {}),", pascal(&u.id), u.id, u.buildable).unwrap();
    }
    s += "        };\n\n";
    s += "        public static LandUseData Data(this LandUse use) => LandUses[(int)use];\n\n";
    s += "        public static readonly IReadOnlyList<string> ClimateIds = new[] { ";
    s += &c.climates.iter().map(|cl| format!("{:?}", cl.id)).collect::<Vec<_>>().join(", ");
    s += " };\n\n";
    s += "        public static string Id(this Climate climate) => ClimateIds[(int)climate];\n\n";
    writeln!(s, "        public static readonly LaborRules Labor = new({});", csharp_values(&labor)).unwrap();
    s += "    }\n}\n";
    s
}

/// Every output as (path relative to the repo root, contents).
pub fn render(c: &Content) -> [(&'static str, String); 2] {
    [(RUST_OUT, render_rust(c)), (CSHARP_OUT, render_csharp(c))]
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOVEMENT: &str = r#"
        [[mode]]
        id = "walk"
        wire = 0
        max_speed_mps = 2.0

        [[mode]]
        id = "run"
        wire = 1
        max_speed_mps = 5.4
    "#;

    const SHIPS: &str = r#"
        [[hull]]
        id = "clipper"
        wire = 0
        max_speed_mps = 11.0
        astern_speed_mps = 3.0
        accel_mps2 = 1.5
        drift_decel_mps2 = 0.8
        turn_rate_dps = 12.0
        deck_half_width_m = 2.5
        deck_half_length_m = 9.0
        deck_height_m = 2.0
        draft_m = 1.2
        hp = 12000
        helm = [0.0, 7.5]

        [hull.cannon]
        muzzle_speed_mps = 40.0
        muzzle_height_m = 1.0
        arc_deg = 30.0
        min_pitch_deg = -5.0
        max_pitch_deg = 15.0
        reload_s = 5.0
        damage = 1500
        splash_m = 2.0

        [[hull.mount]]
        x = -2.0
        z = 0.0
        facing_deg = 90.0

        [[hull.mount]]
        x = 2.0
        z = 0.0
        facing_deg = -90.0
    "#;

    const LAND: &str = r#"
        [[land_use]]
        id = "wild"
        wire = 0
        buildable = false

        [[land_use]]
        id = "housing"
        wire = 1
        buildable = true

        [[climate]]
        id = "temperate"
        wire = 0

        [[climate]]
        id = "arid"
        wire = 1
    "#;

    const LABOR: &str = r#"
        cap = 5000
        regen_points = 10
        regen_period_s = 300
        start_points = 5000
    "#;

    fn sources() -> Sources {
        Sources { movement: MOVEMENT.into(), ships: SHIPS.into(), land: LAND.into(), labor: LABOR.into() }
    }

    fn parse_movement(movement: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { movement: movement.into(), ..sources() })
    }

    fn parse_ships(ships: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { ships: ships.into(), ..sources() })
    }

    fn parse_land(land: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { land: land.into(), ..sources() })
    }

    fn parse_labor(labor: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { labor: labor.into(), ..sources() })
    }

    fn ship_errors(ships: &str) -> String {
        parse_ships(ships).unwrap_err().join("\n")
    }

    fn errors(movement: &str) -> String {
        parse_movement(movement).unwrap_err().join("\n")
    }

    #[test]
    fn modes_parse_in_wire_order() {
        let swapped = MOVEMENT.replace("wire = 0", "wire = 9").replace("wire = 1", "wire = 0").replace("wire = 9", "wire = 1");
        let c = parse_movement(&swapped).unwrap();
        assert_eq!(c.move_modes.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["run", "walk"]);
    }

    #[test]
    fn layout_and_comments_do_not_change_the_hash() {
        let a = parse_movement(MOVEMENT).unwrap().hash;
        let reordered = r#"
            # a comment
            [[mode]]
            max_speed_mps = 2.0
            wire = 0
            id = "walk"
            [[mode]]
            id = "run"
            max_speed_mps = 5.40
            wire = 1
        "#;
        assert_eq!(parse_movement(reordered).unwrap().hash, a);
    }

    #[test]
    fn any_value_change_changes_the_hash() {
        let a = parse_movement(MOVEMENT).unwrap().hash;
        for changed in [
            MOVEMENT.replace("5.4", "5.5"),
            MOVEMENT.replace("\"run\"", "\"jog\""),
            MOVEMENT.replace("wire = 0", "wire = 2").replace("wire = 1", "wire = 0").replace("wire = 2", "wire = 1"),
        ] {
            assert_ne!(parse_movement(&changed).unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn bad_toml_and_unknown_fields_are_errors() {
        assert!(errors("[[mode]\n").starts_with("movement.toml:"));
        let e = errors(&MOVEMENT.replace("wire = 0", "wire = 0\nspeed = 1"));
        assert!(e.starts_with("movement.toml:5: unknown field `speed`"), "{e}");
        assert!(errors("[[mode]]\nid = \"walk\"\nwire = 0\n").contains("missing field `max_speed_mps`"));
    }

    #[test]
    fn every_problem_is_reported() {
        let bad = MOVEMENT.replace("\"run\"", "\"walk\"").replace("5.4", "-1.0").replace("wire = 1", "wire = 3");
        let e = errors(&bad);
        assert!(e.contains("duplicate id"), "{e}");
        assert!(e.contains("no gaps"), "{e}");
        assert!(e.contains("max_speed_mps must be above 0"), "{e}");
    }

    #[test]
    fn ids_and_speeds_are_checked() {
        assert!(errors(&MOVEMENT.replace("\"run\"", "\"Run\"")).contains("ids are lower-case"));
        assert!(errors(&MOVEMENT.replace("\"run\"", "\"\"")).contains("ids are lower-case"));
        assert!(errors(&MOVEMENT.replace("5.4", "51.0")).contains("at most 50"));
        assert!(errors(&MOVEMENT.replace("5.4", "nan")).contains("max_speed_mps"));
        assert!(errors("mode = []").contains("no modes"));
    }

    #[test]
    fn both_outputs_carry_the_same_hash_and_values() {
        let c = parse_movement(MOVEMENT).unwrap();
        let hex = format!("0x{:016x}", c.hash);
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        assert!(rs.contains(&format!("CONTENT_HASH: u64 = {hex};")));
        assert!(cs.contains(&format!("Hash = {hex}UL;")));
        assert!(rs.contains("Run = 1,") && cs.contains("Run = 1,"));
        assert!(rs.contains(r#"id: "run", max_speed_mps: 5.4 }"#));
        assert!(cs.contains(r#"new(MoveMode.Run, "run", 5.4f),"#));
    }

    #[test]
    fn hulls_parse_with_their_cannon_and_mounts() {
        let c = parse_ships(SHIPS).unwrap();
        let h = &c.hulls[0];
        assert_eq!((h.id.as_str(), h.max_speed_mps, h.hp, h.helm), ("clipper", 11.0, 12000, [0.0, 7.5]));
        assert_eq!(h.cannon.damage, 1500);
        assert_eq!(h.mount, [MountDef { x: -2.0, z: 0.0, facing_deg: 90.0 }, MountDef { x: 2.0, z: 0.0, facing_deg: -90.0 }]);
    }

    #[test]
    fn any_hull_change_changes_the_hash() {
        let a = parse_ships(SHIPS).unwrap().hash;
        for changed in [
            SHIPS.replace("11.0", "11.5"),
            SHIPS.replace("hp = 12000", "hp = 12001"),
            SHIPS.replace("damage = 1500", "damage = 1501"),
            SHIPS.replace("x = 2.0", "x = 2.1"),
            SHIPS.replace("[0.0, 7.5]", "[0.0, 7.0]"),
        ] {
            assert_ne!(parse_ships(&changed).unwrap().hash, a, "{changed}");
        }
        let without_a_mount = SHIPS.rsplit_once("[[hull.mount]]").unwrap().0;
        assert_ne!(parse_ships(without_a_mount).unwrap().hash, a);
    }

    #[test]
    fn bad_hulls_are_reported_together() {
        let bad = SHIPS
            .replace("max_speed_mps = 11.0", "max_speed_mps = 0.0")
            .replace("hp = 12000", "hp = 0")
            .replace("[0.0, 7.5]", "[0.0, 12.0]")
            .replace("max_pitch_deg = 15.0", "max_pitch_deg = -10.0")
            .replace("x = 2.0", "x = 4.0")
            .replace("wire = 0", "wire = 1");
        let e = ship_errors(&bad);
        for want in ["max_speed_mps must be", "hp must be above 0", "the helm must be on the deck", "cannon.max_pitch_deg", "mount 1 must be on the deck", "no gaps"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(ship_errors("hull = []").contains("no hulls"));
        assert!(ship_errors(&SHIPS.replace("draft_m = 1.2", "draft_m = 1.2\nkeel = 1")).contains("unknown field `keel`"));
    }

    #[test]
    fn errors_in_both_files_are_reported() {
        let bad = Sources { movement: "[[mode]\n".into(), ships: "[[hull]\n".into(), land: "[[x]\n".into(), labor: "cap =\n".into() };
        let e = parse(&bad).unwrap_err().join("\n");
        for file in ["movement.toml", "ships.toml", "land.toml", "labor.toml"] {
            assert!(e.contains(file), "{file} missing from:\n{e}");
        }
    }

    #[test]
    fn hulls_reach_both_outputs() {
        let c = parse_ships(SHIPS).unwrap();
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        assert!(rs.contains("Clipper = 0,") && cs.contains("Clipper = 0,"));
        assert!(rs.contains("max_speed_mps: 11.0,"));
        assert!(rs.contains("Mount { x: 2.0, z: 0.0, facing_deg: -90.0 },"));
        assert!(cs.contains(r#"new(Hull.Clipper, "clipper", 11.0f,"#));
        assert!(cs.contains("new Mount[] { new(-2.0f, 0.0f, 90.0f), new(2.0f, 0.0f, -90.0f) }"));
    }

    #[test]
    fn land_uses_and_climates_parse_in_wire_order_and_reach_both_outputs() {
        let swapped = LAND.replace("\"wild\"\n        wire = 0", "\"wild\"\n        wire = 1").replacen(
            "\"housing\"\n        wire = 1",
            "\"housing\"\n        wire = 0",
            1,
        );
        let c = parse_land(&swapped).unwrap();
        assert_eq!(c.land_uses.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["housing", "wild"]);
        let c = parse_land(LAND).unwrap();
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        assert!(rs.contains("Housing = 1,") && cs.contains("Housing = 1,"));
        assert!(rs.contains(r#"LandUseData { land_use: LandUse::Housing, id: "housing", buildable: true },"#));
        assert!(cs.contains(r#"new(LandUse.Housing, "housing", true),"#));
        assert!(rs.contains("Arid = 1,") && cs.contains("Arid = 1,"));
        assert!(rs.contains(r#"CLIMATE_IDS: [&str; 2] = ["temperate", "arid"];"#));
        assert!(cs.contains(r#"ClimateIds = new[] { "temperate", "arid" };"#));
    }

    #[test]
    fn any_land_change_changes_the_hash() {
        let a = parse_land(LAND).unwrap().hash;
        for changed in [
            LAND.replace("buildable = true", "buildable = false"),
            LAND.replace("\"arid\"", "\"dry\""),
            LAND.replace("\"housing\"", "\"farmland\""),
        ] {
            assert_ne!(parse_land(&changed).unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn land_tables_are_checked_and_fit_the_raster() {
        let e = |land: &str| parse_land(land).unwrap_err().join("\n");
        assert!(e(&LAND.replace("wire = 1\n        buildable", "wire = 2\n        buildable")).contains("no gaps"));
        assert!(e(&LAND.replace("\"arid\"", "\"temperate\"")).contains("duplicate id"));
        assert!(e("land_use = []\nclimate = []").contains("no land_uses"));
        let many_uses: String = (0..17).map(|i| format!("[[land_use]]\nid = \"u{i}\"\nwire = {i}\nbuildable = false\n")).collect();
        assert!(e(&format!("{many_uses}[[climate]]\nid = \"c\"\nwire = 0\n")).contains("at most 16 land uses"));
        let many_climates: String = (0..9).map(|i| format!("[[climate]]\nid = \"c{i}\"\nwire = {i}\n")).collect();
        let one_use = "[[land_use]]\nid = \"wild\"\nwire = 0\nbuildable = false\n";
        assert!(e(&format!("{one_use}{many_climates}")).contains("at most 8 climates"));
    }

    #[test]
    fn labor_rules_reach_both_outputs() {
        let c = parse_labor(LABOR).unwrap();
        assert_eq!(c.labor, LaborDef { cap: 5000, regen_points: 10, regen_period_s: 300, start_points: 5000 });
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        assert!(rs.contains("pub static LABOR: LaborRules = LaborRules { cap: 5000, regen_points: 10, regen_period_s: 300, start_points: 5000 };"), "{rs}");
        assert!(cs.contains("public static readonly LaborRules Labor = new(5000, 10, 300, 5000);"), "{cs}");
    }

    #[test]
    fn any_labor_change_changes_the_hash() {
        let a = parse_labor(LABOR).unwrap().hash;
        for (from, to) in [("cap = 5000", "cap = 5001"), ("= 10", "= 11"), ("= 300", "= 301"), ("start_points = 5000", "start_points = 0")] {
            let changed = LABOR.replace(from, to);
            assert_ne!(parse_labor(&changed).unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn bad_labor_rules_are_reported_together() {
        let e = parse_labor(&LABOR.replace("regen_points = 10", "regen_points = 6000").replace("start_points = 5000", "start_points = 5001").replace("= 300", "= 0"))
            .unwrap_err()
            .join("\n");
        for want in ["regen_points must be", "start_points must be", "regen_period_s must be"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(parse_labor(&LABOR.replace("cap = 5000", "cap = 0")).unwrap_err().join("\n").contains("cap must be"));
        assert!(parse_labor(&format!("{LABOR}\nbonus = 1")).unwrap_err().join("\n").contains("unknown field `bonus`"));
        assert!(parse_labor("cap = 5000").unwrap_err().join("\n").contains("missing field"));
    }

    #[test]
    fn ids_become_pascal_case() {
        assert_eq!(pascal("run"), "Run");
        assert_eq!(pascal("deep_swim2"), "DeepSwim2");
    }

    /// The committed outputs match `content/`; `cargo xtask content` rewrites them.
    #[test]
    fn committed_outputs_are_current() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let c = parse(&Sources::read(&root.join("content")).unwrap()).unwrap();
        for (path, text) in render(&c) {
            let on_disk = std::fs::read_to_string(root.join(path)).unwrap().replace("\r\n", "\n");
            assert!(on_disk == text, "{path} is stale; run `cargo xtask content`");
        }
    }
}
