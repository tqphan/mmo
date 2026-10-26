# Spike 1: throughput

Status: done, 2026-10-25. PLAN §11, M0 spike 1. Code: `sidecars/loadbot/src/swarm.rs`, `server/world/src/dev.rs`.

> **Later:** spike 2 (docs/spikes/mass-fight.md, ADR 0011) batched movement into the tick and switched clients to Gzip. That fixed the crowd result: 150 in view now costs about 19 KB/s, and 500 about 44 KB/s. The numbers below are from before that change.

## Question

Can one SpacetimeDB 2.13 instance on this laptop carry 500 players moving and fighting?

**Go if:**
- the tick takes 20 ms or less (p99)
- reducer latency is 100 ms or less (p99)
- each client in a crowd receives about 40 KB/s or less

The spike also measures CPU and memory per player, to size the production server.

## Method

`loadbot swarm --bots N --combat 0.3 --area M --secs 60`, release build, against `cargo xtask server` on the same laptop. The database is cleared before each run.

- **Bots:** every bot has its own WebSocket connection, and all of them run as tasks on one tokio runtime.
  - Each bot creates a character, enters the world and teleports (`dev_teleport`) to a random spot in an M × M square around the origin.
  - It then runs at 95% of run speed, turning a little each step, and turns back inward at the square's edge.
  - It sends a `move_event` every 0.5 s, with the stride sized by the time that actually passed. When the server corrects it, it snaps back.
  - It keeps a Near-tier subscription (5×5 cells: `entity`, `transform`, `combat_event`) plus `character_public` and its own views. When it changes cell, it subscribes to the new window first and drops the old one once that is applied.
- **"In combat":** 30% of bots move every 0.25 s and, once a second, swing at the nearest bot within 20 m.
  - The swing calls the dev-only stand-in `dev_swing`. It checks range and inserts a row into the public `combat_event` event table at the attacker's cell, shaped like PLAN §5.2's `combat_event`.
  - Real combat will also write health and buffs, so this underestimates combat's write load. It does exercise the broadcast path.
- **Warm-up and window:** a 10 s warm-up once every bot is in, then a 60 s measured window. Every figure is the difference between readings at the window's start and end.
- **Where each number comes from:**

| Number | Source |
|---|---|
| Tick time, reducer CPU, database busy | The host's `/v1/metrics` (ADR 0010): `spacetime_txn_cpu_time_sec` by reducer and transaction type |
| Reducer latency | In the bots: from calling a reducer to its completion notice arriving, which comes in the same message as the row updates |
| Bytes per client | `ss -tinp`: `bytes_received` on each bot's TCP socket. These are wire bytes, after the default Brotli compression |
| Server CPU and RSS | `/proc/<pid>/stat` and `status` of `spacetimedb-standalone` |

- **"Database busy"** is the CPU time of all transactions per second of wall time. Writing transactions run one at a time, so this is the headroom that matters. The tick still steps nothing, so its own timing says little yet.

## Results

Laptop: 32 threads, 14 GB RAM, with the server and every bot on the same machine. "Spread" means the bots were scattered over the square, so each one sees about a dozen others. "Crowd" means everyone sees nearly everyone.

| Run | Bots (combat) | DB busy | Tick p99 | Reducer latency p50 / p99 | Received per client, mean / max | Server CPU | Server RSS | Loadbot CPU |
|---|---|---|---|---|---|---|---|---|
| spread, 2 km | 100 (31) | 2.8% | ≤ 100 µs | 0.9 / 2.3 ms | 2.1 / 3.4 KB/s | 0.18 cores | 476 MB | 0.03 cores |
| spread, 2 km | 250 (75) | 6.1% | ≤ 100 µs | 1.1 / 2.2 ms | 3.6 / 5.7 KB/s | 0.45 cores | 503 MB | 0.12 cores |
| **spread, 2 km** | **500 (150)** | **13.1%** | **≤ 100 µs** | **1.5 / 2.7 ms** | **6.2 / 9.2 KB/s** | **1.03 cores** | **583 MB** | 0.46 cores |
| crowd, 200 m | 150 (45) | 5.5% | ≤ 100 µs | 1.3 / 3.0 ms | **52.8 / 57.3 KB/s** | 1.19 cores | — | 1.33 cores |
| crowd, 300 m | 500 (150) | 60.1% | ≤ 500 µs | 2.5 s / 12.9 s | **124 / 182 KB/s** | 8.98 cores | 4.2 GB | 12.6 cores |

