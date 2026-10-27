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
const FORMAT: &str = "contentgen 9";
/// Anything faster is a typo, not a design choice.
const MAX_SPEED_LIMIT_MPS: f32 = 50.0;
/// More cannons than this on one hull is a typo too.
const MAX_MOUNTS: usize = 16;
/// The land-use raster packs a use class into 4 bits and a climate into 3 (docs/design/world.md).
pub const MAX_LAND_USES: usize = 16;
pub const MAX_CLIMATES: usize = 8;
/// More in one slot than this is a typo.
const MAX_STACK: u32 = 10_000;
/// Bags may grow to 150 slots (docs/design/items.md); a bigger starting bag is a typo.
const MAX_BAG_SLOTS: u16 = 150;
const MAX_ITEM_NAME: usize = 40;
/// Item kinds, in wire order. Code depends on them, so they're fixed here rather than in content.
pub const ITEM_KINDS: [&str; 5] = ["material", "consumable", "gear", "deed", "seed"];
/// Resource node kinds, in wire order, fixed here like the item kinds.
pub const NODE_KINDS: [&str; 3] = ["plant", "ore", "tree"];
/// Crop kinds, in wire order, fixed here like the item kinds (docs/design/farming.md).
pub const CROP_KINDS: [&str; 3] = ["crop", "tree", "animal"];
/// Longer than a week to regrow is a typo.
const MAX_REGROW_S: u32 = 7 * 86_400;
/// More inputs than this to one recipe is a typo.
const MAX_INPUTS: usize = 8;
/// Land texels are 4 m (docs/design/world.md); plots cover whole texels, and one cell at most.
const TEXEL_M: u32 = 4;
const MAX_PLOT_M: u32 = 64;
/// More steps of heavy tax than this is a typo.
const MAX_HEAVY_TAX_STEPS: usize = 10;

/// The raw text of every content file.
pub struct Sources {
    pub movement: String,
    pub ships: String,
    pub land: String,
    pub labor: String,
    pub items: String,
    pub gathering: String,
    pub crafting: String,
    pub farming: String,
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
            items: read("items.toml")?,
            gathering: read("gathering.toml")?,
            crafting: read("crafting.toml")?,
            farming: read("farming.toml")?,
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
    /// Required; optional here so a missing table is reported with the other problems.
    property: Option<PropertyRules>,
    #[serde(default)]
    plot: Vec<PlotDef>,
}

/// `[property]` in `land.toml`: tax and demolition (docs/design/land.md).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PropertyRules {
    /// An item id from `items.toml`.
    pub tax_item: String,
    pub week_s: u32,
    pub grace_s: u32,
    pub demolish_hour_utc: u8,
    pub prepay_weeks: u32,
    pub heavy_tax_pct: Vec<u32>,
    pub reach_m: f32,
}

