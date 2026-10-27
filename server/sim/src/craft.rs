//! Crafting (docs/design/crafting.md): who may use a station, how far a unit has got, and how many
//! units the bag and labor cover. The client ports this for the craft window and the progress bar,
//! and the server decides.

pub use crate::content::{
    CRAFT_MAX_COUNT, CRAFT_REACH_M, Ingredient, RECIPES, RecipeData, STATIONS, StationData, recipe, recipe_by_id, station, station_by_id,
};
pub use crate::gather::{MOVE_CANCEL_M, moved_off};

const MICROS_PER_S: i64 = 1_000_000;

/// A character standing at `at` (its feet) can use a station at `station`: within the reach
/// horizontally, and within it up or down.
pub fn in_reach(station: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - station[0], at[2] - station[2]);
    dx * dx + dz * dz <= CRAFT_REACH_M * CRAFT_REACH_M && (at[1] - station[1]).abs() <= CRAFT_REACH_M
}

pub fn craft_micros(r: &RecipeData) -> i64 {
    (f64::from(r.craft_s) * MICROS_PER_S as f64).round() as i64
}

/// How far a unit finishing at `done_at` has got at `now`, from 0 to 1.
pub fn progress(r: &RecipeData, done_at: i64, now: i64) -> f32 {
    let total = craft_micros(r);
    let left = done_at.saturating_sub(now);
    (1.0 - left as f64 / total as f64).clamp(0.0, 1.0) as f32
}

/// How many units the inputs held (`have`, by item wire) and `labor` cover, up to `CRAFT_MAX_COUNT`.
/// Room in the bag isn't counted.
pub fn affordable(r: &RecipeData, have: impl Fn(u16) -> u32, labor: i32) -> u32 {
    let by_labor = if r.labor > 0 { u32::try_from(labor.max(0) / r.labor).unwrap_or(0) } else { CRAFT_MAX_COUNT };
    r.inputs.iter().map(|i| have(i.item) / i.count).fold(by_labor, u32::min).min(CRAFT_MAX_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twine() -> &'static RecipeData {
        recipe_by_id("saltreed_twine").unwrap()
    }

    #[test]
    fn reach_is_horizontal_and_vertical() {
        let s = [10.0, 5.0, -4.0];
        assert!(in_reach(s, s));
        assert!(in_reach(s, [10.0 + CRAFT_REACH_M, 5.0, -4.0]), "the edge counts");
        assert!(!in_reach(s, [10.0, 5.0, -4.0 - CRAFT_REACH_M - 0.01]));
        assert!(!in_reach(s, [13.0, 5.0, -1.0]), "4.24 m on the diagonal");
        assert!(in_reach(s, [10.0, 5.0 + CRAFT_REACH_M, -4.0]));
        assert!(!in_reach(s, [10.0, 5.0 - CRAFT_REACH_M - 0.01, -4.0]), "too far below");
    }

    #[test]
    fn progress_runs_from_zero_to_one() {
        let t = 1_800_000_000_000_000;
        let r = twine();
        let done = t + craft_micros(r);
        assert_eq!(progress(r, done, t), 0.0);
        assert_eq!(progress(r, done, t - 5_000_000), 0.0, "clamped before the start");
        assert!((progress(r, done, t + craft_micros(r) / 2) - 0.5).abs() < 1e-6);
        assert_eq!(progress(r, done, done), 1.0);
        assert_eq!(progress(r, done, done + 1_000_000), 1.0, "clamped after the end");
    }

    #[test]
    fn affordability_is_limited_by_each_input_labor_and_the_cap() {
        let bar = recipe_by_id("rustiron_bar").unwrap(); // 3 ore + 1 charcoal, 10 labor
        let ore = crate::content::item_by_id("rustvein_ore").unwrap().wire;
        let coal = crate::content::item_by_id("charcoal").unwrap().wire;
        let bag = |o: u32, c: u32| move |w: u16| if w == ore { o } else if w == coal { c } else { 0 };
        assert_eq!(affordable(bar, bag(9, 5), 5000), 3, "ore for 3");
        assert_eq!(affordable(bar, bag(30, 2), 5000), 2, "charcoal for 2");
        assert_eq!(affordable(bar, bag(30, 10), 25), 2, "labor for 2");
        assert_eq!(affordable(bar, bag(2, 10), 5000), 0);
        assert_eq!(affordable(bar, bag(30, 10), -5), 0, "a pool below 0 covers nothing");
        assert_eq!(affordable(bar, bag(100_000, 100_000), 1_000_000), CRAFT_MAX_COUNT);
    }

    #[test]
    fn every_recipe_names_a_station_and_items() {
        for r in &RECIPES {
            assert!(station(r.station).is_some(), "{}", r.id);
            assert!(crate::content::item(r.output).is_some(), "{}", r.id);
            assert!(r.inputs.iter().all(|i| crate::content::item(i.item).is_some()), "{}", r.id);
            assert_eq!(recipe(r.wire), Some(r));
        }
        for s in &STATIONS {
            assert!(RECIPES.iter().any(|r| r.station == s.wire), "{} has no recipes", s.id);
        }
    }
}
