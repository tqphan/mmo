# 0019: The client draws ships at the present, and keeps a passenger in the ship's frame

- **Status:** Accepted, 2026-10-26
- **Context:**
  - The client now walks on decks and drives ships (BACKLOG M0 task 2, docs/design/ships.md). The server checks a passenger's moves in the ship's frame. When the frame changes (boarding, stepping off, jumping across), it converts both ends with its current pose and allows `|ship speed| × 0.5 s` for the client's view lagging (ADR 0014).
  - Characters are drawn 200 ms back (ADR 0015). A deck drawn 200 ms back would be 2.2 m behind the server's at a clipper's 11 m/s, and every boarding and step-off would use up a third of the lag allowance before the client's own error.
  - Godot can carry a body on a moving platform, but only in world space, through the platform's velocity. It can't keep the player's place on deck exact while the ship turns, and the server never sees the ship's motion in a passenger's moves anyway.
- **Decision:**
  - **Ships are drawn at the present.** `ShipPredictor` steps the newest `transform` row forward with the port of `sim::ship::step` (checked against vectors) and the helm's input from `vehicle`, in 0.1 s steps like the server's tick. A new row or a new input restarts the prediction. The jump that causes is kept as an offset that fades at 4/s, unless it's over 8 m. The helmsman's own input is predicted at once.
  - **Ships are placed every physics frame, before the player,** and the hull is a static body with the hull's box shape. The local player stands on it and bumps into it.
  - **A passenger's position is kept in the ship's frame.** Each physics frame the body is put where that position is on the ship's pose, steps in world space with only its own velocity, and only the step is turned back into the ship's frame. Standing still on deck never moves the position, so float error far from the origin doesn't wander, and stepping off never inherits the ship's speed. The engine's own platform carrying is off (`PlatformOnLeave = DoNothing`, and a static hull has no platform velocity).
  - **The frame follows the server's rules** (`Boarding.FrameAfter`): standing on a deck joins it; over the rail or in the air you stay aboard until 0.25 m inside the server's 1.5 m rail limit, or 1 m below the deck; then you're in the world. Climbing aboard from the water is an action (F), not a jump, and it waits until the server's allowance covers the distance.
  - Characters, passengers included, are still drawn 200 ms back, in the frame of their ship as drawn now.
- **Consequences:**
  - Boarding and stepping off are judged against a pose that differs from the server's by the network delay and the prediction error, a few cm at 5 Hz rows, not by 200 ms of sailing.
  - Ships can jump a little when a row disagrees with the prediction. That's eased over about a second, and the passengers move with the deck.
  - A grounding isn't predicted: the client sails on until the server's row stops the ship, then eases back. The client doesn't port `sim::ship::afloat`.
  - The Near window has to follow the player every frame, not only when a move is sent, because a ship carries a helmsman across cells while it stands still.
