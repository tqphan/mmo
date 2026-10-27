# 0024: A craft is a private scheduled job that makes one unit per run and reschedules itself

- **Status:** Accepted, 2026-10-26
- **Context:**
  - Crafting (M1 task 4) needs timed crafts at stations that cost labor and materials. Batches ("craft 10") are the normal case, and the bag and labor can run out partway.
  - A scheduled reducer can't reply to a client. Unlike gathering, a craft has no public row to show progress on: stations are shared, so their rows can't carry one crafter's progress.
  - Inputs freeing a slot is common (four reeds in, one twine out), so checking room before taking the inputs would wrongly refuse crafts in a full bag.
- **Decision:**
  - **One private `craft_job` row per crafting character**, scheduled for when the current unit finishes. It holds `made`, `count` and `done_at` in game time. The owner reads it through the **`my_craft` view**, which the client draws the progress bar from.
  - **Each run makes one unit.** It checks that the crafter is still in place and the station still exists. It then takes the inputs, grants the output and spends the labor in one transaction, or does none of it and ends the job.
  - If more units are wanted and the next one can be made, the job **reschedules itself**: it deletes its own row first and inserts the next. Otherwise it ends at once. The scheduler deletes the row that fired after the reducer returns, which would otherwise collide with the next unit's row on the character's unique index; the live `craft` scenario confirms the order works on SpacetimeDB 2.13.
  - **Only the first unit is checked at the start.** The window shows how many units the bag and labor cover, computed by `sim::craft::affordable` and ported to C#.
  - **Taking and granting are one all-or-nothing exchange** (`items::exchange`). It plans the bag after the inputs are taken, then checks room for the output, so slots the inputs free count as room.
  - As in gathering (ADR 0023), materials and labor are spent when a unit finishes, so cancelling is free. Moving 0.5 m, `cancel_craft`, leaving the world and the station's removal all cancel. Crafting and gathering exclude each other.
  - **Stations are public, static rows in the Static tier**, placed from `world/stations.tsv` by `cargo xtask world`, like nodes. They have no busy state; anyone may use any station at once.
- **Consequences:**
  - A batch of N writes the job row N times, once per unit, and nothing in between. Cheap at seconds per unit.
  - The client tells a finished craft from a cancelled one only by its bag, as with gathering. That's enough for the log line; a richer outcome needs an event table later.
  - Proficiency ranks (M2) will change labor and time per unit. They belong in `can_make` and `craft_micros`, which take the recipe now and will take the crafter too.
  - The new tables need `cargo xtask publish --clear` and `cargo xtask world` once.
