# 0011: Movement is published once per tick, and clients use Gzip

- **Status:** Accepted, 2026-10-25
- **Context:**
  - The throughput spike wrote every accepted `move_event` straight into the public `transform` table. Each move was then its own transaction, and each observer got a separate small message for it.
  - A 150-player crowd cost each client 53 KB/s, against a budget of 40.
  - A 500-player crowd cost 124 KB/s per client, and the server used 9 cores with 4.2 GB of queued messages.
  - Messages under about 1 KiB aren't compressed, so per-message overhead dominated.
- **Decision:**
  - `move_event` checks the move against the authoritative position in the private `presence` row and updates only that row, plus a private `moved` marker.
  - `world_tick` copies every moved position into `transform` in one transaction every 100 ms.
  - The mover's own client reads its authoritative position and correction count from `my_presence` at once. Everyone else sees moves up to 100 ms later.
  - Clients ask the server for Gzip compression instead of the SDK's default, Brotli.
- **Consequences:**
  - **Smaller and cheaper (spike 2):**

    | Crowd | Per client before | Per client now | Reducer p99 now | Server CPU now |
    |---|---|---|---|---|
    | 150 players | 53 KB/s | 19 KB/s | 3 ms | 0.4 cores |
    | 500 players | 124 KB/s | 44 KB/s | 10 ms | 2.2 cores |

  - **Observers see positions 0–100 ms later.** Their interpolation buffer must hold at least two ticks. The mover isn't affected.
  - **The tick now does real work,** about 0.6 ms of CPU for 500 players, plus about 1 ms of subscription updates. Its 20 ms budget has plenty of room.
  - **Brotli costs too much to decode.** In the Rust SDK, Brotli decoding cost each client about 55 ms of CPU per second at 19 KB/s. Gzip was almost as small (19.2 vs 18.4 KB/s) at a small fraction of the cost.
  - **Anything that reads a character's position for game rules** reads `presence`, not `transform`. `transform` is only for showing other players.
