# The coast: Hearthshore, its outpost and where characters start

Status: v0.1, 2026-10-31 (M1 "The coast", first half: the layout). Code: `tools/worldtool/src/{coast,areas,map}.rs` (and `placeholder.rs`, `nodes.rs`, `spawns.rs`, `stations.rs`, `posts.rs`, `shrines.rs`, which take the layout from `coast.rs`), `server/world/src/combat.rs` (`start_point`), `server/world/src/session.rs` (`create_character`), `client/src/World/Areas.cs`, `client/src/UI/Hud.cs`. Data: `world/`, `content/items.toml` (`[bag] start`), `content/combat.toml` (`start = true`). Related: docs/design/world.md, docs/design/quests.md (the second half: quests and their givers), PLAN §11 (M1), ADR 0036.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Each race began in its own starting zone on its home continent, with towns, roads between them, and quest hubs that led new characters outward | B | RESEARCH §2, §2.1; common player knowledge |
| Zones were split into named sub-areas, shown to the player as they entered them | B | common player knowledge |
| Monsters grew stronger with distance from the starting town | B | common player knowledge |
| Gathering spots, farms and housing land were laid out in fixed places within zones | B | RESEARCH §2.4 |
| Early quests taught farming and gave the seeds; seed merchants sold more near farms | C | RESEARCH §2.4, docs/design/farming.md §1 |
| Exactly what a new character carried, and which seeds early quests gave | **Unknown** | not found |

Only the shape matters for M1: a zone that reads as places, an easy start beside the beach, harder ground farther out, and trade routes that go inland on foot and out across the water.

## 2. Our rules

### The zone
- **Hearthshore** is about 3 × 3 km of the west continent's east coast (x −5,900 to −2,900, z −1,500 to 1,800). **Gullhaven Isle**, the trade island round the old spawn, is its outpost across about 3 km of sea. All names are in `docs/world/README.md`.
- The layout is code, `tools/worldtool/src/coast.rs`: landforms, towns, roads, stations, posts, Wayshrines, NPC camps, node fields and named areas. `cargo xtask world --gen` regenerates `world/` from it; the result is committed. Nothing outside the zone and the island changes from the placeholder (deviation below).

### The ground
`coast::shape` changes the placeholder's heights inside the zone, in order. Every outline is pulled off a true circle or line by noise (`WANDER`, 60% of the distance).

| Landform | Kind | What it is |
|---|---|---|
| Bristle Downs | two hills, 30 m and 18 m | rolling hills north-west of Furrowdale |
| Greymane Wood | a 12 m rise | the wood along the Cairnwick road |
| Cairn Crags | an 85 m craggy hill | above Cairnwick in the north-west |
| Ashwing Scarp | a 1.5 km ridge, 85 m | the zone's west side |
| Wrackline Point | a 20 m headland | the north shore |
| Croaker Fen | levelled to 0.6 m ± 1.4 m | wet flats with pools at sea level |
| Hexer's Tarn | levelled to −3 m | a small lake, at sea level like every water body (world.md §3) |
| Wreckers' Cove | levelled to −7 m | a bay cut into the south shore |
| Town pads | levelled flat | Furrowdale (5 m, by the beach), Cairnwick, Redfen |

### Towns, roads and services
| Town | Where | Has |
|---|---|---|
| Furrowdale | the shore, (−3330, 380) | **the start** (Furrowdale Wayshrine), the Joiner's Bench, Bloomery, Cookhearth and Packwright's Bench |
| Cairnwick | under the crags, (−4600, −1050), 1.9 km from the bench | Cairnwick Post, a Wayshrine |
| Redfen | the fen's west edge, (−3930, 1250), 1.0 km | Redfen Post, a Wayshrine |
| Gullhaven | Gullhaven Isle, (0, 0), 3.3 km across the sea | Gullhaven Post, a Wayshrine, three stations |

Each town has a quest giver a few steps from its Wayshrine (`coast::GIVERS`, `world/givers.tsv`): Odda Fernwright in Furrowdale, Brannoch Hale in Cairnwick, Yeva Mossgrove in Redfen and Quill Ambry in Gullhaven (docs/design/quests.md).

- **Roads** join Furrowdale to Cairnwick (over the downs, through the wood) and to Redfen (round the fen). Land within 4 m of a road is the `road` use (no building, docs/design/land.md). Roads don't change the ground.
- **Housing:** Furrowdale Commons, the placeholder's housing area between Furrowdale and the scarp (500 m round (−4300, 900)).

