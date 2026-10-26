# Server-side terrain

Status: v0.1, 2026-10-25. M0 spike 5. Code: `server/sim/src/{terrain,movement}.rs`, `server/world/src/terrain.rs`, `sidecars/loadbot/src/terrain.rs`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Movement was client-side and the server checked it weakly. Speed, teleport and "wall climb" or "fly" hacks kept recurring | C | RESEARCH §2.11 |
| Characters jump about their own height, take damage from long falls, and can glide down from height with a glider | B | common player knowledge; gliders RESEARCH §2.6 |
| Skills need line of sight to the target; terrain and buildings block it | B | common to tab-target MMOs; PLAN §4.2 |
| The server's terrain resolution and how it checked height | **Unknown** | not public |

The original's numbers (jump height, gravity, fall damage) aren't known. Ours are placeholders, chosen to be lenient, and live in `sim::movement` until the client needs them too; then they move to `content/`.

## 2. Our rules

### Heightmap
- One global grid of height samples every **2 m**, from x, z = −8000 to +8000 (8001 × 8001 samples).
- A sample is a `u16`: height = `raw / 32 − 512` m, so −512 m to +1536 m in 3.125 cm steps.
- Stored in the private `terrain_chunk` table, one row per 64 m cell, keyed by the cell key (docs/design/movement.md): **33 × 33 samples** (row-major, z rows, x fastest), including the far edge, so any point in the cell can be interpolated from its own row. Neighbouring chunks repeat their shared edge.
- Size: 2.2 KB a chunk, about 136 MB if all 62,500 cells had terrain. Open ocean can go without (see deviations).
- A missing chunk means **unknown terrain**: no vertical checks there and nothing blocks sight. Off the map is unknown too.

