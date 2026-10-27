# Status

**Milestone:** M1, "One Coast" (M0 closed 2026-10-26). **Updated:** 2026-10-26 (M1 task 3: gathering).

## Where things stand
- **Server, working end to end on a local SpacetimeDB 2.13.0:**
  - accounts and characters
  - entering and leaving the world
  - movement events checked against speed limits, with corrections and a suspicion log. The authoritative position is in private `presence`, and the tick publishes moves to `transform` once per 100 ms (ADR 0011).
  - 64 m cell keys and Near-tier subscriptions, one `cell = key` query per cell (the throughput spike showed range queries cost 7× more)
  - a subscription director (loadbot for now): one subscription per cell, new cells before old are dropped, and the window moves only 8 m past its centre cell (docs/design/interest.md)
  - the dev test clock
  - the 10 Hz `world_tick` with a measured, capped `dt` (it steps nothing yet)
  - server timings read from the host's Prometheus endpoint by `cargo xtask bot metrics` (ADR 0010). An idle tick costs about 90 µs of CPU.
  - game data from `content/` through `contentgen` (docs/design/content.md, ADR 0012): the movement-mode table, generated into Rust (`sim::content`) and C# (`client/content`), with a `CONTENT_HASH` that `enter_world` checks
  - server-side terrain (docs/design/terrain.md, ADR 0013): a 2 m `u16` heightmap in private `terrain_chunk` (one 33 × 33 row per cell), uploaded by admins from `.mmoh` files; vertical checks on every move (below ground, a jump-plus-51° climb ceiling from the last footing, a minimum fall after a 1.5 s grace, swimming at sea level), always taking the kinder of the two triangulations; corrected moves settle on the ground; `line_of_sight` used by `dev_swing`
  - ships and moving decks (docs/design/ships.md, ADR 0014): clipper hulls from `content/ships.toml`, stepped by `world_tick` with the shared `sim::ship::step` (throttle and rudder, draft checked against terrain), published to `transform` at 5 Hz with the helm, HP and reloads in public `vehicle`; passengers move in the ship's frame (`parent` on `presence`, `transform` and `move_event`) and change cells with it; `take_helm`, `helm`, `leave_helm`; `fire` schedules a `cannon_shot` row that lands after its flight time and damages ships it hits; dev `dev_spawn_ship` and `dev_remove_ship`
  - dev-only `dev_teleport`, `dev_set_gm`, and `dev_swing` with a `combat_event` event table, as a stand-in for combat in load tests
- **Labor** (M1 task 1, docs/design/labor.md, ADR 0021): one pool per account in private `labor`, read through `my_labor`. It regenerates 10 points every 5 minutes, online and offline, up to 5,000, computed from a timestamp by `sim::labor` and never ticked. New accounts start full. The rules are in `content/labor.toml`. `labor::spend` is ready for gathering; dev builds add `dev_spend_labor` and `dev_set_labor`. The test clock's offset is now the public `game_clock` row, so clients compute game time. The client's port (checked against vectors) drives a HUD line, `labor 1,234 / 5,000 (+10 in 4:57)`; `--labor N` and F8 are dev helpers. The `labor` bot scenario passes, and a capture shows the HUD tracking a dev-clock jump: `docs/captures/2026-10-26-labor-{before,after}.png`.
- **Items and the bag** (M1 task 2, docs/design/items.md, ADR 0022):
  - Nine items in `content/items.toml` (plants, ore, logs, a tart and a knife, named in `docs/world/`), with permanent wire numbers, kinds and stack sizes. The bag's size is content too: 50 slots, the original's starting bag.
  - One private `item` row per stack (`holder`, `place`, `slot`, `def`, `count`), read through `my_inventory`.
  - `move_item` moves a stack, merges as much as fits into the same item, or swaps. `split_item` splits into an empty slot or onto the same item. `destroy_item` destroys some or all of a stack.
  - `items::grant` is all or nothing and is ready for gathering. Dev builds add `dev_grant_item(id, count)`.
  - The client has a bag window (I or B): click to pick up and click to place, shift-click to split half, ctrl + right-click to destroy after a confirmation. Its click rules are tested in xUnit.
  - `--bag-demo` works through every operation against a live server. Captures: `docs/captures/2026-10-26-bag-{split,destroy}.png`.
  - The `items` bot scenario passes.
