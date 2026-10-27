# Farming: crops, trees and livestock

Status: v0.1, 2026-10-26. M1 task 6 (BACKLOG M1 #1). Code: `content/farming.toml`, `server/sim/src/farm.rs`, `server/world/src/farming.rs`; `sidecars/loadbot/src/farm.rs`; client `client/src/Sim/Farm.cs`, `World/CropRegistry.cs`, `Gameplay/Farming.cs`, `UI/FarmDemo.cs`. Related: docs/design/land.md (plots and access), docs/design/world.md (the climate bits), ADR 0026.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Each crop, tree and animal had a fixed time to mature, shortened in the climate that suited it | B | RESEARCH §2.4; [Wikipedia: ArcheAge](https://en.wikipedia.org/wiki/ArcheAge) |
| Example: barley took 30 minutes in its ideal climate (temperate) and 43 minutes elsewhere, about 70% | C | community crop table seen in a search extract (2026-10-26); the fandom wiki now refuses fetches |
| Mature plants could be harvested repeatedly (fruit trees, livestock) or removed for base materials (felled for timber); trees took hours to days | B | RESEARCH §2.4; [GameSkinny: tree planting](https://www.gameskinny.com/jrthn/archeage-farming-guide-efficient-tree-planting-and-placement) |
| Harvesting cost labor; growth times weighed heavily on labor use | B | [GameSkinny: labor and gold](https://www.gameskinny.com/wwr0n/new-archeage-players-learn-to-manage-your-labor-and-time-to-make-gold) |
| Planting on unclaimed land was allowed, but anyone could harvest or uproot the mature plants, earning crime points | B | RESEARCH §2.4 |
| Livestock had to stand on protected land (a farm), and fell sick if not tended | C | [SweClockers forum post](https://www.sweclockers.com/forum/post/12013747) |
| Watering halved a crop's growth time; without water, feed or medicine plants withered and died | C | same forum post; RESEARCH §2.4 |
| Livestock occasionally produced offspring | B | RESEARCH §2.4 |
| Seeds were bought from seed merchants near farms | B | [Fandom wiki: Seed Merchant](https://archeage.fandom.com/wiki/Seed_Merchant) (stub, search extract) |
| Exact timers, yields, labor costs and the climate multipliers for each plant | **Unknown** | not found; never taken from client data |

## 2. Our rules

### Content (`content/farming.toml`)
- **`[farming]`** rules:
  - `reach_m` (3): how far a character may plant, harvest or uproot, horizontally.
  - `ideal_pct` (70): growth time, in percent, in the plant's ideal climate. From the barley example (C).
- **`[[crop]]`** kinds (crops, trees and livestock alike), each with:
  - `wire` (stored in crop rows, 0, 1, 2, …), `id`, `name`, and `kind`: `crop`, `tree` or `animal`.
  - `seed`: the item planted, of the new item kind `seed`. Each seed plants one kind.
  - `climate`: the ideal climate, an id from `land.toml`.
  - `grow_s`: time to mature outside the ideal climate.
  - `product`, `yield_min`, `yield_max`: what one harvest gives.
  - `labor`: spent on each harvest.
  - `regrow_s`: after a harvest it's ready again this much later (the climate applies); 0 means a harvest removes it.
  - `wither_s`: how long it stays ripe before it withers; 0 means never.
  - `space_m`: its radius. Two plantings may not be closer than the sum of their radii.
  - `plot_only`: it may only stand on a plot.
  - `seed_pct`: the chance in percent that a harvest also gives one of its seed (livestock offspring).
- Contentgen checks: ids, names, kinds, the seed exists and is of kind `seed` and plants one kind only, the climate and product exist, yields within the product's stack, labor 1–1000, `grow_s` 60 s to 7 days, `regrow_s` 0 or 60 s to 7 days, `wither_s` 0 or 60 s to 30 days, `space_m` 0.25–8, `seed_pct` 0–100, `reach_m` 1–20, `ideal_pct` 10–100. All of it is part of the content hash.
- `land.toml` gains `plantable` on each land use: `wild` and `housing` are, `town` and `road` aren't.

| Kind | Name | Seed | Ideal climate | Grows (ideal) | Harvest | Labor | Again | Withers | Space |
|---|---|---|---|---|---|---|---|---|---|
| crop | Hearthgrain | Hearthgrain Seed | temperate | 43 min (30) | 3–5 Hearthgrain | 3 | — | 1 day | 0.5 m |
| crop | Whiteflax | Whiteflax Seed | temperate | 60 min (42) | 2–4 Whiteflax Fibre | 3 | — | 1 day | 0.5 m |
| crop | Sunroot | Sunroot Cutting | tropical | 90 min (63) | 2–4 Sunroot | 4 | — | 1 day | 0.5 m |
| crop | Cinderpepper | Cinderpepper Seed | arid | 60 min (42) | 2–4 Cinderpepper | 3 | — | 1 day | 0.5 m |
| crop | Dustbean | Dustbean Seed | arid | 43 min (30) | 3–5 Dustbean | 3 | — | 1 day | 0.5 m |
| crop | Frostkale | Frostkale Seed | subarctic | 90 min (63) | 2–4 Frostkale | 4 | — | 1 day | 0.5 m |
| tree | Amberpear Tree | Amberpear Sapling | temperate | 6 h (4.2 h) | 3–6 Amberpear | 5 | 4 h | never | 2 m |
| tree | Driftpine Tree | Driftpine Sapling | subarctic | 8 h (5.6 h) | 4–6 Driftpine Log | 10 | felled | never | 2 m |
| animal | Woolback | Woolback Lamb | temperate | 4 h (2.8 h) | 2–3 Woolback Fleece, 10% a lamb | 5 | 2 h | never | 1.5 m, plots only |

Hearthgrain matches the barley example; every other number is ours, for the playtest to tune.

- **Seeds** come from recipes until merchants and gold exist: each crop's seed at the Cookhearth (1 of the crop → 2 seeds, 2 labor, 2 s), the Amberpear Sapling at the Joiner's Bench (Amberpear 2, Saltreed Twine 1 → 1, 5 labor, 4 s), the Driftpine Sapling there too (Driftpine Log 1, Saltreed Twine 1 → 1, 5 labor, 4 s). Lambs come only from harvests (and dev grants). Our numbers.

### Plantings
- The public **`crop`** table (crops, trees and livestock): `id`, `cell` (indexed), `kind`, `owner` (the planter's `char_id`), `plot` (the plot it stands on, or 0), `x`, `y`, `z`, `yaw`, `ideal` (it's in its ideal climate), `planted_at`, `grown_from` (when the current growth began: planting, or the last harvest), `ripe_at` and `harvests`. In the **Static tier**, with nodes, stations and plots.
- Nothing is ticked. From `grown_from`, `ripe_at` and game time, `sim::farm::stage` gives **growing** (with progress 0–1), **ripe** from `ripe_at`, and **withered** from `ripe_at + wither_s` (kinds that wither). The client draws the stage from the same rule.
- `ripe_at = grown_from + grow_s` (or `regrow_s` after a harvest), times `ideal_pct` when `ideal`.

### Planting (`plant(item_id, x, z)`)
- The item must be a seed in the caller's bag. The planter must be in the world, on land (not aboard a ship or swimming), and within `reach_m` of the spot horizontally. One seed is taken.
- **Spot checks** (`sim::farm::check_spot`, ported to C# for the preview), in order:
  1. On the map.
  2. The 4 m land texel under the spot is a plantable use (`wild` or `housing`). No land data is wild.
  3. The four height samples around the spot are known, and the lowest is at least 0.5 m above sea level. That lowest sample is the planting's `y`.
  4. If the spot is on a standing plot, the planter may tend it (below). If it isn't and the kind is `plot_only`: refused.
  5. No other planting is closer than the two radii added. Plantings that are gone (below) don't count, and are removed by the planting that needs their place.
- `ideal` is whether the texel's climate (bits 4–6) is the kind's.
- Which plot a spot is on is decided by its centre; a planting may overhang a plot's edge.

### Who may tend what
- A planting **on a plot** may be harvested and uprooted by the plot owner's account, and by anyone while the plot is public. Planting there follows the same rule.
- A planting **off any plot** may always be harvested (when ripe) and uprooted by the planter's account. Anyone else may harvest it once ripe and uproot it once ripe or withered. Harvesting or uprooting someone else's ripe planting off a plot is **theft**: it's allowed, and logged as a crime for the crime task to turn into evidence and crime points. Clearing a withered planting is no crime.

### Harvest (`harvest(crop)`) and uproot (`uproot(crop)`)
- Both need the caller in the world, on land and within reach, and the planting not gone.
- **Harvest** needs it ripe and the labor, rolls the yield, and checks the bag has room for it (and for the seed if the `seed_pct` roll gives one). It spends the labor and grants the items in one transaction. Then a kind with `regrow_s` starts growing again (`grown_from = now`, `harvests + 1`); any other is removed. It's instant (see deviations).
- **Uproot** removes it and gives nothing back. It costs nothing.
- A planting is **gone** when it has withered, or when its plot is missing or demolished by game time. A gone planting can't be harvested; it can still be uprooted by those who may tend it, or cleared by planting over it.
- When a plot is demolished or abandoned, the plantings on it are removed with it.

### The client
- Plantings are drawn from the Static tier as placeholder geometry by kind, scaled by growth: sprouts that grow, then ripe with fruit or grain heads; withered ones brown and slumped. A small label shows the name and the time left, or `ripe`.
- **Planting:** right-click a seed in the bag to start. A disc of the kind's radius follows a point 1.5 m in front of the player, green where `check_spot` passes and red with the reason in the HUD. F or a left click plants, Esc or a right click cancels. Planting stays on after each seed, so a row can be planted step by step, until the seeds run out or it's cancelled.
- **Tending:** with nothing else in reach, F harvests the nearest ripe planting in reach that we may harvest (the prompt names it, the labor and whether it's theft). G uproots the nearest planting we may uproot, after a second press to confirm.
- `--farm-demo` (dev builds) finds the nearest housing site, places a farm (from a deed and seals it grants), plants a row of Hearthgrain and a Woolback, moves the dev clock until they're ripe, harvests both, and then shows a crop planted off the plot withering by the clock.

## 3. Deviations and not yet done
- **No care.** Watering, feeding and medicine (and sickness) come in M2 with climates' full rules; until then plants grow unattended and crops only wither when left ripe.
- **Instant harvest.** The original had a short harvest cast; ours finishes at once. A cast can reuse the gathering job if the playtest wants one.
- **No crime yet.** Theft is logged; evidence and crime points come with the crime task (BACKLOG M1 #6).
- **No seed merchants.** Seeds are crafted until gold exists.
- **Planting costs no labor**; only harvests do.
- **No growth rings for livestock offspring as pets or mounts**, only lambs.
- Withered and orphaned plantings stay as rows until someone plants over them or uproots them; a sweep can come later if they pile up.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `[farming]` and `[[crop]]` parse in wire order and reach both outputs and the hash; `plantable` reaches both. Unknown or wrong-kind seeds, a seed used twice, unknown climates and products, bad yields, labor, times, space and percentages are reported together | `server/contentgen` |
| sim | `growth_micros` with and without the ideal climate; `stage` at each boundary, with and without withering; `progress`; each spot refusal (off the map, not plantable, no ground, under water, no access, plots only, too close) and a pass, with gone plantings ignored; `ideal` from the climate bits; the yield and seed rolls cover their ranges | `server/sim/src/farm.rs` |
| sim → C# | `farm_stage` and `farm_spot` vectors | `client/tests/SimVectorTests.cs` |
| module | Planting takes one seed and sets `ripe_at` by climate. Each refusal changes nothing: not in the world, ship, swimming, not a seed, out of reach, every spot refusal. On a private plot only the owner plants; on a public plot anyone. Harvest by the dev clock: refused while growing, then gives the yield, spends labor, regrows or removes; refused without labor or room; withered can't be harvested. Theft: a stranger harvests a ripe crop off a plot, but not one on a private plot, and can't uproot a growing one. Livestock only on plots. Demolishing or abandoning a plot removes its plantings; a demolished plot's plantings can't be harvested. Planting over a withered one clears it | `server/world/src/tests/farming.rs` |
| C# | The planting preview on the client's own readers, and the stage text | `client/tests/FarmTests.cs` |
| scenario | `loadbot farm`: a bot on uploaded test ground places a farm, plants crops and a Woolback, is refused off the plot and too close, moves the dev clock to ripe, harvests, and a second bot steals a ripe crop off the plot and is refused one on it. Then it cleans up | `cargo xtask bot farm` |
| client | `--farm-demo` against a live server, with captures of the preview, growing and ripe plantings, and a withered crop | manual, with a capture |
