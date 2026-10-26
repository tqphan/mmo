# Ships and moving decks

Status: v0.3, 2026-10-26 (the client walks on decks and drives ships). M0 spike 3, then BACKLOG M0 task 2. Code: `content/ships.toml`, `server/sim/src/ship.rs`, `server/world/src/{ship,movement}.rs`, `sidecars/loadbot/src/ship.rs`; client `client/src/Sim/Ship.cs`, `World/{ShipPredictor,ShipView}.cs`, `Gameplay/{Boarding,ShipControls,PlayerController}.cs`. Decisions: ADR 0014, ADR 0019.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Players build ships and sail them; several players share one, walking about on deck while it moves | A | RESEARCH §2.7 |
| The crew splits into helm, cannons, repairs and harpoon; if the helmsman dies, the ship drifts | B | RESEARCH §2.7 |
| The clipper is the fastest hull at about 11.1 m/s with one cannon and 7,000–9,000 HP; the merchant schooner about 10 m/s with two cannons and 42,000 HP; the rowboat 4 m/s | B | RESEARCH §2.7, §2A.4 (fan wikis) |
| A clipper add-on cannon: 48 m range, 4 m splash, 700 damage, an 88 m/s shell, 3 s cooldown. Range was capped, not left to ballistics | B | RESEARCH §2A.4 (item database) |
| Cannons are manned by players, aimed within an arc on one side of the ship, and fire visible shots on an arc that land after a flight time | B | common player knowledge, RESEARCH §2.7 |
| Larger ships carry tens of thousands of HP, and ships are repaired at sea | B | RESEARCH §2.7 |
| Acceleration, turn rate, and the stock cannon's range, damage and reload | **Unknown** | searched 2026-10-26, not found |

Every hull and cannon number below is a placeholder, confidence C, in `content/ships.toml`. The speed matches the clipper's reported 11 m/s. Two known differences wait for M1 tuning: our clipper has 12,000 HP where the original had 7,000–9,000, and two cannons (spike 3 asked for two) where the original had one.

## 2. Our rules

