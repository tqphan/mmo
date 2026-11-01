# 0041: Trees, rocks and ground cover are cosmetic, placed by the client from the world's files

- **Status:** Accepted, 2026-11-01 (user: "keep going. trees. rocks. variety.", before the M1 playtest).
- **Context:** The ground and the sea had their look (ADR 0040), but the land was bare. Trees and rocks were wanted before the playtest. Gatherable nodes (Driftpines, Hollowoaks, ore) are already server rows; scenery is something else.
- **Decision:**
  - **Scenery is cosmetic and client-only.** It has no rows, no collision and no effect on movement, and the server never hears of it. Gatherable nodes stay what they are.
  - **Placement is a pure function of `world/`** (`FloraPlan`): a jittered grid per cell, integer hashing, and rules shared with the terrain shader. Beaches come from the open-sea mask, marsh from the flats and pools, rock from steep ground, and forest density from wide noise and the named area (Greymane Wood thick, the downs and the meadow open). Every client sees the same forest, and editing `world/` moves it.
  - **Thirteen kinds, three variants each:** pine, oak, birch, autumn tree, dead tree, bush, boulder, rock, driftwood, grass tuft, flowers, reeds and pebbles. They're built procedurally (`Props.cs`), with no assets.
  - **Two layers, drawn by `Greenery`:** trees, bushes and stones in 256 m cells out to 900 m, and ground cover in 32 m cells out to 80 m. Cells are planned on worker threads and drawn as one MultiMesh per kind and variant. The flora shader sways them by height.
- **Consequences:**
  - The world test's GPU p50 rose from 1.76 to 2.41 ms walking (p99 3.1 ms) on the RTX 5070 Ti Mobile, within spike 4's 4 ms. The Radeon 610M needs measuring.
  - Nothing blocks movement: players and NPCs walk through trees. Collision would need the server to know the trees (a later decision).
  - Housing land keeps only the odd bush, so it stays clear for building. Towns and roads are bare.
