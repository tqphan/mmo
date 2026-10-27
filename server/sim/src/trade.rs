//! Trade packs (docs/design/trade.md): what a pack sells for at a post, from where it was made, the
//! post's demand and its age, and how much slower its carrier goes. Demand and freshness come from
//! timestamps; nothing is ticked. The client ports this for the sell prompt, and the server decides.

pub use crate::content::{PACKS, POSTS, PackData, PostData, TRADE, pack, post, post_by_id};

const MICROS_PER_S: i64 = 1_000_000;
const MICROS_PER_H: i64 = 3_600 * MICROS_PER_S;
/// Demand and freshness are in tenths of a percent.
pub const FULL: u32 = 1000;

/// A whole number from the rules; contentgen checks they aren't negative.
fn pct(v: i32) -> u32 {
    v.max(0).unsigned_abs()
}

/// Every movement mode's speed limit for a carrier.
pub fn carry_limit(max_speed_mps: f32) -> f32 {
    max_speed_mps * TRADE.carry_speed_pct as f32 / 100.0
}

/// A character at `at` (its feet) can sell at a post, or pick up a pack, at `thing`: within reach
/// horizontally, and up or down.
pub fn in_reach(thing: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - thing[0], at[2] - thing[2]);
    dx * dx + dz * dz <= TRADE.reach_m * TRADE.reach_m && (at[1] - thing[1]).abs() <= TRADE.reach_m
}

/// The route factor in percent for a pack made at `origin` sold at a post at `post` (both x, z)
/// with `bonus_pct`, or `None` if the post is too close to buy it.
pub fn route_pct(origin: [f32; 2], post: [f32; 2], bonus_pct: u32) -> Option<u32> {
    let (dx, dz) = (f64::from(post[0]) - f64::from(origin[0]), f64::from(post[1]) - f64::from(origin[1]));
    let km = (dx * dx + dz * dz).sqrt() / 1000.0;
    if km < f64::from(TRADE.min_km) {
        return None;
    }
    let route = (100.0 + f64::from(TRADE.per_km_pct) * km).min(f64::from(TRADE.max_route_pct));
    Some(route.floor() as u32 + bonus_pct)
}

/// A post's demand for a pack kind now, from what it was (`permille`) at `at`; `FULL` without a row.
pub fn demand_now(permille: u32, at: i64, now: i64) -> u32 {
    let gained = now.saturating_sub(at).max(0).saturating_mul(i64::from(TRADE.recover_pct_per_h) * 10) / MICROS_PER_H;
    u32::try_from(i64::from(permille).saturating_add(gained).min(i64::from(FULL))).unwrap_or(FULL)
}

/// Demand right after a sale at `demand` (its value now).
pub fn after_sale(demand: u32) -> u32 {
    demand.saturating_sub(pct(TRADE.sale_drop_pct) * 10).max(pct(TRADE.demand_min_pct) * 10)
}

/// How fresh a pack made at `made_at` is at `now`, in tenths of a percent, or `None` once it has spoiled.
pub fn fresh_permille(made_at: i64, now: i64) -> Option<u32> {
    let age = now.saturating_sub(made_at).max(0);
    let (fresh, expire) = (i64::from(TRADE.fresh_s) * MICROS_PER_S, i64::from(TRADE.expire_s) * MICROS_PER_S);
    if age >= expire {
        return None;
    }
    if age <= fresh {
        return Some(FULL);
    }
    let fall = i64::from(FULL - pct(TRADE.stale_pct) * 10);
    Some(FULL - u32::try_from(fall * (age - fresh) / (expire - fresh)).unwrap_or(0))
}

/// When a pack made at `made_at` spoils.
pub fn spoils_at(made_at: i64) -> i64 {
    made_at.saturating_add(i64::from(TRADE.expire_s) * MICROS_PER_S)
}

