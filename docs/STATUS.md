# Status

**Milestone:** M0, Foundations and risk spikes. **Updated:** 2026-10-25.

## Where things stand
- **Server, working end to end on a local SpacetimeDB 2.13.0:**
  - accounts and characters
  - entering and leaving the world
  - movement events checked against speed limits, with corrections and a suspicion log. The authoritative position is in private `presence`, and the tick publishes moves to `transform` once per 100 ms (ADR 0011).
  - 64 m cell keys and Near-tier subscriptions, one `cell = key` query per cell (the throughput spike showed range queries cost 7× more)
  - a subscription director (loadbot for now): one subscription per cell, new cells before old are dropped, and the window moves only 8 m past its centre cell (docs/design/interest.md)
  - the dev test clock
  - the 10 Hz `world_tick` with a measured, capped `dt` (it steps nothing yet)
  - server timings read from the host's Prometheus endpoint by `cargo xtask bot metrics` (ADR 0010). An idle tick costs about 90 µs of CPU.
  - game data from `content/` through `contentgen` (docs/design/content.md, ADR 0012): the movement-mode table, generated into Rust (`sim::content`) and C# (`client/content`), with a `CONTENT_HASH` that `enter_world` checks
  - dev-only `dev_teleport`, and `dev_swing` with a `combat_event` event table, as a stand-in for combat in load tests
- **Spikes 1 and 2 done** (docs/spikes/throughput.md, mass-fight.md):
  - 500 players spread out: about 1 core and 0.6 GB on this laptop, 6 KB/s per client.
  - With batched publishing and Gzip, a 150-player fight costs each client about 19 KB/s (reducer p99 3 ms), and 500 all in view about 44 KB/s.
  - The levers for siege-sized crowds are written down for M3.
- **Tests:**
  - 20 `sim` unit tests, 9 `contentgen` tests, 28 module tests (in-memory datastore) and 3 loadbot tests, with clippy clean; the generated C# compiles with warnings as errors
  - the `see-each-other`, `tick` and `cross-cells` bot scenarios, which pass against a live server, plus `swarm` for load
- **Dev tasks:** `cargo xtask` (see CLAUDE.md, Commands).
- **No client yet, but the toolchain works on Linux.** A throwaway Godot 4.7.2 C# project using `SpacetimeDB.ClientSDK.Godot` 2.13.0 and our generated bindings built cleanly, connected to the local server and read `dev_tick` (headless). The SDK declares GodotSharp 4.6.2; NuGet resolves it to 4.7.2 without complaint.

## Machines
| | Linux laptop (Arch) | Windows PC |
|---|---|---|
| Hardware | 32 threads, 14 GB RAM, RTX 5070 Ti Mobile + Radeon 610M | Ryzen 9 7900X, 32 GB, RX 6950 XT |
| Rust | 1.99.1 + wasm32 target | 1.90, wasm32 target missing |
| SpacetimeDB CLI | 2.13.0 (`~/.local/bin`, not on PATH) | 1.10, needs upgrading |
| .NET | SDK 8.0.132 and 10.0.113 (pacman, `/usr/share/dotnet`) | 7 and 10 |
| Godot | 4.7.2 .NET (`~/.local/opt`, linked as `~/.local/bin/godot`) | none |
| Blender | none | none |

## Next
1. Spike 5, server-side terrain (top of BACKLOG).
2. The Godot client skeleton is unblocked. Its net layer follows ADR 0011 (Gzip, snap from `my_presence`, buffer two ticks), ports the subscription director, and sends `GameContent.Hash` on entering.

## Blockers and open questions
- **Art look:** decided after the style test (ADR 0004).
- **PLAN Q3:** anything from the final version to leave out or bring forward? Still open.
- **Crowd measurements need a second machine for the bots.** Spread-out runs fit on the laptop (500 bots used 0.5 cores and 0.4 GB). Crowds are heavier for the bots: 500 in one place took 3 cores even after spike 2. The Windows PC could host the bots once it has the toolchain (the Windows task in BACKLOG). This matters for M3's large battles.
