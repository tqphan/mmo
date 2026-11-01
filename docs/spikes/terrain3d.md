# Spike: Terrain3D for the whole map

Status: **done**, 2026-10-31. PLAN §11, M2's first task (BACKLOG). Decisions: ADR 0032, ADR 0038. Compared against: docs/spikes/world-scale.md (spike 4).

## Question

Can Terrain3D v1.1.0 hold the whole 16 km map loaded at once, within budget on both GPUs, with a lossless round trip to `world/` and collision that the server agrees with?

**Done when** (BACKLOG):
- the round trip leaves `world/` byte-identical when nothing was edited;
- VRAM, RAM and GPU p99 are recorded on the RTX 5070 Ti and the Radeon 610M, for spike 4's four routes;
- a 10-minute walk in the game client on Terrain3D's collision has zero corrections;
- captures.

If it passes, the client's own tiles (`Presentation/Terrain.cs`, `TerrainPlan`) and `TerrainCollision`'s shapes are removed.

## What was built

- **The addon:** `client/addons/terrain_3d/` from the release zip (MIT, `assets/LEDGER.md`), without the demo. Its C# bindings compile as generated code (`client/addons/.editorconfig`), so our nullable warnings-as-errors leave them alone. Only its Linux and Windows x86_64 libraries are committed (ADR 0038).
- **Layout** (`client/src/World/Terrain3DLayout.cs`, xUnit `Terrain3DLayoutTests`):
  - Regions are 1024 px at 2 m spacing (2,048 m). The 8 × 8 regions at locations −4 … 3 cover −8,192 … 8,192 m.
  - World sample *i* is pixel *i* − 4000. Pixels off the map repeat the edge.
  - Heights are 32-bit floats. Every `u16` sample (1/32 m steps) survives the trip exactly; the test covers all 65,536.
- **Land use in the control map:**
  - Terrain3D v1.1.0's control map uses bits 0–2 (auto, nav, hole) and 7–31 (scale, angle, blend, overlay, base). **Bits 3–6 are unused.** They hold the texel's use class: 4 bits, exactly the 16 classes `content/land.toml` allows.
  - Each 4 m texel covers 2 × 2 pixels; the export reads its north-west pixel.
  - **The climate doesn't fit**, so it isn't stored in Terrain3D. `apply-terrain3d` keeps each texel's climate from `world/`.
- **In-game** (`--terrain3d`, `Presentation/Terrain3DWorld.cs`):
  - All 64 regions are built in memory from `world/` at start: pixels in parallel, then one `update_maps`. Nothing streams, which avoids the rebuild-on-add cost (ADR 0032).
  - The colour map holds today's placeholder colours.
  - The terrain is drawn by our own shader, `client/shaders/terrain3d.gdshader` (below).
  - At start, the client checks Terrain3D's height at 4,096 random samples against the map. Every run so far: 0.000 m apart.
  - With `--world-test` it replaces the tiles. In the game it also replaces `TerrainCollision`'s shapes with Terrain3D's dynamic collision around the player. The window is still followed, so the snap-wait logic is unchanged.
- **Editing round trip:**
  1. `cargo xtask client --terrain3d-import` writes the 64 regions into `client/terrain3d/` (112 MB, not committed) for the Godot editor.
  2. Edit there.
  3. `cargo xtask client --terrain3d-export` writes `target/terrain3d/export/` (`heights.mmoh`, `uses.raw`).
  4. `cargo run --release -p worldtool -- apply-terrain3d` writes `world/`, keeps climates, and reports what changed (Rust test).

### Deviations from ADR 0032
- **The import step is in the client, not `worldtool`.** Only Godot can write Terrain3D's region resources. `worldtool` does the way back (`apply-terrain3d`), so `world/` is still only ever written by `worldtool`.
- **Climate stays out of Terrain3D** (no room in the control map), so it can't be edited there.
- The players' client doesn't read Terrain3D's files; it builds regions from `world/` at each start (4.4 s on the laptop, 2.3 s of it computing pixels).
- **Our own shader** instead of Terrain3D's default (ADR 0038).

### Found on the way
- **The region size must be set after the node enters the tree.** Its data object is created then, with the default 256 px, and a size set earlier is lost. With 1024-px images in 256-px regions, everything lands in the wrong place (heights off by up to 255 m). `Terrain3DWorld` sets the size after `AddChild`, and the startup height check catches any regression.
- The first headless `--import` after adding the addon crashed (SIGSEGV) after finishing. A second run was clean and registered the extension (`client/.godot/extension_list.cfg`). Not seen again.
- A `SceneTree` script must wait one frame after adding a `Terrain3D` before its `data` exists.

## Results

### Round trip
- **Unedited:** import, export and `apply-terrain3d` reported "0 height samples and 0 land texels changed". `git status world/` was clean: **byte-identical**.
- **Edited** through Terrain3D's API (`set_height` +1.5 m at one vertex; `set_control_base_id` and `set_control_blend` on a Furrowdale pixel): the export gave back exactly **1 changed sample**, in `x06z13.mmor`, and the manifest's hash. The painted town pixel **kept its use class** (bits 3–6), so texture painting doesn't clobber land use. `world/` was restored afterwards.