- **Gathering** (M1 task 3, docs/design/gathering.md, ADR 0023):
  - Seven node kinds in `content/gathering.toml` (three plants, two ores, two trees, named in `docs/world/`). Each has its item, yield range, labor, gather time and regrow time, and there's a 3 m reach. Ore costs 10 labor, as the original's iron vein did (B); the other numbers are ours.
  - A public `resource_node` row per node, in a new **Static tier** (9 × 9 cells). Regrowth is `ready_at`, a timestamp, and nothing ticks.
  - `gather(node)` refuses with nothing changed when the character isn't in the world, is aboard a ship or swimming, or is already gathering; when the node is missing, out of reach, regrowing or taken; or when labor or bag room runs short. Otherwise it starts a private scheduled `gather_job` with the yield rolled.
  - When the job is due it spends labor, grants the items and sets `ready_at` in one transaction, or does none of that if anything changed. Moving more than 0.5 m, `cancel_gather` and leaving the world cancel for free.
  - Placements are in `world/nodes.tsv`: 11,920 nodes over wild land from `worldtool`, by height, slope and climate. `cargo xtask world` uploads them, and admins have `admin_put_nodes`, `admin_remove_nodes` and `admin_clear_nodes`.
  - The client draws placeholder tufts, rocks and trees, with stumps while they regrow. F on land gathers the nearest ready node in reach (otherwise F still climbs aboard). The HUD prompts `F: gather …` or `… regrows in m:ss`, and a progress bar shows the gather. Moving cancels it.
  - `--gather-demo` teleports beside the nearest plant and then the nearest tree and gathers both. Against the live server it got 4 Saltreed for 5 labor and 1 Driftpine Log for 10, with zero corrections. Captures: `docs/captures/2026-10-26-gather-{reed,tree,stump}.png`.
  - The `gather` bot scenario passes on an empty database and on the uploaded world.
- **Godot client skeleton, working against the local server** (docs/design/client.md, ADR 0015):
  - The client connects with Gzip, keeps a token per `--profile`, makes a character if needed and enters with the content hash.
  - The player is a predicted `CharacterBody3D`. It sends movement events (start, stop, turns, jump, landing, 0.5 s heartbeats) and snaps to `my_presence` on each correction.
  - The Near tier runs through a C# port of the subscription director.
  - Other entities are capsules with nameplates (ships are hull-sized boxes), drawn 200 ms back: interpolated, then dead-reckoned along their heading.
  - There is a debug HUD, `--autopilot` routes, `cargo xtask client`, and `cargo xtask capture` for Movie Maker frames.
  - **Reconnect** (ADR 0020): the SDK's automatic reconnect, with the cache and every subscription replayed, and the client enters again after a drop. The server despawns a character only when the connection that entered it closes (`presence.connection_id`), so a late disconnect can't remove a character that has come back. Tested by killing and restarting the server under two clients: both re-entered where they stood and saw each other, with zero corrections.
  - Two windows and `bot visitor` all saw each other with zero corrections. Remote runners were drawn with no reversals. A test cheat was corrected and snapped back.
  - **The client walks on the server's terrain** (docs/design/worldstream.md). It loads the world's files (`--world`, default `world/`) and draws them with the spike's tiles and sea. The player stands on `HeightMapShape3D`s for the 3 × 3 cells around it, holding the server's own samples, and the camera stays above the ground. The flat placeholder ground is gone.
  - Spawn (0, 0) is now on the placeholder world's trade island, about 13 m up. `cargo xtask world` uploads the world, and `bot visitor` runs on its ground.
  - **Ships and decks** (docs/design/ships.md, ADR 0019). Ships are drawn at the present with a C# port of `sim::ship` (checked against vectors) and the helm's input from `vehicle`. The hull is solid. The player swims in deep water, climbs aboard with F, and walks the deck in the ship's frame, sending `parent` and ship-local moves. It takes no momentum when it steps off. F works the helm (W/S throttle, A/D rudder) and cannons (aim, elevation, a landing preview, Space to fire). Dev keys: F9 spawns a clipper, F10 removes one, and `--spawn-ship` does it from the command line. New autopilot routes: `deck`, `helm`, `gun`.
  - Three autopilot clients crewed a clipper for 6 minutes: it turned 3,689°, the deck walker sent 476 moves and then went over the side, the gunner fired 72 shots, all with **zero corrections**. Captures: `docs/captures/2026-10-26-deck-*.png`.
