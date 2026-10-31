# Vehicles

Status: v0.2, 2026-10-31. M1 task "vehicles". First half (**Vehicles A**, §2): owned vehicles summoned from the bag, the Furrow Cart on land, the Driftpine Skiff, and pack slots on vehicles. Second half (**Vehicles B**, §2B): the Swiftgull built at a shipyard, wrecked vehicles and their cargo, mending and repairs, and a private view of one's own vehicles. Code: `content/ships.toml`, `server/sim/src/ship.rs`, `server/world/src/{ship,vehicle}.rs`, `sidecars/loadbot/src/vehicle.rs`; client `client/src/Sim/{Ship,Terrain}.cs`, `World/{ShipPredictor,ShipView}.cs`, `Gameplay/{ShipControls,Vehicles}.cs`, `UI/VehicleDemo.cs`. Builds on docs/design/ships.md (hulls, motion, decks, the helm) and docs/design/trade.md (packs). Decisions: ADR 0031 (A) and ADR 0033 (B). Vehicles B adds `server/world/src/shipyard.rs`, client `World/ShipyardRegistry.cs`, `Gameplay/{Shipyards,ShipyardModel}.cs`, `UI/ShipyardDemo.cs` and `sidecars/loadbot/src/shipyard.rs`.

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
| The dry dock was placed in fairly deep water from the ship's design plus a fee (10 lumber, 10 iron, 10 gold in 2014; 30 gold and 25 labor later) | B | [Fandom: Harpoon Clipper](https://archeage.fandom.com/wiki/Harpoon_Clipper), [MMO2Go, 2014](http://mmo2go.blogspot.com/2014/09/archeage-building-clipper.html) |
| A clipper took a lumber, an iron and a fabric pack, applied one at a time in that order for 25 labor each, then three more 25-labor steps with no materials; then the ship item appeared in the bag | B | same; [Fandom: Adventure Clipper](https://archeage.fandom.com/wiki/Adventure_Clipper) |
| Friends could carry packs to someone else's dock | B | MMO2Go, 2014 |
| A dock was protected for 3 days; after that it could be attacked and destroyed with everything put into it | B | same |
| A destroyed ship's summon item stayed with its owner; a consumable (Shatigon's Sand) restored it, and it could be summoned again after about 10–30 minutes | B/C | [Fandom: Lutesong Junk](https://archeage.fandom.com/wiki/Lutesong_Junk), [Steam thread](https://steamcommunity.com/app/304030/discussions/0/616189106591071559/), [old forum](http://forums.archeagegame.com/showthread.php?60671-Ship-destroyed-how-to-repair=) |
| A ship couldn't be dismissed in combat, while harpooned, or with a pack aboard | B | [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=416801428) |
| A sunk ship's packs went to the bottom (light ones floated) where anyone, pirates included, could take them | B/C | [old forum](http://forums.archeagegame.com/archive/index.php/t-11862.html), [Fandom: Merchant Schooner](https://archeage.fandom.com/wiki/Merchant_Schooner) |
| How ships were repaired at sea, and what it cost | **Unknown** | not found |
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

## 2B. Vehicles B: the shipyard, wrecks and repairs

### Content
- **Items** (`items.toml`): a new kind, **`design`** (stack 1), with the **Swiftgull Design** (`swiftgull_design`). Three material packs, **Timber Bundle**, **Ironwork Crate** and **Sailcloth Bale** (`timber_bundle`, `ironwork_crate`, `sailcloth_bale`), and a consumable, the **Hullmender's Kit** (`hullmenders_kit`).
- **Recipes** (`crafting.toml`): the design at the Joiner's Bench (Driftpine Plank 10, Sailcloth 2, Saltrope 4; 50 labor, 10 s) in place of the original's purchase. The three packs at the Packwright's Bench: Timber Bundle (Hollowoak Beam 4, Driftpine Plank 20), Ironwork Crate (Rustiron Bar 20, Rustiron Fittings 4), Sailcloth Bale (Sailcloth 6, Saltrope 6), 25 labor (B) and 10 s each. The kit at the Joiner's Bench (Driftpine Plank 4, Rustiron Nails 4, Saltrope 1; 10 labor, 4 s). Material packs are also sellable, for less than a Grain Bale (`trade.toml`).
- **Hulls** (`ships.toml`) gain **`mend_kits`** and **`mend_s`**: the kits and the wait to mend a wreck (Swiftgull 5 and 30 minutes, skiff 1 and 10, cart 2 and 10; the wait is B, the rest C).
- **`[vehicles]`** in `ships.toml`: `mend_item` (the kit), `repair_labor` 10, `repair_pct` 10 (of the hull's HP per kit), `under_fire_s` 10, `sink_s` 8.
- **`[[shipyard]]`** in `ships.toml`: `wire`, `id`, `name`, the `design` item (kind `design`, used by one shipyard), the sea `hull` it builds, `place_labor` 25, and its **`[[shipyard.stage]]`**s in order, each a `name`, `labor` and an optional `pack` (a pack item). The Swiftgull Shipyard: Keel and Frames (Timber Bundle, 25), Ironwork (Ironwork Crate, 25), Rigging (Sailcloth Bale, 25), then Fitting Out I–III (25 each, no pack) (B). Contentgen checks every reference and kind, 1–12 stages, labor 0–1,000, and that the last stage takes no pack; all of it goes into the hash.

### The shipyard
- **`place_shipyard(item_id)`**: the caller is in the world, alive and not aboard (swimming is fine), the item is a design in its bag, and its characters have no other shipyard (one each, ours). The yard's footprint is its hull's deck, placed exactly where `summon_vehicle` would put that hull (stern 1.5 m ahead, facing the character's yaw), and the hull must be afloat there. Refused "no room for it there" when a vehicle's centre or another yard's is closer than the two half-lengths. It takes the design and `place_labor` and makes a public **`shipyard`** row (Static tier): id, cell, kind, owner (the character), `x`, `z`, `yaw`, **`stage`** (stages done) and `placed_at`.
- **`work_shipyard(yard_id)`** does the next stage. The caller is alive and within `YARD_REACH_M` (4 m, horizontally) of the footprint, from land, water or a deck. A stage with a pack needs that pack on the caller's back ("this stage needs a Timber Bundle"), unspoiled, and takes it. Every stage spends its labor from the caller's account. **Anyone** may do a stage, except the last: only the owner's account may launch.
- **The launch** (the last stage) puts the hull's vehicle item into the owner's bag ("no room in the bag" otherwise) and deletes the yard. The ship isn't put in the water: summon it as any other.
- **`abandon_shipyard(yard_id)`**: the owner's account removes its yard, and everything put into it is lost.
- Yards are never attacked in M1 (sieges and structure damage come later), so the 3 days of protection has nothing to protect against; an unfinished yard stays until it's finished or abandoned.

### Damage, sinking and wrecks
- Cannon hits (and dev damage) go through one rule, `ship::damage`: HP falls, and **`vehicle.hit_at`** (public) is set. A vehicle hit in the last `under_fire_s` is **under fire**: it can't be stowed ("it's under fire") or repaired.
- **At 0 HP** the vehicle is **sinking** (or, for a cart, breaking): the helm is let go, it takes no more input or damage, and a private scheduled **`vehicle_wreck`** row fires `sink_s` later. Clients draw a hull with 0 HP settling into the water.
- **The wreck** (`vehicle_wreck` runs): each loaded pack becomes a public **`dropped_pack`** at its slot's world point: on the ground for a cart, or **floating** at sea level for a ship. A spoiled pack falls apart. Anyone may take them, and taking another account's pack is logged as theft as before. **`pick_up_pack` now works while swimming**, so floating packs can be taken from the water. Then the vehicle is removed as by stowing, with riders put off.
- **The item stays its owner's**, as a **wreck**: it goes back to the first free bag slot (or to `RETURNS` if the bag is full, as a demolished plot's deed does) with a private **`wreck`** row (item, holder, `ready_at`, empty until mending starts), read through **`my_wrecks`**. Summoning a wreck is refused: "it's wrecked: mend it" or "still being mended". **`mend_vehicle(item_id)`** takes the hull's `mend_kits` kits from the bag and sets `ready_at` to now plus `mend_s`, computed from that timestamp and never ticked. The first summon after `ready_at` deletes the row. Destroying the item deletes its wreck row too. Unowned (dev) vehicles just go.

### Repairs
- **`repair_vehicle(vehicle_id)`**: the caller is alive and aboard the vehicle or within its half-length plus 3 m of its centre (horizontally), and the vehicle is above 0 HP, below full and not under fire. It takes one kit and `repair_labor` labor and restores `repair_pct` of the hull's HP, up to full. **Anyone** may: crews repair, and so can a boarder.

### Seeing one's own vehicles
- **`my_vehicles`** (a view): the `vehicle` rows of the caller's characters, wherever they are. **V** stows the vehicle we're aboard, else our nearest in sight, else any of ours from `my_vehicles`, so a vehicle left far away can be stowed.

### The client
- **Shipyards** are drawn as a scaffold the size of the deck, with the hull rising in it stage by stage, and a label with the name and stage. **F** within reach does the next stage; the prompt says which, what it needs, and its labor: "F: deliver the Timber Bundle (Keel and Frames, 25 labor)", "F: launch the Swiftgull". Right-clicking a design in the bag places the yard ahead of us.
- **Wrecks** in the bag are marked "wrecked" or "mending, N min"; right-clicking a wreck mends it.
- **H** repairs the vehicle we're aboard, or our nearest in reach. The HUD shows "sinking" at 0 HP and "under fire" while it lasts.
- `--shipyard-demo` places a yard with dev items, delivers the three packs, fits it out, launches, summons the Swiftgull, loads a pack, has it wrecked with dev damage, takes the floating pack from the water, mends it with the dev clock and summons it again.

### Dev
- `dev_damage_vehicle(vehicle, amount)` damages any vehicle through `ship::damage`.

## 3. Deviations and not yet done
- **The Swiftgull has two pack slots** where the original's smallest clipper had none, so M1 can sail packs before the merchant ship (M2).
- **One land and one sea vehicle out per character**, our choice. **Vehicles stay out** after logging off and can be stowed from anywhere; the original's rules aren't known.
- **No fuel or boosts**, no wagon or hauler upgrades, no seats: riders stand on the deck or the cart's bed.
- **Only the owner drives and unloads** until parties exist.
- **Carts don't collide** with each other, characters, stations or plots, and don't take damage from falls. Cannons can hit a cart standing on the beach, since they hit anything at the landing point.
- **The design is crafted**, not bought, and placing a yard costs no gold or materials, only the design and labor: there are no merchants yet.
- **One shipyard per character**, ours. **Yards can't be attacked** until structures can be (M2 or later), so the original's 3 days of protection is moot.
- **Sunk packs float** at the surface instead of sinking to the bottom: characters can't dive yet.
- **Mending** takes our own kit, crafted, in place of the original's bought consumable. **Repairs at sea** are ours entirely: the original's weren't found.
- **Anyone may repair** a vehicle, and only damage from cannons exists: skills and NPCs don't hit vehicles yet.

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
| content | Designs, shipyards and stages, the new hull fields and `[vehicles]` reach both outputs and the hash; a missing or wrong-kind design, pack or hull, a land hull, a shared design, a last stage with a pack, too many stages are reported | `server/contentgen/src/lib.rs` |
| sim | Yard reach round the footprint; repair amounts up to full; under fire | `server/sim/src/ship.rs` |
| module | Place a yard: refused aboard, not a design, on land, crowded, with a second yard; takes design and labor | `server/world/src/tests/shipyard.rs` |
| module | Work: each stage in order; refused out of reach, without the pack, with the wrong pack, a spoiled pack, without labor; a helper does a stage; only the owner launches; full bag refused; the launch grants the item and removes the yard; abandon | same |
| module | Damage: under fire refuses stow and repair; 0 HP sinks, lets go of the helm and takes no more damage; the wreck floats packs at sea and drops them on land, puts riders off, returns the item as a wreck (to `RETURNS` with a full bag); a swimmer picks up a floating pack; summon refused until mended; mend takes kits and waits by the clock | same |
| module | Repair: by anyone aboard or near, takes a kit and labor, caps at full; refused at full, sinking, under fire, out of reach; `my_vehicles` lists only one's own | same |
| C# | The yard prompt's stage, pack and launch lines; reach; wreck labels; V's far target | `client/tests/ShipyardTests.cs` |
| scenario | `loadbot shipyard`: A places a yard on the world's sea, B delivers the Timber Bundle, A the rest and fits it out; B can't launch, A launches, summons the Swiftgull and loads a pack; dev damage sinks it; B swims to the floating pack and takes it (theft logged); A mends, waits by the dev clock and summons again; B repairs it after damage. Pass: every stage and refusal seen, no corrections | `cargo xtask bot shipyard` |

### Results for Vehicles A, 2026-10-31 (Linux laptop, local server)
- **Tests:** 8 module tests (`tests/vehicles.rs`), 5 `sim::ship` tests for footing, summoning and slots, 2 contentgen tests, and on the client the terrain and footing ports against 60 `ground` and 200 `vehicle_stand` vectors, plus 5 xUnit tests (the predictor stops a cart at a bank and rides a slope; the slot picker; V's target; the cargo line).
- **`bot vehicle`** passes on the world's ground (a cart at (−150, 30), a skiff at (−374, 30)) and on its own test shore on a cleared server. A drove the cart 18 m with B riding in the bed, deck 10.20 → 9.90 m, with **zero corrections** for either; every refusal in the scenario was seen.
- **`--vehicle-demo`:** the cart was summoned, loaded, driven 52 m with a bend, unloaded and stowed with **zero corrections**; the skiff was summoned off the west beach, boarded from the water and rowed at 4 m/s. The only two corrections were the snaps when stowing put the player off. Captures: `docs/captures/2026-10-31-vehicles-cart-loaded.png` (a Grain Bale in the cart's first slot), `-cart-driving.png` (on the bench, turning) and `-skiff.png` (rowing west).
- **Found by these runs:** the bench's prompt offered F to unload while F actually let go of the reins, and the climb prompt showed while F would load a slot; both prompts now say what F does. A demo route that turned the skiff ran it aground in the shallows, where the server and the predictor both stopped it.

### Results for Vehicles B, 2026-10-31 (Linux laptop, local server)
- **Tests:** 7 module tests (`tests/shipyard.rs`): placing, every stage and refusal, abandoning, under fire and repairs, sinking with a floating pack taken by a swimmer and the wreck mended, a broken cart's pack on the ground with the wreck held in `RETURNS`, and `my_vehicles`/`my_wrecks`. 2 `sim::ship` tests, 2 contentgen tests, and on the client 120 `yard_reach` vectors plus the repair and under-fire vectors and 4 xUnit tests (the yard prompt's lines, its reach, stage and wreck labels, V's out-of-sight target and H's target).
- **`bot shipyard`** passes on the world's sea off the west beach at (−400, 30) and on its own test shore on a cleared server, and twice in a row, so its cleanup works. Every stage and refusal was seen. The Swiftgull went down 8.0 s after the killing hit, and B took A's floating pack at sea level. The only corrections were the two snaps as the sinking put A and B in the water.
- **`--shipyard-demo`:** placed a yard off the west beach, built it through all six stages, launched and summoned the Swiftgull, loaded a pack, repaired it after a hit, sank it, took the floating pack from the water, mended the wreck by the dev clock and summoned it again. The one correction was the snap into the water as it sank. Captures (`--cam-yaw 60 --cam-distance 26`, a new client option): `docs/captures/2026-10-31-shipyard-building.png` (the Rigging prompt with the Sailcloth Bale carried), `-fitting-out.png` (the launch prompt, the hull grown in its scaffold), `-launched.png` (the Swiftgull out with its pack), `-sinking.png` (awash) and `-floating-pack.png` (the Grain Bale by the swimmer).
- **Found by these runs:** `take_helm` checks reach before sinking, so the bot now checks stowing a sinking ship instead. Floating packs sank out of sight in the waves, so the client now draws them 0.25 m higher. The follow camera was too close for a yard, hence `--cam-distance`.
