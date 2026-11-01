# 0039: No Terrain3D; our own terrain tiles and `world/` throughout, with our own editing tools

- **Status:** Accepted, 2026-10-31 (user's call: "forget Terrain3D altogether"). Supersedes ADR 0032 and ADR 0038; ADR 0017 applies again with no end date.
- **Context:**
  - ADR 0032 planned Terrain3D v1.1.0 as M2's world editor and renderer. Its spike (docs/spikes/terrain3d.md) showed a lossless editing round trip. As the renderer, though, it needed about 1.2 GB of video memory and about 3 GB of RAM, and ran at 25–33 fps on the Radeon 610M, and none of its settings fixed that.
  - The user first accepted that cost (ADR 0038's amendment), then decided to drop Terrain3D altogether.
- **Decision:**
  - **Terrain3D is removed:** the addon and its libraries, the `--terrain3d` modes, `Terrain3DLayout` and its tests, the colour-map shader, and `worldtool apply-terrain3d`. The client's code is back to its state before the spike.
  - **The client keeps drawing and colliding with its own tiles** from `world/` (ADR 0017, docs/design/worldstream.md), within spike 4's budgets.
  - **`world/` stays the single source**, written only by `worldtool`. Building the M2 world needs editing tools of our own, writing `world/` (heights, land use, placements). Their shape is M2's first planning question.
- **Consequences:**
  - No native third-party code in the client.
  - The spike's findings stay as history in docs/spikes/terrain3d.md, including the control-map layout and the costs measured.
  - The Terrain3D libraries committed in `e4b4b54` stay in the repository's history; they were deleted, not purged. Purging them means rewriting the public history, which needs the user's OK.
  - PLAN §3 and §11 and docs/design/coast.md no longer say M2 builds the world in Terrain3D.