### Coordinates
- **World:** x east, y up, z south (Godot's axes; -z is north). A **yaw** is a rotation about +y in Godot's sense: yaw 0 faces -z, and positive yaw turns left (counter-clockwise seen from above). On the wire, `yaw: u16` is `yaw_rad / 2π × 65536`, wrapping.
- **Ship-local:** origin at the centre of the deck, on its surface. -z is the bow, +x is starboard (right), +y is up. A ship's pose is its world position (`x`, `y` = deck height, `z`) and yaw.
- `local → world`: `wx = sx + lx·cos θ + lz·sin θ`, `wy = sy + ly`, `wz = sz − lx·sin θ + lz·cos θ`. That's Godot's `Basis(Vector3.UP, θ)`. Ships don't roll or pitch on the server; waves and roll are cosmetic (PLAN §4.2).

### Hulls (`content/ships.toml`)
One `[[hull]]` per ship type with a wire id, speeds, acceleration, turn rate, deck size, deck height, draft, HP, the helm's position, a `[hull.cannon]` table and one `[[hull.mount]]` per cannon (position on deck and facing). contentgen emits `sim::content::{Hull, HullData, HULLS}` and the C# equivalents, and the hulls are part of the content hash. The clipper: 11 m/s, 3 m/s astern, 1.5 m/s² acceleration, 0.8 m/s² drift deceleration, 12°/s turn rate, an 18 × 5 m deck 2 m above the water, 1.2 m draft, 12,000 HP, the helm at the stern and one cannon on each side.

### Ship motion (`sim::ship::step`)
- **State:** position, yaw and signed speed (m/s). **Input:** `throttle` and `rudder`, each −100 to 100 percent. The helmsman sets them and they persist.
- One step of `dt` seconds, in this order (the client predicts its own ship with the same function):
  1. **Speed** moves toward the target (`throttle% × top speed` ahead, `throttle% × astern speed` astern) by `accel × dt`, or by `drift decel × dt` when the throttle is 0.
  2. **Yaw** changes by `−rudder% × turn rate × authority × dt`, where `authority = min(1, |speed| / (30% of top speed))`, negated when going astern. A positive rudder turns to starboard. A ship barely moving barely turns.
  3. **Position** moves along the new yaw by `speed × dt`.
- **Draft:** after stepping, the bow, centre and stern are checked against the terrain. If the ground at any of them is above `sea level − draft`, the step is cancelled and the ship stops (speed 0) where it was. Unknown terrain counts as deep water.
- **Drift:** with nobody at the helm the input is zero, so the ship slows at its drift rate and holds its heading.
- Turning circle at full speed: `top speed / turn rate`, 52.5 m for the clipper.

### Tables
| Table | Visibility | Contents |
|---|---|---|
| `entity` | public | A ship is `kind = 2` with `ref_id` = hull wire id |
| `transform` | public | The ship's pose: deck height as `y`, `speed` in cm/s. Written at **5 Hz** (every other tick) and whenever the ship changes cell, only if it moved. Clients extrapolate with `sim::ship::step` and the input in `vehicle` |
| `vehicle` | public, by `cell` | `hull`, `hp`, `throttle`, `rudder`, `helm` (the helmsman's entity id, or 0), `cannon_ready_at` per mount. Written only when these change |
| `ship_motion` | private | The exact state (f32 yaw and speed) that the tick steps |
| `cannon_shot` | private, scheduled | One row per shot in flight, due when it lands |

### The tick
`world_tick` steps every ship by its measured `dt` before it publishes moves. When a ship changes cell, the ship's `entity`, `transform` and `vehicle` rows and every passenger's `entity` and `transform` rows move to the new cell in the same tick, so anyone who can see the ship can see who is on it.

### Moving decks
- `presence.parent` is 0 in the world or the ship's entity id on a deck. **The position, anchor and speed in `presence` are in the parent's frame.** `transform.parent` says the same to observers, and a passenger's `transform.cell` is the ship's cell.
- `move_event` gains a `parent` argument: the frame the client's position is in.
- **Same frame:** the existing checks run in that frame. On a deck the "ground" is the deck: flat at local y = 0 within the deck rectangle. Within 1.5 m outside it (leaning over the rail, stepping off) there is no ground and no vertical check. Further out is refused (`off the deck`). The ship's own motion never enters into it, so walking on a turning ship is checked exactly like walking on land.
- **Changing frame** (boarding, stepping off, jumping between ships): both positions are converted to world coordinates with the ships' current server poses, so the ship's own travel since the last move cancels out. The horizontal distance is allowed the mode's usual distance, plus `|ship speed| × 0.5 s` for the client's view of the ship lagging the server's. The vertical check then runs in the new frame, anchored at the new position (boarding) or at the old one in world coordinates (stepping off).
- **No momentum.** Stepping or falling off a moving deck doesn't carry the ship's speed; the ship sails on and leaves you behind. The client's body must not inherit the platform's velocity (in Godot, `platform_on_leave = DO_NOTHING`).
- A frame that names a ship that doesn't exist is refused.
- **Corrections** settle the kept position on the deck (local y = 0) when on the deck rectangle.
- **Rules that need world positions** (range, line of sight, saving on leave) convert with the ship's pose. Leaving the world on a deck saves the world position.

### Helm
- `take_helm()`: on a deck, within 2.5 m of the helm, and the helm is free (or its holder has left the ship).
- `helm(throttle, rudder)`: the helmsman only; each −100 to 100.
- `leave_helm()`. The helm is also released when the helmsman moves more than 2.5 m from it, leaves the ship or leaves the world. Releasing zeroes throttle and rudder, so the ship drifts.

### Cannons
- `fire(mount, yaw, pitch)`: on that ship's deck, within 2.5 m of the mount, the mount loaded (`now ≥ cannon_ready_at`), `yaw` (radians, relative to the mount's facing) within ± the arc, `pitch` within the cannon's limits.
- **Flight** (`sim::ship::cannon_flight`, shared with the client's aiming preview): from the muzzle (the mount, 1 m above the deck) at muzzle speed along the aimed direction, under 9.8 m/s² gravity, until it comes down to sea level. The ship's own velocity isn't added. For the clipper's 40 m/s cannon the range runs from about 25 m (−5°) to about 90 m (+15°).
- The server inserts a `cannon_shot` row scheduled for the landing time, and sets the mount's reload (5 s).
- **Impact** (`cannon_impact`, scheduled): every ship in the 3 × 3 cells around the landing point except the shooter's is tested. A hit is a landing point inside the hull's deck rectangle grown by the splash radius (2 m), in that ship's pose at impact time. A hit takes the damage (1,500) off `hp`, never below 0. A `combat_event` (`skill_id` 2, `flags` 1 for a hit, 0 for a splash, `target` the ship or 0) goes to the landing cell either way.

### The client (ADR 0019)
- **Prediction.** `client/src/Sim/Ship.cs` ports `sim::ship` (poses, step, decks, aim, muzzle, flight), checked against generated vectors within a tolerance. Ships are drawn **at the present**, not 200 ms back like characters: `ShipPredictor` steps the newest `transform` row forward at 10 Hz with the input from `vehicle`. A new row or input restarts it, and the jump is kept as an offset that fades at 4/s (snapping past 8 m). The helmsman predicts their own input at once.
- **The hull** is a static body, a box of the deck's size from the deck down to the keel. Ships are placed every physics frame before the player, so it stands on the deck and bumps into the hull's sides.
- **On deck** the player's position is kept in the ship's frame. Each physics frame it's put on the ship's pose, steps in world space with its own velocity only, and the step is turned back into the ship's frame. Moves are sent with `parent` and ship-local positions and yaw. Standing still on a turning ship sends nothing. The camera turns with the ship.
- **Frames** (`Boarding.FrameAfter`): landing on a deck joins its frame. Over the rail or in the air you stay aboard until 0.25 m inside the 1.5 m rail limit, or 1 m below the deck; then you're in the world, with no momentum. A frame change is sent at once.
- **Climbing aboard:** F within 1.5 m of a deck's edge, from up to 8 m below it (swimming alongside, or on the sea floor beside a hull in shallow water), puts you 0.5 m inside the edge. It waits, up to 1 s, until the server's frame-change allowance covers the distance from the last position sent.
- **The helm:** F within 2.2 m of it takes it, if it's free. W and S step the throttle by 25%, and A and D hold the rudder hard over while pressed. F lets go. While at the helm you don't walk.
- **Cannons:** F within 2.2 m of a mount mans it (client-side; the server only checks reach when firing). A and D aim within the arc (0.5 rad/s), W and S set the elevation (0.2 rad/s), and Space fires when loaded. An arc and a ring show where the shot will land, computed with the shared flight. Our own shots are drawn along that flight and splash when they land. Reloads are timed locally.
- **HUD:** the ship's HP, speed, throttle and rudder, and what F will do.
- **Dev keys** (dev builds): F9 puts a clipper side-on 30 m ahead of the camera, if the water there is at least the draft plus 0.3 m deep. F10 removes the nearest ship within 60 m. `--spawn-ship X,Z,YAW` replaces any ship within 30 m of (X, Z) with a fresh one, once we're within 60 m of it.
- **Autopilot routes** for tests without a person: `deck:X,Z,S`, `helm:X,Z,R` and `gun:X,Z,M` (docs/design/client.md).

### Dev reducers
- `dev_spawn_ship(hull, x, z, yaw)` puts a ship on the water at full HP. Ships are built at shipyards in M1.
- `dev_remove_ship(ship)` removes it. Its passengers are put in the water where they stood, swimming, with a correction so their clients snap.
- `dev_teleport` also takes the caller off any ship.

## 3. Deviations and not yet done
- **The deck is a flat rectangle.** There are no masts, cabins, rails, stairs or tapering bow, and no walls to stop walking off the deck. Deck furniture needs ship-local obstacle outlines, which come with the obstacle work (docs/design/terrain.md §3).
- **No ship-to-ship collisions** and **no coastline distance field** (PLAN §8.4). Ships stop at shallow water, tested at three points.
- **Shots land at sea level.** Terrain doesn't stop them and they don't hit players. The hit test is the landing point, not the path, so a shot that skims over a ship's deck lands beyond it.
- **No sinking, repairs, seats for passengers, ownership or Far tier.** Ships appear in the Near tier only, so they aren't yet seen from far away. These come with M1 ships and the Far tier.
- **No buoyancy.** Waves, roll and pitch are cosmetic; the deck's height is constant.
- **Changing frame trusts the client's view of the ship** within the lag allowance, so a cheater can gain about 6 m (11 m/s × 0.5 s) when boarding or stepping off a fast ship.
- **A character who leaves the world on a deck** comes back at that spot in the water, with no ship. Ships don't persist players across sessions yet.
- **Client:** other players' shots aren't drawn (the `combat_event` row has no position), and hits show only as HP on the target's label. The client doesn't predict groundings; it sails on until the server's row stops the ship.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| content | Hulls parse, wire ids run 0, 1, 2, …, bad values are reported, values reach both outputs and the hash | `server/contentgen/src/lib.rs` |
| sim | Local ↔ world round trip, Godot's basis, yaw ↔ u16; acceleration to top speed and drift; turning circle matches speed / turn rate; low speed barely turns; positive rudder turns starboard; astern; the step is deterministic | `server/sim/src/ship.rs` |
| sim | Draft: deep, unknown and shallow water; cannon flight time and range at 0° match the closed form, longer at higher pitch, none from below sea level | same |
| module | Spawn and remove ships; the tick moves a ship and publishes at 5 Hz; changing cell carries passengers | `server/world/src/tests/ships.rs` |
| module | Boarding a still and a moving ship, walking on deck while it turns with zero corrections, off-deck refused, deck speed hack corrected and settled on deck, stepping off into the water, boarding from too far corrected, unknown ship refused | same |
| module | Helm: take only near it and when free; inputs only from the helmsman; released on walking away, leaving and despawning; the ship drifts after | same |
| module | Fire: in reach, loaded, in the arc; a shot is scheduled for its flight time; impact hits the target ship's HP and logs an event, misses splash; the shooter's ship is never hit | same |
| module | Range and saving on leave use world positions for passengers | same |
| module | Grounding: a ship stops at a shoal and can back off | same |
| C# | The port matches `sim` on 30 frame, 200 step, 6 long-course and 40 flight vectors | `client/tests/SimVectorTests.cs` |
| C# | Predictor: within 5 cm of the server's course between 5 Hz rows, sails on under the helm without rows, eases a correction and snaps a teleport, an input change takes effect from then. Boarding: standing on a deck joins it; rail and air keep you aboard; past the rail or below the deck leaves; climb targets and reach; the climb waits for the allowance; stations in reach. Sender: a frame change goes at once. Autopilot ship routes | `client/tests/ShipTests.cs`, `MovementTests.cs` |
| live | Three autopilot clients: one spawns a clipper and takes the helm, one runs laps of the deck and jumps, then goes over the side, one mans a cannon and fires on every reload. Pass: no corrections, the ship turns at least 360° | manual, results below |
| scenario | `loadbot ship`: a dev admin uploads a flat seabed and spawns two clippers. A helmsman and four walkers swim to the first and board it; the helmsman runs it up to speed and then holds it in a turn while the walkers roam the deck, jump and swing at each other. One walker mans the starboard cannon and shells the second ship, which sits at the centre of the turn. Pass: no corrections on any honest bot, the ship turns at least 360°, every swing lands, at least one shot hits after its flight time, and passengers are published in the ship's cell with the ship as parent. A deck speed hack and a long-range boarding are corrected, an out-of-reach shot is refused, and a ship left with nobody at the helm slows down. Everyone steps off into the water, and the ships and seabed are removed. Acceptance run: 10 minutes. | `cargo xtask bot ship` |

### Client results, 2026-10-26 (Linux laptop, local server, the placeholder world)
- **Three clients crewing one clipper for 6 minutes.** Alpha (`--spawn-ship -430,0,90 --autopilot helm:-430,0,100`) put a clipper in 2.3 m of water off the trade island's west coast, climbed aboard over the stern and took the helm. Bravo (`deck:-430,0,180`) and Charlie (`gun:-430,0,1`) walked out along the sea floor and climbed aboard. After the 20 s wait Alpha sailed at full ahead, then held full starboard rudder on the 52.5 m circle.
  - The ship turned **3,689°** under the three of them.
  - **0 corrections** for all three.
  - Bravo ran laps of the deck, jumping every 3 s, and sent 476 moves. After 180 s aboard it ran over the starboard side at 11 m/s and fell into the sea with only its own speed.
  - Charlie fired **72 shots** from the starboard cannon, one per reload. None was refused.
  - Alpha stood at the helm and sent 37 moves.
- **In deep water, with captures.** Two more runs crewed a clipper in 10–20 m of water further west, with Bravo and Charlie recording. The crew swam out to the ship (or started under its hull) and climbed aboard. In one run the ship turned 1,919°, Bravo sent 489 deck moves and Charlie fired 41 shots. In the other it turned 1,727°, Charlie fired 39 shots, and after 60 s aboard Bravo went over the side at 11 m/s and floated where it fell. All with **0 corrections**. Captures: `docs/captures/2026-10-26-deck-crew.png` (the crew at sea, mid-turn), `-overboard.png` (Bravo swimming as the ship sails on) and `-cannon.png` (the aiming arc, the landing ring and a shot in flight).
- **Found by these runs:**
  - The Near window only followed the player when a move was sent. A helmsman standing still was carried out of their own ship's cells, and the ship vanished from their view. The window now follows every physics frame.
  - The first helm route sailed before the others had boarded, so it now waits 20 s at the helm.
  - Without swimming, a player in deep water walked on the sea floor, more than 8 m below the deck and out of climbing reach. The client now swims (docs/design/client.md).
  - The camera behind a gunner looked back inboard, so it now swings round to look along the cannon. The aiming arc and ring were drawn under the sea, which is transparent and sorts closer because it follows the camera, so they now render last.
  - `--spawn-ship` can lose a race: a client that reaches an older ship within 60 m of the spot boards it before the new one is spawned. That only matters for tests.
