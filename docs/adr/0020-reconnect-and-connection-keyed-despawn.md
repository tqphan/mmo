# 0020: The client reconnects through the SDK and enters again; only the entering connection despawns a character

- **Status:** Accepted, 2026-10-26
- **Context:**
  - A dropped connection ended the client's session (BACKLOG M0 task 2). The C# SDK 2.13 has `WithAutomaticReconnect`: it retries with backoff, keeps the client cache and every subscription handle, replays the subscriptions in one batch, and runs each one's `OnApplied` again. It doesn't retry a failed first connection, and it doesn't repeat reducer calls whose result was lost.
  - The server despawns a character in `client_disconnected`, and `presence` is keyed by identity. A client whose network drops may reconnect, on a new connection with the same identity, and enter again before the server notices the old socket has gone. The old connection's disconnect would then despawn the new character.
- **Decision:**
  - **The client uses `WithAutomaticReconnect`** (1 s to 15 s backoff). On a drop, `Session` forgets its presence and raises `Dropped`, and the player stops stepping and sending. After recovery, the views' `OnApplied` runs again and the client calls `enter_world`, even if the old presence is still in the cache. Entering snaps the player to where the server saved it, as on first entry. The Near director's cell subscriptions are the SDK's to replay. The director only had to tolerate a second `OnApplied` per cell.
  - **`presence.connection_id` records the connection that entered,** and `client_disconnected` despawns only a presence its own connection made. `enter_world` already replaces any older presence for the identity.
- **Consequences:**
  - A short outage costs a re-entry: a new entity id, and the character stands where it was saved. Others see it leave and come back.
  - A passenger who drops is saved at its world position and comes back in the world frame, not on the deck: in the sea if the ship sailed on.
  - A failed first connection still ends the session.
  - `presence` gained a column, so this needs `cargo xtask publish --clear` (and `cargo xtask world`) once.
