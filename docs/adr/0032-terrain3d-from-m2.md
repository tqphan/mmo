# 0032: From M2, the world is built and drawn with Terrain3D v1.1.0; `world/` stays the server's source

- **Status:** Accepted, 2026-10-31 (user's call). Supersedes ADR 0017 from M2 on; M1 keeps its own tiles.
- **Context:**
  - ADR 0017 dropped Terrain3D in spike 4 for two reasons: it is native third-party code, and adding or removing a region rebuilds every loaded region's texture arrays.
  - M2 builds a much bigger world ("Two Continents"). Sculpting, texture painting and foliage are the tools ADR 0017 left us to write ourselves. The user wants the M2 world built with Terrain3D v1.1.0, which accepts the native code.
  - Terrain3D v1.1.0-stable came out on 30 Oct 2026. It supports Godot 4.5–4.8, so it works with our 4.7.2. It adds C# bindings, an ocean mesh and a setting that limits how many regions are rendered (PR #1003).
  - Its streaming cost hasn't changed. In `src/terrain_3d_data.cpp` at `v1.1.0-stable`, `add_region` and `remove_region` with `update` still call `update_maps(TYPE_MAX, true, …)`, which rebuilds the height, control and colour arrays from every active region, and a removed region stays in `_regions` until it's saved (RESEARCH §4.3).
  - The server can't run Godot code. It checks moves against the `u16` 2 m heightmap in `world/` (ADRs 0013, 0018), and a client whose terrain hash differs is refused.
- **Decision:**
  - **Pin Terrain3D v1.1.0-stable.** It's added to `client/addons/` and recorded in `assets/LEDGER.md` (MIT). Upgrading it is its own task, as with Godot and SpacetimeDB. **Open:** use the prebuilt release binaries or build them from source (the user decides before the first M2 task).
  - **`world/` stays the single committed source of truth** for heights and land use. The server and the terrain hash are unchanged.
  - **Terrain3D is the editor and the renderer.** A `worldtool` step imports `world/` into Terrain3D regions at 2 m vertex spacing for editing in the Godot editor. An export step writes the edited heights back to `world/`, quantised to the `u16` format, and paints land use from a control-map layer. Texture painting and foliage stay in Terrain3D's own files, because the server never needs them.
  - **The client loads every region once at start and never streams.** This avoids the rebuild-everything cost. Whether that fits in memory on the Radeon 610M is the first thing M2 measures (below).
  - **Collision:** the client uses Terrain3D's collision built from the same imported heights. The server's checks already take the kinder of the two triangulations (ADR 0013), so the choice of diagonal doesn't matter.
- **Consequences:**
  - M2's first task is a spike that redoes spike 4's routes with Terrain3D. It measures VRAM and RAM for the whole 16 km map loaded at once (64 regions of 1024 px at 2 m spacing is about 12 MB each uncompressed, about 770 MB in all), GPU frame time on the RTX 5070 Ti and the 610M, and zero false corrections on a walk. If the 610M doesn't fit, the fallbacks are, in order: the max-regions setting, compressed colour maps, then larger vertex spacing far from the player.
  - The client's own terrain code (`WorldStream` tiles and `TerrainCollision`) is removed once the spike passes, and its xUnit tests go with it.
  - Native third-party code now runs in the client; this reverses ADR 0017's first consequence. The Windows PC needs the Windows binary of the same version.
  - Godot's .NET build loads GDExtensions, and v1.1.0's C# bindings mean our code can call Terrain3D directly.
