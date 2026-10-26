# 0017: The client draws terrain from the server's heightmap file, not Terrain3D

- **Status:** Accepted, 2026-10-26 (user chose this over Terrain3D's prebuilt binaries or building it from source).
- **Context:** PLAN §3 picked Terrain3D for client terrain. In spike 4 two problems showed up. First, Terrain3D is a native GDExtension, so we would ship and run a third party's compiled code, and the user would rather not. Second, its 1.0.2 source showed that adding or removing any region rebuilds the texture arrays of every loaded region, and removed regions stay in memory until a save. That makes streaming slower the more is loaded. The art is placeholder until the game systems are built (ADR 0004), so Terrain3D's material and foliage tools aren't needed yet.
- **Decision:**
  - The client draws terrain itself, in C#, from the same `.mmoh` heightmap the server checks movement against (ADR 0013, docs/design/terrain.md). One file is the source for both.
  - Terrain is cut into 256 m tiles. Each tile is a mesh at one of six levels of detail (2 m to 64 m between vertices) chosen by distance, built on worker threads and swapped in on the main thread. Skirts hide the cracks between levels. Tiles entirely in deep water aren't drawn.
  - Placeholder look only: vertex colours from height and slope.
  - The editor tools Terrain3D would have given come later, as our own tooling that writes `.mmoh` and the land-use raster.
- **Consequences:**
  - No native third-party code in the client. Only C# and Godot.
  - The client holds the whole heightmap in memory: 128 MB for the full 16 km map as `u16`. Ocean compression can come later if needed.
  - Foliage instancing, terrain texturing and sculpting tools are ours to build when the art pass and the world-building work need them.
  - PLAN §3's Terrain row, §6.1's WorldStream, §7 and §9 are updated. BACKLOG's world-exporter task becomes a world-building task without a Terrain3D plugin.
