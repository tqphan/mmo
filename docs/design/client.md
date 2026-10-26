# Client skeleton (Godot)

Status: v0.2, 2026-10-26 (walking on terrain). M0 slice. Code: `client/` (`src/`, `tests/`), `server/sim/src/vectors.rs`, `sidecars/loadbot/src/visitor.rs`. Decision: ADR 0015.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Third-person camera behind the character, turned with the mouse; WASD movement, Space to jump | A | common to the genre, RESEARCH §2.11 |
| Other players are drawn moving smoothly, and characters face the way they run | A | as above |
| Movement was predicted on the client; speed and teleport hacks recurred, so the server's checks were weak | C | RESEARCH §2.11, docs/design/movement.md |

The client's controls and feel are public, but nothing here is tuned to them yet. This slice is the plumbing: connect, enter, move, see others. Controls, animation and UI come later in M0 and M1.

## 2. Our rules

### Layout (PLAN §4.8, §6.1)
- `client/` is the Godot 4.7.2 .NET project (`project.godot`, `Mmo.csproj`). `net/` holds the generated bindings (`cargo xtask generate`) and `content/` the generated game data (`cargo xtask content`). Both are compiled into the game.
- `src/` is split by layer:
  - `Sim/`: ports of `sim` (cells, yaws), checked against golden vectors.
  - `Net/`: `Session` (the connection) and `SubscriptionDirector`.
  - `World/`: `EntityRegistry`, `RemoteView`, `SnapshotBuffer`, placeholder meshes, and `HeightMap`, `TerrainPlan` and `TerrainCollision` for terrain (docs/design/worldstream.md).
  - `Gameplay/`: `PlayerController`, `MoveSender`, `FollowCamera`, `Autopilot`.
  - `UI/`: the debug HUD.
  - `Presentation/`: the art style test scene and the pieces it's built from (docs/design/art.md), terrain tiles and the world test scene (docs/design/worldstream.md).
- The scene tree is built in code by `src/Main.cs` from a one-node `main.tscn`. Scenes as files come with real art.
- Files under `Sim/`, `Net/SubscriptionDirector.cs`, `World/SnapshotBuffer.cs`, `Gameplay/MoveSender.cs`, `Gameplay/Autopilot.cs`, `World/HeightMap.cs` and `World/TerrainPlan.cs` don't use Godot, so the xUnit project in `tests/` compiles and tests them directly.

### Connecting and entering
- One `DbConnection` with Gzip (ADR 0011). `FrameTick` runs in `Session._Process`, so every SDK callback runs on the main thread.
- The identity token is kept per profile in `user://tokens/<profile>.token`. Two windows with different `--profile`s are two players.
- On connecting, it subscribes to `my_characters`, `my_presence` and `character_public`. If the account has no character, it creates one named after the profile (letters only, capitalised: `alpha` → `Alpha`). Then it calls `enter_world(char_id, GameContent.Hash, terrainHash)`, with the terrain hash of the world it loaded (docs/design/world.md). A client on a different world is refused, and the HUD shows the server's message.
- **`my_presence` is a view, and view rows have no primary key.** The SDK delivers each change as an insert of the new row plus a delete of the old one, and the insert comes first. So an insert for the entity we already have is an update. We've left the world only when no row is left at the end of a frame.

### The world
- The client loads the world's files at start (`--world`, default `world/`, the files `cargo xtask world` uploads to the server; about 0.2 s) and draws them with the terrain tiles and the sea from the world-scale spike, centred on the camera (docs/design/worldstream.md, world.md). If they can't be read, or don't match their manifest's terrain hash, it stops with a message. Housing land and towns are tinted, and the HUD shows the land use and climate underfoot.
- The player stands on `HeightMapShape3D`s for the 3 × 3 cells around it, holding the server's own samples (`TerrainCollision`, docs/design/worldstream.md). The camera stays 0.5 m above the ground.
- Until the world has spawn points, new characters appear at (0, 0), on the placeholder world's trade island about 13 m up. The server settles them on its ground.

