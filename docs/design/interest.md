# Interest management: cells, tiers and the subscription director

Status: v0.1, 2026-10-25. M0 slice. Code: `server/sim/src/cell.rs`, `sidecars/loadbot/src/director.rs`, and the C# mirror in `client/src/Net/SubscriptionDirector.cs` (docs/design/client.md).

## 1. How the original worked

The original's interest management isn't public. Players saw other characters out to roughly a couple of hundred metres, and ships and some large objects much further, which is where the Far tier comes from (PLAN §4.3). Confidence C. Players never see the mechanism, so there's nothing to copy; this is our architecture.

## 2. Our rules

### Cells
- The map is 64 m cells, 250×250, keyed by `(instance, cy, cx)` packed into a `u64` (`sim::cell`, PLAN §4.3).
- Every public table with a world position has an indexed `cell` column.
- Clients subscribe with one `cell = key` query per cell and table. Range queries cost 7× more database time (docs/spikes/throughput.md).

### Tiers
Self, Near, Static, Far and Global, as in PLAN §4.3. Only Self and Near exist so far. Each windowed tier (Near, Static, Far) gets its own director with its tables and radius. Self and Global are fixed subscriptions made once.

### The director
- **One subscription per cell**, covering all of the tier's tables for that cell. When the window moves one cell, only the 5 new cells of a 5×5 window are fetched, and the 20 it already has stay as they are. Resubscribing the whole window would resend all 25 cells' rows.
- **New before old.** Cells that leave the window are unsubscribed only once every cell of the new window is applied, so nothing in reach ever drops out of view. A cell that leaves and comes back before that is never dropped.
- **Hysteresis.** The window is centred on a cell and stays there until the player is more than `HYSTERESIS_M` = 8 m outside that cell on either axis (`sim::cell::recenter`). Then it re-centres on the cell the player is in. Walking along a border, or pacing back and forth over one, costs nothing.
- **What's guaranteed in view:** the window reaches at least `radius × 64 − 8` m past the player on each axis: 120 m for the Near tier.
- **It follows the predicted position.** The client calls it with its own position after each move, not with what the server echoes back.
- A non-finite position leaves the window where it is.

## 3. Deviations and not yet done
- **Instances:** the director works in the open world only. Dungeons and arenas will pass their instance id when they exist.
- **Static, Far and Global tiers** come with the systems that need them (housing, ships, world state).
- **The siege lever** (outer ring at 2 Hz, PLAN §4.3) isn't built.

## 4. Test plan
- **`sim` unit tests** (`cell.rs`): the window stays inside the margin and moves past it, on both axes and diagonally; zigzagging over a border never moves it, and coming back over the border after it moved doesn't move it back; the instance is kept; NaN and infinity keep the window; off the map the window stays on the edge cell.
- **`cargo xtask bot cross-cells`** against a live server. Five bots stand along a route and one stands out of reach. A walker runs 290 m east at run speed through five column borders, zigzagging ±4 m across a row border the whole way, then paces twice to 6 m back over the last column border. It checks 10 times per heartbeat that:
  - every standing bot within 110 m (Chebyshev distance) is in view, which is the 120 m guarantee less slack for subscription latency
  - the one out of reach never appears
  
  At the end it checks that the window moved exactly 5 times, that the cells behind were dropped and the first bot left the cache, that nothing outside the window is cached, and that there were no corrections.
- **The client's C# director** (`client/tests/SubscriptionDirectorTests.cs`) runs the same rules against a fake server. Its `recenter` and `window_cells` match `sim` on the generated vectors in `client/tests/vectors/sim.txt`.
- **`swarm`** uses the director. With 500 bots spread over 2 km, mean bandwidth per client went from 6.0 to 4.9 KB/s and server CPU from 0.86 to 0.71 cores, compared with resubscribing the whole window on every cell change (3028 window moves in the run).
