# Ships and moving decks

Status: v0.2, 2026-10-26. M0 spike 3. Code: `content/ships.toml`, `server/sim/src/ship.rs`, `server/world/src/{ship,movement}.rs`, `sidecars/loadbot/src/ship.rs`. Decision: ADR 0014.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Players build ships and sail them; several players share one, walking about on deck while it moves | A | RESEARCH §2.7 |
| The crew splits into helm, cannons, repairs and harpoon; if the helmsman dies, the ship drifts | B | RESEARCH §2.7 |
| The clipper is the fastest hull at about 11 m/s; the merchant schooner about 10 m/s | B | RESEARCH §2.7 (fan wikis) |
| Cannons are manned by players, aimed within an arc on one side of the ship, and fire visible shots on an arc that land after a flight time | B | common player knowledge, RESEARCH §2.7 |
| Ships carry a lot of HP (tens of thousands) and are repaired at sea | B | RESEARCH §2.7 |
| Acceleration, turn rate, cannon range, damage and reload | **Unknown** | not public in a usable form |

Every hull and cannon number below is a placeholder, confidence C, in `content/ships.toml`. The speed matches the clipper's reported 11 m/s.

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
| scenario | `loadbot ship`: a dev admin uploads a flat seabed and spawns two clippers. A helmsman and four walkers swim to the first and board it; the helmsman runs it up to speed and then holds it in a turn while the walkers roam the deck, jump and swing at each other. One walker mans the starboard cannon and shells the second ship, which sits at the centre of the turn. Pass: no corrections on any honest bot, the ship turns at least 360°, every swing lands, at least one shot hits after its flight time, and passengers are published in the ship's cell with the ship as parent. A deck speed hack and a long-range boarding are corrected, an out-of-reach shot is refused, and a ship left with nobody at the helm slows down. Everyone steps off into the water, and the ships and seabed are removed. Acceptance run: 10 minutes. | `cargo xtask bot ship` |
