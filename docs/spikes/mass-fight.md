# Spike 2: mass fight and send rates

Status: done, 2026-10-25. PLAN §11, M0 spike 2. Follows docs/spikes/throughput.md. Decision: ADR 0011.

## Question

The throughput spike showed crowds are limited by bandwidth: each client receives about 0.35 KB/s per player in view, so a 150-player crowd costs 53 KB/s against a 40 KB/s budget. How should movement reach clients so that a mass fight fits?

**Done when:** 150 bots in a 3×3-cell area stay at 40 KB/s or less per client, with reducer p99 at 100 ms or less.

## Method

Same tool and setup as spike 1: `loadbot swarm`, everything on the laptop, the database cleared before each run, a 60 s window, 30% of bots in combat.
- **"Crowd 150":** 150 bots in a 200 m square (about 3×3 cells), so everyone sees everyone.
- **"Crowd 500":** 500 bots in 300 m, the worst case.

Changes were tried one at a time:

1. **Per move** (spike 1's baseline): every accepted `move_event` writes `transform`. One transaction, and one message to each observer, per move.
2. **Per tick:** `move_event` writes only the private `presence` row and a `moved` marker. `world_tick` publishes all moved positions to `transform` in one transaction, so each observer gets one message per tick.
3. **Compression:** the server compresses messages over about 1 KiB. Runs compare the SDK's default (Brotli) with Gzip and with none (`--compression`).

## Results

**Crowd 150:**

| Publishing | Compression | Received per client, mean / max | Reducer p99 | Server CPU | Loadbot CPU | Tick (+ subscription update) |
|---|---|---|---|---|---|---|
| per move | Brotli (unused: messages too small) | 52.8 / 57.3 KB/s | 3.0 ms | 1.19 cores | 1.33 cores | 36 µs |
| per tick | Brotli | 18.4 / 20.3 KB/s | 29 ms | 0.65 cores | **8.50 cores** | 212 (+272) µs |
| per tick | none | 38.7 / 42.9 KB/s | 3.2 ms | 0.37 cores | 0.28 cores | 219 (+271) µs |
| **per tick** | **Gzip** | **19.2 / 21.4 KB/s** | **3.0 ms** | **0.43 cores** | **0.33 cores** | — |

**500 bots, per tick with Gzip:**

| Run | Received per client, mean / max | Reducer p99 | DB busy | Tick p99 | Server CPU / RSS | Loadbot CPU |
|---|---|---|---|---|---|---|
| crowd, 300 m (spike 1: 124 / 182 KB/s, p99 12.9 s, 9 cores, 4.2 GB) | **43.5 / 57.8 KB/s** | 10.3 ms | 17.0% | ≤ 5 ms (mean 0.6 ms + 1.1 ms update) | 2.19 cores / 826 MB | 3.16 cores |
| spread, 2 km (spike 1: 6.2 / 9.2 KB/s) | 6.0 / 8.4 KB/s | 5.0 ms | 12.5% | ≤ 1 ms | 0.86 cores / 641 MB | 0.36 cores |

- Zero corrections and zero reducer errors in every run.
- **Batching cut a crowd's bytes by about two thirds.** Hundreds of small messages per second became 10 larger ones, and larger messages can be compressed. Spread-out play hardly changes, because few players are in view each tick.
- **Brotli is expensive to decode in the Rust SDK.** It cost about 55 ms of CPU per second per client at under 20 KB/s, enough that 150 bots fell behind and reducer p99 rose to 29 ms. Gzip was within 5% of Brotli's size at a small fraction of the cost. The C# client decodes with .NET's native code, so its Brotli cost may be lower, but Gzip's size is close enough that it isn't worth the risk.
- **What the change costs:** observers see moves 0–100 ms later, and the tick now does real work (0.6 ms mean for 500 players) against its 20 ms budget.

## Verdict and policy

**Go:** a 150-player fight costs each client about 19 KB/s, under half the budget, with reducer p99 at 3 ms. Even 500 players all in view come close: 44 KB/s on average, about 10% over.

**Send-rate and level-of-detail policy (PLAN §4.3, ADR 0011):**
1. **Within the Near tier (5×5 cells), positions are published once per 10 Hz tick, batched.** Players send move events plus a 2 Hz heartbeat, as before.
2. **Clients ask for Gzip.**
3. **Rules read positions from `presence`; displays read `transform`.**
4. **Not built yet, the next levers for siege-sized crowds (M3), in order:**
   - **Outer ring at 2 Hz.** A second public table, written every fifth tick. Clients subscribe to the inner 3×3 cells in `transform` and the outer 16 in the slower table. Estimate: halves a 500-player crowd.
   - **A smaller `transform` row.** It's 41 bytes now; positions relative to the cell as i16 centimetres, with yaw, speed and mode packed, would bring it to about 20.
   - **A cap on how many players are shown at full rate.** The nearest N at full rate, the rest at the outer ring's rate.

   M3's large-battle tests decide which of these are needed, measured with bots on a second machine.

## Reproduce

```
cargo xtask server                       # terminal 1
cargo xtask publish --clear
cargo xtask bot swarm --bots 150 --area 200 --secs 60                 # Gzip, as the client
cargo xtask bot swarm --bots 150 --area 200 --secs 60 --compression brotli
```
