# Status

**Milestone:** M0, Foundations and risk spikes. **Updated:** 2026-10-25.

## Where things stand
- **Server, working end to end on a local SpacetimeDB 2.13.0:**
  - accounts and characters
  - entering and leaving the world
  - movement events checked against speed limits, with corrections and a suspicion log
  - 64 m cell keys and Near-tier subscriptions, one `cell = key` query per cell (the throughput spike showed range queries cost 7× more)
  - the dev test clock
  - the 10 Hz `world_tick` with a measured, capped `dt` (it steps nothing yet)
  - server timings read from the host's Prometheus endpoint by `cargo xtask bot metrics` (ADR 0010). An idle tick costs about 90 µs of CPU.
  - dev-only `dev_teleport`, and `dev_swing` with a `combat_event` event table, as a stand-in for combat in load tests
- **Throughput spike done** (docs/spikes/throughput.md): go for 500 players spread out on this laptop, using about 1 core and 0.6 GB. Crowds are limited by bandwidth: about 0.35 KB/s per visible player, so more than about 110 in view goes over 40 KB/s. That's spike 2's job (BACKLOG 1).
- **Tests:**
  - 17 `sim` unit tests, 26 module tests (in-memory datastore) and 3 loadbot tests, with clippy clean
  - the `see-each-other` and `tick` bot scenarios, which pass against a live server, plus `swarm` for load
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
1. BACKLOG 1: spike 2, mass fight and send rates. It settles how movement reaches clients, so it comes before the client's net layer.
2. The Godot client skeleton (BACKLOG 6) is unblocked.

## Blockers and open questions
- **Art look:** decided after the style test (ADR 0004).
- **PLAN Q3:** anything from the final version to leave out or bring forward? Still open.
- **Crowd measurements need a second machine for the bots.** Spread-out runs fit on the laptop (500 bots used 0.5 cores and 0.4 GB). In a 500-player crowd, though, the bots needed 12.6 cores, which skews the latency numbers. The Windows PC could host the bots once it has the toolchain (BACKLOG 10).