### The local player
- A `CharacterBody3D` with its origin at the feet, where the server measures. It can climb slopes up to Godot's default 45°; the server allows 51° (docs/design/terrain.md). Gravity is 15 m/s² and a jump starts at 5 m/s, for an apex of about 0.8 m. That's inside the server's 2 m jump allowance and falls faster than its minimum-fall check.
- Speed is the content limit for the mode: run 5.4 m/s, or walk 2.0 with `--walk`. Input is relative to the camera, and the character faces the way it moves, so the heading it sends is also its direction of travel.
- `MoveSender` sends a `move_event` on start, stop, a turn of more than 10° (at most every 0.1 s), a jump, landing, a mode change or a speed change of more than 0.5 m/s. It also sends a heartbeat every 0.5 s while moving or in the air. Numbering continues from the server's `last_seq`.
- **Snapping.** On entering, and whenever `my_presence.corrections` changes, the player is put at the presence position with zero velocity, and the sender restarts from there. If that moved the collision window, the player waits two physics frames so the engine has the new ground before gravity applies.
- The Near-tier director is updated with the predicted position each time a move is sent.

### Other entities
- `EntityRegistry` makes a `RemoteView` for every `transform` row in a subscribed cell, except our own entity. It deletes the view when the row goes. A row that moves between two subscribed cells may arrive as a delete and an insert in one frame, so deletions are applied at the end of the frame, after the matching insert has had its chance to cancel them.
- **Where to draw (ADR 0015):** `SnapshotBuffer` stamps each row with its local receive time and draws 200 ms (two ticks) in the past. It interpolates between the rows either side of that time. Past the newest row, it carries on along that row's yaw at its speed for up to 0.8 s (dead reckoning). A jump that the rows' speeds can't explain, by more than 8 m, is a teleport: the history is dropped and the view snaps there. The view then eases towards the buffer's answer at a rate of 15/s, which hides the small steps when a late row arrives.
- Characters are capsules with a nose that shows their facing, and a nameplate taken from `character_public`. Ships (`entity.kind` 2) are boxes the size of their hull from content. Passengers (`transform.parent` ≠ 0) are drawn in their ship's frame. Ships are only extrapolated in a straight line so far.

### Command line (after `--`)
| Option | Effect |
|---|---|
| `--uri`, `--db` | Server (default `http://127.0.0.1:3000`, `mmo-world`) |
| `--profile NAME` | Identity token and character name |
| `--autopilot circle:X,Z,R` / `pace:X,Z,LEN` | Run a scripted route instead of reading the keyboard |
| `--walk` | Walk mode |
| `--world DIR` | The world's files (default `world`), or a bare `.mmoh` height file |
| `--secs S` | Quit after S seconds |
| `--cam-yaw DEG` | Starting camera yaw, 0 looking north (-z) |
| `--trace` | Log where each other entity is drawn every frame, to measure smoothness |
| `--test-cheat` | 3 s after entering, jump 30 m in one frame, to check that the correction snaps us back |
| `--style-test flat\|gradient` | Show the offline art style test instead of connecting; `--outline` and `--shot follow\|wide` go with it (docs/design/art.md) |

Controls: WASD or arrows to move, Q/E or right-drag to turn the camera, wheel to zoom, Space to jump.

### Captures
`cargo xtask capture <name> <frames> [client args]` runs Godot's Movie Maker at 30 fps and writes PNGs to `target/captures/<name>/`. Movie Maker renders as fast as it can, but the server checks moves against its own clock, so in movie mode the client holds each frame back to real time. Encoding can still make it slower than real time (about 70% with two windows and a bot on the laptop). Our own movement then looks slower in the video and other players faster, but nothing is corrected.

