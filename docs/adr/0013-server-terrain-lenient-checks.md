# 0013: Server terrain is a 2 m heightmap per cell, and its checks take the kinder triangulation

- **Status:** Accepted, 2026-10-25
- **Context:** Spike 5 (PLAN §8, M0) needs the server to check height and line of sight without false corrections. The client collides with Godot's heightmap triangles, and the server shouldn't depend on which diagonal each 2 m quad is split along, or on Terrain3D's internals. SpacetimeDB keeps every table in memory, so the format's size matters.
- **Decision:**
  - Heights are `u16` samples every 2 m (`raw / 32 − 512` m), stored in a private `terrain_chunk` table with one 33 × 33 row per 64 m cell, keyed by the cell key. Rows repeat their shared edge, so any point reads one row.
  - Every reading reports both triangulations. Below-ground and line-of-sight checks use the lower; "touching the ground" uses the higher; slope uses the gentler.
  - Vertical rules are anchored to the last solid footing rather than simulated: a jump-plus-51° ceiling from the anchor, a minimum fall (6 m/s², 20 m/s cap) after a 1.5 s grace, and the anchor moving only down on ground steeper than 51°. Every number is lenient compared with the client's physics (45°, 9.8 m/s², 1.5 m jump).
  - A rejected move settles the server position on the ground, so a corrected client can't be stranded in mid-air against the fall clock.
  - Admins are the identity that published the database, or accounts with `gm_level ≥ 3`. Dev builds add `dev_set_gm`.
  - Height files use a small `.mmoh` format (`sim::terrain::HeightFile`), which the world exporter will write. (Since ADR 0018 the world itself is region files in `world/`; `.mmoh` stays for test terrain.)
- **Consequences:**
  - About 2.2 KB per cell, or up to 136 MB for a fully covered 16 km map. Open ocean will need a cheaper representation.
  - Cheaters keep some room: about 3.5 s of hovering, a 51° climb onto walkable ground, and cutting through thin ridges between heartbeats. The spike's goal was no false positives. Tightening waits for anomaly data from playtests.
  - Jump height and gravity are server constants for now. They move to `content/` when the client's controller needs the same numbers.
  - `presence` gained anchor columns, a schema change that needs a `--clear` publish.
