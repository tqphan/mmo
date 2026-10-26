# Backlog

In order. Each task should fit in one session and has acceptance criteria. Tasks marked **[needs OK]** wait for a user decision. Done tasks move to the bottom with their date.

## M0: Foundations and risk spikes

1. **Spike 4: world scale**: a 16 km Terrain3D map with water and streaming, within budget at mid-range settings. Placeholder look only (ADR 0004): terrain coloured by height and slope, no art.
2. **World exporter v0**: Godot plugin → heightmap as a `.mmoh` file (docs/design/terrain.md) and a land-use raster → uploaded as an admin. Production uploads need the publisher's identity (the owner), since `dev_set_gm` is dev-only.
3. **Verify on the Windows PC**: upgrade the SpacetimeDB CLI to 2.13.0, add the wasm target, install Godot 4.7.2 .NET, and check that `cargo xtask check` passes and `cargo xtask client` runs.

4. **Loadbot: unstick terrain walkers.** A walker that lands in a steep-sided dip slides every frame (one 30-minute run counted 10,020 slides against 19 in another). Count a slide once per fall, and pick a new heading or respawn after a few seconds stuck.

5. **Client: walk on decks and drive ships** (M1 ships). Send `parent` and ship-local positions, extrapolate ships with `sim::ship::step` ported to C# and checked against vectors, and don't let a body inherit the deck's velocity when it steps off (ADR 0014). Add the helm and cannon controls.
6. **Client: reconnect.** Use the SDK's `WithAutomaticReconnect`, and re-enter and resubscribe the director after a drop.

## Research backlog
- Original on-foot speeds (walk, run, swim, sprint) and the pack-carrying slowdown. Needed before M1 tuning (docs/design/movement.md §1).
- Original jump height, how falling felt, and the fall-damage threshold. The terrain checks use lenient placeholders (docs/design/terrain.md §1); fall damage needs the threshold in M1.
- Original ship handling and gunnery: acceleration, turn rate, cannon range, damage, reload and ship HP. All placeholders now except the clipper's 11 m/s (docs/design/ships.md §1). Needed before M1 ships.

## Done
- 2026-10-25 **Repository scaffold**: STATUS, BACKLOG, ADRs 0001–0009, docs folders, asset ledger.
- 2026-10-25 **Server skeleton and dev tasks**: Cargo workspace, `world` module, `sim` crate, `cargo xtask` (test/build/server/publish/generate/bot/check).
- 2026-10-25 **Sessions, movement, cells (server side)**: accounts, characters, enter and leave, checked movement events with corrections, cell keys, Near-tier window queries.
- 2026-10-25 **Test clock and loadbot skeleton**: `now(ctx)` with a dev offset and `dev_advance_clock`; the `loadbot see-each-other` scenario passes against a live server.
- 2026-10-25 **World tick and server metrics**: 10 Hz `world_tick` with a measured, capped `dt`; timings from the host's `/v1/metrics` via `loadbot metrics` instead of a module table (ADR 0010); `tick` scenario.
- 2026-10-25 **Client toolchain on the Linux machine**: .NET SDK 8.0.132 and 10.0.113 (pacman), Godot 4.7.2 .NET in `~/.local`; `cargo xtask generate` formats the C# bindings; a throwaway Godot project with `SpacetimeDB.ClientSDK.Godot` 2.13.0 built and connected headless.
- 2026-10-25 **Spike 1: throughput**: `loadbot swarm`; go for 500 players spread out (DB 13% busy, reducer p99 2.7 ms, busiest client 9.2 KB/s), no-go for crowds above about 110 in view (bandwidth); Near tier now subscribed cell by cell, which cut database time 7× (docs/spikes/throughput.md).
- 2026-10-25 **Spike 2: mass fight and send rates**: moves published once per tick from private `presence`, clients use Gzip (ADR 0011); a 150-player crowd went from 53 to 19 KB/s per client, and 500 in view from 124 to 44 KB/s (docs/spikes/mass-fight.md).
- 2026-10-25 **Cell switching with hysteresis**: `sim::cell::recenter` (8 m margin) and a subscription director in loadbot with one subscription per cell, new before old; `cross-cells` scenario (290 m, 5 window moves, no gap); the swarm uses it, and 500 spread bots went from 6.0 to 4.9 KB/s per client (docs/design/interest.md).
- 2026-10-25 **`contentgen` v0**: `content/movement.toml` → `sim::content` (Rust) and `client/content/Content.g.cs` (C#), both committed and checked for staleness by `cargo xtask test`; FNV-1a `CONTENT_HASH`, checked by `enter_world` (docs/design/content.md, ADR 0012).
- 2026-10-25 **Spike 5: server-side terrain**: private `terrain_chunk` (2 m `u16` heightmap, 33 × 33 per cell), admin upload from `.mmoh` files, vertical checks anchored to the last footing and line of sight in `sim`, both taking the kinder triangulation (ADR 0013); `terrain` scenario, 30 minutes with 16 bots and zero false corrections (docs/design/terrain.md).
- 2026-10-26 **Godot client skeleton**: `client/` project; `Session` (Gzip, per-profile token, create, enter with the content hash); predicted player with movement events and snapping on corrections; C# subscription director; remote capsules drawn 200 ms back, interpolated then dead-reckoned (ADR 0015); HUD, autopilot, `cargo xtask client` and `capture`; golden vectors from `sim`; 27 xUnit tests; `bot visitor`. Two windows and a bot saw each other with zero corrections; frames in `docs/captures/` (docs/design/client.md).
- 2026-10-26 **Spike 3: ship and moving deck**: `content/ships.toml` hulls, `sim::ship` (poses, step, draft, cannon flight), ships stepped in `world_tick` and published at 5 Hz, passengers in the ship's frame, helm and cannons with scheduled impacts (ADR 0014); `ship` scenario with zero false corrections, 8 of 8 shots hitting, cheats caught (docs/design/ships.md).
- 2026-10-26 **Art style test scene**: offline `--style-test flat|gradient` scene with a procedural island on the 2 m grid, Gerstner water with depth colour and shore foam, procedural trees and rocks, and KayKit Adventurers (CC0) walking, running and idling; `--outline`, `--shot follow|wide`; assets in `client/assets/` with the xtask importing before each run (ADR 0016); captures in `docs/captures/` (docs/design/art.md). The decision itself is task 1.
- 2026-10-26 **Art look decision**: deferred until the game systems are built; simple placeholder geometry until then (user, ADR 0004 amended).