### Frame times and memory
1920 × 1080, 2× MSAA, shadows, glow, as in spike 4. GPU is whole-scene time. Tiles and Terrain3D were measured the same evening.

**RTX 5070 Ti** (GPU p50 / p99 ms; frame max):

| Route | Our tiles | Terrain3D, default shader | Terrain3D, our shader |
|---|---|---|---|
| `sail` | 1.07 / 1.37, max 10.4 | 2.46 / 2.90, max 4.4 | 1.96 / 2.33, max 6.4 |
| `walk` | 1.45 / 1.85, max 23.0 | 1.77 / 2.34, max 6.3 | 1.39 / 1.83, max 5.4 |
| `cross` | 1.45 / 1.89, max 13.1 | 2.35 / 3.04, max 7.2 | 1.75 / 2.42, max 17.0 |
| `high` | 1.28 / 1.67, max 16.9 | 2.28 / 2.68, max 6.8 | 1.67 / 2.02, max 5.8 |

**Radeon 610M** (GPU p50 / p99 ms; frames over 33 ms; tiles from spike 4):

| Route | Our tiles | Terrain3D, default shader | Terrain3D, our shader |
|---|---|---|---|
| `sail` | 17.1 / 17.9, 0 | 58.5 / 59.0 | 39.9 / 43.7, 1,358 |
| `walk` | 20.7 / 23.6, 0 | 42.3 / 51.0 | 30.4 / 36.3, 323 |
| `cross` | 22.9 / 27.5, 0 | 58.6 / 69.5 | 36.5 / 45.2, 2,068 |
| `high` | 19.0 / 19.5, 0 | 66.7 / 74.3 | 36.8 / 39.7, 1,479 |

**Memory** (the same on both GPUs):

| | Our tiles | Terrain3D |
|---|---|---|
| Video memory | 283–313 MB | **1,148–1,154 MB** |
| Client resident | 1.1–1.2 GB | **2.5–3.0 GB** |
| Draw calls | 567–985 | 17–83 |
| Start-up | 0.5 s to the first view | 4.4 s to build all regions, then nothing streams |

- Terrain3D has **no streaming hitches**: its worst RTX frames are mostly 4–7 ms (one 17 ms frame on `cross`), against 10–23 ms for our tiles while they stream.
- The extra ~860 MB of video memory is the 64 regions' height, control and colour maps (12 MB each, 768 MB), as ADR 0032 estimated. Terrain3D also keeps the images on the CPU (for collision and `get_height`), which is most of the extra resident memory.

**610M experiments** (30 s, `sail` and `walk`, GPU p50):
- Terrain3D's default shader → ours: −30% (58.5 → 39.9, 42.3 → 30.4).
- Shadows off: about −1 ms. Not kept.
- A trivial fragment stage: 34.5 and 26.6. So per-pixel work is only about 4 ms of ours; the rest is the clipmap's vertex work over the whole seabed and land, in the depth, colour and shadow passes.
- Mesh size 24 instead of 48 (half the triangles): `sail` 32.4, `walk` 29.4. Little gain; not kept.

### Collision walk
- `cargo xtask client --terrain3d --profile walker --teleport -3360,-520 --autopilot circle:-3480,-520,120 --secs 620`: running round Bristle Downs' second hill against the live server for about 10 minutes.
- **1,234 moves, 0 corrections**, standing at most **0.01 m** off the server's heightmap.

### Captures
`docs/captures/2026-10-31-terrain3d-walk.png` (with `-walk-tiles.png`, the same frame from our tiles: they look the same), `-high.png` (the whole map from 3 km) and `-furrowdale.png` (the game client at the start, with the town tint, the beach and the sea).

## Verdict

**Terrain3D works for M2's world, but not yet as the players' renderer.**
- **Pass:** the round trip is lossless and edits come through to the sample. Land use survives texture painting. Collision gives zero corrections. On the RTX, our shader keeps the whole scene under 2.5 ms p99, close to our tiles.
- **Not met:** spike 4's budgets.
  - Video memory: 1.15 GB against 300 MB.
  - The 610M: 25–33 fps against 36–60 with our tiles. A mid-range GPU (9–16× the 610M by specifications) would be at roughly 2.5–4.5 ms for the whole scene, at the edge of the 4 ms budget.
- **So the tiles stay.** The client still draws with its own tiles by default; `--terrain3d` is opt-in. Removing them waits until the levers below are tried, or the user accepts the cost (ADR 0038).

## Levers, in order
1. **Compressed colour maps** (ADR 0032's fallback; `Terrain3D.ColorCompressMode`): 4 MB → 1 MB a region, about −190 MB of video memory. Or no colour map at all, once real textures replace the placeholder colours.
2. **Free the CPU images** after upload where Terrain3D allows it, keeping only the heights for collision: about −0.5 GB resident.
3. **Skip regions wholly below −25 m** (ADR 0032 allows it until diving). Only the outer ocean's corners qualify at 2 km regions, so this needs a smaller region size, which means more regions.
4. **A cheaper far field:** the vertex work over the seabed dominates on the 610M. Try fewer clipmap LODs, or a flat sea-floor mesh beyond the shore.
5. A low-settings preset (spike 4's follow-up) helps both renderers equally.
