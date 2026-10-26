# Content pipeline (`contentgen`)

Status: v0.1, 2026-10-25. M0 slice: the movement-mode table only. Code: `server/contentgen`, `content/`, outputs in `server/sim/src/content.rs` and `client/content/Content.g.cs`. Decision record: ADR 0012.

## 1. How the original worked

Players never see how the original stored its data, and its client data is off limits anyway (CLAUDE.md). The one visible behaviour is that an out-of-date client was made to update before it could play. Confidence B (common to MMOs; the original's launcher patched before login). Everything else here is our architecture (PLAN §5.4).

## 2. Our rules

### Sources
- `content/` holds human-edited TOML, one file per table group: `movement.toml`, `ships.toml` (docs/design/ships.md) and `land.toml`, the land-use classes and climates of the world's land-use raster (docs/design/world.md). Land uses become `LandUse`/`LAND_USES` (with `buildable`), climates `Climate`/`CLIMATE_IDS`; at most 16 and 8, to fit the raster's byte.
- Unknown fields are errors, so a typo can't silently fall back to a default.
- `contentgen` reports every problem it finds, as `content/<file>[:line]: <what>`, not just the first.

### Movement modes (`content/movement.toml`)
| Field | Rule |
|---|---|
| `id` | lower-case letters, digits and `_`, starting with a letter; unique. Becomes the enum variant (`run` → `MoveMode::Run`). |
| `wire` | the value in `move_event.mode`. The values must run 0, 1, 2, … with no gaps or repeats, so the wire value is also the table index. |
| `max_speed_mps` | horizontal speed limit; above 0 and at most 50 m/s. |

The values are the placeholders from docs/design/movement.md (walk 2.0, run 5.4, swim 3.5 m/s, confidence C).

### Outputs
- **Rust:** `server/sim/src/content.rs` (module `sim::content`): `CONTENT_HASH`, the `MoveMode` enum, `MoveModeData` and the static `MOVE_MODES`, with `MoveMode::from_u8` and `MoveMode::data`. `sim::movement` re-exports `MoveMode`, so the module and loadbot read the limits from content.
- **C#:** `client/content/Content.g.cs` (namespace `Mmo.Content`): the same enum, a `MoveModeData` record, and `GameContent.Hash` and `GameContent.MoveModes`. The Godot project will compile it; until then `cargo xtask test` compiles it on its own with warnings as errors.
- Both are committed. `cargo xtask content` rewrites them; `contentgen check` (the first step of `cargo xtask test`) fails if either is stale, and so does a unit test.

### The content hash
- 64-bit FNV-1a over a canonical form: a generator format tag, then one line per row with the floats as raw bits. Comments, whitespace, key order and `5.4` vs `5.40` don't change it; any value, id, wire or generator-format change does.
- It only has to tell builds apart. It isn't a defence against a modified client; the server still checks every outcome.
- `enter_world(char_id, content_hash)` refuses a hash that differs from the server's, with an error naming both. Nothing is spawned.

## 3. Deviations and not yet done
- Only one table. Skills, items and the rest join as their systems arrive, along with reference checks between tables (PLAN §5.4) and CSV for large tables.
- The server's hash isn't published in a table, so a client finds out it is out of date only when `enter_world` fails. A public `server_info` row can come with the client's login screen if that's wanted.
- Clients aren't checked on connect, only on entering the world, because nothing before that depends on content.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | Rows sorted by wire; layout, comments and number spelling don't change the hash; any value change does | `server/contentgen/src/lib.rs` |
| contentgen | Bad TOML and unknown or missing fields are errors with file and line; duplicate ids, wire gaps, bad ids, bad speeds (negative, NaN, too fast, none) are all reported together | same |
| contentgen | Both outputs carry the same hash and values; committed outputs match `content/` | same |
| C# | The generated file compiles with warnings as errors | `cargo xtask test` |
| sim | Speed checks use the content limits | `server/sim/src/movement.rs` |
| module | A client with a different hash is refused and nothing spawns; the right hash then enters | `server/world/src/tests.rs` |
| scenario | Bots pass `sim::content::CONTENT_HASH` and enter as before | `cargo xtask check` |
