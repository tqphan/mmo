# 0010: Server timings come from the host's Prometheus metrics

- **Status:** Accepted, 2026-10-25
- **Context:**
  - BACKLOG 2 asked for a private `metrics` table holding tick-duration p50/p99 and reducer counts.
  - A SpacetimeDB 2.13 wasm reducer can't read a clock while it runs. `ctx.timestamp` is fixed for the transaction, `LogStopwatch` only writes to the module log, and the clock-reading syscall exists only for unstable procedures and is marked for removal. So the module can't time its own tick.
  - The host already times every reducer, per database: CPU-time and wall-time histograms, transaction counts, and scheduler delay. It serves them at `GET /v1/metrics`.
  - Every tick is a transaction, and every row it writes goes into the commit log. A stats row rewritten 10 times a second would add bulk to the commit log and tell us nothing the host doesn't already know.
  - SpacetimeDB 2.13 parses `#[reducer(update)]` but doesn't treat it as a lifecycle reducer. It becomes an ordinary reducer that any client can call.
- **Decision:**
  - There's no metrics table. `loadbot metrics` reads the host's Prometheus endpoint, picks out our database by its identity, and reports tick timings, scheduler delay, and per-reducer calls and mean CPU time.
  - The module keeps only what the tick needs anyway: the private `tick_state` row, with the tick count, the last tick time and the last `dt`. A dev-only `dev_tick` view exposes it.
  - The tick is started from `init` and from `on_connect`, not from an `update` reducer.
- **Consequences:**
  - Percentiles are bounded by the host's buckets. They're fine at the 1–10 ms scale but coarse between 10 and 50 ms. If the throughput spike lands near the 20 ms budget, it samples `_sum` and `_count` every second instead.
  - `/v1/metrics` has no authentication on the standalone server. Keeping it off the public internet becomes a production deployment requirement.
  - If a later SpacetimeDB version gives reducers a clock or a real update hook, we revisit this as part of that upgrade's ADR.
