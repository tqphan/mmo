# Gathering

Status: v0.1, 2026-10-26. M1 task 3. Code: `content/gathering.toml`, `server/sim/src/gather.rs`, `server/world/src/gathering.rs`; `tools/worldtool/src/nodes.rs`, `world/nodes.tsv`; client `client/src/World/NodeRegistry.cs`, `Gameplay/Gathering.cs`, `UI/GatherBar.cs`; `sidecars/loadbot/src/gather.rs`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Gathering wild plants, mining veins and logging trees all cost labor, and fed the gathering, mining and logging proficiencies | A | RESEARCH §2.3 |
| An iron vein cost 10 labor to mine, with no proficiency needed; rarer veins cost more (Fortuna 20, Star Shard 25, Anya 50 with a proficiency requirement, Thiorium 100) | B | Gamepedia mining page (search extract, 2026-10-26; page undated) |
| An iron vein gave iron ore and raw stone, with a chance of a rarer ore | B | same |
| Small gathering such as picking flowers cost 1–2 labor | C | [Fandom wiki: Crafting, old revision](https://archeage.fandom.com/wiki/Crafting?oldid=5688) (search extract) |
| Gathering was a short cast at the node, interrupted by moving | C | common player knowledge; not found in a source |
| Wild nodes respawned some time after they were taken | C | common player knowledge; times not found |
| Cast times, yields and respawn times | **Unknown** | not found (the Gathering wiki page returned HTTP 402) |

## 2. Our rules

### Node kinds (content)
- `content/gathering.toml` defines node kinds: `wire` (stored in node rows; dense, from 0, like hulls), `id`, `name`, `kind` (`plant`, `ore` or `tree`), the `item` it yields (an item id from `items.toml`), `yield_min`–`yield_max`, `labor`, `gather_s` and `regrow_s`.
- `[gathering] reach_m` (3 m) is how close a character must stand: horizontally within `reach_m` of the node, and within `reach_m` vertically.
- Contentgen checks ids, names, wires, kinds, that the item exists, `1 ≤ yield_min ≤ yield_max ≤` the item's stack, `labor` from 1 to 1,000, `gather_s` from 0.5 to 60 and `regrow_s` from 1 to 7 days. The kinds are part of the content hash.
- Seven kinds, one per gatherable material. Their names are in `docs/world/`.

| Node | Kind | Yields | Labor | Gather | Regrows |
|---|---|---|---|---|---|
| Saltreed Clump | plant | Saltreed 2–4 | 5 | 2 s | 5 min |
| Emberleaf Patch | plant | Emberleaf 2–4 | 5 | 2 s | 5 min |
| Duskberry Bush | plant | Duskberry 2–4 | 5 | 2 s | 5 min |
| Tinstone Vein | ore | Tinstone 2–3 | 10 | 4 s | 15 min |
| Rustvein Outcrop | ore | Rustvein Ore 2–3 | 10 | 4 s | 15 min |
| Driftpine | tree | Driftpine Log 1–3 | 10 | 5 s | 20 min |
| Hollowoak | tree | Hollowoak Log 1–3 | 10 | 5 s | 20 min |

Ore's 10 labor is the original's iron vein (B). The rest is ours, for the M1 playtest to tune.

### Nodes in the world
- The public **`resource_node`** table has one row per node: `id`, `cell` (indexed), `kind` (the wire), `x`, `y`, `z`, `yaw`, `ready_at`, `gatherer` and `done_at`.
- **Regrowth is a timestamp** (CLAUDE.md): a node can be gathered when `now ≥ ready_at`. Taking it sets `ready_at = now + regrow_s`. Nothing ticks.
- `gatherer` is the entity id of whoever is gathering it (0 if nobody), and `done_at` is when that gather finishes, in game time. Both are public. Others can see that someone is working the node, and the gatherer's client draws a progress bar from them.
- Placements are part of the world's files: **`world/nodes.tsv`**, one node per line (`kind-id x y z yaw-degrees`), with kinds by content id so wire changes don't break the file. `worldtool gen` writes it from the generated land, and `worldtool nodes` rewrites only this file. `cargo xtask world` uploads the placements after the terrain, replacing every node.
- Placements aren't in the terrain hash. The client takes nodes from the server, never from the file.
- Admins place nodes with `admin_put_nodes(Vec<NodePlacement>)`. They remove them with `admin_remove_nodes(ids)` and `admin_clear_nodes()`.
- **Placeholder placement:** a jittered 64 m grid over wild land, with one candidate per cell and about half of them kept. The kind follows the ground. Plants grow on low, gentle ground, trees on gentle ground higher up, and ore on steep or high ground. Climate picks between the two kinds of each category. Nothing is placed in towns or housing areas, in water, or on ground steeper than a character can stand on. The coast zone gets hand-placed nodes in M1 task 11.

### Gathering
- **`gather(node_id)`** starts a gather. It's refused, with nothing changed, if:
  - the caller isn't in the world ("not in the world");
  - the caller is aboard a ship ("can't gather from a ship") or swimming ("can't gather while swimming");
  - there's no such node ("no such node");
  - it's out of reach ("out of reach");
  - it isn't ready yet ("nothing to gather here yet");
  - someone is gathering it ("someone else is gathering this"), or the caller is gathering already ("already gathering");
  - the account doesn't have the labor now ("not enough labor: have H, need C");
  - the yield wouldn't fit in the bag ("bag is full: …").
- The yield is rolled when the gather starts, with the reducer's RNG, and kept in the private job. A gather that starts is then checked again against that same yield when it finishes.
- Starting inserts a private **`gather_job`**, a one-shot timer due in `gather_s` real seconds. It records the character, the node, where the character stood and the yield. Starting also sets the node's `gatherer` and `done_at`.
- **`gather_done`** (server schedule only) finishes it. If the character is still in the world, within 0.5 m of where it started, and the node is still there and theirs, then:
  - labor is spent;
  - the yield is granted;
  - `ready_at = now + regrow_s`.
- If anything fails at the end (labor spent elsewhere, the bag filled up, the node removed), nothing is spent or granted and the node is freed. The reason goes to the server log. A scheduled reducer can't reply to a client, so the client sees only the node freed and no change in its bag.
- **Moving cancels.** `move_event` cancels a gather once an accepted move takes the character more than 0.5 m from where it started. **`cancel_gather()`** cancels it outright. Leaving the world cancels it too. Cancelling frees the node and costs nothing.
- Labor is spent at the end, not the start, so a cancelled gather is free. That's our choice; the original's timing isn't known.
- Timing uses `now(ctx)` for `ready_at` and `done_at`, so the dev clock regrows nodes. The job's timer runs in real time, like cannon shots.

### Client
- A **Static tier** (PLAN §4.3): a second subscription director over `resource_node`, 9 × 9 cells (radius 4, about ±290 m), set up on entering.
- `NodeRegistry` draws each node as placeholder geometry: a green tuft for plants, a grey rock for ore, a trunk and crown for trees. Each is tinted by its kind. A node that isn't ready is drawn as a low stump or bare patch, worked out every frame from `ready_at` and game time. One someone else is gathering gets a small marker.
- **F on land gathers** the nearest ready node in reach. With none in reach, F climbs aboard as before. The HUD shows `F: gather Saltreed Clump` when one is in reach. It shows `Saltreed Clump regrows in 3:12` when the nearest is still regrowing.
- While our entity is a node's `gatherer`, a progress bar fills from `done_at − gather_s` to `done_at`. When it ends, the log says what was gathered, judged from the bag. Walking away cancels on the server; the client sends `cancel_gather` as soon as the player starts moving, so the bar stops at once.
- `--gather-demo` (dev builds) teleports next to the nearest node after entering, gathers it, and logs the bag and labor before and after, for captures.
- Reach and readiness are in `sim::gather`, ported to C# and checked against vectors.

## 3. Deviations and not yet done
- **No proficiencies.** Gathering, mining and logging ranks, and their cheaper costs and bonus yields, come in M2 with the other proficiencies.
- **One item per node**, and no rare extra drops (the original's iron vein also gave raw stone and sometimes pure ore). Bonus drops come with proficiencies.
- **Nodes regrow in place.** The original's nodes may have reappeared elsewhere; that wasn't confirmed.
- **No tools.** The Reedcutter's Knife isn't needed yet.
- No gathering animation; the progress bar is the only sign. No sound.
- Times, yields and most labor costs are ours (§1).

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `gathering.toml` parses; bad kinds, unknown items, yields out of range or above the stack, labor and times out of range, duplicate ids, names and wires are reported together; each value changes the hash; both outputs carry the nodes with their item wires | `server/contentgen/src/lib.rs` |
| sim | Reach: inside, at the edge, too far sideways, too far up; readiness at, before and after `ready_at`; the move tolerance; progress clamps to 0–1 | `server/sim/src/gather.rs` |
| sim → C# | `gather_reach` and `gather_progress` vectors | `client/tests/SimVectorTests.cs` |
| module | A gather grants the rolled yield, spends labor at the end and sets `ready_at`. Out of reach, not ready, taken by someone else, already gathering, no labor, a full bag, swimming and no such node are all refused with nothing changed. Moving past 0.5 m cancels and a heartbeat in place doesn't. `cancel_gather` and leaving the world free the node. Labor spent in between makes the finish fail cleanly. The dev clock regrows a node. `gather_done` can't be called by a client. Admin placement, removal and clearing; non-admins are refused | `server/world/src/tests/gathering.rs` |
| worldtool | Placements are on wild land above water, never on steep ground; kinds follow the rules; the file round-trips; the committed `world/nodes.tsv` loads and every kind exists | `tools/worldtool/src/nodes.rs` |
| scenario | `loadbot gather`: two bots by a node the scenario places. Out of reach and no labor are refused. A gathers while B is refused as taken. A's bag and labor change by the yield and cost, and the node is depleted and refused. The dev clock regrows it, and A gathers again. A move cancels a gather, and so does `cancel_gather`. Then it cleans up | `cargo xtask bot gather` |
| client | `--gather-demo` against a live server, with a capture of the bar and the node before and after | manual, with a capture |
