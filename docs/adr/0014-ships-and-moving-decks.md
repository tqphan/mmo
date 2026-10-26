# 0014: Ships are stepped by the server; passengers move in the ship's frame

- **Status:** Accepted, 2026-10-26
- **Context:** Spike 3 (PLAN §8.4, M0) needs players to walk on a ship while it sails and turns, without false corrections, and needs a cannon whose shot lands later. Checking deck walkers in world coordinates would mix the ship's motion into every move check and make the result depend on how stale each client's view of the ship is.
- **Decision:**
  - The server steps every ship in `world_tick` with `sim::ship::step` (throttle and rudder in, speed and yaw out). The helmsman's client and observers extrapolate with the same function, so prediction is deterministic between publishes.
  - The exact state is private (`ship_motion`). `transform` gets a rounded copy at 5 Hz, and when the ship changes cell. The helm's inputs and the ship's HP and reloads are in a public `vehicle` table, written only when they change.
  - `presence` and `transform` gain a `parent`: 0 for the world, or a ship's entity id. A passenger's position is in the ship's frame, and `move_event` names the frame the client used. Moves within one frame are checked exactly like walking on land, with the deck as flat ground. A move that changes frame is checked in world coordinates using the server's current pose, with `|ship speed| × 0.5 s` of extra allowance for the client's lag.
  - Passengers are published in their ship's cell, and move cells with it in the same tick.
  - Stepping off carries no momentum.
  - A cannon shot is a scheduled row, `cannon_shot`, due at the landing time from `sim::ship::cannon_flight`. Its impact tests ships' deck rectangles in their poses at that time.
  - Hulls come from `content/ships.toml` and are part of the content hash.
- **Consequences:**
  - Boarding and stepping off a fast ship leave a cheater about 6 m of slack. Walking on deck leaves none beyond the usual move checks.
  - Server ships don't roll or pitch. Waves are cosmetic, and the deck's height is constant.
  - The spike measured 5 Hz ship publishes, extrapolated with the shared step, at 0.2 m mean and 2.2 m worst error one second ahead. A straight line was 1.2 m mean. The client should use the shared step.
  - `presence`, `transform` and `move_event` changed shape, which needs a `--clear` publish and regenerated bindings.
