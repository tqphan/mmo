# 0023: Resource nodes are public rows with a regrowth timestamp; a gather is a scheduled job

- **Status:** Accepted, 2026-10-26
- **Context:**
  - Gathering (M1 task 3) needs nodes that everyone sees, that can be taken, and that come back later. It also needs a timed action, which can be interrupted, and which costs labor and yields items.
  - Nothing may tick what a timestamp can compute (CLAUDE.md). Thousands of nodes regrowing on a timer would be thousands of writes for nothing.
  - Scheduled reducers can't reply to a client, so whatever happens when a gather finishes has to show up in rows the client already reads.
  - Node placements need a home before the coast zone is laid out by hand (M1 task 11).
- **Decision:**
  - **One public `resource_node` row per node**, keyed by cell, in a new **Static tier** (9 × 9 cells), the first use of PLAN §4.3's Static tier. `ready_at` is a game-time timestamp: a node is gatherable when `now ≥ ready_at`, and taking it sets `ready_at = now + regrow_s`. The client draws ready and regrowing nodes from the same comparison against `game_clock`.
  - **The gatherer is public on the node** (`gatherer`, `done_at`). Others see the node being worked, `gather` refuses a node someone else is gathering, and the gatherer's client draws the progress bar without a private row.
  - **A gather is a private one-shot scheduled `gather_job`.** It records where the character stood and the yield, rolled at the start. `move_event` cancels it once the character is more than 0.5 m away, and leaving the world cancels it too. When it's due, it checks everything again. It then spends labor, grants items and takes the node in one transaction, or else does none of it and frees the node. It never returns an error, since nobody would see one.
  - **Labor is spent when a gather finishes**, so a cancelled gather costs nothing. The checks at the start are a quick refusal, not a reservation.
  - **Placements live in `world/nodes.tsv`**, written by worldtool and uploaded by `cargo xtask world`, which replaces every node. Kinds are named by content id there, while rows use the wire. Placements stay out of the terrain hash.
- **Consequences:**
  - A node changes only when someone starts, stops or finishes gathering it, so the Static tier stays quiet. The placeholder world has about 12,000 nodes, about 50 in a 9 × 9 window.
  - Re-uploading the world resets every node to ready. That's fine for an admin action. A live server will need a merge that keeps `ready_at`.
  - `gatherer` must be cleared on every path that ends a job; the module tests cover finish, failed finish, move, `cancel_gather`, leaving and node removal.
  - The new tables need `cargo xtask publish --clear` and `cargo xtask world` once.
