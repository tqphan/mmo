# Sessions, movement and cells

Status: v0.3, 2026-10-25. M0 slice; v0.2 publishes moves once per tick (ADR 0011); v0.3 adds the terrain checks (docs/design/terrain.md). Code: `server/sim/src/{cell,movement}.rs`, `server/world/src/{session,movement,time}.rs`, `sidecars/loadbot`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Tab-target MMO with client-side movement; speed and teleport hacks recurred throughout its life, which means server checks were weak | C | RESEARCH §2.11 |
| Carrying a trade pack makes you much slower, and rules out gliders and teleports | B | [gamepressure trade guide](https://www.gamepressure.com/archeage/6-trade/z669eb) |
| A donkey mount moves at about 5 m/s, which players describe as slow; with a pack, a donkey keeps about 4 m/s and a horse drops to about 1.8 m/s | C | [Magic Game World: the donkey](https://www.magicgameworld.com/?p=42608), RESEARCH §2A.4 |
| Base on-foot run speed is 5.4 m/s | B | RESEARCH §2A.4 (fan wiki and a gear calculator agree) |
| A sprint skill from level 3 runs at 8 m/s while it drains mana | B | RESEARCH §2A.4 |
| Swimming was fast: about 6.8 m/s unbuffed in early play, later slowed by a nerf of unknown size | C | RESEARCH §2A.4 |
| Walk speed and the pack slowdown as a percentage | **Unknown** | RESEARCH §2A.3 |

The run speed matches the original. Walk and swim are placeholders: walk is unknown, and the only swim figure predates a nerf, so we keep a conservative 3.5 m/s until M1 tuning. Sprint comes with skills in M1. They live in `content/movement.toml` (docs/design/content.md), so tuning them is a one-line change plus `cargo xtask content`.

## 2. Our rules

### Accounts and characters
- An account (private) is created the first time an identity connects.
- `create_character(name)`: 3–16 letters A–Z, stored as `Abcd`, unique ignoring case; at most 4 per account.
- A character has a public part (`character_public`: id, name) and a private part (`character_private`: owner, saved position), which the owner reads through the `my_characters` view.

### Entering and leaving
- `enter_world(char_id, content_hash)`: the client's content hash must match the server's (docs/design/content.md), and the caller must own the character. Inserts an `entity` (cold) and a `transform` (hot) at the saved position (settled on the ground where there is terrain), plus a private `presence` row (identity ↔ character ↔ entity, movement bookkeeping), which the owner reads through `my_presence`. Entering again first removes the old presence, so one identity has at most one entity in the world.
- `leave_world`, or disconnecting, saves the position and deletes the entity, transform and presence.
- New characters start at `SPAWN` (the origin) until the world has spawn points.

### Cells (PLAN §4.3)
- 64 m cells, 250 × 250 over 16 km, origin at the map centre; anything outside the map is clamped to the edge cells.
- `key = instance << 32 | cy << 16 | cx`, where `cy` comes from z and `cx` from x. The x index varies fastest, so one row of a window is a contiguous run of keys.
- `entity.cell` and `transform.cell` are indexed and updated whenever a move crosses a cell boundary.
- Near tier = the 5×5 window: one `cell = key` query per cell and table (`sim::cell::window_cells(center, 2)`). Range queries (`cell >= lo AND cell <= hi`) work too, but SpacetimeDB checks every range subscription against every change, which cost 7× the database time at 250 players (docs/spikes/throughput.md).

### Movement events (PLAN §4.2)
- `move_event(seq, x, y, z, yaw, speed_cm_s, mode)`. The client sends one on start, stop, turn, jump and mode change, plus a heartbeat every 0.5 s while moving.
- `seq` must be greater than the last accepted one; stale or duplicate events are dropped silently (`Ok`).
- Check (horizontal only): distance from the server's position ≤ `max_speed(mode) × (1 + 15%) × min(dt, 2 s) + 0.5 m`, where `dt` is measured since the last accepted event, using `now(ctx)`.
  - The 2 s cap means a client can't stay quiet and then jump.
  - Placeholder limits from `content/movement.toml`: walk 2.0 m/s, run 5.4 m/s, swim 3.5 m/s (confidence C).
- **The authoritative position lives in the private `presence` row** (x, y, z, yaw, speed, mode). Moves are checked against it and update it at once.
- **Others see moves once per tick** (ADR 0011). An accepted move also adds the entity to the private `moved` set. `world_tick` copies every moved position into the public `transform` (and `entity.cell`) in one transaction every 100 ms, so observers lag by 0–100 ms. Game rules that need a character's position read `presence`.
- **Then the height is checked against the terrain** (docs/design/terrain.md): not below the ground, no higher than a jump and a 51° climb from the last solid footing (the anchor, kept in `presence`), falling once airborne for more than 1.5 s, and swimming only in water. Where there's no terrain, height is accepted as sent.
- **Rejected move:** the position stays where it was with speed 0, a row goes into the private `suspicion` log, and `presence.corrections` goes up by one. The client watches `my_presence.corrections` and snaps to the position in that row when the count changes. `seq` and the time still advance, so the next legal step counts from the server position. Where there's terrain, the kept position is also settled on the ground.
- Unknown mode → `Err`. NaN or infinite position → treated as a rejected move.

### Time
- Every reducer reads game time through `time::now(ctx)`. In `dev` builds it adds the offset from the public `game_clock` row, which `dev_advance_clock(micros)` moves forward only and clients read to compute timestamp-based values (docs/design/labor.md). Production builds never write that row, and their `now` ignores it.

## 3. Deviations and not yet done
- **No obstacle checks yet.** Vertical checks against the heightmap came with spike 5 (docs/design/terrain.md); walls and buildings come with world-building tooling and housing.
- **No speed modifiers** (buffs, packs, mounts, gliding). These come with those systems.
- **Corrections are rate-free**, so a client that keeps cheating gets corrected each time but isn't kicked. Kicking and thresholds come once the anomaly data has been looked at.
- **Public positions are readable by any client** (PLAN §4.5). That's accepted.
- **CLI calls can't hold a session.** `spacetime call` connects and disconnects around each call, and disconnecting despawns the character, so anything that needs a session goes through `loadbot`.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| sim | Cell packing, clamping, row-major layout, 5×5 windows clipped at edges | `server/sim/src/cell.rs` |
| sim | Speed limits per mode, tolerance, 2 s gap cap, NaN, unknown mode | `server/sim/src/movement.rs` |
| module | Account once per identity; name rules and case-insensitive uniqueness; 4-character limit; no account → error | `server/world/src/tests.rs` |
| module | Enter spawns entity and transform in the right cell; can't use someone else's character; re-entering replaces the entity | same |
| module | Legal run accepted across a cell boundary (entity and transform cells agree); speed hack corrected and logged, then recovers; quiet-then-jump rejected; stale and duplicate seq ignored; NaN corrected; unknown mode errors | same |
| module | Moves reach `transform` only when the tick publishes them, one publish per moved entity, nothing when nothing moved; corrections publish speed 0 | same |
| module | Leave saves position, despawns (dropping any unpublished move), and re-enter restores it; two players appear in the same cell query; views return only your own rows; dev clock moves forward only | same |
| scenario | Two bots on a live server subscribe to the Near tier, see each other, B sees A's run, A's speed hack is corrected and B never sees the jump, A disconnects and B sees it despawn | `cargo xtask bot see-each-other` |
