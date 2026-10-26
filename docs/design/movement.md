# Sessions, movement and cells

Status: v0.1, 2026-10-25. M0 slice. Code: `server/sim/src/{cell,movement}.rs`, `server/world/src/{session,movement,time}.rs`, `sidecars/loadbot`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Tab-target MMO with client-side movement; speed and teleport hacks recurred throughout its life, which means server checks were weak | C | RESEARCH §2.11 |
| Carrying a trade pack makes you much slower, and rules out gliders and teleports | B | [gamepressure trade guide](https://www.gamepressure.com/archeage/6-trade/z669eb) |
| A donkey mount moves at about 5 m/s, which players describe as slow | C | [Magic Game World: the donkey](https://www.magicgameworld.com/?p=42608) |
| Base on-foot run speed | **Unknown** | Not found in a first search; added to RESEARCH §2A.3 |

Our speed numbers are placeholders until the research is done. They live in `sim::movement` so tuning them is a one-line change.

## 2. Our rules

### Accounts and characters
- An account (private) is created the first time an identity connects.
- `create_character(name)`: 3–16 letters A–Z, stored as `Abcd`, unique ignoring case; at most 4 per account.
- A character has a public part (`character_public`: id, name) and a private part (`character_private`: owner, saved position), which the owner reads through the `my_characters` view.

### Entering and leaving
- `enter_world(char_id)`: the caller must own the character. Inserts an `entity` (cold) and a `transform` (hot) at the saved position, plus a private `presence` row (identity ↔ character ↔ entity, movement bookkeeping), which the owner reads through `my_presence`. Entering again first removes the old presence, so one identity has at most one entity in the world.
- `leave_world`, or disconnecting, saves the position and deletes the entity, transform and presence.
- New characters start at `SPAWN` (the origin) until the world has spawn points.

### Cells (PLAN §4.3)
- 64 m cells, 250 × 250 over 16 km, origin at the map centre; anything outside the map is clamped to the edge cells.
- `key = instance << 32 | cy << 16 | cx`, where `cy` comes from z and `cx` from x. The x index varies fastest, so one row of a subscription window is one key range: `cell >= lo AND cell <= hi`.
- `entity.cell` and `transform.cell` are indexed and updated whenever a move crosses a cell boundary.
- Near tier = the 5×5 window: 5 range queries per table (`sim::cell::window_rows(center, 2)`).

### Movement events (PLAN §4.2)
- `move_event(seq, x, y, z, yaw, speed_cm_s, mode)`. The client sends one on start, stop, turn, jump and mode change, plus a heartbeat every 0.5 s while moving.
- `seq` must be greater than the last accepted one; stale or duplicate events are dropped silently (`Ok`).
- Check (horizontal only): distance from the server's position ≤ `max_speed(mode) × (1 + 15%) × min(dt, 2 s) + 0.5 m`, where `dt` is measured since the last accepted event, using `now(ctx)`.
  - The 2 s cap means a client can't stay quiet and then jump.
  - Placeholder limits: walk 2.0 m/s, run 5.4 m/s, swim 3.5 m/s (confidence C).
- **Rejected move:** the transform keeps the server position with speed 0, a row goes into the private `suspicion` log, and `presence.corrections` goes up by one. The client watches `my_presence.corrections` and snaps to its `transform` row when the count changes. `seq` and the time still advance, so the next legal step counts from the server position.
- Unknown mode → `Err`. NaN or infinite position → treated as a rejected move.

### Time
- Every reducer reads game time through `time::now(ctx)`. In `dev` builds it adds the offset from the private `dev_clock` row, which `dev_advance_clock(micros)` moves forward only. Production builds compile all of this out.

## 3. Deviations and not yet done
- **No vertical or obstacle checks yet.** They need the server heightmap (M0 spike 5). Height is accepted as sent.
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
| module | Leave saves position, despawns, and re-enter restores it; two players appear in the same cell query; views return only your own rows; dev clock moves forward only | same |
| scenario | Two bots on a live server subscribe to the Near tier, see each other, B sees A's run, A's speed hack is corrected and B never sees the jump, A disconnects and B sees it despawn | `cargo xtask bot see-each-other` |