## 3. Deviations and not yet done
- **The world file isn't shipped with the client yet.** It's generated into `target/` by `cargo xtask world` or on the first `cargo xtask client`. If the server's terrain came from a different file, moves are corrected. Checked-in world files are BACKLOG "World building v0".
- **No swimming.** In water the player runs along the sea floor. The server allows it in run mode, since the swimming rule only applies to swim mode.
- **No walking on decks yet.** The local player always sends `parent = 0`. Helm and cannon controls come with M1 ships. Ships should be extrapolated with `sim::ship::step` ported to C# (ADR 0014).
- **No reconnect.** A dropped connection ends the session.
- **Characters face their direction of travel**, including when walking backwards. Strafing and backpedalling with the original's feel come with animation.
- **No `.sln`.** `dotnet build client/Mmo.csproj` and the xtask commands work without one. Opening the project in the editor offers to create it.
- **Captures run slower than real time** when the machine is busy (see Captures).

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| vectors | `cell_of` (edges, clamping, NaN, 200 random points), `recenter` (240 points around four centres, margin edges, NaN and infinity), `window_cells` (radii 0–4, corners), `yaw_to_u16` and `yaw_from_u16`: all generated from `sim` and matched exactly by the C# | `client/tests/SimVectorTests.cs`; `server/sim/src/vectors.rs` fails if the file is stale |
| C# | Director: one query set per cell, only the new column fetched, new before old, no resubscribing on a border, coming back before the new window applies, teleport, NaN, map corner | `client/tests/SubscriptionDirectorTests.cs` |
| C# | Sender: nothing before entering or standing still; start, heartbeats and stop; every event within the server's speed allowance while running a circle; turns rate-limited; jump, airborne heartbeat, landing and mode; numbering continues from the server | `client/tests/MovementTests.cs` |
| C# | Buffer: draws 200 ms back; dead reckoning along yaw, capped; a heartbeat-rate runner with tick jitter never jumps or steps back far; teleports snap; yaw turns the short way. Autopilot: holds the circle, paces, rejects bad specs | same |
| build | The Godot project compiles with warnings as errors | `cargo xtask test` |
| live | Two clients and `bot visitor --expect 2` all see each other with zero corrections; Movie Maker frames from both windows | manual, results below |
| live | `--trace` on a watching client: no reversals and no big frame steps for a client-driven or bot-driven runner | manual, with a script over the log |
| live | `--test-cheat`: one correction, the client snaps back, and later moves are accepted | manual |
| C# | `HeightMap.CellHeights` gives each cell's 33 × 33 samples, edges shared, null outside the file | `client/tests/HeightMapTests.cs` |
| live | On the uploaded world: two autopilot clients for 10 minutes over hills, coast and shallow water with `bot visitor --expect 2`: zero corrections; the body stands within a few cm of the heightmap's height | manual, results below |

### Results, 2026-10-26 (Linux laptop, local server)
- **Bot plus two windows.** `bot visitor --secs 45 --expect 2` passed and saw Alpha and Bravo. Alpha ran `circle:-3,-7,4`, saw the bot and Bravo, and sent 187 moves with 0 corrections. Bravo stood at spawn and saw both. 750 frames were captured from each window (`docs/captures/2026-10-26-client-*.png`).
- **Smoothness**, from Bravo's trace:
  - Alpha (a client sending turns): 0 reversals, biggest step 0.41 m per frame at about 23 fps.
  - The bot after its fix: 0 reversals, biggest step 0.11 m per frame at 60 fps.
  - Before the fix, the bot sent its heading backwards at 2 Hz, and the trace showed 119 reversals of up to 1.45 m. The trace is how that was found.
- **Correction:** `--test-cheat` jumped 30 m, the server corrected it once, the client snapped back to (0.1, -5.1), and it then sent 16 more moves with no further corrections.

### Results on terrain, 2026-10-26 (Linux laptop, local server, the placeholder world uploaded)
- **10 minutes, two clients and a bot.** Alpha paced `pace:150,-14,1150` east and west through spawn, from shallow water off the west coast over the 17 m hills. Bravo ran `circle:100,0,450` round the island's coast. `bot visitor --expect 2` circled at spawn on the file's ground and saw both. Alpha sent 1,186 moves and Bravo 1,185, with **0 corrections**. On the floor the body stood at most 0.01 m (Alpha) and 0.06 m (Bravo, the capsule on slopes) from the heightmap's bilinear height.
- **Correction:** `--test-cheat` from spawn was corrected once, and the client snapped back to (0, 12.8, 0) on the hilltop.
- Captures: `docs/captures/2026-10-26-terrain-{coast,island,spawn}.png`.