/// `value` copper times the route (percent), demand and freshness (both permille), rounded down.
pub fn price(value: u32, route_pct: u32, demand: u32, fresh: u32) -> u64 {
    u64::from(value) * u64::from(route_pct) * u64::from(demand) * u64::from(fresh) / (100 * u64::from(FULL) * u64::from(FULL))
}

/// The `trade_demand` key for a post kind and a pack item.
pub fn demand_key(post: u8, item: u16) -> u32 {
    u32::from(post) << 16 | u32::from(item)
}

/// What a post would pay for a pack, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub route_pct: u32,
    pub demand: u32,
    pub fresh: u32,
    pub copper: u64,
}

/// Why a post won't buy a pack. The discriminant is the code in the client's vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NotAPack = 1,
    TooClose = 2,
    Spoiled = 3,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Refusal::NotAPack => "that isn't a pack",
            Refusal::TooClose => "this post is too close to where the pack was made",
            Refusal::Spoiled => "this pack has spoiled",
        }
    }
}

/// A pack of item `def`, made at `origin` (x, z) at `made_at`, offered at a post at `at` (x, z)
/// with `bonus_pct` whose demand for it is `demand` now.
pub fn quote(def: u16, origin: [f32; 2], made_at: i64, at: [f32; 2], bonus_pct: u32, demand: u32, now: i64) -> Result<Quote, Refusal> {
    let value = pack(def).ok_or(Refusal::NotAPack)?.value;
    let fresh = fresh_permille(made_at, now).ok_or(Refusal::Spoiled)?;
    let route_pct = route_pct(origin, at, bonus_pct).ok_or(Refusal::TooClose)?;
    Ok(Quote { route_pct, demand, fresh, copper: price(value, route_pct, demand, fresh) })
}