### Ground height and the two triangulations
The client collides with triangles (Godot's heightmap shape), and which diagonal it splits each 2 m quad along isn't something the server should depend on. For a point, the server works out the height under **both** triangulations:
- `ground.lo` / `ground.hi`: the lower and higher of the two heights. Bilinear interpolation lies between them.
- `ground.gradient`: the gentler of the two triangles' slopes (rise over run).

Every check below takes the reading that is kinder to the player, so the choice of diagonal can never cause a false correction.

### Uploading
- `admin_put_terrain(chunks)`: upsert a batch of chunks. Each must have exactly 1089 samples and an open-world cell key on the map. Caller must be an admin.
- `admin_clear_terrain()`: delete every chunk. Admin only.
- **Admins:** the identity that published the database (recorded by `init`), or an account with `gm_level ≥ 3`. Dev builds add `dev_set_gm(level)`, which any caller may use. `cargo xtask world` uploads as the publisher, with the CLI's token, so it works on production builds too (docs/design/world.md).
- Uploads and clears keep the public `terrain_info` row (chunk count and terrain hash) up to date, and `enter_world` refuses a client whose terrain hash differs (docs/design/world.md, ADR 0018).
- **File format** (`.mmoh`, `sim::terrain::HeightFile`), little-endian: magic `MMOH`, `u16` version 1, `u16` first cell x, `u16` first cell z, `u16` cells across, `u16` cells down, then `(down·32+1) × (across·32+1)` `u16` samples, rows of increasing z. Regions are whole cells. `loadbot terrain-upload <file>` sends one in batches of 32 chunks. The client draws terrain from the same file (docs/design/worldstream.md, ADR 0017). The world itself comes from the region files in `world/` (docs/design/world.md): `loadbot world-upload` sends the whole map in about 6 s (`cargo xtask world`), adding 172 MB to the server (docs/spikes/world-scale.md). Tools that need the ground in a few places on a big file read it through `sim::terrain::FileChunks`, which cuts out each chunk the first time it's asked for.

### Vertical checks on movement
Every accepted horizontal move (docs/design/movement.md) is then checked against the ground at its destination, when the terrain there is known. The server keeps an **anchor** in `presence`: the position of the last solid footing and when the character last touched ground.

| Rule | Check |
|---|---|
| Not below ground | `y ≥ ground.lo − 0.5 m` |
| Climb ceiling (walk, run) | `y ≤ anchor.y + 2.0 m + 1.25 × horizontal distance from the anchor`. 2.0 m is the 1.5 m jump plus slack; 1.25 (51°) is steeper than the client's 45° walkable limit. |
| Falling (walk, run) | When airborne (`y > ground.hi + 0.3 m`), the slope credit stops at 1 s of running (the ground walked between the last reported contact and taking off), and the ceiling drops by a minimum fall: none for the first 1.5 s since the last ground contact, then gravity of 6 m/s² capped at 20 m/s. Real falls (9.8 m/s²) are always faster. In practice hovering is caught after about 3.5 s. |
| Swimming | No climb ceiling; `y ≤ sea level (0 m) + 1.0 m`. Swimming on land above sea level fails one of the two checks. |

The anchor, after an accepted move:
- **Standing** (touching ground with `gradient ≤ 1.25`), **swimming**, or terrain unknown: anchor = this position, contact time = now.
- **Steep** (touching ground that is too steep to stand on): contact time = now; the anchor moves here only if this is lower. So a cliff face can be slid down but not climbed.
- **Airborne:** unchanged.

A rejected move is handled like a speed hack (logged, `corrections + 1`), and the server's position is also **settled on the ground** (`y = ground.hi`, or kept if swimming and above the seabed), with the anchor there. That keeps a corrected client from being stuck in mid-air against the fall clock. `enter_world` and `dev_teleport` settle the same way, so a character saved before terrain existed doesn't start underground.

### Line of sight
- `sim::terrain::line_of_sight(a, b)`: walk the segment in steps of at most 1 m; blocked if any step is below `ground.lo`. Unknown terrain doesn't block.
- Used for now by the dev stand-in `dev_swing`, eye (feet + 1.6 m) to chest (feet + 1.0 m). Combat (M1) uses the same function.

## 3. Deviations and not yet done
- **No obstacle outlines** (walls, gates, buildings). They come with world-building tooling and housing, and will block both movement and sight.
- **No water raster.** Sea level is a constant 0 m, so lakes and rivers above sea level can't be swum in yet. The land-use raster (PLAN §5.4) will carry water height.
- **Missing chunks aren't checked.** The production world must upload every land cell. For open ocean a later version can store one depth per chunk instead of 1089 samples.
- **Endpoints only.** Moves are checked where they end, not along the way, so a fast cheater can still cut through a thin ridge between two heartbeats. The 51° credit means a cheater can climb about 1.25 m per metre walked onto walkable ground; tall cliffs (where the face itself is too steep) can't be climbed.
- **A cheater can hover for about 3.5 s** (the grace plus the slope credit). Gliders, a separate mode later, are the legal way to stay up.
- **No fall damage yet.** It needs health (M1).
- **Uploading while players are in the world** doesn't move them. Upload with the world empty, or expect a correction or two.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| sim | Height encoding round trip; a tilted plane is exact under both triangulations; a twisted quad gives lo < bilinear < hi; gradients; chunk edges agree; missing chunks and off-map are unknown | `server/sim/src/terrain.rs` |
| sim | Line of sight: clear over flat ground, blocked by a ridge, clear above the ridge, unknown terrain is clear | same |
| sim | File: round trip, chunk split matches the samples, bad magic and sizes fail | same |
| sim | Vertical: walk up a 40° slope; teleport up a cliff fails; climbing a cliff face in small steps fails; a jump passes; a fly hack fails; walking off a cliff and falling passes; hovering past the grace fails; underground fails; swim rules; anchor updates | `server/sim/src/movement.rs` |
| module | Admin-only upload, bad chunk rejected, clear; walk up a slope accepted; cliff jump corrected and settled on the ground; underground corrected; enter and teleport settle on the ground; `dev_swing` blocked by a hill | `server/world/src/tests.rs` |
| scenario | `loadbot terrain`: generate procedural test terrain (hills, mesas with cliffs), write it to a file, upload it, walk bots over it with simple physics (gravity, jumps, 45° limit, falling off cliffs, sliding down what's too steep) and a mix of both triangulations for `--secs`; zero corrections allowed. A slide counts once per fall; a walker whose slide gets under 1 m in 1.5 s tries two random headings, then respawns on walkable ground with `dev_teleport` (unit tests: a steep pit respawns after 4.5 s with one slide, a steep bank slides once and walks on). A cheater bot's cliff teleport, fly hack and dive underground are each corrected. Line of sight across a mesa fails and in the open succeeds. The terrain is cleared at the end. Acceptance run: 30 minutes. | `cargo xtask bot terrain` |
