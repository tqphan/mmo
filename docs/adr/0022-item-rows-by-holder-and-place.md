# 0022: Item rows addressed by holder and place, with permanent wire numbers

- **Status:** Accepted, 2026-10-26
- **Context:**
  - Items are the first private data that many rows share per player and that later systems (gathering, crafting, packs, chests, ships' holds, mail, the market) all move between containers.
  - PLAN §5.2 sketched one `container: u64` column that packs a kind and an owner together.
  - Item definitions will grow to hundreds, and some will be removed, while rows in the database still point at them.
  - A view is recomputed when a table it reads changes, and `presence` changes on every move.
- **Decision:**
  - **Each stack is one private `item` row with `holder`, `place` and `slot`**, indexed together as `(holder, place, slot)`. `place` says what kind of container it is, and so what kind of id `holder` is: a `char_id` for the bag, and later a chest's or ship's own id. This replaces the packed column, which would need the same packing in Rust and C#. The two columns read more plainly and index just as well.
  - **Items are content with a permanent `wire` number** that is never changed or reused. Gaps are allowed, unlike the other content tables, whose wire values double as array indexes. The generated code looks items up by binary search (Rust) or a dictionary (C#).
  - **`my_inventory` finds the caller's characters through `character_private`, not `presence`**, and returns their bags. The client filters to the character it's playing.
  - The rules are in docs/design/items.md.
- **Consequences:**
  - The server enforces one stack per slot. SpacetimeDB has no unique constraint over several columns, so every reducer that changes the bag checks it.
  - Removing an item from content leaves its rows in place. They still move and can be destroyed, they can't merge, and the client shows them as "item N".
  - The new table needs `cargo xtask publish --clear` (and `cargo xtask world`) once.
