# 0038: Terrain3D is opt-in behind `--terrain3d` with our own shader; our tiles stay the default; its Linux and Windows libraries are committed

- **Status:** Accepted, 2026-10-31. Amended the same day: the user chose to commit the libraries, and, after the levers (docs/spikes/terrain3d.md), chose **Terrain3D as the renderer too**, accepting its cost.
- **Context:**
  - M2's Terrain3D spike (docs/spikes/terrain3d.md, ADR 0032) passed its functional checks: a byte-identical round trip, edits that come back to the sample, land use that survives texture painting, and 10 minutes of walking with zero corrections.
  - It missed spike 4's budgets: 1.15 GB of video memory against 300 MB, 2.5–3 GB resident against 1.2 GB, and 25–33 fps on the Radeon 610M against 36–60 for our tiles.
  - Terrain3D's default shader blends textures that our placeholder look (ADR 0004) doesn't use. On the 610M it cost 30% more than a shader that reads only the colour map.
  - The release zip is 45 MB. Its Linux and Windows x86_64 libraries are 16 MB, and committed binaries stay in a public repository's history for good.
- **Decision:**
  - **`--terrain3d` is opt-in**, in the world test and the game. Without it, the client draws and collides with its own tiles as before. ADR 0032's removal of the tiles waits until the spike's levers bring memory and the 610M within budget, or the user accepts the cost.
  - **Our own shader:** `client/shaders/terrain3d.gdshader` is Terrain3D's `minimum.gdshader` (MIT) with a bilinear colour-map lookup, lit like our tiles (roughness 1, no specular). It's set as the material's shader override.
  - **Land use goes in the control map's bits 3–6**, which Terrain3D v1.1.0 doesn't use. Climate stays only in `world/`.
  - **The import is a client mode, and the way back is `worldtool apply-terrain3d`.** Only Godot writes Terrain3D's resources; only `worldtool` writes `world/`.
  - **The Linux and Windows x86_64 libraries are committed** in `client/addons/terrain_3d/bin/` (16 MB, let through `.gitignore`; user, 2026-10-31). Other platforms' libraries are left out. Upgrading Terrain3D replaces them in its own task.
- **Consequences:**
  - Two terrain paths in the client until the tiles go: `Presentation/Terrain` and `TerrainCollision` (default), and `Terrain3DWorld` with Terrain3D's collision.
  - Gotcha: a `Terrain3D` node's region size must be set after it enters the tree, or its data keeps the default 256 px. A height check at start catches it.
  - The repository carries 16 MB of native code per Terrain3D version in its history.
- **Amendment, 2026-10-31 (user's call after the levers):** Terrain3D is the editor **and** the players' renderer and collision, at about 1.2 GB of video memory, about 3 GB of RAM and 25–33 fps on integrated GPUs; spike 4's budgets are replaced by these figures. M1 keeps our tiles (ADR 0032), so the playtest isn't disturbed. The switch is M2's next task: Terrain3D by default, our tiles, `TerrainPlan` and `TerrainCollision`'s shapes removed with their tests, and colour maps compressed by shipping editor-made region files if memory needs it later.
