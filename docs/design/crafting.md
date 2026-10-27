# Crafting

Status: v0.1, 2026-10-26. M1 task 4. Code: `content/crafting.toml`, `server/sim/src/craft.rs`, `server/world/src/crafting.rs`; `tools/worldtool/src/stations.rs`, `world/stations.tsv`; client `client/src/World/StationRegistry.cs`, `Gameplay/Crafting.cs`, `Gameplay/CraftModel.cs`, `UI/CraftWindow.cs`; `sidecars/loadbot/src/craft.rs`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Items were crafted at the station that matched the recipe, such as an anvil, smelter, oven or workbench. The main towns had every station; smaller towns had some | B | [Fandom wiki: Crafting, old revision](https://archeage.fandom.com/wiki/Crafting?oldid=5688) (search extract, 2026-10-26) |
| A craft needed the ingredients, enough labor and a suitable station, and nothing else | B | same |
| Almost every crafting action cost labor, from a few points up to several dozen for advanced work such as building | B | same; RESEARCH §2.3 |
| Every recipe could be browsed by anyone at any time from a recipe book | C | same (a 2014 guide) |
| Crafting proficiencies had ten ranks; higher ranks lowered labor costs and crafting time, and some recipes needed a rank | B | RESEARCH §2.3; [The Games Wiki](https://thegameswiki.com/archeage-chronicles/wiki/crafting-and-proficiencies) |
| A craft was a cast at the station, and several could be queued at once ("craft all") | C | common player knowledge; not found in a source |
| Craft times, whether moving interrupted a craft, and how a batch spent labor | **Unknown** | not found (searched 2026-10-26) |

## 2. Our rules

### Stations and recipes (content)
- `content/crafting.toml` has `[crafting]` rules, `[[station]]` kinds and `[[recipe]]`s.
  - `reach_m` (4 m): how close a character must stand to a station, horizontally and vertically.
  - `max_count` (100): the most units one `craft` call may ask for.
- A **station** has a `wire` (stored in station rows; dense, from 0), an `id` and a `name`.
- A **recipe** has:
  - a `wire` (dense, from 0; stored only in craft jobs, which last seconds) and an `id`;
  - the `station` it needs (a station id);
  - `inputs`, a list of `{ item, count }`;
  - the `output` item and `output_count`;
  - `labor` and `craft_s`, both per unit.
- Contentgen checks:
  - the ids, wires and station names;
  - that every station and item exists;
  - 1 to 8 inputs, with no item listed twice and the output not among them;
  - input counts from 1 to 10,000;
  - `1 ≤ output_count ≤` the output's stack;
  - `labor` from 1 to 1,000 and `craft_s` from 0.5 to 600;
  - `reach_m` from 0.5 to 20 and `max_count` from 1 to 1,000.
- Recipes and stations are part of the content hash.
- There are three stations and 20 recipes. Every name is in `docs/world/`, and the numbers are ours.

| Station | Recipe | Inputs | Makes | Labor | Time |
|---|---|---|---|---|---|
| Joiner's Bench | Saltreed Twine | Saltreed 4 | 1 | 5 | 2 s |
| | Reedcloth | Saltreed Twine 4 | 1 | 10 | 4 s |
| | Saltrope | Saltreed Twine 3 | 1 | 5 | 3 s |
| | Sailcloth | Reedcloth 3, Saltrope 1 | 1 | 20 | 6 s |
| | Driftpine Plank | Driftpine Log 1 | 2 | 5 | 3 s |
| | Hollowoak Plank | Hollowoak Log 1 | 2 | 5 | 3 s |
| | Hollowoak Beam | Hollowoak Plank 3, Rustiron Nails 4 | 1 | 15 | 5 s |
| | Reedcutter's Knife | Rustiron Bar 1, Driftpine Plank 1, Saltreed Twine 1 | 1 | 20 | 6 s |
| | Felling Axe | Rustiron Bar 2, Hollowoak Plank 1 | 1 | 25 | 8 s |
| | Prospector's Pick | Rustiron Bar 2, Driftpine Plank 1 | 1 | 25 | 8 s |
| Bloomery | Tin Ingot | Tinstone 3, Charcoal 1 | 1 | 10 | 5 s |
| | Rustiron Bar | Rustvein Ore 3, Charcoal 1 | 1 | 10 | 5 s |
| | Rustiron Nails | Rustiron Bar 1 | 10 | 5 | 3 s |
| | Rustiron Fittings | Rustiron Bar 2, Tin Ingot 1 | 1 | 15 | 6 s |
| | Tin Lantern | Tin Ingot 2, Saltreed Twine 1 | 1 | 15 | 6 s |
| Cookhearth | Charcoal | Driftpine Log 2 | 3 | 5 | 4 s |
| | Duskberry Tart | Duskberry 6, Emberleaf 1 | 2 | 10 | 4 s |
| | Duskberry Preserve | Duskberry 8 | 2 | 10 | 5 s |
| | Emberleaf Salve | Emberleaf 4, Duskberry 1 | 2 | 10 | 4 s |
| | Emberleaf Tea | Emberleaf 3 | 2 | 5 | 3 s |

The chains are deliberate. Charcoal from the hearth feeds the bloomery, and bars and nails from the bloomery feed the bench. Planks, beams, sailcloth, rope and fittings are there for the later tasks: land, vehicles and ships.

### Stations in the world
- The public **`station`** table has one row per station: `id`, `cell` (indexed), `kind` (the wire), `x`, `y`, `z` and `yaw`. Nothing else changes on it, and any number of players may use one station at once.
- Stations are in the **Static tier**, next to resource nodes.
- Placements are part of the world's files, in **`world/stations.tsv`**: one per line (`kind-id x y z yaw-degrees`), the same format as `nodes.tsv`. For now it's written by hand: one of each station in the town round spawn. A worldtool test checks that each stands on the ground there. `cargo xtask world` uploads them after the nodes, replacing every station.
- Admins have `admin_put_stations(Vec<StationPlacement>)`, `admin_remove_stations(ids)` and `admin_clear_stations()`. Removing a station stops any craft at it.

### Crafting
- **`craft(station_id, recipe, count)`** starts crafting `count` units, one after another. It's refused, with nothing changed, if:
  - the caller isn't in the world ("not in the world");
  - the caller is aboard a ship ("can't craft from a ship") or swimming ("can't craft while swimming");
  - the caller is already crafting ("already crafting") or gathering ("already gathering");
  - there's no such recipe ("no such recipe");
  - `count` isn't from 1 to `max_count` ("craft 1 to 100 at a time");
  - there's no such station ("no such station");
  - the station is the wrong kind ("this needs a Bloomery");
  - the station is out of reach ("out of reach");
  - the account doesn't have one unit's labor ("not enough labor: have H, need C");
  - the bag lacks one unit's inputs ("not enough Saltreed: have 2, need 4");
  - one unit's output won't fit once its inputs are taken ("bag is full: room for R of C").
- Only the first unit is checked at the start. The window shows how many the bag and labor cover, and a batch stops early when it runs out (below).
- Starting inserts a private **`craft_job`**, a one-shot timer due in `craft_s` real seconds. It records the character, the station, the recipe, where the character stood, `made` and `count`, and `done_at` in game time for the progress bar.
- **`my_craft`** is a view that shows the caller their own jobs, at most one per character.
- **`craft_done`** (server schedule only) finishes one unit. The character must still be in the world, within 0.5 m of where it started, and the station must still be there. Then, in one transaction:
  - the inputs are taken;
  - the output is granted;
  - the labor is spent.

  It takes from the smallest stacks first, ties going to the lowest slot. Taking and granting are one all-or-nothing exchange (`items::exchange`), so a full bag fails cleanly even when the inputs would have freed a slot.
- After a unit, if more are wanted and the next unit's labor, inputs and room are all there, the job is rescheduled `craft_s` later. Otherwise it ends. The batch stops as soon as it can't go on, so the bar never runs for a unit that would fail.
- If a unit can't be made when it falls due, nothing is spent or granted and the job ends. Units already made are kept. The reason goes to the server log, since a scheduled reducer can't reply.
- **Moving cancels**, by the same 0.5 m rule as gathering. So do `cancel_craft()` and leaving the world. A cancelled unit costs nothing.
- Materials and labor are spent when each unit finishes, not when it starts, as in gathering (ADR 0023). The original's timing isn't known.
- Station removal stops crafts there.

### Client
- The Static tier subscribes to `station` as well as `resource_node`.
- `StationRegistry` draws placeholder stations, each with a floating name:
  - the Joiner's Bench is a plank table on legs;
  - the Bloomery is a squat stone stack with a glowing mouth;
  - the Cookhearth is a ring of stones round an ember.
- **F at a station** opens the craft window for it, when no node is in reach to gather. The HUD prompts `F: use Bloomery`. Walking out of reach closes the window.
- The **craft window** lists the station's recipes. Each row shows:
  - the output and how many one unit makes;
  - the inputs as have/need, red when short;
  - the labor and time per unit.

  Selecting a recipe shows how many units the bag and labor cover. The count can be set with − and + (up to that number), or with Max. Craft sends `craft`, and Cancel sends `cancel_craft`. Nothing is predicted.
- While `my_craft` has a row, a progress bar fills over the current unit, `Crafting Saltreed Twine 2 / 5`. Moving sends `cancel_craft` at once, as gathering does. When the job ends, the log says what was made, judged from the bag.
- The rules the window uses (`CraftModel`: have/need, how many units are affordable, whether Craft is allowed) have no Godot dependency and are covered by xUnit. Reach, progress and affordability are in `sim::craft`, ported to C# and checked against vectors.
- `--craft-demo` (dev builds) teleports beside the Cookhearth, grants Driftpine Logs and Rustvein Ore, opens the window, and crafts Charcoal. It then moves to the Bloomery and crafts a Rustiron Bar from it. It logs the bag and labor before and after, for captures.

## 3. Deviations and not yet done
- **No proficiencies.** No ranks, no rank requirements, and no cheaper or faster crafts; those come in M2. Every recipe is open to everyone.
- **Three stations, not the original's dozen or more.** More come with the systems that need them, such as the specialty bench in the trade-pack task (M1).
- **No recipe book away from a station.** The window lists only the station in front of you.
- **No quality, crafter's name or crits.** Gear grades come in M2.
- **No tools needed.** The axe, pick and knife are made but not yet used (gathering doesn't require tools).
- No crafting animation or sound; the bar is the only sign.
- Station placements are a placeholder until the coast is laid out (M1, the coast task). There are no player-placed stations yet; housing will bring them.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `crafting.toml` parses. Unknown stations and items, empty or oversized inputs, a repeated input, the output among the inputs, counts, labor and times out of range, an output count above the stack, and duplicate ids, names and wires are reported together. Each value changes the hash. Both outputs carry the stations and recipes with item and station wires | `server/contentgen/src/lib.rs` |
| sim | Reach: inside, at the edge, too far sideways or up. Progress clamps to 0–1. Affordability is limited by each input, by labor, and by `max_count`. Every recipe's station and items exist | `server/sim/src/craft.rs` |
| sim → C# | `craft_reach`, `craft_progress` and `craft_affordable` vectors | `client/tests/SimVectorTests.cs` |
| module | A batch of 3 takes inputs and spends labor per unit, grants each output, and reschedules between units. A batch stops early when the inputs run out. Refusals change nothing: not in the world, ship, swimming, already crafting, already gathering, unknown recipe, bad count, no station, wrong station, out of reach, no labor, missing inputs and a full bag. A unit whose inputs free the slot its output needs succeeds. Moving cancels and a shuffle doesn't. `cancel_craft` and leaving cancel. A unit that falls due after its materials were destroyed changes nothing and ends the job. `craft_done` can't be called by a client. The view shows only your own job. Admin placement, removal (which stops crafts) and clearing; non-admins are refused. Gathering refuses while crafting | `server/world/src/tests/crafting.rs` |
| module | `items::exchange` takes from the smallest stacks first, is all or nothing, and counts slots the inputs free | `server/world/src/tests/items.rs` |
| worldtool | `stations.tsv` round-trips. Bad lines are refused. The committed stations load, stand on the committed ground in the town, and cover every kind | `tools/worldtool/src/stations.rs` |
| C# | `CraftModel`: have/need per input, affordable units by materials and labor, the count clamped to what's affordable, and Craft disabled at 0 | `client/tests/CraftTests.cs` |
| scenario | `loadbot craft`: a bot by stations the scenario places. Wrong station, out of reach, no materials and no labor are refused. A batch of 3 Charcoal takes 6 logs and 15 labor and gives 9 Charcoal. A batch of 5 with inputs for 2 stops after 2. A move cancels at no cost. `cancel_craft` cancels. Then it cleans up | `cargo xtask bot craft` |
| client | `--craft-demo` against a live server, with captures of the window, the bar and the stations | manual, with a capture |