- **No false corrections.** Zero corrections in every run, so the movement check holds up under load.
- **No reducer errors,** except 1,365 out-of-range swings in the 500-bot crowd, where positions were seconds stale.
- **500 spread, breakdown of the 13.1% busy:** reducers 7.8% (`move_event` 60 µs mean), subscription updates 2.4%, and subscribing plus unsubscribing on cell changes 2.9%.
- **The empty tick** costs 30–80 µs. This tells us nothing about the tick budget until systems run in it.
- **Server memory:** about 480 MB idle, plus about 0.19 MB per player (spread, 500). The crowd run's 4.2 GB was outgoing messages queued for bots that couldn't keep up.
- **Range vs equality subscriptions (250 spread, same load):** the first version subscribed to the Near tier with one range query per row (`cell >= lo AND cell <= hi`). The database was 44.7% busy, 39.9% of it subscription updates, and it would have been saturated at 500. With one `cell = key` query per cell it was 6.1%.
  - Reason: SpacetimeDB 2.13 prunes subscriptions only through equality filters (`SearchArguments` in `module_subscription_manager.rs`). Every range subscription is checked against every change, so the cost grew with players × moves per second.
  - Clients now subscribe cell by cell (`sim::cell::window_cells`; PLAN §4.3, docs/design/movement.md).

## Verdict

**Go for 500 players spread over the world.**
- Tick time: about 0.1 ms against 20 ms.
- Reducer p99: 2.7 ms against 100 ms.
- Busiest client: 9.2 KB/s against 40 KB/s.
- The database is 13% busy and the server uses about one core.

**No-go for big crowds at today's send rates.** Bandwidth grows with the number of players in view: about 0.35 KB/s each, so the 40 KB/s budget runs out at about 110 visible players.
- In the 150-player crowd, the server was fine (5.5% busy, p99 3 ms), but each client received 53 KB/s.
- In the 500-player crowd, the bots needed 12.6 cores just to read their updates, so the latency figures there measure the bots more than the server. The bandwidth figure stands either way.

## Mitigation (goes into spike 2, the mass fight)

Each changed row costs about 120 bytes on the wire, because every `move_event` is its own transaction and its own message, and messages under 1 KiB aren't compressed. Ways to bring it down, roughly in order of expected gain:

1. **Batch movement into the tick.** `move_event` checks the move and keeps it in a private table; `world_tick` copies changed positions into `transform` 10 times a second. Each client then gets one larger message per tick, which is worth compressing, instead of hundreds of small ones. Latency rises by up to 100 ms for observers only; the mover predicts.
2. **Send less often for players further away.** Full rate within about 30 m, then 2 Hz, then 1 Hz at the edge of the Near tier. This needs per-distance tiers, for example a near ring and a far ring of cells.
3. **A smaller `transform` row.** Positions relative to the cell as i16 centimetres, yaw as u8, and speed and mode packed together.
4. **A cap on how many players are shown.** Above a threshold, show the nearest N at full rate and the rest at a low rate.

Spike 2 measures 150 bots in a 3×3-cell area with these changes and settles on the send-rate policy. Crowd measurements need the bots on a second machine. Even at 150 the bots used more than a core, and the 500-player crowd overloaded the laptop.

## Production sizing (first estimate)

For 500 players spread out, expect about 1 core and 0.6 GB for the database. That leaves plenty of room, so a 4-core, 8 GB server is enough for now. Recheck once real combat, NPCs and ships run in the tick.

## Reproduce

```
cargo xtask server                       # terminal 1
cargo xtask publish --clear
cargo xtask bot swarm --bots 500 --area 2000 --secs 60
cargo xtask bot swarm --bots 150 --area 200 --secs 60
```