### Where characters start
- `combat.toml` marks one Wayshrine `start = true` (Furrowdale's); contentgen requires exactly one and generates `START_SHRINE`.
- `create_character` puts the character `SHRINE_STEP_M` in front of that Wayshrine if the world has it, or at the old spawn (0, 0) on a world without it (tests, bare servers). The dead with no Wayshrine at all get up there too.
- Existing characters stay where they were saved.

### The starting bag
- `[bag] start` in `content/items.toml` lists what every new character's bag holds: **10 Hearthgrain Seed, 6 Whiteflax Seed and an Amberpear Sapling** (ours), enough for a Grain Bale's 20 Hearthgrain at 3–5 a plant. contentgen checks each item exists, fits one stack, is listed once, and that they fit the bag.
- `create_character` grants them, one stack each from slot 0, in the same transaction.

### NPC camps
Levels rise with distance from Furrowdale; `worldtool spawns` places each camp's points on dry, gentle wild land round its centre.

| Level | Kind | Camp | From Furrowdale |
|---|---|---|---|
| 1 | Shore Skitter | the beaches either side of town; Gullhaven's east beach | 250 m |
| 2 | Bristleback Boar | Bristle Downs | 700 m |
| 3 | Fen Croaker | Croaker Fen | 680 m |
| 4 | Greymane Wolf | Greymane Wood | 1.2 km |
| 6 | Wreck Brigand | Wrackline Point | 1.3 km |
| 7 | Brine Hexer | round Hexer's Tarn | 1.8 km |
| 9 | Cliff Stalker | the Cairn Crags | 2.4 km |
| 10 | Mossback Tortoise | Mossback Meadow | 1.7 km |
| 11 | Ashwing Drone | Ashwing Scarp | 2.2 km |
| 15 | Wreck Captain, with four Brigands | Wreckers' Cove | 1.3 km, a danger near the Redfen road on purpose |

### Resource nodes
- Inside the zone, nodes stand only in hand-laid **fields** (`coast::NODE_FIELDS`): Saltreed on Saltreed Strand and in the fen, Emberleaf and Duskberry round Furrowdale and on the downs, Driftpines on the coast, Hollowoaks in Greymane Wood and on the meadow, Tinstone at the crags' foot and on the downs, Rustvein high on the crags and the scarp. 198 nodes in the zone, and 14 on Gullhaven's hills.
- Outside the zone the placeholder still scatters them (docs/design/gathering.md).

### Named areas
- `world/areas.tsv` (`name x z radius`, written by `worldtool areas` and `gen`) lists 17 circles: the zone, the isle, the four towns and the wild areas above. The client loads it with the world. The HUD shows the smallest area the player is in, then the largest: "area Croaker Fen, Hearthshore". The server doesn't use them.

### Tools
- `worldtool map --area X0,Z0,X1,Z1 [--px N] [--out F]` draws part of the world as a PNG: relief shaded from the north-west, sea by depth, towns, housing and roads tinted, nodes as dots, spawn points red, stations orange, posts yellow, Wayshrines white, grid lines every 100 m. `worldtool probe X,Z...` prints the ground at points.
- Client: `--teleport X,Z` (dev builds) goes there a second after entering, and `--cam-pitch DEG` with `--cam-distance` up to 400 m frames a landmark from above (docs/design/client.md).

## 3. Deviations and not yet done
- **The layout is code, not an editor's files.** Changing the zone means editing `coast.rs` and regenerating; M2 builds the world in Terrain3D (ADR 0032), and the layout moves into its regions then.
- **No buildings, docks or props.** Towns are flat tinted pads with their stations and Wayshrine. Ships are summoned in deep water off the beach as before.
- **The rest of the map stays placeholder:** the other continent, the north and the other islands keep scattered nodes and no camps.
- **Seed merchants** don't exist yet; the starting bag stands in for them (docs/design/farming.md §3).
- **Housing stays the placeholder's circle.**

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `[bag] start` reaches `BAG_START`; unknown items, counts over a stack or 0, duplicates and more stacks than slots are refused; exactly one shrine has `start`, which reaches `START_SHRINE` | `server/contentgen` |
| module | A new character's bag holds `BAG_START`; new characters start in front of the start Wayshrine, or at (0, 0) without one | `server/world/src/tests/{items,combat}.rs` |
| worldtool | Landforms leave the island and the rest of the map alone and shape the zone; on the committed world the towns are dry, flat town land, every road is road, and the start Wayshrine is in Furrowdale; in the zone every node belongs to a field of its kind and every field is full; every NPC kind lives in the zone, level 1 near town and level 4 up over 1 km out; Redfen and Cairnwick are a walk from the bench and Gullhaven a sail; areas round-trip, the smallest wins, the committed file is current and every name is in the world bible; the map's PNG and CRC | `tools/worldtool` |
| xUnit | Areas parse and describe a place; the committed world names Furrowdale and Gullhaven | `client/tests/WorldFilesTests.cs` |
| live | Every bot checks the starting bag when it arrives, then empties it; `cargo xtask check` passes; with the world uploaded, `land`, `farm`, `vehicle`, `shipyard`, `npc` and `trade` pass and `bot visitor` circles beside the start | manual |
| capture | A new character at the Furrowdale Wayshrine with the area line; Furrowdale, the fen, Cairnwick under the crags, the tarn, the cove and the scarp from above; the zone's map | `docs/captures/2026-10-31-coast-*.png` |
