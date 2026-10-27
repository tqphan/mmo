# World building: the world's files, upload and the terrain check

Status: v0.1, 2026-10-26 (M0 "World building v0"). Code: `tools/worldtool/`, `server/sim/src/terrain.rs` (`chunk_hash`), `server/world/src/terrain.rs` (`terrain_info`), `sidecars/loadbot/src/terrain.rs` (`world-upload`), `client/src/World/{WorldFiles,LandUse}.cs`. Data: `world/`, `content/land.toml`. Related: docs/design/terrain.md (the heightmap and the server's checks), docs/design/worldstream.md (how the client draws it), ADR 0018.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| One seamless world of two continents, a central sea with islands and a northern region | A | RESEARCH §2, ADR 0002 |
| Houses and farms could only be placed inside designated areas of open-world land | A | RESEARCH §2.4 |
| Climate zones, from tropical to subarctic, sped up the crops suited to them | B | RESEARCH §2.4 |
| Towns and roads joined the regions; trade-pack routes ran along them | B | common player knowledge |
| How the world's data was stored, versioned or checked between client and server | **Unknown** | not public; it ran on CryEngine |

Only what players see matters: where building is allowed, which climate a farm is in, and that everyone stands on the same ground.

## 2. Our rules

### The world's files are the source
- The world lives in the repo under `world/`, as files our own tool writes: `tools/worldtool` (`cargo run -p worldtool`). For now its generator writes them (the placeholder map from docs/design/worldstream.md). Editor tooling will later edit the same files, and from then on the files, not the generator, are the source of truth. Regenerating would overwrite hand edits.
- `world/world.toml` is the manifest: the format version, the region size, and the **terrain hash** of the heights (below). `worldtool` writes it, and nobody edits it by hand.
- `world/nodes.tsv`: resource node placements, one per line (docs/design/gathering.md). `worldtool gen` writes it after the regions, and `worldtool nodes` rewrites it alone from the regions there. It isn't part of the terrain hash, since clients take nodes from the server.
- `world/regions/x{RX}z{RZ}.mmor`: **625 regions** of 10 × 10 cells (640 m), 25 along each side. Small regions keep an edit's diff small. The whole placeholder world is about 9 MB, against 128 MB raw.

### Region file (`.mmor`, version 1, little-endian)
| Field | Bytes |
|---|---|
| magic `MMOR` | 4 |
| version (`u16` = 1), first cell x, first cell z, cells along a side (`u16` each) | 8 |
| heights: `u32` length, then a zlib stream | 4 + n |
| land use: `u32` length, then a zlib stream | 4 + n |

- **Heights:** the region's `(cells·32+1)²` samples (2 m, the `.mmoh` encoding: `raw / 32 − 512` m), edges included, so neighbours repeat their shared edge exactly as server chunks do. Loading checks that shared edges agree. Before compression each sample is replaced by its difference from a prediction, `left + above − above-left` (only `left` on the first row, only `above` in the first column, 0 for the first sample), modulo 2¹⁶. The difference is zig-zag coded as an `i16`, and all the low bytes are stored before all the high bytes. Smooth ground predicts well, so most differences are tiny.
- **Land use:** one byte per **4 m texel** (2 × 2 quads), `(cells·16)²` per region, rows of increasing z, no shared edges. The low 4 bits are the **use class** and bits 4–6 the **climate**. Bit 7 is reserved and must be 0.

### Use classes and climates (`content/land.toml`)
They go through contentgen (docs/design/content.md) like other game data, so Rust and C# share their ids, and they're part of the content hash.

| Use | Wire | Building | Meaning |
|---|---|---|---|
| `wild` | 0 | no | open land and sea floor |
| `housing` | 1 | yes | houses and farms may be placed (M2) |
| `town` | 2 | no | settlements and ports |
| `road` | 3 | no | roads; nothing may be built on them |

Climates: `temperate` 0, `tropical` 1, `arid` 2, `subarctic` 3. A planting in its ideal climate grows in 70% of the time (docs/design/farming.md).

Each use also says whether it's `plantable`: crops, trees and livestock may go on `wild` and `housing` land, not in towns or on roads (docs/design/farming.md).

At most 16 use classes and 8 climates fit the byte. contentgen enforces that.

### The placeholder world's land use
- **Climate** by place: the north (z < −4400 m) is subarctic, the west continent's south half (z > 1500 m) tropical, the east continent's south half arid, and everything else temperate.
- **Town:** the trade island within 250 m of spawn (0, 0).
- **Housing:** six areas of 400–600 m radius, three on each continent's lowlands, wherever the ground there is between 2 m and 60 m up and gentler than 1 in 4.
- **Roads:** none yet (deviation).

### The terrain hash
- A chunk's hash is FNV-1a 64 over its cell key (`u64`) then its 1,089 samples (`u16`), all little-endian, passed through the splitmix64 finaliser (`sim::terrain::chunk_hash`). The **terrain hash** is the wrapping sum of the hashes of every chunk present. No terrain is 0. Because it's a sum, it doesn't depend on order, and the server can keep it up to date one chunk at a time.
- The server keeps it in the public single-row table **`terrain_info`** (`chunks`, `hash`). `admin_put_terrain` subtracts a replaced chunk's hash and adds the new one, and `admin_clear_terrain` zeroes it. If the row is missing (a database from before this table), it's rebuilt from the chunks the first time it's needed.
- `enter_world(char_id, content_hash, terrain_hash)` refuses a client whose terrain hash differs from the server's: "client terrain … does not match the server's …; run `cargo xtask world`". That's a refusal rather than a stream of corrections.
- The client works out the hash from the heights it loaded, rather than trusting the manifest, so a bad load is caught too. The manifest's hash is checked by tests and by `world-upload`.
- Bots enter with the hash they read from `terrain_info`. They test the server, not a world file. `bot visitor` stands on the world's files, so it fails early if the server has different terrain.

### Uploading
- `cargo xtask world` uploads `world/` to the local server through `loadbot world-upload`, with the **publisher's identity**: the xtask reads the SpacetimeDB CLI's token (`spacetime login show --token`) and passes it to loadbot in `MMO_ADMIN_TOKEN`. The publisher is always an admin (`owner`, docs/design/terrain.md), so this also works on `--prod` builds, which have no `dev_set_gm`. Without the variable, loadbot falls back to a fresh identity and `dev_set_gm` (dev builds only).
- After uploading, `world-upload` checks that the server's `terrain_info` hash equals the manifest's. Then it replaces every resource node with `world/nodes.tsv` (`admin_clear_nodes`, then `admin_put_nodes` in batches of 1,000) and checks the count. That resets every node to ready.
- `cargo xtask world --gen` regenerates `world/` first. The result should be committed.
- Then it replaces the server's land use with the world's (`admin_clear_land`, `admin_put_land`): one private `land_chunk` row per cell that isn't all wild and temperate, 32,568 for the placeholder world, and checks the land hash in `terrain_info` (docs/design/land.md).

### The client
- `--world DIR` (default `world`) loads the manifest and all regions in parallel into one `HeightMap` and one `LandRaster`, in about 0.2 s on the laptop. `--world FILE.mmoh` still loads a bare height file, for tests and the old spike files.
- The terrain's placeholder colours tint housing land and towns (ADR 0004, placeholder look). The HUD shows the use class and climate under the player.
- The client passes its terrain hash to `enter_world`.

### Measured (laptop, 2026-10-26)
- `world/` is 9.4 MB in 625 files, against 128 MB of raw heights. LZMA would be 14% smaller; zlib is pure Rust in `flate2` and built into .NET (ADR 0018).
- `worldtool gen` takes 3.5 s. Loading takes 0.4 s in Rust and 0.2 s in the client, and uploading 62,500 chunks takes 6 s.
- The placeholder world is 40% land. Housing covers 3.6 km² in six areas, the town 0.2 km².

## 3. Deviations and not yet done
- **No roads, no water bodies, no obstacle outlines or placements** (PLAN §5.4). Lakes and rivers above sea level need a water-surface layer; it can be another stream in the region file (version 2).
- **No editor tooling.** The generator writes everything.
- **Ocean regions are stored in full.** They compress to almost nothing, so a special case isn't worth it yet.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| sim | `chunk_hash` depends on the cell and on every sample; the terrain hash is order-independent; replacing a chunk by subtracting and adding matches recomputing | `server/sim/src/terrain.rs` |
| sim → C# | Golden vectors: the hashes of a small height file's chunks and their sum | `client/tests/vectors/sim.txt`, `SimVectorTests` |
| contentgen | Land uses and climates parse in wire order, reach both outputs and the hash; more than 16 uses or 8 climates is an error | `server/contentgen` |
| worldtool | Region round trip (heights at the encoding's extremes, land use), corrupt and truncated files fail, a reserved bit fails, shared edges must agree; a world round trip through a directory; the generator's layout (moved from loadbot) and land use (spawn is town, housing on both continents, the four climates) | `tools/worldtool` |
| worldtool | The committed `world/` loads, and its manifest hash matches its heights | `tools/worldtool` |
| module | `terrain_info` follows puts, replacements and clears, and equals `sim`'s sum; a missing row is rebuilt; `enter_world` refuses a wrong terrain hash and accepts the right one | `server/world/src/tests` |
| xUnit | The committed `world/` loads in C#, its terrain hash matches the manifest, and it has the trade-island town under spawn; a truncated region and a bad manifest fail | `client/tests/WorldFilesTests.cs` |
| live | `cargo xtask world` uploads with the owner's token on dev and `--prod` builds, and the server's hash matches; the client enters; a client with a different world is refused with the message | manual |
| capture | The trade island's town and a housing area tinted, with the HUD's land-use line | `docs/captures/` |