/// Copper as players read it: `3g 22s 50c`, leaving out leading zero parts.
pub fn coins(copper: u64) -> String {
    let (g, s, c) = (copper / 10_000, copper / 100 % 100, copper % 100);
    match (g, s) {
        (0, 0) => format!("{c}c"),
        (0, _) => format!("{s}s {c}c"),
        _ => format!("{g}g {s}s {c}c"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = MICROS_PER_H;

    #[test]
    fn content_is_sane() {
        assert!(TRADE.fresh_s < TRADE.expire_s);
        for p in &PACKS {
            assert_eq!(crate::content::item(p.item).map(|i| i.kind), Some(crate::content::ItemKind::Pack));
        }
        assert_eq!(post_by_id("gullhaven_post").map(|p| p.bonus_pct), Some(20));
    }

    #[test]
    fn carriers_are_slower() {
        assert!((carry_limit(5.4) - 5.4 * 0.6).abs() < 1e-5);
    }

    #[test]
    fn reach_is_horizontal_and_vertical() {
        assert!(in_reach([0.0, 0.0, 0.0], [TRADE.reach_m - 0.01, 0.0, 0.0]));
        assert!(!in_reach([0.0, 0.0, 0.0], [TRADE.reach_m + 0.01, 0.0, 0.0]));
        assert!(!in_reach([0.0, 0.0, 0.0], [0.0, TRADE.reach_m + 0.01, 0.0]));
    }

    #[test]
    fn route_rises_with_distance_to_a_cap() {
        let min = TRADE.min_km * 1000.0;
        assert_eq!(route_pct([0.0, 0.0], [min - 1.0, 0.0], 0), None);
        let at_min = route_pct([0.0, 0.0], [0.0, min], 0).unwrap();
        assert_eq!(at_min, 100 + (TRADE.per_km_pct as f32 * TRADE.min_km) as u32);
        assert_eq!(route_pct([0.0, 0.0], [2000.0, 0.0], 0), Some(100 + 2 * pct(TRADE.per_km_pct)));
        assert_eq!(route_pct([1000.0, 1000.0], [1000.0, 3000.0], 20), Some(100 + 2 * pct(TRADE.per_km_pct) + 20));
        assert_eq!(route_pct([-8000.0, 0.0], [8000.0, 0.0], 0), Some(pct(TRADE.max_route_pct)));
        assert_eq!(route_pct([-8000.0, 0.0], [8000.0, 0.0], 20), Some(pct(TRADE.max_route_pct) + 20));
    }

    #[test]
    fn demand_recovers_to_full_and_falls_to_a_floor() {
        let per_h = pct(TRADE.recover_pct_per_h) * 10;
        assert_eq!(demand_now(500, 0, 0), 500);
        assert_eq!(demand_now(500, 0, H), 500 + per_h);
        assert_eq!(demand_now(500, 0, H / 2), 500 + per_h / 2);
        assert_eq!(demand_now(500, 0, 1000 * H), FULL);
        assert_eq!(demand_now(500, H, 0), 500, "a clock behind the row gains nothing");
        assert_eq!(after_sale(FULL), FULL - pct(TRADE.sale_drop_pct) * 10);
        let floor = pct(TRADE.demand_min_pct) * 10;
        assert_eq!(after_sale(floor + 1), floor);
        assert_eq!(after_sale(0), floor);
        let mut d = FULL;
        for _ in 0..1000 {
            d = after_sale(d);
        }
        assert_eq!(d, floor);
    }

    #[test]
    fn freshness_holds_then_falls_then_spoils() {
        let (fresh, expire) = (i64::from(TRADE.fresh_s) * MICROS_PER_S, i64::from(TRADE.expire_s) * MICROS_PER_S);
        assert_eq!(fresh_permille(0, 0), Some(FULL));
        assert_eq!(fresh_permille(0, -H), Some(FULL), "made in the future reads as new");
        assert_eq!(fresh_permille(0, fresh), Some(FULL));
        let half = fresh + (expire - fresh) / 2;
        assert_eq!(fresh_permille(0, half), Some(FULL - (FULL - pct(TRADE.stale_pct) * 10) / 2));
        assert!(fresh_permille(0, expire - 1).unwrap() >= pct(TRADE.stale_pct) * 10);
        assert_eq!(fresh_permille(0, expire), None);
        assert_eq!(spoils_at(5), 5 + expire);
    }

    #[test]
    fn price_multiplies_and_rounds_down() {
        assert_eq!(price(15000, 100, FULL, FULL), 15000);
        assert_eq!(price(15000, 215, FULL, FULL), 32250);
        assert_eq!(price(15000, 215, 960, 1000), 30960);
        assert_eq!(price(3, 133, 999, 999), 3);
        assert_eq!(price(10_000_000, 2000, FULL, FULL), 200_000_000);
    }

    #[test]
    fn quotes_refuse_in_order() {
        let bale = crate::content::item_by_id("grain_bale").unwrap().wire;
        let q = quote(bale, [0.0, 0.0], 0, [2000.0, 0.0], 0, FULL, 0).unwrap();
        assert_eq!(q.copper, price(pack(bale).unwrap().value, q.route_pct, FULL, FULL));
        assert_eq!(quote(1, [0.0, 0.0], 0, [2000.0, 0.0], 0, FULL, 0), Err(Refusal::NotAPack));
        let spoiled = i64::from(TRADE.expire_s) * MICROS_PER_S;
        assert_eq!(quote(bale, [0.0, 0.0], 0, [0.0, 0.0], 0, FULL, spoiled), Err(Refusal::Spoiled));
        assert_eq!(quote(bale, [0.0, 0.0], 0, [0.0, 0.0], 0, FULL, 0), Err(Refusal::TooClose));
    }

    #[test]
    fn coins_read_as_gold_silver_copper() {
        assert_eq!(coins(0), "0c");
        assert_eq!(coins(99), "99c");
        assert_eq!(coins(150), "1s 50c");
        assert_eq!(coins(32250), "3g 22s 50c");
        assert_eq!(coins(10_000), "1g 0s 0c");
    }

    #[test]
    fn demand_keys_keep_post_and_item_apart() {
        assert_eq!(demand_key(2, 500), 2 << 16 | 500);
        assert_ne!(demand_key(1, 0), demand_key(0, 1));
    }
}