/// `[[plot]]` in `land.toml`: a kind of plot and the deed that claims it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlotDef {
    pub wire: u8,
    pub id: String,
    pub name: String,
    /// An item id from `items.toml`, of kind `deed`.
    pub deed: String,
    pub size_m: u32,
    pub max_rise_m: f32,
    pub tax: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LandUseDef {
    pub id: String,
    pub wire: u8,
    pub buildable: bool,
    pub plantable: bool,
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

/// `items.toml`: item definitions and the bag (docs/design/items.md).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemsFile {
    bag: BagDef,
    item: Vec<ItemDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BagDef {
    pub slots: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ItemDef {
    /// Stored in item rows: permanent, never reused, gaps allowed, 0 reserved.
    pub wire: u16,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub stack: u32,
}

/// `gathering.toml`: resource node kinds (docs/design/gathering.md).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GatheringFile {
    gathering: GatheringRules,
    node: Vec<NodeDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GatheringRules {
    pub reach_m: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodeDef {
    pub wire: u8,
    pub id: String,
    pub name: String,
    pub kind: String,
    /// An item id from `items.toml`.
    pub item: String,
    pub yield_min: u32,
    pub yield_max: u32,
    pub labor: i32,
    pub gather_s: f32,
    pub regrow_s: u32,
}

/// `crafting.toml`: station kinds and recipes (docs/design/crafting.md).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CraftingFile {
    crafting: CraftingRules,
    station: Vec<StationDef>,
    recipe: Vec<RecipeDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CraftingRules {
    pub reach_m: f32,
    pub max_count: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StationDef {
    pub wire: u8,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecipeDef {
    pub wire: u16,
    pub id: String,
    /// A station id.
    pub station: String,
    pub inputs: Vec<IngredientDef>,
    /// An item id from `items.toml`.
    pub output: String,
    pub output_count: u32,
    pub labor: i32,
    pub craft_s: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IngredientDef {
    /// An item id from `items.toml`.
    pub item: String,
    pub count: u32,
}

/// `farming.toml`: what grows from each seed (docs/design/farming.md).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FarmingFile {
    farming: FarmingRules,
    crop: Vec<CropDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FarmingRules {
    pub reach_m: f32,
    pub ideal_pct: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CropDef {
    pub wire: u8,
    pub id: String,
    pub name: String,
    pub kind: String,
    /// An item id from `items.toml`, of kind `seed`.
    pub seed: String,
    /// A climate id from `land.toml`.
    pub climate: String,
    pub grow_s: u32,
    /// An item id from `items.toml`.
    pub product: String,
    pub yield_min: u32,
    pub yield_max: u32,
    pub labor: i32,
    pub regrow_s: u32,
    pub wither_s: u32,
    pub space_m: f32,
    pub plot_only: bool,
    pub seed_pct: u32,
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
    pub property: PropertyRules,
    /// Sorted by `wire`, like the modes.
    pub plots: Vec<PlotDef>,
    pub labor: LaborDef,
    pub bag: BagDef,
    /// Sorted by `wire`, which may have gaps.
    pub items: Vec<ItemDef>,
    pub gathering: GatheringRules,
    /// Sorted by `wire`, like the modes.
    pub nodes: Vec<NodeDef>,
    pub crafting: CraftingRules,
    /// Sorted by `wire`, like the modes.
    pub stations: Vec<StationDef>,
    /// Sorted by `wire`, like the modes.
    pub recipes: Vec<RecipeDef>,
    pub farming: FarmingRules,
    /// Sorted by `wire`, like the modes.
    pub crops: Vec<CropDef>,
    pub hash: u64,
}

impl Content {
    /// The wire of the item a node yields; parsing checked it exists.
    fn item_wire(&self, id: &str) -> u16 {
        self.items.iter().find(|i| i.id == id).map_or(0, |i| i.wire)
    }

    /// The wire of a climate; parsing checked it exists.
    fn climate_wire(&self, id: &str) -> u8 {
        self.climates.iter().find(|c| c.id == id).map_or(0, |c| c.wire)
    }

    /// The wire of a station kind; parsing checked it exists.
    fn station_wire(&self, id: &str) -> u8 {
        self.stations.iter().find(|s| s.id == id).map_or(0, |s| s.wire)
    }
}

/// Parse and check everything, collecting every problem rather than stopping at the first.
pub fn parse(src: &Sources) -> Result<Content, Vec<String>> {
    let mut errors = Vec::new();
    let movement: Result<MovementFile, _> = from_toml("movement.toml", &src.movement);
    let ships: Result<ShipsFile, _> = from_toml("ships.toml", &src.ships);
    let land: Result<LandFile, _> = from_toml("land.toml", &src.land);
    let labor: Result<LaborDef, _> = from_toml("labor.toml", &src.labor);
    let items: Result<ItemsFile, _> = from_toml("items.toml", &src.items);
    let gathering: Result<GatheringFile, _> = from_toml("gathering.toml", &src.gathering);
    let crafting: Result<CraftingFile, _> = from_toml("crafting.toml", &src.crafting);
    let farming: Result<FarmingFile, _> = from_toml("farming.toml", &src.farming);
    let (mut modes, mut hulls, mut land, labor, mut items, mut gathering, mut crafting, mut farming) =
        match (movement, ships, land, labor, items, gathering, crafting, farming) {
            (Ok(m), Ok(s), Ok(l), Ok(b), Ok(i), Ok(g), Ok(c), Ok(f)) => (m.mode, s.hull, l, b, i, g, c, f),
            (m, s, l, b, i, g, c, f) => {
                let all = m.err().into_iter().chain(s.err()).chain(l.err()).chain(b.err()).chain(i.err()).chain(g.err()).chain(c.err()).chain(f.err());
                return Err(all.flatten().collect());
            }
        };
    modes.sort_by_key(|m| m.wire);
    hulls.sort_by_key(|h| h.wire);
    land.land_use.sort_by_key(|u| u.wire);
    land.climate.sort_by_key(|c| c.wire);
    land.plot.sort_by_key(|p| p.wire);
    items.item.sort_by_key(|i| i.wire);
    gathering.node.sort_by_key(|n| n.wire);
    crafting.station.sort_by_key(|s| s.wire);
    crafting.recipe.sort_by_key(|r| r.wire);
    farming.crop.sort_by_key(|c| c.wire);
    check_move_modes(&modes, &mut errors);
    check_hulls(&hulls, &mut errors);
    check_land(&land, &items.item, &mut errors);
    check_labor(&labor, &mut errors);
    check_items(&items, &mut errors);
    check_gathering(&gathering, &items.item, &mut errors);
    check_crafting(&crafting, &items.item, &mut errors);
    check_farming(&farming, &items.item, &land.climate, &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }
    let Some(property) = land.property else { unreachable!("check_land reports a missing [property]") };
    let mut c = Content { move_modes: modes, hulls, land_uses: land.land_use, climates: land.climate, property, plots: land.plot, labor, bag: items.bag,
        items: items.item,
        gathering: gathering.gathering,
        nodes: gathering.node,
        crafting: crafting.crafting,
        stations: crafting.station,
        recipes: crafting.recipe,
        farming: farming.farming,
        crops: farming.crop,
        hash: 0,
    };
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
fn check_ids<'a>(file: &str, what: &str, rows: impl Iterator<Item = (&'a str, usize)>, errors: &mut Vec<String>) {
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
        if wire != i {
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
    check_ids("movement.toml", "mode", modes.iter().map(|m| (m.id.as_str(), usize::from(m.wire))), errors);
    for m in modes {
        let at = format!("movement.toml: mode {:?}", m.id);
        if !(m.max_speed_mps.is_finite() && m.max_speed_mps > 0.0 && m.max_speed_mps <= MAX_SPEED_LIMIT_MPS) {
            errors.push(format!("{at}: max_speed_mps must be above 0 and at most {MAX_SPEED_LIMIT_MPS}"));
        }
    }
}

fn check_hulls(hulls: &[HullDef], errors: &mut Vec<String>) {
    check_ids("ships.toml", "hull", hulls.iter().map(|h| (h.id.as_str(), usize::from(h.wire))), errors);
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

fn check_land(land: &LandFile, items: &[ItemDef], errors: &mut Vec<String>) {
    check_ids("land.toml", "land_use", land.land_use.iter().map(|u| (u.id.as_str(), usize::from(u.wire))), errors);
    check_ids("land.toml", "climate", land.climate.iter().map(|c| (c.id.as_str(), usize::from(c.wire))), errors);
    if land.land_use.len() > MAX_LAND_USES {
        errors.push(format!("land.toml: at most {MAX_LAND_USES} land uses (4 bits of the raster)"));
    }
    if land.climate.len() > MAX_CLIMATES {
        errors.push(format!("land.toml: at most {MAX_CLIMATES} climates (3 bits of the raster)"));
    }
    check_property(land, items, errors);
}

fn check_property(land: &LandFile, items: &[ItemDef], errors: &mut Vec<String>) {
    let file = "land.toml";
    match &land.property {
        None => errors.push(format!("{file}: no [property]")),
        Some(p) => {
            if !items.iter().any(|i| i.id == p.tax_item) {
                errors.push(format!("{file}: property.tax_item: no item {:?} in items.toml", p.tax_item));
            }
            for (name, v) in [("week_s", p.week_s), ("grace_s", p.grace_s)] {
                if !(60..=30 * 86_400).contains(&v) {
                    errors.push(format!("{file}: property.{name} must be from 60 to 2592000"));
                }
            }
            if p.demolish_hour_utc > 23 {
                errors.push(format!("{file}: property.demolish_hour_utc must be from 0 to 23"));
            }
            if !(1..=52).contains(&p.prepay_weeks) {
                errors.push(format!("{file}: property.prepay_weeks must be from 1 to 52"));
            }
            let steps = &p.heavy_tax_pct;
            if !(1..=MAX_HEAVY_TAX_STEPS).contains(&steps.len())
                || steps.iter().any(|v| !(100..=10_000).contains(v))
                || steps.windows(2).any(|w| w[1] < w[0])
            {
                errors.push(format!(
                    "{file}: property.heavy_tax_pct needs 1 to {MAX_HEAVY_TAX_STEPS} entries from 100 to 10000 that never fall"
                ));
            }
            check_range(file, "property.reach_m", p.reach_m, 1.0, 64.0, errors);
        }
    }
    check_ids(file, "plot", land.plot.iter().map(|p| (p.id.as_str(), usize::from(p.wire))), errors);
    let mut names = std::collections::HashSet::new();
    for (k, pl) in land.plot.iter().enumerate() {
        let at = format!("{file}: plot {:?}", pl.id);
        if !is_item_name(&pl.name) {
            errors.push(format!("{at}: names are 1 to {MAX_ITEM_NAME} letters, digits, spaces, ' and -, starting with a capital"));
        }
        if !names.insert(pl.name.as_str()) {
            errors.push(format!("{at}: duplicate name {:?}", pl.name));
        }
        match items.iter().find(|i| i.id == pl.deed) {
            None => errors.push(format!("{at}: no item {:?} in items.toml", pl.deed)),
            Some(i) if i.kind != "deed" => errors.push(format!("{at}: {:?} isn't of kind \"deed\"", pl.deed)),
            Some(_) => {}
        }
        if land.plot[..k].iter().any(|o| o.deed == pl.deed) {
            errors.push(format!("{at}: deed {:?} claims another plot too", pl.deed));
        }
        if pl.size_m % TEXEL_M != 0 || !(TEXEL_M..=MAX_PLOT_M).contains(&pl.size_m) {
            errors.push(format!("{at}: size_m must be a multiple of {TEXEL_M} from {TEXEL_M} to {MAX_PLOT_M}"));
        }
        check_range(&at, "max_rise_m", pl.max_rise_m, 0.25, 16.0, errors);
        if !(1..=1000).contains(&pl.tax) {
            errors.push(format!("{at}: tax must be from 1 to 1000"));
        }
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

fn check_items(f: &ItemsFile, errors: &mut Vec<String>) {
    let file = "items.toml";
    if !(1..=MAX_BAG_SLOTS).contains(&f.bag.slots) {
        errors.push(format!("{file}: bag.slots must be from 1 to {MAX_BAG_SLOTS}"));
    }
    if f.item.is_empty() {
        errors.push(format!("{file}: no items"));
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for (i, it) in f.item.iter().enumerate() {
        let at = format!("{file}: item {:?}", it.id);
        if it.wire == 0 {
            errors.push(format!("{at}: wire 0 is reserved"));
        }
        if i > 0 && f.item[i - 1].wire == it.wire {
            errors.push(format!("{at}: duplicate wire {}", it.wire));
        }
        if !is_id(&it.id) {
            errors.push(format!("{at}: ids are lower-case letters, digits and _, starting with a letter"));
        }
        if !ids.insert(it.id.as_str()) {
            errors.push(format!("{at}: duplicate id"));
        }
        if !is_item_name(&it.name) {
            errors.push(format!("{at}: names are 1 to {MAX_ITEM_NAME} letters, digits, spaces, ' and -, starting with a capital"));
        }
        if !names.insert(it.name.as_str()) {
            errors.push(format!("{at}: duplicate name {:?}", it.name));
        }
        if !ITEM_KINDS.contains(&it.kind.as_str()) {
            errors.push(format!("{at}: kind must be one of {}", ITEM_KINDS.join(", ")));
        }
        if !(1..=MAX_STACK).contains(&it.stack) {
            errors.push(format!("{at}: stack must be from 1 to {MAX_STACK}"));
        } else if it.kind == "gear" && it.stack != 1 {
            errors.push(format!("{at}: gear doesn't stack (stack = 1)"));
        }
    }
}

fn check_gathering(g: &GatheringFile, items: &[ItemDef], errors: &mut Vec<String>) {
    let file = "gathering.toml";
    check_range(file, "gathering.reach_m", g.gathering.reach_m, 0.5, 20.0, errors);
    check_ids(file, "node", g.node.iter().map(|n| (n.id.as_str(), usize::from(n.wire))), errors);
    let mut names = std::collections::HashSet::new();
    for n in &g.node {
        let at = format!("{file}: node {:?}", n.id);
        if !is_item_name(&n.name) {
            errors.push(format!("{at}: names are 1 to {MAX_ITEM_NAME} letters, digits, spaces, ' and -, starting with a capital"));
        }
        if !names.insert(n.name.as_str()) {
            errors.push(format!("{at}: duplicate name {:?}", n.name));
        }
        if !NODE_KINDS.contains(&n.kind.as_str()) {
            errors.push(format!("{at}: kind must be one of {}", NODE_KINDS.join(", ")));
        }
        match items.iter().find(|i| i.id == n.item) {
            None => errors.push(format!("{at}: no item {:?} in items.toml", n.item)),
            Some(i) if !(1..=n.yield_max).contains(&n.yield_min) || n.yield_max > i.stack => {
                errors.push(format!("{at}: yields must be 1 ≤ yield_min ≤ yield_max ≤ the item's stack ({})", i.stack));
            }
            Some(_) => {}
        }
        if !(1..=1000).contains(&n.labor) {
            errors.push(format!("{at}: labor must be from 1 to 1000"));
        }
        check_range(&at, "gather_s", n.gather_s, 0.5, 60.0, errors);
        if !(1..=MAX_REGROW_S).contains(&n.regrow_s) {
            errors.push(format!("{at}: regrow_s must be from 1 to {MAX_REGROW_S}"));
        }
    }
}

fn check_crafting(c: &CraftingFile, items: &[ItemDef], errors: &mut Vec<String>) {
    let file = "crafting.toml";
    check_range(file, "crafting.reach_m", c.crafting.reach_m, 0.5, 20.0, errors);
    if !(1..=1000).contains(&c.crafting.max_count) {
        errors.push(format!("{file}: crafting.max_count must be from 1 to 1000"));
    }
    check_ids(file, "station", c.station.iter().map(|s| (s.id.as_str(), usize::from(s.wire))), errors);
    let mut names = std::collections::HashSet::new();
    for st in &c.station {
        let at = format!("{file}: station {:?}", st.id);
        if !is_item_name(&st.name) {
            errors.push(format!("{at}: names are 1 to {MAX_ITEM_NAME} letters, digits, spaces, ' and -, starting with a capital"));
        }
        if !names.insert(st.name.as_str()) {
            errors.push(format!("{at}: duplicate name {:?}", st.name));
        }
    }
    check_ids(file, "recipe", c.recipe.iter().map(|r| (r.id.as_str(), usize::from(r.wire))), errors);
    for r in &c.recipe {
        let at = format!("{file}: recipe {:?}", r.id);
        if !c.station.iter().any(|s| s.id == r.station) {
            errors.push(format!("{at}: no station {:?}", r.station));
        }
        match items.iter().find(|i| i.id == r.output) {
            None => errors.push(format!("{at}: no item {:?} in items.toml", r.output)),
            Some(i) if !(1..=i.stack).contains(&r.output_count) => {
                errors.push(format!("{at}: output_count must be from 1 to the item's stack ({})", i.stack));
            }
            Some(_) => {}
        }
        if !(1..=MAX_INPUTS).contains(&r.inputs.len()) {
            errors.push(format!("{at}: 1 to {MAX_INPUTS} inputs"));
        }
        for (k, input) in r.inputs.iter().enumerate() {
            if !items.iter().any(|i| i.id == input.item) {
                errors.push(format!("{at}: no item {:?} in items.toml", input.item));
            }
            if r.inputs[..k].iter().any(|o| o.item == input.item) {
                errors.push(format!("{at}: input {:?} is listed twice", input.item));
            }
            if input.item == r.output {
                errors.push(format!("{at}: the output can't be an input"));
            }
            if !(1..=MAX_STACK).contains(&input.count) {
                errors.push(format!("{at}: input {:?} count must be from 1 to {MAX_STACK}", input.item));
            }
        }
        if !(1..=1000).contains(&r.labor) {
            errors.push(format!("{at}: labor must be from 1 to 1000"));
        }
        check_range(&at, "craft_s", r.craft_s, 0.5, 600.0, errors);
    }
}

fn check_farming(f: &FarmingFile, items: &[ItemDef], climates: &[ClimateDef], errors: &mut Vec<String>) {
    let file = "farming.toml";
    check_range(file, "farming.reach_m", f.farming.reach_m, 1.0, 20.0, errors);
    if !(10..=100).contains(&f.farming.ideal_pct) {
        errors.push(format!("{file}: farming.ideal_pct must be from 10 to 100"));
    }
    check_ids(file, "crop", f.crop.iter().map(|c| (c.id.as_str(), usize::from(c.wire))), errors);
    let mut names = std::collections::HashSet::new();
    for (k, c) in f.crop.iter().enumerate() {
        let at = format!("{file}: crop {:?}", c.id);
        if !is_item_name(&c.name) {
            errors.push(format!("{at}: names are 1 to {MAX_ITEM_NAME} letters, digits, spaces, ' and -, starting with a capital"));
        }
        if !names.insert(c.name.as_str()) {
            errors.push(format!("{at}: duplicate name {:?}", c.name));
        }
        if !CROP_KINDS.contains(&c.kind.as_str()) {
            errors.push(format!("{at}: kind must be one of {}", CROP_KINDS.join(", ")));
        }
        match items.iter().find(|i| i.id == c.seed) {
            None => errors.push(format!("{at}: no item {:?} in items.toml", c.seed)),
            Some(i) if i.kind != "seed" => errors.push(format!("{at}: {:?} isn't of kind \"seed\"", c.seed)),
            Some(_) => {}
        }
        if f.crop[..k].iter().any(|o| o.seed == c.seed) {
            errors.push(format!("{at}: seed {:?} plants another crop too", c.seed));
        }
        if !climates.iter().any(|cl| cl.id == c.climate) {
            errors.push(format!("{at}: no climate {:?} in land.toml", c.climate));
        }
        match items.iter().find(|i| i.id == c.product) {
            None => errors.push(format!("{at}: no item {:?} in items.toml", c.product)),
            Some(i) if !(1..=c.yield_max).contains(&c.yield_min) || c.yield_max > i.stack => {
                errors.push(format!("{at}: yields must be 1 ≤ yield_min ≤ yield_max ≤ the item's stack ({})", i.stack));
            }
            Some(_) => {}
        }
        if !(1..=1000).contains(&c.labor) {
            errors.push(format!("{at}: labor must be from 1 to 1000"));
        }
        if !(60..=MAX_REGROW_S).contains(&c.grow_s) {
            errors.push(format!("{at}: grow_s must be from 60 to {MAX_REGROW_S}"));
        }
        if c.regrow_s != 0 && !(60..=MAX_REGROW_S).contains(&c.regrow_s) {
            errors.push(format!("{at}: regrow_s must be 0 or from 60 to {MAX_REGROW_S}"));
        }
        if c.wither_s != 0 && !(60..=30 * 86_400).contains(&c.wither_s) {
            errors.push(format!("{at}: wither_s must be 0 or from 60 to 2592000"));
        }
        check_range(&at, "space_m", c.space_m, 0.25, 8.0, errors);
        if c.seed_pct > 100 {
            errors.push(format!("{at}: seed_pct must be from 0 to 100"));
        }
    }
}

/// Safe to print as a string literal in both Rust and C# without escaping.
fn is_item_name(s: &str) -> bool {
    s.chars().count() <= MAX_ITEM_NAME
        && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '\'' | '-'))
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
        writeln!(s, "land_use\t{}\t{}\t{}\t{}", u.id, u.wire, u.buildable, u.plantable).unwrap();
    }
    for cl in &c.climates {
        writeln!(s, "climate\t{}\t{}", cl.id, cl.wire).unwrap();
    }
    let p = &c.property;
    let steps: Vec<String> = p.heavy_tax_pct.iter().map(u32::to_string).collect();
    writeln!(
        s,
        "property\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        p.tax_item,
        p.week_s,
        p.grace_s,
        p.demolish_hour_utc,
        p.prepay_weeks,
        steps.join(","),
        Value::F32(p.reach_m).bits()
    )
    .unwrap();
    for pl in &c.plots {
        writeln!(s, "plot\t{}\t{}\t{}\t{}\t{}\t{}\t{}", pl.wire, pl.id, pl.name, pl.deed, pl.size_m, Value::F32(pl.max_rise_m).bits(), pl.tax)
            .unwrap();
    }
    s += "labor";
    for f in labor_fields(&c.labor) {
        write!(s, "\t{}", f.value.bits()).unwrap();
    }
    s += "\n";
    writeln!(s, "bag\t{}", c.bag.slots).unwrap();
    for i in &c.items {
        writeln!(s, "item\t{}\t{}\t{}\t{}\t{}", i.wire, i.id, i.name, i.kind, i.stack).unwrap();
    }
    writeln!(s, "gathering\t{}", Value::F32(c.gathering.reach_m).bits()).unwrap();
    for n in &c.nodes {
        writeln!(
            s,
            "node\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            n.wire,
            n.id,
            n.name,
            n.kind,
            n.item,
            n.yield_min,
            n.yield_max,
            n.labor,
            Value::F32(n.gather_s).bits(),
            n.regrow_s
        )
        .unwrap();
    }
    writeln!(s, "crafting\t{}\t{}", Value::F32(c.crafting.reach_m).bits(), c.crafting.max_count).unwrap();
    for st in &c.stations {
        writeln!(s, "station\t{}\t{}\t{}", st.wire, st.id, st.name).unwrap();
    }
    for r in &c.recipes {
        write!(s, "recipe\t{}\t{}\t{}\t{}\t{}\t{}\t{}", r.wire, r.id, r.station, r.output, r.output_count, r.labor, Value::F32(r.craft_s).bits())
            .unwrap();
        for i in &r.inputs {
            write!(s, "\t{}:{}", i.item, i.count).unwrap();
        }
        s += "\n";
    }
    writeln!(s, "farming\t{}\t{}", Value::F32(c.farming.reach_m).bits(), c.farming.ideal_pct).unwrap();
    for cr in &c.crops {
        writeln!(
            s,
            "crop\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            cr.wire,
            cr.id,
            cr.name,
            cr.kind,
            cr.seed,
            cr.climate,
            cr.grow_s,
            cr.product,
            cr.yield_min,
            cr.yield_max,
            cr.labor,
            cr.regrow_s,
            cr.wither_s,
            Value::F32(cr.space_m).bits(),
            cr.plot_only,
            cr.seed_pct
        )
        .unwrap();
    }
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
    s += "    /// Houses and farms may be placed here.\n    pub buildable: bool,\n";
    s += "    /// Crops, trees and livestock may be planted here.\n    pub plantable: bool,\n}\n\n";
    writeln!(s, "pub static LAND_USES: [LandUseData; {}] = [", c.land_uses.len()).unwrap();
    for u in &c.land_uses {
        writeln!(
            s,
            "    LandUseData {{ land_use: LandUse::{}, id: {:?}, buildable: {}, plantable: {} }},",
            pascal(&u.id),
            u.id,
            u.buildable,
            u.plantable
        )
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

    let p = &c.property;
    s += "/// Property rules (`content/land.toml`, docs/design/land.md).\n#[derive(Debug, PartialEq)]\npub struct PropertyRules {\n";
    s += "    /// The wire of the item tax is paid in.\n    pub tax_item: u16,\n";
    s += "    /// One tax period, s.\n    pub week_s: u32,\n";
    s += "    /// How long a plot stands after its tax runs out, s.\n    pub grace_s: u32,\n";
    s += "    /// The hour of the day (UTC, game time) demolitions happen.\n    pub demolish_hour_utc: u8,\n";
    s += "    /// How many weeks ahead tax may be paid.\n    pub prepay_weeks: u32,\n";
    s += "    /// The weekly tax in percent, by how many plots the account owns (the first entry for one);\n";
    s += "    /// its length is the most an account may own.\n    pub heavy_tax_pct: &'static [u32],\n";
    s += "    /// How far from a plot's edge a character may place it, pay it or change it, m.\n    pub reach_m: f32,\n}\n\n";
    let steps: Vec<String> = p.heavy_tax_pct.iter().map(u32::to_string).collect();
    writeln!(
        s,
        "pub static PROPERTY: PropertyRules = PropertyRules {{ tax_item: {}, week_s: {}, grace_s: {}, demolish_hour_utc: {}, prepay_weeks: {}, heavy_tax_pct: &[{}], reach_m: {:?} }};\n",
        c.item_wire(&p.tax_item),
        p.week_s,
        p.grace_s,
        p.demolish_hour_utc,
        p.prepay_weeks,
        steps.join(", "),
        p.reach_m
    )
    .unwrap();
    s += "/// Plot kinds (`content/land.toml`, docs/design/land.md).\n#[derive(Debug, PartialEq)]\npub struct PlotData {\n";
    s += "    /// Stored in plot rows.\n    pub wire: u8,\n    pub id: &'static str,\n    /// What players see.\n    pub name: &'static str,\n";
    s += "    /// The wire of the deed item that claims it.\n    pub deed: u16,\n";
    s += "    /// The side of its square, m: a multiple of the 4 m land texel.\n    pub size_m: u32,\n";
    s += "    /// The most the ground may rise across it, m.\n    pub max_rise_m: f32,\n";
    s += "    /// Tax items a week, before the heavy-tax multiplier.\n    pub tax: u32,\n}\n\n";
    s += "/// Every plot kind, sorted by wire, which is the index.\n";
    writeln!(s, "pub static PLOTS: [PlotData; {}] = [", c.plots.len()).unwrap();
    for pl in &c.plots {
        writeln!(
            s,
            "    PlotData {{ wire: {}, id: {:?}, name: {:?}, deed: {}, size_m: {}, max_rise_m: {:?}, tax: {} }},",
            pl.wire,
            pl.id,
            pl.name,
            c.item_wire(&pl.deed),
            pl.size_m,
            pl.max_rise_m,
            pl.tax
        )
        .unwrap();
    }
    s += "];\n\n";
    s += "/// The plot kind stored as `wire`.\n";
    s += "pub fn plot(wire: u8) -> Option<&'static PlotData> {\n    PLOTS.get(usize::from(wire))\n}\n\n";
    s += "/// The plot kind a deed item claims.\n";
    s += "pub fn plot_by_deed(item: u16) -> Option<&'static PlotData> {\n    PLOTS.iter().find(|d| d.deed == item)\n}\n\n";

    let labor = labor_fields(&c.labor);
    s += "/// Labor rules (`content/labor.toml`, docs/design/labor.md).\n#[derive(Debug, PartialEq)]\npub struct LaborRules {\n";
    rust_struct_fields(&mut s, &labor);
    s += "}\n\n";
    writeln!(s, "pub static LABOR: LaborRules = LaborRules {{ {} }};\n", rust_values(&labor)).unwrap();

    s += "/// Item kinds (`content/items.toml`, docs/design/items.md). The discriminant is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum ItemKind {\n";
    for (w, k) in ITEM_KINDS.iter().enumerate() {
        writeln!(s, "    {} = {w},", pascal(k)).unwrap();
    }
    s += "}\n\n";
    s += "#[derive(Debug, PartialEq)]\npub struct ItemData {\n";
    s += "    /// Stored in item rows. It never changes and is never reused.\n    pub wire: u16,\n";
    s += "    pub id: &'static str,\n    /// What players see.\n    pub name: &'static str,\n";
    s += "    pub kind: ItemKind,\n    /// The most in one slot.\n    pub stack: u32,\n}\n\n";
    s += "/// Every item, sorted by wire.\n";
    writeln!(s, "pub static ITEMS: [ItemData; {}] = [", c.items.len()).unwrap();
    for i in &c.items {
        writeln!(
            s,
            "    ItemData {{ wire: {}, id: {:?}, name: {:?}, kind: ItemKind::{}, stack: {} }},",
            i.wire,
            i.id,
            i.name,
            pascal(&i.kind),
            i.stack
        )
        .unwrap();
    }
    s += "];\n\n";
    s += "/// Slots in every character's bag.\n";
    writeln!(s, "pub const BAG_SLOTS: u16 = {};\n", c.bag.slots).unwrap();
    s += "/// The item stored as `wire`, if it still exists.\n";
    s += "pub fn item(wire: u16) -> Option<&'static ItemData> {\n";
    s += "    ITEMS.binary_search_by_key(&wire, |d| d.wire).ok().map(|i| &ITEMS[i])\n}\n\n";
    s += "/// The item with content id `id`.\n";
    s += "pub fn item_by_id(id: &str) -> Option<&'static ItemData> {\n";
    s += "    ITEMS.iter().find(|d| d.id == id)\n}\n\n";

    s += "/// Resource node kinds (`content/gathering.toml`, docs/design/gathering.md). The discriminant is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum NodeKind {\n";
    for (w, k) in NODE_KINDS.iter().enumerate() {
        writeln!(s, "    {} = {w},", pascal(k)).unwrap();
    }
    s += "}\n\n";
    s += "#[derive(Debug, PartialEq)]\npub struct NodeData {\n";
    s += "    /// Stored in node rows.\n    pub wire: u8,\n    pub id: &'static str,\n    /// What players see.\n    pub name: &'static str,\n";
    s += "    pub kind: NodeKind,\n    /// The wire of the item it yields.\n    pub item: u16,\n";
    s += "    /// How many of the item one gather yields, inclusive.\n    pub yield_min: u32,\n    pub yield_max: u32,\n";
    s += "    /// Labor spent when a gather finishes.\n    pub labor: i32,\n";
    s += "    /// How long a gather takes, in seconds.\n    pub gather_s: f32,\n";
    s += "    /// How long until it can be gathered again, in seconds.\n    pub regrow_s: u32,\n}\n\n";
    s += "/// Every node kind, sorted by wire, which is the index.\n";
    writeln!(s, "pub static NODES: [NodeData; {}] = [", c.nodes.len()).unwrap();
    for n in &c.nodes {
        writeln!(
            s,
            "    NodeData {{ wire: {}, id: {:?}, name: {:?}, kind: NodeKind::{}, item: {}, yield_min: {}, yield_max: {}, labor: {}, gather_s: {:?}, regrow_s: {} }},",
            n.wire,
            n.id,
            n.name,
            pascal(&n.kind),
            c.item_wire(&n.item),
            n.yield_min,
            n.yield_max,
            n.labor,
            n.gather_s,
            n.regrow_s
        )
        .unwrap();
    }
    s += "];\n\n";
    s += "/// How close a character must stand to gather, horizontally and vertically, in metres.\n";
    writeln!(s, "pub const GATHER_REACH_M: f32 = {:?};\n", c.gathering.reach_m).unwrap();
    s += "/// The node kind stored as `wire`.\n";
    s += "pub fn node(wire: u8) -> Option<&'static NodeData> {\n    NODES.get(usize::from(wire))\n}\n\n";
    s += "/// The node kind with content id `id`.\n";
    s += "pub fn node_by_id(id: &str) -> Option<&'static NodeData> {\n";
    s += "    NODES.iter().find(|d| d.id == id)\n}\n\n";

    s += "/// Crafting station kinds (`content/crafting.toml`, docs/design/crafting.md).\n";
    s += "#[derive(Debug, PartialEq)]\npub struct StationData {\n";
    s += "    /// Stored in station rows.\n    pub wire: u8,\n    pub id: &'static str,\n    /// What players see.\n    pub name: &'static str,\n}\n\n";
    s += "/// Every station kind, sorted by wire, which is the index.\n";
    writeln!(s, "pub static STATIONS: [StationData; {}] = [", c.stations.len()).unwrap();
    for st in &c.stations {
        writeln!(s, "    StationData {{ wire: {}, id: {:?}, name: {:?} }},", st.wire, st.id, st.name).unwrap();
    }
    s += "];\n\n";
    s += "/// One input to a recipe.\n#[derive(Debug, PartialEq)]\npub struct Ingredient {\n";
    s += "    /// The item's wire.\n    pub item: u16,\n    pub count: u32,\n}\n\n";
    s += "/// Recipes (`content/crafting.toml`, docs/design/crafting.md).\n";
    s += "#[derive(Debug, PartialEq)]\npub struct RecipeData {\n";
    s += "    /// Stored in craft jobs.\n    pub wire: u16,\n    pub id: &'static str,\n";
    s += "    /// The wire of the station kind it needs.\n    pub station: u8,\n";
    s += "    /// Taken for each unit.\n    pub inputs: &'static [Ingredient],\n";
    s += "    /// The wire of the item it makes, and how many each unit makes.\n    pub output: u16,\n    pub output_count: u32,\n";
    s += "    /// Labor spent for each unit.\n    pub labor: i32,\n";
    s += "    /// How long one unit takes, in seconds.\n    pub craft_s: f32,\n}\n\n";
    s += "/// Every recipe, sorted by wire, which is the index.\n";
    writeln!(s, "pub static RECIPES: [RecipeData; {}] = [", c.recipes.len()).unwrap();
    for r in &c.recipes {
        let inputs: Vec<String> =
            r.inputs.iter().map(|i| format!("Ingredient {{ item: {}, count: {} }}", c.item_wire(&i.item), i.count)).collect();
        writeln!(
            s,
            "    RecipeData {{ wire: {}, id: {:?}, station: {}, inputs: &[{}], output: {}, output_count: {}, labor: {}, craft_s: {:?} }},",
            r.wire,
            r.id,
            c.station_wire(&r.station),
            inputs.join(", "),
            c.item_wire(&r.output),
            r.output_count,
            r.labor,
            r.craft_s
        )
        .unwrap();
    }
    s += "];\n\n";
    s += "/// How close a character must stand to a station, horizontally and vertically, in metres.\n";
    writeln!(s, "pub const CRAFT_REACH_M: f32 = {:?};\n", c.crafting.reach_m).unwrap();
    s += "/// The most units one craft may ask for.\n";
    writeln!(s, "pub const CRAFT_MAX_COUNT: u32 = {};\n", c.crafting.max_count).unwrap();
    s += "/// The station kind stored as `wire`.\n";
    s += "pub fn station(wire: u8) -> Option<&'static StationData> {\n    STATIONS.get(usize::from(wire))\n}\n\n";
    s += "/// The station kind with content id `id`.\n";
    s += "pub fn station_by_id(id: &str) -> Option<&'static StationData> {\n";
    s += "    STATIONS.iter().find(|d| d.id == id)\n}\n\n";
    s += "/// The recipe stored as `wire`.\n";
    s += "pub fn recipe(wire: u16) -> Option<&'static RecipeData> {\n    RECIPES.get(usize::from(wire))\n}\n\n";
    s += "/// The recipe with content id `id`.\n";
    s += "pub fn recipe_by_id(id: &str) -> Option<&'static RecipeData> {\n";
    s += "    RECIPES.iter().find(|d| d.id == id)\n}\n\n";

    s += "/// Crop kinds (`content/farming.toml`, docs/design/farming.md). The discriminant is the wire value.\n";
    s += "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[repr(u8)]\npub enum CropKind {\n";
    for (w, k) in CROP_KINDS.iter().enumerate() {
        writeln!(s, "    {} = {w},", pascal(k)).unwrap();
    }
    s += "}\n\n";
    s += "/// What grows from a seed: a crop, tree or animal (`content/farming.toml`, docs/design/farming.md).\n";
    s += "#[derive(Debug, PartialEq)]\npub struct CropData {\n";
    s += "    /// Stored in crop rows.\n    pub wire: u8,\n    pub id: &'static str,\n    /// What players see.\n    pub name: &'static str,\n";
    s += "    pub kind: CropKind,\n    /// The wire of the seed item that plants it.\n    pub seed: u16,\n";
    s += "    /// The wire of its ideal climate.\n    pub climate: u8,\n";
    s += "    /// Seconds to mature outside its ideal climate.\n    pub grow_s: u32,\n";
    s += "    /// The wire of the item a harvest gives, and how many, inclusive.\n    pub product: u16,\n    pub yield_min: u32,\n    pub yield_max: u32,\n";
    s += "    /// Labor spent on each harvest.\n    pub labor: i32,\n";
    s += "    /// Seconds until it's ripe again after a harvest; 0 if a harvest removes it.\n    pub regrow_s: u32,\n";
    s += "    /// Seconds it stays ripe before it withers; 0 if it never does.\n    pub wither_s: u32,\n";
    s += "    /// Its radius on the ground, m.\n    pub space_m: f32,\n";
    s += "    /// It may only be planted on a plot.\n    pub plot_only: bool,\n";
    s += "    /// The chance, in percent, that a harvest also gives one of its seed.\n    pub seed_pct: u32,\n}\n\n";
    s += "/// Every crop kind, sorted by wire, which is the index.\n";
    writeln!(s, "pub static CROPS: [CropData; {}] = [", c.crops.len()).unwrap();
    for cr in &c.crops {
        writeln!(
            s,
            "    CropData {{ wire: {}, id: {:?}, name: {:?}, kind: CropKind::{}, seed: {}, climate: {}, grow_s: {}, product: {}, yield_min: {}, yield_max: {}, labor: {}, regrow_s: {}, wither_s: {}, space_m: {:?}, plot_only: {}, seed_pct: {} }},",
            cr.wire,
            cr.id,
            cr.name,
            pascal(&cr.kind),
            c.item_wire(&cr.seed),
            c.climate_wire(&cr.climate),
            cr.grow_s,
            c.item_wire(&cr.product),
            cr.yield_min,
            cr.yield_max,
            cr.labor,
            cr.regrow_s,
            cr.wither_s,
            cr.space_m,
            cr.plot_only,
            cr.seed_pct
        )
        .unwrap();
    }
    s += "];\n\n";
    s += "/// How close a character must stand to plant, harvest or uproot, horizontally, in metres.\n";
    writeln!(s, "pub const FARM_REACH_M: f32 = {:?};\n", c.farming.reach_m).unwrap();
    s += "/// Growth time in a crop's ideal climate, in percent.\n";
    writeln!(s, "pub const FARM_IDEAL_PCT: u32 = {};\n", c.farming.ideal_pct).unwrap();
    s += "/// The crop kind stored as `wire`.\n";
    s += "pub fn crop(wire: u8) -> Option<&'static CropData> {\n    CROPS.get(usize::from(wire))\n}\n\n";
    s += "/// The crop kind with content id `id`.\n";
    s += "pub fn crop_by_id(id: &str) -> Option<&'static CropData> {\n";
    s += "    CROPS.iter().find(|d| d.id == id)\n}\n\n";
    s += "/// The crop kind a seed item plants.\n";
    s += "pub fn crop_by_seed(item: u16) -> Option<&'static CropData> {\n    CROPS.iter().find(|d| d.seed == item)\n}\n";
    s
}

pub fn render_csharp(c: &Content) -> String {
    let mut s = String::new();
    s += "// <auto-generated>\n";
    s += "// Generated by contentgen from content/. Do not edit; run `cargo xtask content`.\n";
    s += "// </auto-generated>\n\n";
    s += "#nullable enable\n\nusing System.Collections.Generic;\nusing System.Linq;\n\nnamespace Mmo.Content\n{\n";
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
    s += "    /// <param name=\"Plantable\">Crops, trees and livestock may be planted here.</param>\n";
    s += "    public sealed record LandUseData(LandUse Use, string Id, bool Buildable, bool Plantable);\n\n";
    s += "    /// <summary>Climates (content/land.toml): bits 4–6 of a land-use texel. The value is the wire value.</summary>\n";
    s += "    public enum Climate : byte\n    {\n";
    for cl in &c.climates {
        writeln!(s, "        {} = {},", pascal(&cl.id), cl.wire).unwrap();
    }
    s += "    }\n\n";
    s += "    /// <summary>Property rules (content/land.toml, docs/design/land.md).</summary>\n";
    s += "    /// <param name=\"TaxItem\">The wire of the item tax is paid in.</param>\n";
    s += "    /// <param name=\"WeekS\">One tax period, s.</param>\n";
    s += "    /// <param name=\"GraceS\">How long a plot stands after its tax runs out, s.</param>\n";
    s += "    /// <param name=\"DemolishHourUtc\">The hour of the day (UTC, game time) demolitions happen.</param>\n";
    s += "    /// <param name=\"PrepayWeeks\">How many weeks ahead tax may be paid.</param>\n";
    s += "    /// <param name=\"HeavyTaxPct\">The weekly tax in percent by plots owned; its length is the most an account may own.</param>\n";
    s += "    /// <param name=\"ReachM\">How far from a plot's edge a character may place it, pay it or change it, m.</param>\n";
    s += "    public sealed record PropertyRules(ushort TaxItem, uint WeekS, uint GraceS, byte DemolishHourUtc, uint PrepayWeeks, IReadOnlyList<uint> HeavyTaxPct, float ReachM);\n\n";
    s += "    /// <summary>Plot kinds (content/land.toml, docs/design/land.md).</summary>\n";
    s += "    /// <param name=\"Wire\">Stored in plot rows.</param>\n";
    s += "    /// <param name=\"Deed\">The wire of the deed item that claims it.</param>\n";
    s += "    /// <param name=\"SizeM\">The side of its square, m.</param>\n";
    s += "    /// <param name=\"MaxRiseM\">The most the ground may rise across it, m.</param>\n";
    s += "    /// <param name=\"Tax\">Tax items a week, before the heavy-tax multiplier.</param>\n";
    s += "    public sealed record PlotData(byte Wire, string Id, string Name, ushort Deed, uint SizeM, float MaxRiseM, uint Tax);\n\n";
    let labor = labor_fields(&c.labor);
    s += "    /// <summary>Labor rules (content/labor.toml, docs/design/labor.md).</summary>\n";
    csharp_param_docs(&mut s, &labor);
    writeln!(s, "    public sealed record LaborRules({});\n", csharp_params(&labor)).unwrap();
    s += "    /// <summary>Item kinds (content/items.toml, docs/design/items.md). The value is the wire value.</summary>\n";
    s += "    public enum ItemKind : byte\n    {\n";
    for (w, k) in ITEM_KINDS.iter().enumerate() {
        writeln!(s, "        {} = {w},", pascal(k)).unwrap();
    }
    s += "    }\n\n";
    s += "    /// <param name=\"Wire\">Stored in item rows. It never changes and is never reused.</param>\n";
    s += "    /// <param name=\"Name\">What players see.</param>\n";
    s += "    /// <param name=\"Stack\">The most in one slot.</param>\n";
    s += "    public sealed record ItemData(ushort Wire, string Id, string Name, ItemKind Kind, uint Stack);\n\n";
    s += "    /// <summary>Resource node kinds (content/gathering.toml, docs/design/gathering.md). The value is the wire value.</summary>\n";
    s += "    public enum NodeKind : byte\n    {\n";
    for (w, k) in NODE_KINDS.iter().enumerate() {
        writeln!(s, "        {} = {w},", pascal(k)).unwrap();
    }
    s += "    }\n\n";
    s += "    /// <param name=\"Wire\">Stored in node rows.</param>\n";
    s += "    /// <param name=\"Item\">The wire of the item it yields.</param>\n";
    s += "    /// <param name=\"Labor\">Labor spent when a gather finishes.</param>\n";
    s += "    /// <param name=\"GatherS\">How long a gather takes, in seconds.</param>\n";
    s += "    /// <param name=\"RegrowS\">How long until it can be gathered again, in seconds.</param>\n";
    s += "    public sealed record NodeData(byte Wire, string Id, string Name, NodeKind Kind, ushort Item, uint YieldMin, uint YieldMax, int Labor, float GatherS, uint RegrowS);\n\n";
    s += "    /// <summary>Crafting station kinds (content/crafting.toml, docs/design/crafting.md).</summary>\n";
    s += "    /// <param name=\"Wire\">Stored in station rows.</param>\n";
    s += "    public sealed record StationData(byte Wire, string Id, string Name);\n\n";
    s += "    /// <summary>One input to a recipe.</summary>\n";
    s += "    /// <param name=\"Item\">The item's wire.</param>\n";
    s += "    public sealed record Ingredient(ushort Item, uint Count);\n\n";
    s += "    /// <summary>Recipes (content/crafting.toml, docs/design/crafting.md).</summary>\n";
    s += "    /// <param name=\"Wire\">Stored in craft jobs.</param>\n";
    s += "    /// <param name=\"Station\">The wire of the station kind it needs.</param>\n";
    s += "    /// <param name=\"Inputs\">Taken for each unit.</param>\n";
    s += "    /// <param name=\"Output\">The wire of the item it makes.</param>\n";
    s += "    /// <param name=\"OutputCount\">How many each unit makes.</param>\n";
    s += "    /// <param name=\"Labor\">Labor spent for each unit.</param>\n";
    s += "    /// <param name=\"CraftS\">How long one unit takes, in seconds.</param>\n";
    s += "    public sealed record RecipeData(ushort Wire, string Id, byte Station, IReadOnlyList<Ingredient> Inputs, ushort Output, uint OutputCount, int Labor, float CraftS);\n\n";
    s += "    /// <summary>Crop kinds (content/farming.toml, docs/design/farming.md). The value is the wire value.</summary>\n";
    s += "    public enum CropKind : byte\n    {\n";
    for (w, k) in CROP_KINDS.iter().enumerate() {
        writeln!(s, "        {} = {w},", pascal(k)).unwrap();
    }
    s += "    }\n\n";
    s += "    /// <summary>What grows from a seed: a crop, tree or animal (content/farming.toml, docs/design/farming.md).</summary>\n";
    s += "    /// <param name=\"Wire\">Stored in crop rows.</param>\n";
    s += "    /// <param name=\"Seed\">The wire of the seed item that plants it.</param>\n";
    s += "    /// <param name=\"Climate\">Its ideal climate.</param>\n";
    s += "    /// <param name=\"GrowS\">Seconds to mature outside its ideal climate.</param>\n";
    s += "    /// <param name=\"Product\">The wire of the item a harvest gives.</param>\n";
    s += "    /// <param name=\"Labor\">Labor spent on each harvest.</param>\n";
    s += "    /// <param name=\"RegrowS\">Seconds until it's ripe again after a harvest; 0 if a harvest removes it.</param>\n";
    s += "    /// <param name=\"WitherS\">Seconds it stays ripe before it withers; 0 if it never does.</param>\n";
    s += "    /// <param name=\"SpaceM\">Its radius on the ground, m.</param>\n";
    s += "    /// <param name=\"PlotOnly\">It may only be planted on a plot.</param>\n";
    s += "    /// <param name=\"SeedPct\">The chance, in percent, that a harvest also gives one of its seed.</param>\n";
    s += "    public sealed record CropData(byte Wire, string Id, string Name, CropKind Kind, ushort Seed, Climate Climate, uint GrowS, ushort Product, uint YieldMin, uint YieldMax, int Labor, uint RegrowS, uint WitherS, float SpaceM, bool PlotOnly, uint SeedPct);\n\n";
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
        writeln!(s, "            new(LandUse.{}, {:?}, {}, {}),", pascal(&u.id), u.id, u.buildable, u.plantable).unwrap();
    }
    s += "        };\n\n";
    s += "        public static LandUseData Data(this LandUse use) => LandUses[(int)use];\n\n";
    s += "        public static readonly IReadOnlyList<string> ClimateIds = new[] { ";
    s += &c.climates.iter().map(|cl| format!("{:?}", cl.id)).collect::<Vec<_>>().join(", ");
    s += " };\n\n";
    s += "        public static string Id(this Climate climate) => ClimateIds[(int)climate];\n\n";
    let p = &c.property;
    let steps: Vec<String> = p.heavy_tax_pct.iter().map(|v| format!("{v}u")).collect();
    writeln!(
        s,
        "        public static readonly PropertyRules Property = new({}, {}, {}, {}, {}, new uint[] {{ {} }}, {:?}f);\n",
        c.item_wire(&p.tax_item),
        p.week_s,
        p.grace_s,
        p.demolish_hour_utc,
        p.prepay_weeks,
        steps.join(", "),
        p.reach_m
    )
    .unwrap();
    s += "        /// <summary>Every plot kind, sorted by wire, which is the index.</summary>\n";
    s += "        public static readonly IReadOnlyList<PlotData> Plots = new PlotData[]\n        {\n";
    for pl in &c.plots {
        writeln!(
            s,
            "            new({}, {:?}, {:?}, {}, {}, {:?}f, {}),",
            pl.wire,
            pl.id,
            pl.name,
            c.item_wire(&pl.deed),
            pl.size_m,
            pl.max_rise_m,
            pl.tax
        )
        .unwrap();
    }
    s += "        };\n\n";
    s += "        /// <summary>The plot kind stored as <paramref name=\"wire\"/>, if it exists.</summary>\n";
    s += "        public static PlotData? Plot(byte wire) => wire < Plots.Count ? Plots[wire] : null;\n\n";
    s += "        /// <summary>The plot kind a deed item claims, if it is a deed.</summary>\n";
    s += "        public static PlotData? PlotByDeed(ushort item) => Plots.FirstOrDefault(p => p.Deed == item);\n\n";
    writeln!(s, "        public static readonly LaborRules Labor = new({});\n", csharp_values(&labor)).unwrap();
    s += "        /// <summary>Slots in every character's bag.</summary>\n";
    writeln!(s, "        public const ushort BagSlots = {};\n", c.bag.slots).unwrap();
    s += "        /// <summary>Every item, sorted by wire.</summary>\n";
    s += "        public static readonly IReadOnlyList<ItemData> Items = new ItemData[]\n        {\n";
    for i in &c.items {
        writeln!(s, "            new({}, {:?}, {:?}, ItemKind.{}, {}),", i.wire, i.id, i.name, pascal(&i.kind), i.stack).unwrap();
    }
    s += "        };\n\n";
    s += "        private static readonly Dictionary<ushort, ItemData> ItemsByWire = Items.ToDictionary(i => i.Wire);\n\n";
    s += "        /// <summary>The item stored as <paramref name=\"wire\"/>, if it still exists.</summary>\n";
    s += "        public static ItemData? Item(ushort wire) => ItemsByWire.TryGetValue(wire, out var d) ? d : null;\n\n";
    s += "        /// <summary>How close a character must stand to gather, horizontally and vertically, in metres.</summary>\n";
    writeln!(s, "        public const float GatherReachM = {:?}f;\n", c.gathering.reach_m).unwrap();
    s += "        /// <summary>Every node kind, sorted by wire, which is the index.</summary>\n";
    s += "        public static readonly IReadOnlyList<NodeData> Nodes = new NodeData[]\n        {\n";
    for n in &c.nodes {
        writeln!(
            s,
            "            new({}, {:?}, {:?}, NodeKind.{}, {}, {}, {}, {}, {:?}f, {}),",
            n.wire,
            n.id,
            n.name,
            pascal(&n.kind),
            c.item_wire(&n.item),
            n.yield_min,
            n.yield_max,
            n.labor,
            n.gather_s,
            n.regrow_s
        )
        .unwrap();
    }
    s += "        };\n\n";
    s += "        /// <summary>The node kind stored as <paramref name=\"wire\"/>, if it exists.</summary>\n";
    s += "        public static NodeData? Node(byte wire) => wire < Nodes.Count ? Nodes[wire] : null;\n\n";
    s += "        /// <summary>How close a character must stand to a station, horizontally and vertically, in metres.</summary>\n";
    writeln!(s, "        public const float CraftReachM = {:?}f;\n", c.crafting.reach_m).unwrap();
    s += "        /// <summary>The most units one craft may ask for.</summary>\n";
    writeln!(s, "        public const uint CraftMaxCount = {};\n", c.crafting.max_count).unwrap();
    s += "        /// <summary>Every station kind, sorted by wire, which is the index.</summary>\n";
    s += "        public static readonly IReadOnlyList<StationData> Stations = new StationData[]\n        {\n";
    for st in &c.stations {
        writeln!(s, "            new({}, {:?}, {:?}),", st.wire, st.id, st.name).unwrap();
    }
    s += "        };\n\n";
    s += "        /// <summary>The station kind stored as <paramref name=\"wire\"/>, if it exists.</summary>\n";
    s += "        public static StationData? Station(byte wire) => wire < Stations.Count ? Stations[wire] : null;\n\n";
    s += "        /// <summary>Every recipe, sorted by wire, which is the index.</summary>\n";
    s += "        public static readonly IReadOnlyList<RecipeData> Recipes = new RecipeData[]\n        {\n";
    for r in &c.recipes {
        let inputs: Vec<String> = r.inputs.iter().map(|i| format!("new({}, {})", c.item_wire(&i.item), i.count)).collect();
        writeln!(
            s,
            "            new({}, {:?}, {}, new Ingredient[] {{ {} }}, {}, {}, {}, {:?}f),",
            r.wire,
            r.id,
            c.station_wire(&r.station),
            inputs.join(", "),
            c.item_wire(&r.output),
            r.output_count,
            r.labor,
            r.craft_s
        )
        .unwrap();
    }
    s += "        };\n\n";
    s += "        /// <summary>The recipe stored as <paramref name=\"wire\"/>, if it exists.</summary>\n";
    s += "        public static RecipeData? Recipe(ushort wire) => wire < Recipes.Count ? Recipes[wire] : null;\n\n";
    s += "        /// <summary>How close a character must stand to plant, harvest or uproot, horizontally, in metres.</summary>\n";
    writeln!(s, "        public const float FarmReachM = {:?}f;\n", c.farming.reach_m).unwrap();
    s += "        /// <summary>Growth time in a crop's ideal climate, in percent.</summary>\n";
    writeln!(s, "        public const uint FarmIdealPct = {};\n", c.farming.ideal_pct).unwrap();
    s += "        /// <summary>Every crop kind, sorted by wire, which is the index.</summary>\n";
    s += "        public static readonly IReadOnlyList<CropData> Crops = new CropData[]\n        {\n";
    for cr in &c.crops {
        let climate = c.climates.iter().find(|cl| cl.id == cr.climate).map_or_else(String::new, |cl| pascal(&cl.id));
        writeln!(
            s,
            "            new({}, {:?}, {:?}, CropKind.{}, {}, Climate.{}, {}, {}, {}, {}, {}, {}, {}, {:?}f, {}, {}),",
            cr.wire,
            cr.id,
            cr.name,
            pascal(&cr.kind),
            c.item_wire(&cr.seed),
            climate,
            cr.grow_s,
            c.item_wire(&cr.product),
            cr.yield_min,
            cr.yield_max,
            cr.labor,
            cr.regrow_s,
            cr.wither_s,
            cr.space_m,
            cr.plot_only,
            cr.seed_pct
        )
        .unwrap();
    }
    s += "        };\n\n";
    s += "        /// <summary>The crop kind stored as <paramref name=\"wire\"/>, if it exists.</summary>\n";
    s += "        public static CropData? Crop(byte wire) => wire < Crops.Count ? Crops[wire] : null;\n\n";
    s += "        /// <summary>The crop kind a seed item plants, if it is a seed.</summary>\n";
    s += "        public static CropData? CropBySeed(ushort item) => Crops.FirstOrDefault(c => c.Seed == item);\n";
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
        plantable = true

        [[land_use]]
        id = "housing"
        wire = 1
        buildable = true
        plantable = true

        [[climate]]
        id = "temperate"
        wire = 0

        [[climate]]
        id = "arid"
        wire = 1

        [property]
        tax_item = "driftpine_log"
        week_s = 604800
        grace_s = 604800
        demolish_hour_utc = 20
        prepay_weeks = 4
        heavy_tax_pct = [100, 100, 200]
        reach_m = 8.0

        [[plot]]
        wire = 1
        id = "farm"
        name = "Farm"
        deed = "farm_deed"
        size_m = 16
        max_rise_m = 3.0
        tax = 10

        [[plot]]
        wire = 0
        id = "garden"
        name = "Garden"
        deed = "garden_deed"
        size_m = 8
        max_rise_m = 2.0
        tax = 5
    "#;

    const LABOR: &str = r#"
        cap = 5000
        regen_points = 10
        regen_period_s = 300
        start_points = 5000
    "#;

    const ITEMS: &str = r#"
        [bag]
        slots = 50

        [[item]]
        wire = 20
        id = "driftpine_log"
        name = "Driftpine Log"
        kind = "material"
        stack = 1000

        [[item]]
        wire = 3
        id = "reedcutter_knife"
        name = "Reedcutter's Knife"
        kind = "gear"
        stack = 1

        [[item]]
        wire = 300
        id = "garden_deed"
        name = "Garden Deed"
        kind = "deed"
        stack = 10

        [[item]]
        wire = 301
        id = "farm_deed"
        name = "Farm Deed"
        kind = "deed"
        stack = 10

        [[item]]
        wire = 400
        id = "pine_seed"
        name = "Pine Seed"
        kind = "seed"
        stack = 100

        [[item]]
        wire = 401
        id = "lamb"
        name = "Lamb"
        kind = "seed"
        stack = 10
    "#;

    const FARMING: &str = r#"
        [farming]
        reach_m = 3.0
        ideal_pct = 70

        [[crop]]
        wire = 1
        id = "sheep"
        name = "Sheep"
        kind = "animal"
        seed = "lamb"
        climate = "arid"
        grow_s = 14400
        product = "driftpine_log"
        yield_min = 2
        yield_max = 3
        labor = 5
        regrow_s = 7200
        wither_s = 0
        space_m = 1.5
        plot_only = true
        seed_pct = 10

        [[crop]]
        wire = 0
        id = "pine"
        name = "Pine"
        kind = "tree"
        seed = "pine_seed"
        climate = "temperate"
        grow_s = 28800
        product = "driftpine_log"
        yield_min = 4
        yield_max = 6
        labor = 10
        regrow_s = 0
        wither_s = 86400
        space_m = 2.0
        plot_only = false
        seed_pct = 0
    "#;

    const GATHERING: &str = r#"
        [gathering]
        reach_m = 3.0

        [[node]]
        wire = 1
        id = "hollowoak"
        name = "Hollowoak"
        kind = "tree"
        item = "driftpine_log"
        yield_min = 1
        yield_max = 3
        labor = 10
        gather_s = 5.0
        regrow_s = 1200

        [[node]]
        wire = 0
        id = "driftpine"
        name = "Driftpine"
        kind = "tree"
        item = "driftpine_log"
        yield_min = 2
        yield_max = 2
        labor = 5
        gather_s = 2.5
        regrow_s = 300
    "#;

    const CRAFTING: &str = r#"
        [crafting]
        reach_m = 4.0
        max_count = 100

        [[station]]
        wire = 1
        id = "cookhearth"
        name = "Cookhearth"

        [[station]]
        wire = 0
        id = "joiners_bench"
        name = "Joiner's Bench"

        [[recipe]]
        wire = 1
        id = "kindling"
        station = "cookhearth"
        inputs = [{ item = "reedcutter_knife", count = 1 }]
        output = "driftpine_log"
        output_count = 3
        labor = 5
        craft_s = 2.5

        [[recipe]]
        wire = 0
        id = "knife"
        station = "joiners_bench"
        inputs = [{ item = "driftpine_log", count = 2 }]
        output = "reedcutter_knife"
        output_count = 1
        labor = 20
        craft_s = 6.0
    "#;

    fn sources() -> Sources {
        Sources {
            movement: MOVEMENT.into(),
            ships: SHIPS.into(),
            land: LAND.into(),
            labor: LABOR.into(),
            items: ITEMS.into(),
            gathering: GATHERING.into(),
            crafting: CRAFTING.into(),
            farming: FARMING.into(),
        }
    }

    fn parse_farming(farming: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { farming: farming.into(), ..sources() })
    }

    fn farming_errors(farming: &str) -> String {
        parse_farming(farming).unwrap_err().join("\n")
    }

    fn parse_crafting(crafting: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { crafting: crafting.into(), ..sources() })
    }

    fn crafting_errors(crafting: &str) -> String {
        parse_crafting(crafting).unwrap_err().join("\n")
    }

    fn parse_gathering(gathering: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { gathering: gathering.into(), ..sources() })
    }

    fn gathering_errors(gathering: &str) -> String {
        parse_gathering(gathering).unwrap_err().join("\n")
    }

    fn parse_items(items: &str) -> Result<Content, Vec<String>> {
        parse(&Sources { items: items.into(), ..sources() })
    }

    fn item_errors(items: &str) -> String {
        parse_items(items).unwrap_err().join("\n")
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
        let bad = Sources {
            movement: "[[mode]\n".into(),
            ships: "[[hull]\n".into(),
            land: "[[x]\n".into(),
            labor: "cap =\n".into(),
            items: "[bag\n".into(),
            gathering: "[gathering\n".into(),
            crafting: "[crafting\n".into(),
            farming: "[farming\n".into(),
        };
        let e = parse(&bad).unwrap_err().join("\n");
        for file in ["movement.toml", "ships.toml", "land.toml", "labor.toml", "items.toml", "gathering.toml", "crafting.toml", "farming.toml"] {
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
        assert!(rs.contains(r#"LandUseData { land_use: LandUse::Housing, id: "housing", buildable: true, plantable: true },"#));
        assert!(cs.contains(r#"new(LandUse.Housing, "housing", true, true),"#));
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
            // Crops name climates by id, so a renamed climate is renamed there too.
            let farming = if changed.contains("\"arid\"") { FARMING.to_string() } else { FARMING.replace("\"arid\"", "\"dry\"") };
            let c = parse(&Sources { land: changed.clone(), farming, ..sources() });
            assert_ne!(c.unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn land_tables_are_checked_and_fit_the_raster() {
        let e = |land: &str| parse_land(land).unwrap_err().join("\n");
        assert!(e(&LAND.replace("wire = 1\n        buildable", "wire = 2\n        buildable")).contains("no gaps"));
        assert!(e(&LAND.replace("\"arid\"", "\"temperate\"")).contains("duplicate id"));
        assert!(e("land_use = []\nclimate = []").contains("no land_uses"));
        let many_uses: String = (0..17).map(|i| format!("[[land_use]]\nid = \"u{i}\"\nwire = {i}\nbuildable = false\nplantable = false\n")).collect();
        assert!(e(&format!("{many_uses}[[climate]]\nid = \"c\"\nwire = 0\n")).contains("at most 16 land uses"));
        let many_climates: String = (0..9).map(|i| format!("[[climate]]\nid = \"c{i}\"\nwire = {i}\n")).collect();
        let one_use = "[[land_use]]\nid = \"wild\"\nwire = 0\nbuildable = false\nplantable = true\n";
        assert!(e(&format!("{one_use}{many_climates}")).contains("at most 8 climates"));
    }

    #[test]
    fn plots_parse_in_wire_order_and_reach_both_outputs() {
        let c = parse_land(LAND).unwrap();
        assert_eq!(c.plots.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["garden", "farm"]);
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        assert!(rs.contains(r#"PlotData { wire: 0, id: "garden", name: "Garden", deed: 300, size_m: 8, max_rise_m: 2.0, tax: 5 },"#), "{rs}");
        assert!(rs.contains("heavy_tax_pct: &[100, 100, 200], reach_m: 8.0 };"), "{rs}");
        assert!(rs.contains("tax_item: 20, week_s: 604800, grace_s: 604800, demolish_hour_utc: 20, prepay_weeks: 4,"), "{rs}");
        assert!(cs.contains(r#"new(1, "farm", "Farm", 301, 16, 3.0f, 10),"#), "{cs}");
        assert!(cs.contains("Property = new(20, 604800, 604800, 20, 4, new uint[] { 100u, 100u, 200u }, 8.0f);"), "{cs}");
        assert!(rs.contains("Deed = 3,") && cs.contains("Deed = 3,"));
    }

    #[test]
    fn any_property_change_changes_the_hash() {
        let a = parse_land(LAND).unwrap().hash;
        for (from, to) in [
            ("week_s = 604800", "week_s = 604801"),
            ("grace_s = 604800", "grace_s = 86400"),
            ("demolish_hour_utc = 20", "demolish_hour_utc = 3"),
            ("prepay_weeks = 4", "prepay_weeks = 2"),
            ("[100, 100, 200]", "[100, 150, 200]"),
            ("reach_m = 8.0", "reach_m = 9.0"),
            ("size_m = 16", "size_m = 20"),
            ("max_rise_m = 2.0", "max_rise_m = 2.5"),
            ("tax = 5", "tax = 6"),
            ("\"Garden\"", "\"Yard\""),
        ] {
            let changed = LAND.replace(from, to);
            assert_ne!(parse_land(&changed).unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn bad_plots_are_reported_together() {
        let e = |land: &str| parse_land(land).unwrap_err().join("\n");
        let bad = LAND
            .replace("tax_item = \"driftpine_log\"", "tax_item = \"gold\"")
            .replace("week_s = 604800", "week_s = 10")
            .replace("demolish_hour_utc = 20", "demolish_hour_utc = 24")
            .replace("prepay_weeks = 4", "prepay_weeks = 0")
            .replace("[100, 100, 200]", "[100, 200, 150]")
            .replace("reach_m = 8.0", "reach_m = 0.5")
            .replace("size_m = 16", "size_m = 18")
            .replace("max_rise_m = 2.0", "max_rise_m = 0.1")
            .replace("tax = 5", "tax = 0")
            .replace("deed = \"farm_deed\"", "deed = \"driftpine_log\"");
        let got = e(&bad);
        for want in [
            "property.tax_item: no item \"gold\"",
            "property.week_s must be",
            "demolish_hour_utc must be",
            "prepay_weeks must be",
            "heavy_tax_pct needs",
            "property.reach_m",
            "size_m must be a multiple of 4",
            "max_rise_m",
            "tax must be from 1",
            "\"driftpine_log\" isn't of kind \"deed\"",
        ] {
            assert!(got.contains(want), "{want} missing from:\n{got}");
        }
        assert!(e(&LAND.replace("deed = \"farm_deed\"", "deed = \"garden_deed\"")).contains("claims another plot too"));
        assert!(e(&LAND.replace("deed = \"farm_deed\"", "deed = \"barn_deed\"")).contains("no item \"barn_deed\""));
        assert!(e(&LAND.replace("[100, 100, 200]", "[]")).contains("heavy_tax_pct needs"));
        assert!(e(&LAND.replace("[100, 100, 200]", "[90]")).contains("heavy_tax_pct needs"));
        let no_property = LAND.split("[property]").next().unwrap();
        assert!(e(no_property).contains("no [property]"));
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
    fn items_parse_in_wire_order_and_reach_both_outputs() {
        let c = parse_items(ITEMS).unwrap();
        assert_eq!(c.items.iter().map(|i| i.wire).collect::<Vec<_>>(), [3, 20, 300, 301, 400, 401]);
        assert_eq!(c.bag.slots, 50);
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        assert!(rs.contains(r#"ItemData { wire: 3, id: "reedcutter_knife", name: "Reedcutter's Knife", kind: ItemKind::Gear, stack: 1 },"#), "{rs}");
        assert!(rs.contains("pub static ITEMS: [ItemData; 6] = [") && rs.contains("pub const BAG_SLOTS: u16 = 50;"));
        assert!(rs.contains("Consumable = 1,") && cs.contains("Consumable = 1,"));
        assert!(cs.contains(r#"new(20, "driftpine_log", "Driftpine Log", ItemKind.Material, 1000),"#), "{cs}");
        assert!(cs.contains("public const ushort BagSlots = 50;"));
    }

    #[test]
    fn any_item_change_changes_the_hash() {
        let a = parse_items(ITEMS).unwrap().hash;
        for (from, to) in [
            ("wire = 20", "wire = 21"),
            ("\"driftpine_log\"", "\"driftwood\""),
            ("\"Driftpine Log\"", "\"Driftwood\""),
            ("\"material\"", "\"consumable\""),
            ("stack = 1000", "stack = 999"),
            ("slots = 50", "slots = 60"),
        ] {
            // Nodes and recipes name items by id, so a renamed item is renamed there too.
            let changed = ITEMS.replace(from, to);
            let c = parse(&Sources {
                items: changed.clone(),
                gathering: GATHERING.replace(from, to),
                crafting: CRAFTING.replace(from, to),
                land: LAND.replace(from, to),
                farming: FARMING.replace(from, to),
                ..sources()
            });
            assert_ne!(c.unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn bad_items_are_reported_together() {
        let bad = ITEMS
            .replace("wire = 3\n", "wire = 0\n")
            .replace("\"material\"", "\"junk\"")
            .replace("stack = 1\n", "stack = 5\n")
            .replace("slots = 50", "slots = 151");
        let e = item_errors(&bad);
        for want in ["wire 0 is reserved", "kind must be one of material, consumable, gear, deed", "gear doesn't stack", "bag.slots must be"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        let twin = format!("{ITEMS}\n[[item]]\nwire = 20\nid = \"driftpine_log\"\nname = \"Driftpine Log\"\nkind = \"material\"\nstack = 1000\n");
        let e = item_errors(&twin);
        for want in ["duplicate wire 20", "duplicate id", "duplicate name"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(item_errors(&ITEMS.replace("stack = 1000", "stack = 0")).contains("stack must be from 1"));
        assert!(item_errors(&ITEMS.replace("stack = 1000", "stack = 10001")).contains("stack must be from 1"));
        assert!(item_errors(&ITEMS.replace("\"Driftpine Log\"", "\"driftpine \\\"log\\\"\"")).contains("names are"));
        assert!(item_errors(&ITEMS.replace("\"driftpine_log\"", "\"Driftpine\"")).contains("ids are lower-case"));
        assert!(item_errors("item = []\n[bag]\nslots = 50").contains("no items"));
        assert!(item_errors(&ITEMS.replace("stack = 1000", "stack = 1000\nweight = 2")).contains("unknown field `weight`"));
    }

    #[test]
    fn nodes_parse_in_wire_order_and_reach_both_outputs() {
        let c = parse_gathering(GATHERING).unwrap();
        assert_eq!(c.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["driftpine", "hollowoak"]);
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        let row = r#"NodeData { wire: 0, id: "driftpine", name: "Driftpine", kind: NodeKind::Tree, item: 20, yield_min: 2, yield_max: 2, labor: 5, gather_s: 2.5, regrow_s: 300 },"#;
        assert!(rs.contains(row), "{rs}");
        assert!(rs.contains("pub static NODES: [NodeData; 2] = [") && rs.contains("pub const GATHER_REACH_M: f32 = 3.0;"));
        assert!(rs.contains("Ore = 1,") && cs.contains("Ore = 1,"));
        assert!(cs.contains(r#"new(1, "hollowoak", "Hollowoak", NodeKind.Tree, 20, 1, 3, 10, 5.0f, 1200),"#), "{cs}");
        assert!(cs.contains("public const float GatherReachM = 3.0f;"));
    }

    #[test]
    fn any_node_change_changes_the_hash() {
        let a = parse_gathering(GATHERING).unwrap().hash;
        for (from, to) in [
            ("reach_m = 3.0", "reach_m = 3.5"),
            ("\"Hollowoak\"", "\"Old Oak\""),
            ("kind = \"tree\"", "kind = \"ore\""),
            ("item = \"driftpine_log\"", "item = \"reedcutter_knife\""),
            ("yield_min = 1", "yield_min = 2"),
            ("yield_max = 3", "yield_max = 4"),
            ("labor = 10", "labor = 11"),
            ("gather_s = 5.0", "gather_s = 5.5"),
            ("regrow_s = 1200", "regrow_s = 1201"),
        ] {
            let changed = GATHERING.replacen(from, to, 1);
            let c = parse_gathering(&changed);
            // The knife stacks to 1, so its yield must be 1 too.
            let c = c.or_else(|_| parse_gathering(&changed.replace("yield_max = 3", "yield_max = 1")));
            assert_ne!(c.unwrap().hash, a, "{changed}");
        }
    }

    #[test]
    fn bad_nodes_are_reported_together() {
        let bad = GATHERING
            .replace("kind = \"tree\"\n        item = \"driftpine_log\"\n        yield_min = 1", "kind = \"fish\"\n        item = \"pearl\"\n        yield_min = 1")
            .replace("yield_min = 2", "yield_min = 3")
            .replace("labor = 5", "labor = 0")
            .replace("gather_s = 2.5", "gather_s = 0.1")
            .replace("regrow_s = 300", "regrow_s = 0")
            .replace("reach_m = 3.0", "reach_m = 50.0");
        let e = gathering_errors(&bad);
        for want in [
            "kind must be one of plant, ore, tree",
            "no item \"pearl\"",
            "yields must be 1 ≤ yield_min ≤ yield_max",
            "labor must be from 1",
            "gather_s must be from",
            "regrow_s must be from 1",
            "reach_m must be from",
        ] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        let e = gathering_errors(&GATHERING.replace("wire = 1", "wire = 0").replace("\"Driftpine\"", "\"Hollowoak\""));
        for want in ["wire values must run", "duplicate name"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(gathering_errors(&GATHERING.replace("yield_max = 3", "yield_max = 1001")).contains("the item's stack (1000)"));
        assert!(gathering_errors(&GATHERING.replace("regrow_s = 300", "regrow_s = 300\nrare = 1")).contains("unknown field `rare`"));
        assert!(gathering_errors("node = []\n[gathering]\nreach_m = 3.0").contains("no nodes"));
    }

    #[test]
    fn recipes_parse_in_wire_order_and_reach_both_outputs() {
        let c = parse_crafting(CRAFTING).unwrap();
        assert_eq!(c.stations.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["joiners_bench", "cookhearth"]);
        assert_eq!(c.recipes.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["knife", "kindling"]);
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        let row = r#"RecipeData { wire: 1, id: "kindling", station: 1, inputs: &[Ingredient { item: 3, count: 1 }], output: 20, output_count: 3, labor: 5, craft_s: 2.5 },"#;
        assert!(rs.contains(row), "{rs}");
        assert!(rs.contains(r#"StationData { wire: 0, id: "joiners_bench", name: "Joiner's Bench" },"#));
        assert!(rs.contains("pub const CRAFT_REACH_M: f32 = 4.0;") && rs.contains("pub const CRAFT_MAX_COUNT: u32 = 100;"));
        assert!(cs.contains(r#"new(0, "knife", 0, new Ingredient[] { new(20, 2) }, 3, 1, 20, 6.0f),"#), "{cs}");
        assert!(cs.contains(r#"new(1, "cookhearth", "Cookhearth"),"#));
        assert!(cs.contains("public const float CraftReachM = 4.0f;") && cs.contains("public const uint CraftMaxCount = 100;"));
    }

    #[test]
    fn any_recipe_change_changes_the_hash() {
        let a = parse_crafting(CRAFTING).unwrap().hash;
        for (from, to) in [
            ("reach_m = 4.0", "reach_m = 4.5"),
            ("max_count = 100", "max_count = 99"),
            ("\"Cookhearth\"", "\"Old Hearth\""),
            ("station = \"cookhearth\"", "station = \"joiners_bench\""),
            ("count = 2 }", "count = 3 }"),
            ("output_count = 3", "output_count = 4"),
            ("labor = 20", "labor = 21"),
            ("craft_s = 6.0", "craft_s = 6.5"),
            ("id = \"kindling\"", "id = \"tinder\""),
            (
                "[{ item = \"driftpine_log\", count = 2 }]",
                "[{ item = \"driftpine_log\", count = 2 }, { item = \"reedcutter_knife\", count = 1 }]",
            ),
        ] {
            let changed = CRAFTING.replacen(from, to, 1);
            assert_ne!(parse_crafting(&changed).map(|c| c.hash), Ok(a), "{changed}");
        }
    }

    #[test]
    fn bad_recipes_are_reported_together() {
        let bad = CRAFTING
            .replace("station = \"cookhearth\"", "station = \"kiln\"")
            .replace("output = \"driftpine_log\"", "output = \"pearl\"")
            .replace("[{ item = \"driftpine_log\", count = 2 }]", "[{ item = \"sand\", count = 0 }, { item = \"sand\", count = 1 }]")
            .replace("labor = 5", "labor = 0")
            .replace("craft_s = 6.0", "craft_s = 0.1")
            .replace("reach_m = 4.0", "reach_m = 50.0")
            .replace("max_count = 100", "max_count = 0");
        let e = crafting_errors(&bad);
        for want in [
            "no station \"kiln\"",
            "no item \"pearl\"",
            "no item \"sand\"",
            "input \"sand\" is listed twice",
            "count must be from 1",
            "labor must be from 1",
            "craft_s must be from",
            "reach_m must be from",
            "max_count must be from 1",
        ] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        let e = crafting_errors(&CRAFTING.replace("count = 1 }", "count = 1 }, { item = \"driftpine_log\", count = 1 }").replace("output_count = 1", "output_count = 2"));
        for want in ["the output can't be an input", "output_count must be from 1 to the item's stack (1)"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(crafting_errors(&CRAFTING.replace("[{ item = \"driftpine_log\", count = 2 }]", "[]")).contains("1 to 8 inputs"));
        let e = crafting_errors(&CRAFTING.replace("wire = 1\n        id = \"kindling\"", "wire = 0\n        id = \"kindling\"").replace("\"Cookhearth\"", "\"Joiner's Bench\""));
        for want in ["wire values must run", "duplicate name"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(crafting_errors(&CRAFTING.replace("craft_s = 2.5", "craft_s = 2.5\nskill = 1")).contains("unknown field `skill`"));
        assert!(crafting_errors("station = []\nrecipe = []\n[crafting]\nreach_m = 4.0\nmax_count = 1").contains("no stations"));
    }

    #[test]
    fn crops_parse_in_wire_order_and_reach_both_outputs() {
        let c = parse_farming(FARMING).unwrap();
        assert_eq!(c.crops.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["pine", "sheep"]);
        let (rs, cs) = (render_rust(&c), render_csharp(&c));
        let row = r#"CropData { wire: 1, id: "sheep", name: "Sheep", kind: CropKind::Animal, seed: 401, climate: 1, grow_s: 14400, product: 20, yield_min: 2, yield_max: 3, labor: 5, regrow_s: 7200, wither_s: 0, space_m: 1.5, plot_only: true, seed_pct: 10 },"#;
        assert!(rs.contains(row), "{rs}");
        assert!(rs.contains("pub const FARM_REACH_M: f32 = 3.0;") && rs.contains("pub const FARM_IDEAL_PCT: u32 = 70;"));
        assert!(rs.contains("Seed = 4,") && cs.contains("Seed = 4,"), "the seed item kind");
        assert!(cs.contains(r#"new(0, "pine", "Pine", CropKind.Tree, 400, Climate.Temperate, 28800, 20, 4, 6, 10, 0, 86400, 2.0f, false, 0),"#), "{cs}");
        assert!(cs.contains("public const float FarmReachM = 3.0f;") && cs.contains("public const uint FarmIdealPct = 70;"));
        assert!(rs.contains(r#"LandUseData { land_use: LandUse::Wild, id: "wild", buildable: false, plantable: true },"#));
        assert!(cs.contains(r#"new(LandUse.Housing, "housing", true, true),"#));
    }

    #[test]
    fn any_crop_change_changes_the_hash() {
        let a = parse_farming(FARMING).unwrap().hash;
        for (from, to) in [
            ("reach_m = 3.0", "reach_m = 3.5"),
            ("ideal_pct = 70", "ideal_pct = 71"),
            ("\"Pine\"", "\"Old Pine\""),
            ("kind = \"tree\"", "kind = \"crop\""),
            ("climate = \"temperate\"", "climate = \"arid\""),
            ("grow_s = 28800", "grow_s = 28801"),
            ("yield_min = 4", "yield_min = 3"),
            ("yield_max = 6", "yield_max = 7"),
            ("labor = 10", "labor = 11"),
            ("regrow_s = 0", "regrow_s = 600"),
            ("wither_s = 86400", "wither_s = 0"),
            ("space_m = 2.0", "space_m = 2.5"),
            ("plot_only = false", "plot_only = true"),
            ("seed_pct = 0", "seed_pct = 1"),
        ] {
            let changed = FARMING.replacen(from, to, 1);
            assert_ne!(parse_farming(&changed).map(|c| c.hash), Ok(a), "{changed}");
        }
        let land = parse_land(&LAND.replacen("plantable = true", "plantable = false", 1)).unwrap().hash;
        assert_ne!(land, a, "plantable is hashed");
    }

    #[test]
    fn bad_crops_are_reported_together() {
        let bad = FARMING
            .replace("kind = \"tree\"", "kind = \"fungus\"")
            .replace("seed = \"pine_seed\"", "seed = \"farm_deed\"")
            .replace("seed = \"lamb\"", "seed = \"acorn\"")
            .replace("climate = \"arid\"", "climate = \"lunar\"")
            .replace("product = \"driftpine_log\"\n        yield_min = 4", "product = \"pearl\"\n        yield_min = 4")
            .replace("yield_min = 2", "yield_min = 4")
            .replace("labor = 5", "labor = 0")
            .replace("grow_s = 28800", "grow_s = 10")
            .replace("regrow_s = 7200", "regrow_s = 5")
            .replace("wither_s = 86400", "wither_s = 30")
            .replace("space_m = 1.5", "space_m = 0.1")
            .replace("seed_pct = 10", "seed_pct = 101")
            .replace("reach_m = 3.0", "reach_m = 50.0")
            .replace("ideal_pct = 70", "ideal_pct = 5");
        let e = farming_errors(&bad);
        for want in [
            "kind must be one of crop, tree, animal",
            "\"farm_deed\" isn't of kind \"seed\"",
            "no item \"acorn\"",
            "no climate \"lunar\"",
            "no item \"pearl\"",
            "yields must be 1 ≤ yield_min ≤ yield_max",
            "labor must be from 1",
            "grow_s must be from 60",
            "regrow_s must be 0 or from 60",
            "wither_s must be 0 or from 60",
            "space_m must be from",
            "seed_pct must be from 0 to 100",
            "reach_m must be from",
            "ideal_pct must be from 10",
        ] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        let e = farming_errors(&FARMING.replace("seed = \"lamb\"", "seed = \"pine_seed\"").replace("wire = 1", "wire = 0").replace("\"Sheep\"", "\"Pine\""));
        for want in ["plants another crop too", "wire values must run", "duplicate name"] {
            assert!(e.contains(want), "{want} missing from:\n{e}");
        }
        assert!(farming_errors(&FARMING.replace("seed_pct = 0", "seed_pct = 0\nwater = 1")).contains("unknown field `water`"));
        assert!(farming_errors("crop = []\n[farming]\nreach_m = 3.0\nideal_pct = 70").contains("no crops"));
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
