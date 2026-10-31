# 0031: Owned vehicles are bag items that leave the bag while out; carts share the ships' step with a stored deck height; cargo is public by item and private in detail

- **Status:** Accepted, 2026-10-31
- **Context:**
  - M1 needs a cart with pack slots, a rowboat, and packs carried by vehicles (BACKLOG M1, docs/design/vehicles.md). Vehicles A covers these; the shipyard is Vehicles B.
  - In the original, small boats and finished ships were summon items kept in the bag. A farm cart carried 2 packs; anyone could load one, and only the owner (or party) could take one off.
  - Ships already step on the server from throttle and rudder, carry passengers in their frame and are predicted by the client (ADRs 0014, 0019). Their deck height was always sea level plus the hull's.
  - A cart's height has to follow the ground, which the client also has (ADR 0017, ADR 0018).
- **Decision:**
  - **Hulls gain a medium (sea or land), a summoning item, pack slots and, for land hulls, a steepest gradient.** The cannon table is optional. A cart is a hull like any other: an `entity` of kind ship, a `vehicle` row and a private `ship_motion` row, stepped in `world_tick` with `sim::ship::step` and driven through the same helm reducers.
  - **`sim::ship::stand`** decides where a hull can be after each step: afloat for a sea hull; for a land hull, known ground at bow, centre and stern no deeper than its ford, no steeper than its gradient, and rising no more than that from stern to bow. It returns the deck's height, now **stored** in `ship_motion.y` and published in `transform.y`. Passengers stay in the vehicle's frame, so a cart going uphill needs no special movement rule.
  - **A vehicle item is the vehicle.** Summoning moves the item's row to a new place, `OUT`, held by the character with the hull's wire as its slot, so it can't be moved, split or destroyed while the vehicle is out. Stowing deletes the vehicle and puts the item back in the first free bag slot. One land and one sea vehicle per character at a time. Vehicles stay out when their owner leaves.
  - **`vehicle.owner`** is the character; only its account drives or unloads. Unowned (dev) ships stay open.
  - **Cargo:** `vehicle.cargo` publishes one pack item wire per slot, so everyone near sees what's loaded; a private `cargo` row holds the rest of the pack (maker, origin, craft time) exactly as a back does. Dying aboard puts the carried pack in a free slot.
  - **The client ports `sim::terrain::ground`** (checked against vectors) and predicts each vehicle's footing with `stand`, so carts stop and climb where the server's do.
- **Consequences:**
  - The schema changed (`vehicle.owner` and `cargo`, `ship_motion.y`, the `cargo` table): run `cargo xtask publish --clear` and then `cargo xtask world` once.
  - The clipper's hull keeps its id `clipper` on the wire; players see it as the Swiftgull. It gained two pack slots, a deviation (the original's smallest clipper had none) so M1 can sail packs.
  - A vehicle out of the Near tier can't be found by the client to stow it. Its owner must walk back to it until a summary of one's own vehicles is published (a private view would do).
  - Carts don't collide with anything yet, and cannons can hit one standing at the water's edge.