- **The world's files are in the repo** (docs/design/world.md, ADR 0018):
  - `world/` holds a manifest and 625 region files of 640 m (9.4 MB, against 128 MB raw). Each region has zlib-compressed heights, stored as differences from a planar prediction, and a 4 m land-use raster (use class plus climate). `tools/worldtool` writes them (`gen`, from the placeholder generator that moved out of loadbot) and checks them (`info`).
  - Land-use classes (`wild`, `housing`, `town`, `road`) and climates (`temperate`, `tropical`, `arid`, `subarctic`) are content, in `content/land.toml`. The placeholder world has six housing areas on the continents' lowlands, a town round spawn and four climate bands. There are no roads yet.
  - **Terrain hash:** a wrapping sum of per-chunk FNV-1a/splitmix64 hashes. The server keeps it in the public `terrain_info` row as chunks are uploaded, and `enter_world(char_id, content_hash, terrain_hash)` refuses a mismatch: "client terrain … does not match the server's …; run `cargo xtask world`". Tested live with a client on test terrain. Bots enter with the server's hash; `bot visitor` checks its world against it first.
  - `cargo xtask world` uploads as the **publisher**, with the CLI's token, so it works on `--prod` builds (tested). `bot world-upload` checks the server's hash afterwards.
  - The client loads `world/` in 0.2 s, tints housing land and towns, and shows the land use and climate in the HUD. Captures: `docs/captures/2026-10-26-world-{housing,town-hud}.png`.
  - Two autopilot clients walked hills, coast and shallow water for 10 minutes each with `bot visitor` for company: 1,186 and 1,185 moves, zero corrections, and the body never more than 0.06 m off the heightmap. Captures: `docs/captures/2026-10-26-terrain-*.png`.
- **Art: deferred.** The user chose simple placeholder geometry until the game systems are built (ADR 0004, 2026-10-26). The style test scene stays as a reference (docs/design/art.md, ADR 0016):
  - `cargo xtask client --style-test flat` (or `gradient`) runs offline. It shows a procedural island, Gerstner water with turquoise shallows and a foam line, procedural low-poly trees and rocks, and three KayKit Adventurers characters (CC0) walking, running and idling.
  - `--outline` outlines the characters, and `--shot wide` circles the island instead of following the knight.
  - Captures: `docs/captures/2026-10-26-style-*.png`.
  - Assets live in `client/assets/`, listed in `assets/LEDGER.md`. The xtask now runs Godot's import before every client run (about 2 s).
- **World-scale test, offline** (docs/design/worldstream.md, ADR 0017):
  - The client draws terrain itself from the server's `.mmoh` heightmap: 256 m tiles at six levels of detail, built on worker threads, deep-water tiles skipped. Terrain3D was dropped (native code, and streaming that slows as more is loaded).
  - The placeholder 16 km map follows PLAN §9's layout (now `world/`, above). `cargo xtask client --world-test world --route sail|walk|cross|high` flies a camera over it and logs frame times, memory and streaming.
  - The sea follows the camera. Captures: `docs/captures/2026-10-26-world-*.png`.
- **Spikes 1–5 done** (docs/spikes/throughput.md, mass-fight.md, world-scale.md):
  - 500 players spread out: about 1 core and 0.6 GB on this laptop, 6 KB/s per client.
  - With batched publishing and Gzip, a 150-player fight costs each client about 19 KB/s (reducer p99 3 ms), and 500 all in view about 44 KB/s.
  - The levers for siege-sized crowds are written down for M3.
  - Ships: four bots walked, jumped and swung at each other on a clipper turning at full speed with zero corrections, and the gunner hit the second ship with every shot after its 1.27 s flight time. Extrapolating the 5 Hz ship publishes with the shared step is 0.2 m off one second ahead (mean), against 1.2 m for a straight line. A 10-minute run: 3,782 moves on deck and 280 jumps with no false corrections, 7,103° of turning, 595 swings landed and 106 of 106 shots hit.
  - World scale: on the RTX 5070 Ti at 1080p the whole scene takes at most 2.0 ms of GPU time at p99 on every route, with no frame over 33 ms. The first view streams in 0.5–0.7 s, and a 15 km teleport re-streams in under 1 s. The world adds 122 MB to the client, and the whole map's terrain adds 172 MB to the server. On the Radeon 610M integrated GPU, far below mid-range, it holds 36–60 fps at 1080p with no frame over 33 ms, which puts mid-range inside the 4 ms budget (docs/spikes/world-scale.md).
  - Terrain: 16 bots walked procedural hills and cliff-sided mesas for 30 minutes with no false corrections, and a cheater bot's cliff teleport, fly hack and underground dive were each corrected. Walkers now count a slide once per fall and respawn when stuck sliding in a steep dip. A rerun on 2026-10-26 walked 151 km with 17 slides (against 10,020 counted frame by frame in the worst earlier run), no respawns needed and no false corrections.
