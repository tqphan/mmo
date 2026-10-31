# Vehicles

Status: v0.1, 2026-10-31. M1 task "vehicles", first half (**Vehicles A**): owned vehicles summoned from the bag, the Furrow Cart on land, the Driftpine Skiff, and pack slots on vehicles. The second half (**Vehicles B**) builds the clipper at a shipyard and sinks ships (§3). Code: `content/ships.toml`, `server/sim/src/ship.rs`, `server/world/src/{ship,vehicle}.rs`, `sidecars/loadbot/src/vehicle.rs`; client `client/src/Sim/{Ship,Terrain}.cs`, `World/{ShipPredictor,ShipView}.cs`, `Gameplay/{ShipControls,Vehicles}.cs`, `UI/VehicleDemo.cs`. Builds on docs/design/ships.md (hulls, motion, decks, the helm) and docs/design/trade.md (packs). Decisions: ADR 0031.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| The farm cart was a land vehicle the owner drove, carrying 2 trade packs besides the one on the driver's back; others could ride on it carrying their own | B | [Fandom: Farm Cart](https://archeage.fandom.com/wiki/Farm_Cart), [archive wiki](https://archeage-archive.fandom.com/wiki/Farm_Cart) |
| The cart ran at 4 m/s (6 with fuel) and turned at 22.9°/s; it couldn't handle rough ground the way a donkey could | B | same |
| Anyone could put a pack into your cart; only you or your party could take one out, and anyone could once the vehicle was destroyed | B | same |
| Upgraded carts (wagon, hauler) held 4 and 6 packs and ran faster | B | same; post-launch upgrades, not in M1 |
| The rowboat was a free early quest reward: 4 m/s, a second seat, no cargo | B | [Fandom: Ships](https://archeage.fandom.com/wiki/Ships) |
| Small boats, and finished ships, were summon items: kept in the bag, summoned onto the water and ridden | B | same; [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=416801428) |
| The smallest clipper had no space for packs, though passengers could wear theirs; a pack dropped on a deck without a slot was lost. Some later small ships had 3–4 pack slots, the merchant schooner 20 | B | same |
| Big ships were built on a drydock placed by the sea from a design, then fed lumber, iron and fabric packs in stages, then launched into a summon item | B | [Fandom: Merchant Schooner](https://archeage.fandom.com/wiki/Merchant_Schooner) (Vehicles B) |
| Cart HP, acceleration and braking; whether a summoned vehicle stayed out after logging off; how far away it could be recalled | **Unknown** | not found |

## 2. Our rules

### Content
- **Hulls** (`content/ships.toml`) gain:
  - `name`, shown to players. The clipper's is **Swiftgull** (its id stays `clipper`).
  - `medium`: `sea` or `land`.
  - `item`: the vehicle item (kind `vehicle`) that summons it. Each vehicle item summons exactly one hull, and each hull has exactly one item.
  - `max_gradient`: for land hulls, the steepest ground (rise over run) it drives on; 0 for sea hulls.
  - `[hull.cannon]` is now optional; a hull with mounts must have one.
  - `[[hull.slot]]`: pack slots, each a ship-local `x`, `z` on the deck.
  - For land hulls, `draft_m` is the **ford depth**: the deepest water it drives through.
- **Items:** a new kind, **`vehicle`**, with a stack of 1.
- Contentgen checks the medium, that `item` names a vehicle item used by no other hull, that slots are on the deck, at most 8 slots and 8 mounts, mounts need a cannon, land hulls have no mounts and a `max_gradient` above 0, and sea hulls have 0. All of it is in the content hash.

| Hull | Medium | Speed ahead / astern | Turn | Deck (w × l, height) | Draft / ford | HP | Slots | Recipe (Joiner's Bench) |
|---|---|---|---|---|---|---|---|---|
| Swiftgull (`clipper`) | sea | 11 / 3 m/s | 12°/s | 5 × 18 m, 2 m | 1.2 m | 12,000 | **2** (stern) | none yet: the shipyard (Vehicles B); dev builds grant it |
| Driftpine Skiff | sea | 4 (B) / 1.5 m/s | 30°/s | 1.6 × 4 m, 0.5 m | 0.4 m | 2,000 | 0 | Driftpine Plank 12, Saltrope 2, Rustiron Nails 6; 30 labor, 10 s |
| Furrow Cart | land | 4 (B) / 1.5 m/s | 22.9°/s (B) | 1.8 × 3.2 m, 0.9 m | 0.4 m ford, gradient 0.6 | 3,000 | 2 (B) | Hollowoak Plank 10, Hollowoak Beam 2, Rustiron Fittings 4; 50 labor, 15 s |

The cart accelerates at 2 m/s² and brakes at 4 m/s² with the throttle at 0; the skiff 1 and 0.8. All C except where marked.

### Land motion (`sim::ship::stand`)
A land hull steps exactly like a ship (`sim::ship::step`). After each step it must **stand** on the ground at its bow, centre and stern (ship-local z of −half length, 0 and +half length):
- the terrain there is known (unknown terrain stops a cart, where it floats a ship);
- the higher triangulation is no more than the ford depth below sea level;
- the gentler triangle's gradient is at most `max_gradient`;
- the rise from stern to bow is at most `max_gradient × length`.

If it stands, the deck's height is the centre's higher triangulation plus `deck_height_m`; otherwise the step is cancelled and the cart stops (speed 0), as a ship does on a shoal. A sea hull "stands" when `afloat` holds, at sea level plus its deck height. So the pose's height is now **stored** with the motion (private `ship_motion.y`) instead of always being sea level plus the deck: the deck rises and falls under a cart's passengers, who stay in its frame exactly as on a ship.

### Owning and summoning
- A vehicle item in the bag is the vehicle. **`summon_vehicle(item_id)`**:
  - The caller is in the world, alive, and not aboard anything (swimming is fine).
  - The item is the caller's, in its bag, of kind `vehicle`.
  - The character has no other vehicle of the **same medium** out: one land and one sea vehicle at a time.
  - The vehicle is placed facing the character's yaw, its stern `SUMMON_GAP_M` (1.5 m) ahead of the character, and must stand there (above). Refused: "no room for it there" if any other vehicle's centre is closer than the two half-lengths added.
  - The item leaves the bag: its row moves to place **`OUT`** (3), holder the character, slot the hull's wire. The vehicle is a ship entity (`entity` kind 2) at full HP with `vehicle.owner` set to the character.
- **`stow_vehicle(vehicle)`**, from anywhere, by the owner's account:
  - Refused with cargo aboard: "unload its cargo first".
  - Refused when the bag is full.
  - Anyone aboard is put off where they stood (in the water, swimming, off a boat; on the ground off a cart), told to snap. Cannon shots it fired that are still in the air are dropped.
  - The vehicle's rows are deleted and the item returns to the first free bag slot.
- A vehicle stays out when its owner leaves the world, and can be stowed after they come back.

### Driving
- `take_helm` on an owned vehicle is refused for every account but the owner's: "only the owner can drive this". Party members join that with parties (BACKLOG M1). Unowned (dev) ships stay open to anyone.
- Boarding, walking on deck and cannons are open to anyone, as before: pirates can board.

### Pack slots
- Public: **`vehicle.cargo`** is one pack item wire per slot (0 for empty), in the hull's slot order, so anyone near sees what's loaded.
- Private: one **`cargo`** row per loaded pack: vehicle, slot, and the pack's item, maker, origin and craft time, exactly as on a back. Packs keep ageing in a slot.
- **`load_pack(vehicle, slot)`**: the caller is alive, carries a pack, and stands within `CARGO_REACH_M` (3 m horizontally and vertically, in world coordinates) of the slot's world point, aboard or not; the slot exists and is empty. **Anyone** may load.
- **`unload_pack(vehicle, slot)`**: the caller is alive, its back is free, it is in reach, the slot holds a pack, and the caller's account owns the vehicle (anyone, for an unowned one): "only the owner can unload this". The pack goes on its back. Taking a pack made by another account's character is logged as **theft** for the crime task. A spoiled pack falls apart instead.
- **`drop_pack` on a deck** is refused: "load it into a pack slot". **Dying aboard** puts the carried pack into the first free slot, if there is one; otherwise it stays on the back as before.
- A vehicle's speed doesn't depend on its cargo.

### Dev
- `dev_spawn_ship` makes an unowned vehicle of any hull; a land hull must stand where it's put. `dev_remove_ship` removes any vehicle, its cargo with it. `dev_grant_item` grants vehicle items like any other.

### The client
- **Prediction:** `ShipPredictor` checks a land hull's footing each step with a C# port of `sim::terrain::ground` over the client's heightmap (`Sim/Terrain.cs`, checked against vectors), so a cart stops at the same steep bank or ford as the server's. The deck's height comes from the same rule.
- **Drawing:** every vehicle is a placeholder box of its deck size; carts get four wheels and a driver's bench, skiffs a lower box. Slots are drawn as pallets, a loaded pack as a crate on it. The hull's static body runs from the deck to the ground for carts.
- **Keys:** right-click a vehicle item in the bag to summon it. **V** stows your vehicle: the one you're aboard, else your nearest in sight. **F** near a slot loads your pack into it, or unloads the pack there (owner only); the prompt says which. A slot takes F before climbing aboard, but not while you stand at the helm or a cannon, where F works that. On a cart's bench, F takes the reins; **W** and **S** drive while held (the cart brakes when they're let go), and A and D steer.
- **HUD:** the vehicle's name, HP, speed and cargo.
- `--vehicle-demo` summons a cart, loads a dev pack, drives it, unloads, stows, then summons a skiff and rows it.

## 3. Deviations and not yet done
- **The Swiftgull has two pack slots** where the original's smallest clipper had none, so M1 can sail packs before the merchant ship (M2).
- **One land and one sea vehicle out per character**, our choice. **Vehicles stay out** after logging off and can be stowed from anywhere; the original's rules aren't known.
- **No fuel or boosts**, no wagon or hauler upgrades, no seats: riders stand on the deck or the cart's bed.
- **Only the owner drives and unloads** until parties exist.
- **Carts don't collide** with each other, characters, stations or plots, and don't take damage from falls. Cannons can hit a cart standing on the beach, since they hit anything at the landing point.
- **The client can only stow a vehicle it can see** (the Near tier). A vehicle left far away must be walked back to; a private view of one's own vehicles would fix it.
- **Vehicles B:** the shipyard (a design placed on a shore, material packs delivered in stages, a launch into a Swiftgull item), destroyed and sunk vehicles (HP 0, cargo falling as dropped packs that anyone may take), and repairs.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| content | Hulls with medium, item, slots and an optional cannon parse and reach both outputs and the hash; bad media, missing or shared items, slots off the deck, mounts without a cannon, land hulls with mounts or no gradient are reported | `server/contentgen/src/lib.rs` |
| sim | A cart stands on gentle known ground with its deck at the centre's height; unknown ground, a ford too deep, a steep gradient and a steep rise each stop it; sea hulls stand where afloat; slot world points; the summon spot | `server/sim/src/ship.rs` |
| module | Summon: from the bag onto land or water, refused aboard, dead, not a vehicle, someone else's item, a second of the same medium, a bad spot, a crowded spot; a land and a sea vehicle together | `server/world/src/tests/vehicles.rs` |
| module | Stow: back into the bag; refused with cargo, by others, with a full bag; passengers put off and snapped | same |
| module | Drive: only the owner takes the helm; a cart drives on land, stops at a steep bank and at the sea, and carries its passengers up and down with zero corrections | same |
| module | Cargo: load by anyone in reach, refused out of reach, without a pack, into a full or missing slot; unload only by the owner, refused with a full back; theft logged; the pack keeps its age; dying aboard fills a free slot; drop on deck refused | same |
| C# | The terrain port matches `sim::terrain::ground` on generated vectors; the predictor stops a cart at a bank like the server's step; land stand vectors | `client/tests/SimVectorTests.cs`, `ShipTests.cs` |
| scenario | `loadbot vehicle`: on uploaded test ground with a slope and a shore, a bot summons a cart, loads a dev pack, boards and drives it to the shore while a passenger rides; a second bot loads its own pack, fails to unload the first's and fails to take the reins; the owner unloads and stows. It then summons a skiff, rows it with a passenger and stows it with the passenger put in the water. Pass: no corrections for honest moves, every refusal seen | `cargo xtask bot vehicle` |

### Results, 2026-10-31 (Linux laptop, local server)
- **Tests:** 8 module tests (`tests/vehicles.rs`), 5 `sim::ship` tests for footing, summoning and slots, 2 contentgen tests, and on the client the terrain and footing ports against 60 `ground` and 200 `vehicle_stand` vectors, plus 5 xUnit tests (the predictor stops a cart at a bank and rides a slope; the slot picker; V's target; the cargo line).
- **`bot vehicle`** passes on the world's ground (a cart at (−150, 30), a skiff at (−374, 30)) and on its own test shore on a cleared server. A drove the cart 18 m with B riding in the bed, deck 10.20 → 9.90 m, with **zero corrections** for either; every refusal in the scenario was seen.
- **`--vehicle-demo`:** the cart was summoned, loaded, driven 52 m with a bend, unloaded and stowed with **zero corrections**; the skiff was summoned off the west beach, boarded from the water and rowed at 4 m/s. The only two corrections were the snaps when stowing put the player off. Captures: `docs/captures/2026-10-31-vehicles-cart-loaded.png` (a Grain Bale in the cart's first slot), `-cart-driving.png` (on the bench, turning) and `-skiff.png` (rowing west).
- **Found by these runs:** the bench's prompt offered F to unload while F actually let go of the reins, and the climb prompt showed while F would load a slot; both prompts now say what F does. A demo route that turned the skiff ran it aground in the shallows, where the server and the predictor both stopped it.
