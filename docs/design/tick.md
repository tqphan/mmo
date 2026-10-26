# World tick and server metrics

Status: v0.1, 2026-10-25. M0 slice. Code: `server/sim/src/tick.rs`, `server/world/src/tick.rs`, `sidecars/loadbot/src/metrics.rs`.

## 1. How the original worked

The original's server internals aren't public. All we know is how it behaved: ships, carts and NPCs moved continuously, buffs and damage over time ticked, and crop growth, labor regeneration and trade demand moved on with wall-clock time even while nobody was nearby. That's why the plan steps only continuous motion in a tick and computes everything else from timestamps (PLAN §4.4). Confidence C. There's nothing to copy here; it's our architecture.

## 2. Our rules

### The tick
- `world_tick` is a scheduled reducer on the private `world_tick_timer` table, with `ScheduleAt::Interval(100 ms)`, so it runs at 10 Hz.
- **Starting it:** `init` inserts the timer row if there is none. SpacetimeDB 2.13 has no lifecycle reducer that runs on republish, so `on_connect` does the same check. A database created before the tick existed starts it when the first client connects.
- **Clients can't call it.** It returns `Err` unless `ctx.sender()` is the database's own identity.
- **Measured `dt`:** the private single-row `tick_state` holds `ticks`, `last_at` (game time, from `now(ctx)`) and `last_dt`. Each tick computes `dt = sim::tick::step_dt(now − last_at)`:
  - The first tick steps 0, since it has nothing to measure from.
  - `dt` is capped at `MAX_DT` = 0.25 s. A stalled server, a restart or a dev-clock jump makes the world run slow for a moment instead of jumping. Anything that must catch up exactly is computed from timestamps, never ticked.
  - Negative elapsed time gives 0.
- **What it steps:** nothing yet. Ships arrive in M0 spike 3, followed by NPCs in active cells, due buffs and regeneration in combat. The budget is 20 ms per tick (PLAN §4.4).
- **Dev view:** `dev_tick` (dev builds only) returns the `tick_state` row, so bots can check the rate.

### Metrics
- **Reducers have no clock.** A wasm reducer can't read the time while it runs: `ctx.timestamp` is fixed for the whole transaction, and the only timer, `LogStopwatch`, writes to the log. So the module can't time itself (ADR 0010).
- **The host measures every reducer.** Its Prometheus endpoint `GET /v1/metrics` reports, per database and reducer:
  - `spacetime_txn_cpu_time_sec` (histogram) with `txn_type="Reducer"`: the reducer itself. With `txn_type="Update"`: working out what subscribers receive afterwards.
  - `spacetime_scheduled_function_delay_seconds{function="world_tick"}`: how late the scheduler started each tick.
  - `spacetime_num_txns_total`: transaction counts.
- **Reading them:** `loadbot metrics` (or `cargo xtask bot metrics`) looks up the database's identity through `GET /v1/database/<name>` and prints:
  - the tick's mean CPU time and its p50 and p99
  - the mean cost of the subscription update after each tick
  - the scheduler delay
  - calls and mean CPU time for every reducer
- **Percentiles are bucket bounds.** The host's buckets are 10 µs, 50 µs, 100 µs, 500 µs, 1 ms, 5 ms, 10 ms, 50 ms, … The report gives the upper bound of the bucket holding the percentile ("p99 ≤ 500 µs"). Means are exact.

## 3. Deviations and not yet done
- **No `metrics` table.** The backlog asked for a private metrics table. Its timings would have had to come from the host anyway, and writing a stats row every tick would grow the commit log for no gain (ADR 0010).
- **Coarse percentiles between 10 and 50 ms.** These buckets straddle the 20 ms budget. If the throughput spike lands there, sample `_sum` and `_count` every second and use the per-second means.
- **`/v1/metrics` isn't authenticated** on the standalone server. Production must not expose it publicly; that's a deployment task in M5.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| sim | `step_dt`: the measured time, the cap for long gaps, 0 for zero or negative elapsed time | `server/sim/src/tick.rs` |
| module | `init` starts exactly one 100 ms interval timer, and connecting afterwards doesn't add another; connecting starts it on an older database | `server/world/src/tests.rs` |
| module | Ticks count up; the first steps 0, then 0.1 and 0.137 s as measured; a 5 s stall steps `MAX_DT`; a client calling `world_tick` gets `Err` and changes nothing | same |
| module (dev) | A one-day dev-clock jump steps `MAX_DT` once, then normal again; `dev_tick` returns the state | same |
| loadbot | Prometheus text parsing, histogram quantile bounds and mean | `sidecars/loadbot/src/metrics.rs` |
| scenario | Against a live server: the tick rate is 8–12 Hz over 3 s, `last_dt` is about 0.1 s, and host timings for `world_tick` can be read and printed | `cargo xtask bot tick` |