- **Tests:**
  - 80 `sim` unit tests (one checks the client's golden vectors are current), 26 `contentgen` tests, 88 module tests (in-memory datastore), 17 `worldtool` tests (including one that loads the committed world against its manifest, and one that checks the committed nodes stand on it and are current) and 5 loadbot tests (two drop a walker into a steep pit and down a steep bank), with clippy clean
  - 86 client xUnit tests: the bag window's click rules, and the C# ports match `sim` on 1,134 generated vectors (labor and gathering included) (including a `.mmoh` file and its chunk and terrain hashes, and ship frames, steps, courses and cannon flights), plus the director, the move sender (frame changes, mode changes inside the swim limit), the snapshot buffer, the ship predictor, boarding, swimming, the autopilot and its ship routes, the heightmap reader (with per-cell collision heights), the terrain tile plan, and the world's files (the committed world loads with its manifest's hash; damaged files are refused). The Godot project compiles with warnings as errors.
  - the `see-each-other`, `tick`, `cross-cells`, `terrain`, `ship`, `labor`, `items` and `gather` bot scenarios, which pass against a live server, plus `swarm` for load and `visitor` as company for client tests
- **Dev tasks:** `cargo xtask` (see CLAUDE.md, Commands).
- **Client toolchain:** Godot 4.7.2 .NET with `SpacetimeDB.ClientSDK.Godot` 2.13.0 on Linux. The SDK declares GodotSharp 4.6.2, and NuGet resolves it to 4.7.2 without complaint. Movie Maker captures work under Wayland on the laptop's GPU.

## Machines
| | Linux laptop (Arch) | Windows PC |
|---|---|---|
| Hardware | 32 threads, 14 GB RAM, RTX 5070 Ti Mobile + Radeon 610M | Ryzen 9 7900X, 32 GB, RX 6950 XT |
| Rust | 1.99.1 + wasm32 target | 1.90, wasm32 target missing |
| SpacetimeDB CLI | 2.13.0 (`~/.local/bin`, not on PATH) | 1.10, needs upgrading |
| .NET | SDK 8.0.132 and 10.0.113 (pacman, `/usr/share/dotnet`) | 7 and 10 |
| Godot | 4.7.2 .NET (`~/.local/opt`, linked as `~/.local/bin/godot`). Mesa RADV (`vulkan-radeon`) for the Radeon 610M: run with `--display-driver wayland --gpu-index 1` | none |
| Blender | none | none |

## Next
1. **M1 task 4, crafting** (BACKLOG): two workstations and about 20 recipes that turn gathered materials into goods for labor.
2. Verify on the Windows PC when you're at it (BACKLOG, "Any time").
3. **After pulling:** the schema changed (`resource_node`, `gather_job`), so run `cargo xtask publish --clear` and then `cargo xtask world` once.
4. **Try the client:** run `cargo xtask server`, `cargo xtask publish` and `cargo xtask world`, then `cargo xtask client --profile <name>` in one or two terminals. `cargo xtask bot visitor --secs 120` adds a bot for company. Press I to open the bag; `--bag-demo` fills it and plays through every operation. To gather, leave the town round spawn: Saltreed grows on the beaches about 250 m out, and herbs and driftpines stand on the hills about 300 m east. Stand by one and press F. `--gather-demo` does it for you. To sail, walk west to the beach (about 350 m), wade out until the water is deep, face the sea and press F9, then swim to the ship and press F alongside it.

## Research
Original movement and ship numbers were searched on 2026-10-26 (RESEARCH §2A.4). Our run speed and clipper speed match. Known differences, left for M1 tuning: our clipper has 12,000 HP and two cannons against the original's 7,000–9,000 and one, and our swim speed (3.5 m/s) is well under the early original's (about 6.8 m/s, later nerfed). Jump height, fall damage and ship handling remain unknown.

## Blockers and open questions
- **PLAN Q3:** anything from the final version to leave out or bring forward? Still open.
- **Crowd measurements need a second machine for the bots.** Spread-out runs fit on the laptop (500 bots used 0.5 cores and 0.4 GB). Crowds are heavier for the bots: 500 in one place took 3 cores even after spike 2. The Windows PC could host the bots once it has the toolchain (the Windows task in BACKLOG). This matters for M3's large battles.
