# Trade packs

Status: v0.1, 2026-10-26. M1 task 7 (BACKLOG M1 #1). Code: `content/trade.toml`, `server/sim/src/trade.rs`, `server/world/src/trade.rs`; `tools/worldtool/src/posts.rs`, `world/posts.tsv`; `sidecars/loadbot/src/trade.rs`; client `client/src/Sim/Trade.cs`, `World/TradeRegistry.cs`, `Gameplay/Trade.cs`, `UI/TradeDemo.cs`. Related: docs/design/crafting.md (the bench), docs/design/movement.md (the speed check), ADR 0027.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Packs were crafted at specialty workbenches from regional produce, and making one cost labor | A | RESEARCH §2.5, §2.2 |
| A pack was worn on the back. The carrier was slowed "greatly", couldn't glide, and could ride only a few mounts (donkeys kept their speed best) | A/C | RESEARCH §2.5, §2A.4; [Fandom: Trade Packages](https://archeage.fandom.com/wiki/Trade_Packages) |
| The slowdown as a percentage | **Unknown** | not published (RESEARCH §2A.4) |
| Payout depended on the destination; farther generally paid more. Payment was gold, a special currency, or the pack went into construction | B | RESEARCH §2.5 |
| Packs sold at trade outposts inland, at ports and overseas; the neutral mid-ocean island paid best for cross-ocean runs | B | RESEARCH §2.5, §3 (the 2017 revamp) |
| An outpost's demand fell as packs were delivered there and recovered over time | C | RESEARCH §2.5 |
| Packs expired after about six days | B | RESEARCH §2.5 |
| A dropped pack lay on the ground and anyone could pick it up; taking another's pack in contested land or at sea was theft, the core risk of trading | A | RESEARCH §2.5, §2.7 |
| Exact pack values, route tables, demand curves and recovery rates | **Unknown** | not found; never taken from client data |

## 2. Our rules

### Content (`content/trade.toml`, and packs in `items.toml` and `crafting.toml`)
- Packs are items of a new kind, **`pack`**, with a stack of 1. They are never in a bag (below).
- **`[trade]`** rules:
  - `carry_speed_pct` (60): every movement mode's speed limit while carrying, in percent. Ours: "greatly slowed" with no number published.
  - `reach_m` (4): how close a character must stand to a post to sell, or to a dropped pack to pick it up, horizontally and vertically.
  - `min_km` (0.5): a post closer than this to where a pack was made won't buy it.
  - `per_km_pct` (25) and `max_route_pct` (250): the route factor, below.
  - `fresh_s` (1 day), `expire_s` (6 days, B) and `stale_pct` (80): freshness, below.
  - `demand_min_pct` (40), `sale_drop_pct` (4) and `recover_pct_per_h` (10): demand, below.
- **`[[pack]]`**: each pack item's `value` in copper. Every pack item has exactly one.
- **`[[post]]`** kinds: `wire`, `id`, `name` and `bonus_pct`, a flat bonus added to the route factor (overseas posts pay more).
- Contentgen checks: the pack items exist and are of kind `pack`, each has one value, values 1 to 10,000,000; post ids, names and wires; every percentage and time in range (`carry_speed_pct` 10–100, `stale_pct` 1–100, `fresh_s < expire_s`, `demand_min_pct` 1–100, and so on); and that a recipe whose output is a pack makes exactly one. All of it is part of the content hash.

| Pack | Recipe at the Packwright's Bench | Labor | Time | Value |
|---|---|---|---|---|
| Grain Bale | Hearthgrain 20, Reedcloth 2 | 40 | 6 s | 1g 50s |
| Preserves Crate | Duskberry Preserve 6, Driftpine Plank 4, Rustiron Nails 10 | 40 | 6 s | 1g 80s |

| Post | Where (placeholder) | Bonus |
|---|---|---|
| Cairnwick Post | inland, 2.1 km north-west of Furrowdale | 0 |
| Redfen Post | 1.2 km south of Furrowdale | 0 |
| Gullhaven Post | on the trade island at spawn, 3.8 km across the sea | +20% |

The Packwright's Bench stands in Furrowdale, a hamlet on the west continent's coast near its housing land, where Hearthgrain grows. Every number is ours, for the playtest to tune.

### Money
- Coins are copper, shown as gold, silver and copper (100 copper to a silver, 100 silver to a gold). The private **`purse`** table holds each character's copper (`char_id`, `copper`); **`my_purse`** shows the owner theirs. A character without a row has none.
- Selling packs is the only way to earn coins for now; nothing spends them yet.

### Packs on the back
- A carried pack is a row in the private **`pack`** table, keyed by its carrier's `char_id` (one each at most): `def` (the item's wire), `owner` (the character that made it), `origin_x`, `origin_z` (where it was made), `made_at`, and `on_at` (when it went on this back). The carrier reads it through **`my_pack`**.
- Everyone sees that a character carries a pack, and which kind, from **`entity.pack`** (the item's wire, or 0) in the Near tier. It changes only when a pack goes on or comes off.
- A pack stays on the back when its carrier leaves the world and is there on return.
- **The slowdown:** the speed check (docs/design/movement.md) multiplies the mode's limit by `carry_speed_pct` for a carrier, on land, swimming and on decks alike, and in the frame-change allowance. A move whose interval began before the pack went on (`on_at` after the last accepted move) is judged at the full limit, so a pick-up never corrects the step under way.
- **No gliding, mounts or teleports with a pack.** None of them exist yet; each must refuse a carrier when it comes (gliders and mounts in M2, teleports with the return scroll).
- A pack may be carried aboard ships. Ships' pack slots come with vehicles (BACKLOG M1).

### Making a pack
- At a station, `craft(station, recipe, 1)` as usual (docs/design/crafting.md), with two more refusals when the recipe makes a pack: `count` above 1 ("packs are made one at a time") and a pack already on the back ("already carrying a pack"), checked when it starts and again when it finishes.
- When it finishes, the inputs are taken and the labor spent as for any craft, and the pack goes onto the crafter's back, made at the station's position at that moment. Nothing goes in the bag.

### Dropping and picking up
- **`drop_pack()`** puts the carried pack on the ground at the carrier's feet, as a row in the public **`dropped_pack`** table (Static tier): `id`, `cell`, `def`, `owner`, `x`, `y`, `z`, `yaw`, `origin_x`, `origin_z`, `made_at`, `dropped_at`. It needs the carrier in the world, on land (not aboard a ship, "can't drop a pack on a ship yet"; not swimming, "can't drop a pack in the water"). A spoiled pack (below) is destroyed instead.
- **`pick_up_pack(id)`** puts a dropped pack on the caller's back. Refused if the caller isn't in the world, is on a ship or swimming, already carries a pack, is out of reach, or the pack is spoiled ("this pack has spoiled") or gone ("no such pack").
- Picking up a pack made by another account's character is **theft**: allowed, and logged for the crime task to turn into evidence and crime points (BACKLOG M1). The pack keeps its owner.

### Posts and selling
- The public **`trade_post`** table (Static tier): `id`, `cell`, `kind` (the post's wire), `x`, `y`, `z`, `yaw`. Placed from **`world/posts.tsv`** (`kind x y z yaw-degrees`, as `stations.tsv`), written by `worldtool posts` and uploaded by `cargo xtask world`. Admins have `admin_put_posts`, `admin_remove_posts` and `admin_clear_posts`.
- **`sell_pack(post_id)`** sells the carried pack at a post in reach, for copper into the seller's purse, whoever made the pack. Refused when not in the world, on a ship, swimming, carrying nothing, no such post, out of reach, the post is within `min_km` of the pack's origin ("this post is too close to where the pack was made"), or the pack has spoiled.
- **Price** = value × route × demand × freshness, rounded down to the copper (`sim::trade::price`):
  - **route** = `min(100 + per_km_pct × km, max_route_pct) + bonus_pct` percent, where `km` is the straight-line distance from the pack's origin to the post. Farther pays more, up to the cap, and overseas posts add their bonus.
  - **demand** is per post kind and pack kind, in tenths of a percent. The public **`trade_demand`** table keeps `key` (post wire × 65,536 + pack wire), `permille` and `at`. No row means full (1,000). Its value now is `min(1000, permille + recover_pct_per_h × 10 × hours since at)`. Each sale sets it to its value now minus `sale_drop_pct × 10`, never below `demand_min_pct × 10`, with `at` = now. Nothing ticks.
  - **freshness** is 1,000 for `fresh_s` after `made_at`, then falls in a straight line to `stale_pct × 10` at `expire_s`. From `expire_s` the pack has **spoiled**: it can't be sold or picked up, and dropping it destroys it.
- The client shows the price at a post before selling, computed by the same rules (`sim::trade`, ported to C#).

### The client
- Posts and dropped packs are drawn from the Static tier as placeholder geometry: a post is a stall with an awning and a floating name; a dropped pack is a crate or bale with its name. A carried pack is a box on the character's back, ours and others'.
- With a pack, F at a post in reach sells it; the prompt names the post, the price and its parts (`F: sell Grain Bale at Gullhaven Post for 3g 22s 50c (route 215%, demand 100%, fresh 100%)`), or why it can't. Without one, F picks up the nearest dropped pack in reach (`F: pick up Grain Bale`, `(theft)` when it isn't ours). X drops the pack. While a pick-up is on its way the player stands still, so the first move after it is already at the slower speed.
- The player's own speed is scaled while `my_pack` has a row. The HUD shows the purse, and the pack carried with its freshness and time to spoil.
- `--trade-demo` (dev builds) teleports to the Packwright's Bench, grants the inputs, crafts a Grain Bale, walks a few steps with it, drops it and picks it up, teleports to Gullhaven Post and sells it, logging the purse before and after.

## 3. Deviations and not yet done
- **Distance, not zone tables.** The original priced each route from a table by origin zone and destination. Ours uses the distance from where the pack was made, so any bench works anywhere and routes need no table. Regional recipes (each zone's bench making its own packs from its own produce) can come with the zones.
- **No special currency and no cargo.** Ship currency, trade towers and cargo with value decay are M3 (PLAN §8.3).
- **No pack slots on carts or ships yet** (the vehicles task), so a pack crosses the sea on its carrier's back.
- **Theft is logged, not punished**, until the crime task.
- **No seller's cut, taxes or proficiency bonuses.**
- Spoiled packs on the ground stay as rows until someone tries to pick them up; a sweep can come if they pile up.
- Posts and the bench are placeholder placements until the coast task lays out the zone.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `trade.toml` parses; `[trade]`, packs and posts reach both outputs and the hash; the `pack` item kind; a pack value for a missing or non-pack item, a pack item without a value, two values for one item, a pack recipe making more than one, bad percentages and times, and duplicate post ids, names and wires are reported together | `server/contentgen` |
| sim | `route_pct` below `min_km`, rising with distance, capped, with a bonus; `demand_now` recovering and capping, `after_sale` flooring; `fresh_permille` at each boundary and spoiled; `price` rounding; `carry_limit` | `server/sim/src/trade.rs` |
| sim → C# | `trade_route`, `trade_demand`, `trade_fresh` and `trade_price` vectors | `client/tests/SimVectorTests.cs` |
| module | Crafting a pack puts it on the back with the station's position and spends inputs and labor; count above 1 and an occupied back are refused, and a back filled while it crafts ends the job with nothing spent. The speed check slows a carrier, but not the first move after the pack went on. Drop on land; refused aboard and swimming; a spoiled pack is destroyed. Pick up: reach, back full, spoiled, gone; theft by another account is allowed. Sell: price from route, demand and freshness; demand drops and floors; recovers by the dev clock; refused too close, out of reach, spoiled, carrying nothing. Leaving and re-entering keeps the pack and `entity.pack`. Admin posts; non-admins refused. The views show only your own pack and purse | `server/world/src/tests/trade.rs` |
| worldtool | `posts.tsv` round-trips; bad lines refused; the committed posts and the bench stand in their towns, each post kind once | `tools/worldtool/src/posts.rs` |
| C# | The sell prompt's price and refusals; coin formatting | `client/tests/TradeTests.cs` |
| scenario | `loadbot trade`: a bot at a bench and two posts the scenario places crafts a Grain Bale, walks with it at the slower speed without corrections, is corrected running at full speed, drops it; a second bot picks it up (theft), drops it; the first picks it up, is refused at the near post, teleports to the far post and sells for the computed price; demand drops, and recovers by the dev clock; a spoiled pack is refused. Then it cleans up | `cargo xtask bot trade` |
| client | `--trade-demo` against a live server, with captures of a carried pack, a dropped pack, a post and the sale | manual, with a capture |
