# World streaming: terrain and sea on the client

Status: v0.1, 2026-10-26. M0 spike 4. Code: `client/src/World/{HeightMap,TerrainPlan}.cs`, `client/src/Presentation/{Terrain,WorldTest}.cs`, `sidecars/loadbot/src/world.rs`. Decisions: ADR 0017 (own terrain), ADR 0004 (placeholder look). Results: docs/spikes/world-scale.md.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| One seamless world of continents and sea, sailed and glided across with no loading screens outside dungeons | A | RESEARCH §2, ADR 0002 |
| Distant land and coasts stayed visible from the sea, so you could steer by them | B | common player knowledge |
| How the client stored or streamed terrain | **Unknown** | not public; it ran on CryEngine |

Only the player-facing result matters: no loading screens in the open world, and land visible from far off.

## 2. Our rules

### One heightmap for client and server
- The client reads the same `.mmoh` file the server checks movement against (docs/design/terrain.md): 2 m samples, `u16`, `raw / 32 − 512` m. The whole 16 km map is 8001 × 8001 samples, 128 MB, and the client keeps all of it in memory (`World/HeightMap.cs`).
- For the spike the file comes from `cargo xtask bot world-gen` (`target/world.mmoh`), a placeholder map laid out as PLAN §9 sketches it: two 4.4 × 9 km continents, a central sea about 6 km wide with three islands, a 10 × 3 km northern band and ocean round the outside. About 40% is land.

### Tiles and levels of detail
- The map is cut into **256 m tiles**, 63 × 63 = 3,969 of them, each 128 quads across at full detail. Tiles share their edge samples. (512 m tiles drew 2.8 M triangles when walking and took up to 11.6 ms to swap in; docs/spikes/world-scale.md.)
- Each tile is drawn at one of six **levels of detail**. Level `k` puts a vertex every `2·2^k` m: 2, 4, 8, 16, 32 and 64 m.
- The level is chosen by the distance from the camera to the nearest point of the tile in the horizontal plane: under 200 m is level 0, and each doubling of the distance adds one level (400, 800, 1600, 3200 m), up to level 5. At 1080p that's about 10 pixels per quad. Tiles more than **10 km** away aren't drawn. Fog hides the edge.
- A tile changes level only when the distance crosses a threshold by more than 10% (hysteresis), so the camera hovering at a boundary doesn't keep rebuilding it.
- **Deep water:** a tile whose highest sample is below −25 m isn't drawn. The sea's depth colour is fully deep long before that.
- **Cracks:** each tile has a skirt, a strip hanging down from its edges by its own vertex spacing plus 2 m, so a gap between two levels shows the skirt instead of the sky.
- The choice of tiles and levels is pure code (`World/TerrainPlan.cs`) and unit-tested.

### Building meshes
- Mesh arrays (positions, normals, colours, indices) are built on up to 4 worker threads, each taking a batch of the 16 nearest waiting tiles. Batches keep streaming speed independent of the frame rate.
- The main thread turns finished arrays into meshes, up to **2 ms per frame**. A tile keeps its old mesh until the new one is in, so nothing pops out.
- Normals come from central differences in the heightmap at the tile's own spacing. Colours are placeholders: sand near sea level, grass, rock on steep slopes and high up, sea floor under water.

### The sea
- The sea is the existing water shader (`shaders/water.gdshader`): waves near the camera inside a flat ring out to the horizon. It **follows the camera**, snapped to its 4 m grid so wave vertices don't slide, and the waves calm towards the edge of the moving square.

### The world test scene
`cargo xtask client --world-test target/world.mmoh --route cross|sail|walk|high [--no-vsync] [--secs S]` runs offline, with no server:

| Route | What it tests |
|---|---|
| `cross` | flies west to east across the whole map at 150 m/s, 250 m up: the worst streaming load |
| `sail` | 4 m above the sea at 11 m/s (a clipper's speed), north up the central sea between the continents |
| `walk` | 1.7 m above the ground at run speed (5.4 m/s) on the west continent |
| `high` | circles the map from 3 km up, to see it all at once |

It logs once a second, and prints a summary on exit: frame time (median, 99th percentile and worst), triangles and draw calls, video memory, process memory, tiles per level, the build queue and the longest time spent applying meshes in one frame.

### Budget (PLAN §6.3)
- **Mid-range** means a GPU around a GTX 1660 or RX 6600 at 1080p. The spike measures on the RTX 5070 Ti Mobile and on the laptop's Radeon 610M integrated GPU, which bracket it (docs/spikes/world-scale.md). On the Radeon, run Godot with `--display-driver wayland --gpu-index 1`.
- **Pass:** at 1080p, the terrain and sea take at most **4 ms of a 16.7 ms frame** on mid-range hardware, leaving the rest for characters, effects and UI. No frame over 33 ms while streaming at sailing speed. Terrain memory under 300 MB of video memory.

## 3. Deviations and not yet done
- **Seen from 3 km up, the far edge of the sea ring shows** against the sky. Players don't get that high.
- **Spike only.** The game client still walks on its flat placeholder ground. Putting these tiles and their collision into the game is the next step (BACKLOG).
- **No collision** for the tiles yet. The player will use `HeightMapShape3D`s for the tiles near it.
- **No props, foliage or texturing.** Placeholder look (ADR 0004).
- **The file is loaded whole.** Fine at 128 MB. Ocean compression or streaming from disk can come later if memory matters.
- **No editor tooling** for the heightmap yet. The generator stands in for it.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| xUnit | `.mmoh` parsing matches the Rust format: header, size checks, decoding, bilinear height between samples | `client/tests/HeightMapTests.cs` |
| xUnit | Level choice by distance with hysteresis; tiles beyond 10 km and in deep water are skipped; nearest tiles come first | `client/tests/TerrainPlanTests.cs` |
| loadbot unit | The generated world has both continents, the north, the islands, the central sea and the outer ocean; heights fit the format | `sidecars/loadbot/src/world.rs` |
| capture | Each route at 1080p: land on the horizon from the sea, no cracks or holes between levels, the sea meets the coast | `cargo xtask capture world-<route> …`, frames in `docs/captures/` |
| measurement | Each route with `--no-vsync` on both GPUs; numbers in docs/spikes/world-scale.md | `cargo xtask client --world-test …` |
