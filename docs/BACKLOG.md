# Backlog

In order. Each task should fit in one session and has acceptance criteria. Tasks marked **[needs OK]** wait for a user decision. Done tasks move to the bottom with their date.

## M0: Foundations and risk spikes

1. **Spike 2: mass fight and send rates** (PLAN §11)
   The throughput spike found crowds cost about 0.35 KB/s per visible player, so the 40 KB/s budget runs out at about 110 (docs/spikes/throughput.md). Try, in order: batching movement into `world_tick` (one message per tick), distance-based send rates, a smaller `transform` row, and a cap on how many players are shown at full rate.
   *Done when:* `loadbot swarm --bots 150 --area 200` stays at 40 KB/s or less per client with reducer p99 at 100 ms or less, and the send-rate and level-of-detail policy is written into `docs/spikes/mass-fight.md` and PLAN §4.3.

2. **Cell switching with hysteresis**
   A subscription director in `loadbot` (later mirrored in C#): subscribe to the new window before dropping the old one, with a hysteresis margin of about 8 m. The swarm already does subscribe-then-drop, without hysteresis; move it into a shared director.
   *Done when:* a `cross-cells` scenario walks 300 m through several cells with no gap in what it sees and no resubscribe flapping at a border.

3. **`contentgen` v0**
   TOML in `content/` → Rust statics plus C# data plus `CONTENT_HASH`. Starts with the movement-mode table (it moves out of `sim::movement`).
   *Done when:* the generated code compiles in both languages, the hash mismatch path is tested, and the speed limits come from content.

4. **Spike 5: server-side terrain**
   A private `terrain_chunk` table holding a u16 heightmap at 2 m resolution, uploaded by an admin reducer from a file. Add a vertical check and a line-of-sight check in `sim`.
   *Done when:* tests in `sim` cover the slope, falling and line-of-sight cases, and a 30-minute bot walk over procedural test terrain produces zero false corrections.

5. **Spike 3: ship and moving deck (server)**
   A server-driven clipper (throttle and rudder) moving in `world_tick`, with seats and players in ship-local coordinates (`transform.parent`).
   *Done when:* a scenario has 4 bots walking on deck while the ship turns, with no false corrections, and a cannon impact scheduled as a timer.

6. **Godot client skeleton**
   Project in `client/`, net layer, enter the world, a predicted CharacterBody3D with heartbeats, other entities shown as capsules, and snapping on correction.
   *Done when:* two client windows plus a bot see each other, and Movie Maker frames are captured.

7. **Art style test scene** (needs 6)
   Stylized terrain, water, and a CC0 character walking (ADR 0004).
   *Done when:* the user has reviewed captures and decided flat low-poly vs hand-painted.

8. **Spike 4: world scale** (needs 6): a 16 km Terrain3D map with water and streaming, within budget at mid-range settings.
9. **World exporter v0** (needs 6): Godot plugin → heightmap and land-use raster → `terrain_chunk` upload.
10. **Verify on the Windows PC**: upgrade the SpacetimeDB CLI to 2.13.0, add the wasm target, and check that `cargo xtask check` passes.

## Research backlog
- Original on-foot speeds (walk, run, swim, sprint) and the pack-carrying slowdown. Needed before M1 tuning (docs/design/movement.md §1).

## Done
- 2026-10-25 **Repository scaffold**: STATUS, BACKLOG, ADRs 0001–0009, docs folders, asset ledger.
- 2026-10-25 **Server skeleton and dev tasks**: Cargo workspace, `world` module, `sim` crate, `cargo xtask` (test/build/server/publish/generate/bot/check).
- 2026-10-25 **Sessions, movement, cells (server side)**: accounts, characters, enter and leave, checked movement events with corrections, cell keys, Near-tier window queries.
- 2026-10-25 **Test clock and loadbot skeleton**: `now(ctx)` with a dev offset and `dev_advance_clock`; the `loadbot see-each-other` scenario passes against a live server.
- 2026-10-25 **World tick and server metrics**: 10 Hz `world_tick` with a measured, capped `dt`; timings from the host's `/v1/metrics` via `loadbot metrics` instead of a module table (ADR 0010); `tick` scenario.
- 2026-10-25 **Client toolchain on the Linux machine**: .NET SDK 8.0.132 and 10.0.113 (pacman), Godot 4.7.2 .NET in `~/.local`; `cargo xtask generate` formats the C# bindings; a throwaway Godot project with `SpacetimeDB.ClientSDK.Godot` 2.13.0 built and connected headless.
- 2026-10-25 **Spike 1: throughput**: `loadbot swarm`; go for 500 players spread out (DB 13% busy, reducer p99 2.7 ms, busiest client 9.2 KB/s), no-go for crowds above about 110 in view (bandwidth); Near tier now subscribed cell by cell, which cut database time 7× (docs/spikes/throughput.md).
