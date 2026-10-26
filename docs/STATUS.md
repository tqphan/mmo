# Status

**Milestone:** M0, Foundations and risk spikes. **Updated:** 2026-10-25.

## Where things stand
- **Server, working end to end on a local SpacetimeDB 2.13.0:**
  - accounts and characters
  - entering and leaving the world
  - movement events checked against speed limits, with corrections and a suspicion log
  - 64 m cell keys and Near-tier window queries
  - the dev test clock
  - the 10 Hz `world_tick` with a measured, capped `dt` (it steps nothing yet)
  - server timings read from the host's Prometheus endpoint by `cargo xtask bot metrics` (ADR 0010). An idle tick costs about 90 µs of CPU.
- **Tests:**
  - 16 `sim` unit tests, 24 module tests (in-memory datastore) and 2 loadbot tests, with clippy clean
  - the `see-each-other` and `tick` bot scenarios, which pass against a live server
- **Dev tasks:** `cargo xtask` (see CLAUDE.md, Commands).
- **No client yet.** Godot and .NET aren't installed on the Linux machine (BACKLOG 1). C# bindings are generated into `client/net` but nothing uses them.

## Machines
| | Linux laptop (Arch) | Windows PC |
|---|---|---|
| Hardware | 32 threads, 14 GB RAM, RTX 5070 Ti Mobile + Radeon 610M | Ryzen 9 7900X, 32 GB, RX 6950 XT |
| Rust | 1.99.1 + wasm32 target | 1.90, wasm32 target missing |
| SpacetimeDB CLI | 2.13.0 (`~/.local/bin`, not on PATH) | 1.10, needs upgrading |
| .NET / Godot / Blender | none | .NET 7 and 10 / none / none |

## Next
1. BACKLOG 1 needs the user's OK. Until then, carry on with server-side tasks 2–6.
2. BACKLOG 2: the throughput spike (`loadbot swarm`), using `loadbot metrics` for the server-side numbers.

## Blockers and open questions
- **Client toolchain on Linux:** not approved yet (user chose server-only for now).
- **Art look:** decided after the style test (ADR 0004).
- **PLAN Q3:** anything from the final version to leave out or bring forward? Still open.
- **Throughput spike:** this laptop has 14 GB RAM, which may be too little for 500 bots plus the server. The spike may have to run on the Windows PC.
