# Research notes: ArcheAge, SpacetimeDB, Godot

Compiled 2026-10-25 as the basis for [PLAN.md](PLAN.md). ArcheAge mechanics below are paraphrased from public sources. The numbers are reference points for tuning. They are not values to copy.

The fidelity target is **the final Western version, as it stood at shutdown in June 2024**. §2 covers the classic systems, which were all still in the game at shutdown. §2A covers what changed or was added between 2016 and 2024.

**Confidence:** **A** = official, or several sources agree · **B** = one fan/community source, or describes an older or different version · **C** = recollection not confirmed in this pass; verify before relying on it.

---

## 1. ArcheAge at a glance

| | |
|---|---|
| Developer | XL Games, directed by Jake Song (previously of *Lineage*). **A** |
| Engine | CryEngine 3. **A** |
| Development | Started in 2006. About two years of closed testing (five closed betas plus an open beta) before launch. **A** |
| Launch | Korea 15 Jan 2013. NA/EU 16 Sep 2014 (Trion, later Kakao Games). **A** |
| *Unchained* | Relaunched in Oct 2019 as buy-to-play with a cosmetic-only shop. **A** |
| Western shutdown | Announced 25 Apr 2024. NA/EU servers closed 27 Jun 2024. **A** |
| Successor | *ArcheAge Chronicles* (announced Sep 2024). Action combat and smaller group sizes. Early access planned for late 2026 (Steam, plus PS5 and Xbox Series). Recent reports say XL Games is self-publishing, and that Jake Song is reportedly leaving (unconfirmed). **B** |
| Genre label | "Sandpark": sandbox systems inside a themepark quest structure. **A** |

**Implication:** the official successor is moving away from the classic design. That leaves room for a game faithful to the classic version. The rights holder is still active, though, so everything expressive must be original (see PLAN §1).

---

## 2. Systems inventory (the classic game)

### 2.1 World, factions, PvP zones
- One seamless world. A western and an eastern continent are separated by ocean. A northern continent was added later for player castles. There are islands, including a neutral one mid-ocean with special trade vendors. **A/B**
- Three factions: West, East, and Pirates. Players became pirates by building up infamy (§2.8). **B**
- Contested zones cycle through peace, rising tension or conflict, war, and back to peace. Tension rises with PvP deaths in the zone, and war lasts a set period. Cross-faction PvP is off during peace. Open ocean and some zones never reach peace, and some zones run special events during war. The exact scoring was never published; players reported tension levels 1–5. **B**
- Players earn or lose honor in wartime PvP. **B**

### 2.2 Characters, classes, combat
- At launch there were two races per faction, and two more races came later. **C**
- Classes are built from **skillsets**. There were ten at launch: Archery, Auramancy, Battlerage, Defense, Occultism, Shadowplay, Songcraft, Sorcery, Vitalism and Witchcraft. Swiftblade and Malediction came later. A character picks three, which gives 120 combinations (220 with twelve). Every combination has its own class name; for example, Battlerage + Shadowplay + Auramancy is "Darkrunner" and Vitalism + Defense + Auramancy is "Templar". **A**
- Skillsets were chosen early, around levels 1, 5 and 10, and could later be swapped for a gold fee. **B**
- Combat is tab-target with heavy crowd control and **combos**: one skill applies a status, and other skills (often from another skillset) get bonus effects against it. **A** for the general system, **C** for specific interactions.
- Level cap was 50 at launch, with "ancestral" progression added later. **C**

### 2.3 Labor and proficiencies
- **Labor points** are spent on gathering, harvesting, crafting, making trade packs and similar work. **A**
- Regeneration in the original: free players got 5 points per 5 minutes, online only. Subscribers ("Patrons") got 10 per 5 minutes, online and offline. *Unchained* gave 10 per 5 minutes whether online or not, with a cap of 5,000. **A/B**
- Tying regeneration to login and subscription status drew complaints for years. **A**
- There are about 22 **proficiencies** in three groups:
  - Harvesting: farming, husbandry, gathering, mining, logging, fishing.
  - Crafting: alchemy, cooking, machining, construction, metalwork, tailoring, leatherwork, weaponry, carpentry, printing, handicrafts, masonry, and others.
  - Special: commerce, larceny, and others.

  Each has ten ranks, and higher ranks lower labor cost and crafting time. Only a limited number can be maxed, which forces specialization. **B**
- Labor was pooled across all characters on an account. **C**

### 2.4 Land: housing and farming
- Houses and farms are placed freely on open-world land inside designated areas. Nothing is instanced. **A**
- Footprints: an 8×8 scarecrow garden (from an early quest) upgrades to a 16×16 farm. Houses come in 16×16, 24×24, 28×28 and 44×44. **B**
- Property tax is paid weekly in tax certificates, with a deposit up front. The rate rises with the number of properties owned, steeply after two. If tax goes unpaid, the property enters a demolition phase and is removed at a fixed time of day, freeing the land. Players camped plots that were about to be demolished. **B**
- Crops, trees and livestock have fixed growth times. **Climate zones** from tropical to subarctic speed up crops suited to them. Without water, feed or medicine, they wither and die. Mature ones can be harvested repeatedly or removed for base materials. Livestock occasionally produces mount or pet offspring. Trees supply timber for houses and ships. **B**
- Planting on unclaimed land is allowed, but anyone can harvest or uproot the plants, which earns crime points. Small public farms protect produce. **B**
- Scarce land led to "land rush" events and to hoarding through alt accounts. **B**

### 2.5 Trade packs
- Packs are crafted at specialty workbenches from regional produce and worn on the back. The carrier is slowed (**A**) and cannot glide or ride most mounts (**C**). Packs expire after about six days (**B**).
- Payout depends on the destination; in general, farther pays more. It comes as gold or as a special currency (used for ships and other goods), or the pack is used in construction. The neutral mid-ocean island paid the best special-currency rates for cross-ocean runs. **B**
- Outlet demand dropped as packs were delivered and recovered over time. **C**
- Packs could be stolen in contested land and at sea, which is the core risk and reward. **A**
- A 2017 revamp turned packs into "cargo" at trade towers, with value that decays over time down to an 80% floor. This is post-classic. **B**

### 2.6 Vehicles, mounts, gliders
- Carts and wagons carry several packs. Mounts level up and learn skills. Gliders let players cover ground quickly from height. Submarines came later. **C**

### 2.7 Ships and naval combat
- Players build ships from a design plus materials. **A**
- Classic lineup, using fan-wiki figures (**B**):

  | Ship | Speed | HP | Cannons | Role |
  |---|---|---|---|---|
  | Clipper | ~11 m/s (fastest) | | 1 | Scouting, quick runs, hit-and-run |
  | Harpoon clipper | | | Harpoon | Pulls ships and players in; pirates used it to catch traders |
  | Merchant schooner | ~10 m/s | ~42k | 2 | About 20 pack slots |
  | Galleon / warship | | ~52k | 8 | Upgraded variants with more HP |

- Ship combat means maneuvering for broadsides. The crew splits into helm, cannons, repairs and harpoon, and boarding is part of it. If the helmsman dies, the ship drifts. Sea bosses take several ships. **B**

### 2.8 Crime and justice
- Crime points come from attacking or killing same-faction players (which requires opting into a hostile flag), harvesting other players' crops, and stealing packs. **B**
- Crimes leave evidence that witnesses can report: bloodstains for violence, footprints for theft. **B**
- At 50 crime points or more, a PvP death sends the player to trial. They can plead guilty for a shorter sentence or face a jury of five volunteers (level 30+, no infamy). Prison disables combat, mounts and pets. Prisoners can work off their time or try to escape. **B**
- Each crime point also adds a permanent infamy point. Around 3,000 infamy turns a player into a pirate. **B**
- Known abuse: jurors extorted defendants and faced little accountability. **B**

### 2.9 Castles, sieges, nations
- On the northern continent, a guild that completed a crystal objective could claim land and build a castle. Castles are attacked in sieges, and capturing the crystal takes the castle. **A**
- Castle-owning guilds could found player nations. **C**

### 2.10 Gear progression
- Items have grades. Regrading uses printed scrolls (weapon, armor, accessory), plus optional charms for better odds. A failed regrade can demote or destroy the item. *Unchained* added "crystallization" above a high tier. Synthesis was the safer path. **B**
- The grade ladder as recalled: basic, grand, rare, arcane, heroic, unique, celestial, divine, epic, legendary, mythic. **C**

### 2.11 Lessons from its history
- Players praised the systems. The main complaints were monetization (pay-to-win items, buying labor) and access to land. *Unchained*'s cosmetic-only relaunch supports this reading. **A/B**, interpretation.
- Bots and movement hacks (speed, teleport) kept recurring. **C**
- Player juries can be weaponized, and scarce land leads to hoarding by absentee owners. **B**

### 2.12 Verify before locking numbers
Still unconfirmed: zone-cycle thresholds, pack payouts and demand curves, climate multipliers, crop timers, regrade odds, stats for every hull, the race list, the level cap, and whether labor is account-wide. Confirm them from archived wikis (fandom, gamepedia), period guides and gameplay video. **Never** use data extracted from the client.

---

## 2A. The final Western version (2016 → June 2024)

### 2A.1 Timeline of major Western updates

| When | Update | What it added that matters here | Conf. |
|---|---|---|---|
| 10 Dec 2016 | 3.0 "Revelation" | Dwarves (West) and Warborn (East) become playable, with racial combat built on transformations. Fresh-start servers. | A/B |
| 7 Jun 2017 | "Erenor Eternal" | **Ancestral levels** above level 55 (7 at first), whose ancestral skills modify core skills with elemental variants. **Erenor**, a crafted gear tier that continues Divine Ayanad gear: you "uncloak" it and pick the variant, with no randomness. **Regrading rework**: a new top grade, "Eternal", plus item levels, socket slots, a displayed success chance, and three difficulty tiers by item type. New zones: an ocean strait, a secluded island, a harbor, and a housing province. **Open-world PvE battlegrounds**, where players defend towers with skills, mines and artillery for honor and crafting materials. **Trade revamp**, adding trade outposts at ports, in inland cities and overseas. | B |
| 18 Jul 2017 | "Reign: Part 1" (fresh-start servers) | Level cap 55 and the Dwarf and Warborn starting zones arrive on fresh-start servers. | B |
| 2017 | Trade rework, continued | Packs are converted into **cargo** at trade towers for overseas delivery. Cargo loses value over time, down to an 80% floor. | B |
| 2019 | 5.0 "Relics of Hiram", 5.5 "Echoes of Hiram" (26 Jun 2019) | A higher tier of **Hiram** gear to awaken. Western Hiram Mountains endgame zone. | B |
| 15 Oct 2019 | *Unchained* launch | Buy-to-play version, merged back into the main game in late 2023. | A |
| 2020 | "Garden of the Gods"; Ipnysh Sanctuary (13 Aug 2020) | A dungeon on the battleground island, unlocked by finishing the expansion's main story. A gear set whose bonuses add effects to skills. A "Gods' Authorities" feature. A planned zone cap of 150 players per faction. | B |
| 25 Mar 2021 | "Akasch Invasion" | **Gunslinger** skillset (rifles, ranged area damage, debuffs), bringing class combinations to 286. A new zone, a new raid, and a new story chapter. | A |
| 3 Dec 2021 | Kakao relaunch | **Spelldance** skillset (heals allies, damages enemies). A five-player dungeon whose difficulty scales with a power level you choose. **Guild residences** at guild level 5. A season pass ("ArchePass") and the Patron subscription. | A |
| 2022 (Mar, 23 Jun) | "Great Prairie of the West" | A new region. The **Guardian Scramble** PvP mode. A new cloak tier. The northern continent becomes a **conflict zone while raid monsters are up**. Class balance changes. | B |
| Mar 2023 | Spring update | A raid instance reworked to a single difficulty, available twice a week. An arena split out as its own instance. Another cloak tier. A second reward track. Menu cleanup. | B |
| 22 Jun 2023 | Summer update | **Daru transformation**: a non-combat lifestyle form for gathering and trade. It hauls packs faster with a donkey, breathes underwater, swims faster and has its own bag, but loses racial and combat skills and can't enter instances. Also a **10v10 flag war** and a cross-region **3v3 arena** (beta). | B |
| Nov–Dec 2023 | Merge | *Unchained* merges into the main game, leaving one server per region. The large scheduled faction battle (Mistmerrow) returns. World bosses spawn twice a day. | B |
| 25 Apr 2024 | Final content patch | Contents not found in this pass. | — |
| 27 Jun 2024 | Shutdown | NA/EU servers close. The game continues in Asia. | A |

Planned but never shipped in the West: a female-only fairy race, which appeared on the 2023–2024 roadmaps. **B**

### 2A.2 State of the game at shutdown

| Area | At shutdown | Conf. |
|---|---|---|
| Races | Six, three per continental faction: the original four plus Dwarves and Warborn. | B |
| Skillsets | Fourteen by our count: the ten launch skillsets plus Swiftblade, Malediction, Gunslinger and Spelldance, giving 364 combinations. One wiki calls Spelldance the "12th", so verify. | B |
| Leveling | Cap 55, then ancestral levels. In *Unchained*, ancestral levels were unlocked by spending honor. Skill morphs came at ancestral levels 1, 4, 7, 10 and 13, with two morphs per eligible skill and five eligible skills per skillset. | B |
| Gear | Grades up to "Eternal", with item level and sockets. A crafted tier that evolves when other gear is fed into it. An awakening tier upgraded with infusions, cheap in gold but heavy in labor, whose first two stats lock at the third tier. Tempering, which can fail and lose temper at high levels. Gems. Synthesis as the safe path. Set bonuses that modify skills. Cloak tiers. | B/C |
| Trade | Regional packs sold at trade outposts (inland, at ports, overseas), with cargo for overseas routes, and a special currency used for ships. | B |
| PvP | Zone cycles, honor, a permanent war state at sea, castle sieges, open-world battlegrounds, arenas (a free-for-all arena from the 2016 era, then 3v3), a 10v10 flag war, Guardian Scramble, a scheduled large faction battle, and conflict in the north while raid monsters are up. | B |
| PvE | Dungeons (including power-scaled ones), raids, world bosses twice a day, sea bosses, and story chapters ("chronicles"). | B |
| Social and housing | Guild levels and residences, families, housing provinces. | B |
| Monetization | A cash shop (credits), a prestige shop, season-pass tracks, and a Patron subscription that gave offline labor regeneration and other perks. All of this is excluded from the remake (PLAN §1.2). | A/B |

### 2A.3 Still unknown (research backlog)
- The contents of the final patch (April 2024)
- "Gods' Authorities" and the "Mistsong Banquet"
- Aquafarms (farms on water): recalled, not confirmed
- The maximum ancestral level
- The full grade ladder above Divine
- The exact Daru rules
- The rules for the flag war and Guardian Scramble
- Sport fishing details
- The formula for power-scaled dungeons

---

## 3. SpacetimeDB (current release v2.13.0, 21 Oct 2026)

### 3.1 Model
- SpacetimeDB is a database that also runs your server logic. Modules are written in Rust, C# or C++ (compiled to WebAssembly) or TypeScript (run on V8). Tables live in memory and persist through a commit log. Reducers are the only way to change state, and each one runs as a transaction.
- Clients subscribe to queries and keep a local cache that is always a consistent subset of committed state. Each committed transaction sends each client at most one update, in commit order.
- **Each database executes on a single thread.** This is deliberate. Throughput scales by splitting work across databases.

### 3.2 Features that matter here

| Feature | State as of Oct 2026 | Use in this project |
|---|---|---|
| Tables, btree indexes, primary key / unique / auto-increment | Stable | Everything |
| Reducers, including lifecycle reducers (`init`, connect, disconnect) | Stable | All state changes, sessions |
| Schedule tables (`Interval`, `Time`) | Stable. An interval tick that overlaps a slow tick is skipped, and timing isn't guaranteed. Since 2.10.1, scheduled calls run concurrently with procedures. | World tick, timers |
| Event tables (`#[table(..., event)]`) | Rows exist only within their transaction. Clients get `on_insert` only. Can't be the lookup side of a join and can't be read from views. Requires the v2 protocol. | Combat log, damage numbers, effects, public chat |
| Views (per-caller `ViewContext`, shared `AnonymousViewContext`) | Procedural views re-run in full when anything they read changes. Query-builder views update incrementally. Parameters are limited. | Per-player data, hidden information |
| Procedures (HTTP calls, explicit transactions) | Beta. The docs require opt-in flags, but a 2.7 commit removed the unstable gate; check the pinned version. | External services only (payments, Steam), not gameplay |
| Row-level security | Experimental | Don't rely on it at launch |
| Subscriptions | Typed query builder or SQL. Equality filters on indexed columns are cheapest. Subscribe to the new query before unsubscribing the old one. | Interest management |
| SpacetimeAuth | Beta, supports Steam tickets | Login |
| Migrations | Automatic and incremental | Schema changes |
| Unit tests against an in-memory datastore | Rust and TypeScript (2.12) | Server tests |
| C# client auto-reconnect with subscription replay | 2.12 | Client |
| Submodules | C# assemblies (2.13), reusable TypeScript submodules | Maybe later |
| Inter-database calls | Groundwork only as of 2.12. A third-party report says asynchronous inter-database calls and tiered storage ship 31 Oct 2026 under the name "Spacetime Continuum". | Future global database |

### 3.3 Official guidance for games (summarized)
- **Syncing nearby entities.** Divide the world into square chunks and store each entity's chunk in an indexed `chunk` column. Each client subscribes to the 3×3 chunks around its player and drops the old subscription in the new one's `onApplied`. Chunks should be at least as wide as the view distance. This limits what honest clients download but **hides nothing**: any client can subscribe to any public row.
- **Fixed tick.** Use a 50 ms interval schedule and compute `dt` from a stored last-tick timestamp. A tick that runs long blocks the whole database, and the ticks it overlaps are skipped. Skip idle entities and write only rows that changed.
- **Hiding information.** Use private tables plus per-player views filtered by `ctx.sender`. Don't leak information through error messages. The built-in RNG is seeded from the reducer timestamp, so it is predictable.
- **Performance.** Index the columns you filter on. Split tables by access pattern and use the smallest integer types. Keep tables private by default. Batch work into a single reducer call and expire old rows.

### 3.4 Performance figures
- In a vendor benchmark (May 2026; i9-14900K; TypeScript module; single node; 64 pipelined clients), SpacetimeDB handled about 280k–300k simple transactions per second, with p50 latency around 7–8 ms under contention. The benchmark says it does **not** cover large subscription fan-out, which is the critical case for an MMO. We have to measure that ourselves (PLAN, Phase 0).

### 3.5 Hosting, pricing, license
- **Maincloud** (managed):

  | Plan | Price | Included energy | Other |
  |---|---|---|---|
  | Free | $0 | 2,500 TeV/month (about 3M function calls) | |
  | Pro | $25/month | 100k TeV (about 120M calls), then 2,592 TeV per $ | Replication, 7-day point-in-time backups |
  | Team | $250/month | 250k TeV | 30-day point-in-time backups, dedicated nodes |
  | Enterprise | Custom | Custom | On-prem or your own cloud |

  Each database's module memory is limited to 8 GB. The limits page and the pricing page give different storage figures, so confirm them with Clockwork Labs.
- **Self-hosting.** The guide covers Ubuntu 24.04 with systemd and nginx/TLS. It doesn't cover high availability, replication or backups.
- **License.** Business Source License 1.1. Production use is allowed if you run **at most one SpacetimeDB instance in production** and don't offer a database service; anything beyond that needs a commercial license. It converts to AGPL-3.0 with a linking exception on 14 Oct 2031. This is not legal advice. Ask Clockwork Labs whether "instance" means one host or one database.

### 3.6 BitCraft, the precedent
- Clockwork Labs' sandbox MMO runs on SpacetimeDB. The server module code was published 21 Jan 2026 at `github.com/clockworklabs/BitCraftPublic`. It's Rust. The README says the code is Apache 2.0, with restrictions on running official or competing servers. The client and assets aren't included.
- The server is split into a **global module** and **region modules**, a split that lets the world grow without a fixed size limit. Packages include `game` and `global_module`, plus custom Rust macros for shared tables, timers and feature gating.
- Phase 2 of the open-sourcing (planned for about mid-2026) covers sidecar services: a relay server, a mob manager and a map server. In other words, heavy or specialized work runs alongside the database rather than inside it.

---

## 4. Godot

### 4.1 Engine
- **4.6** (27 Jan 2026): Jolt is the default 3D physics engine for new projects. Adds a new IK framework (TwoBoneIK3D, FABRIK3D), an overhaul of screen-space reflections, stable node IDs, and LibGodot.
- **4.7** (stable around 19 Jun 2026; 4.7.2 released 18 Aug 2026): adds HDR output, AreaLight3D, DrawableTexture2D, and a new Godot Asset Store.

### 4.2 SpacetimeDB client options
- **Official:** the NuGet package `SpacetimeDB.ClientSDK.Godot`, which shares code with the C# SDK. It requires the .NET build of Godot 4.6.2 or later; the tutorial project targets `net8.0`. Calling `STDBUpdateManager.Add(conn)` drives the connection from Godot's update loop. `.WithAutomaticReconnect()` turns on reconnects. Bindings come from `spacetime generate --lang csharp`. The tutorial's server module is offered in C#, Rust and C++.
- **Unofficial:** a pure-GDScript SDK on the Godot Asset Store (Godot 4.4+, SpacetimeDB 2.2–2.6). It has no reviews yet.

### 4.3 Terrain and world size
- **Terrain3D** supports worlds up to 65.5×65.5 km, and more with vertex spacing. Each region is saved as a separate file, so people can edit in parallel without merge conflicts. It includes a foliage instancer.
- In a standard single-precision build, precision problems start 10–40 km from the origin. Double precision means building Godot **and** Terrain3D from source; Terrain3D calls this experimental. For C#, the .NET assemblies and NuGet packages must also be built with `--precision=double` and served from a local feed.

### 4.4 Ocean
- `tessarakkt/godot4-oceanfft` is open source: Tessendorf FFT waves and buoyancy on compute shaders. Its last push was Nov 2025, so check 4.7 compatibility.
- **OceanPro** (US$25): GPU FFT, surface sampling and buoyancy. Requires Godot 4.7, Forward+/Vulkan, Windows.
- **Ocean3D** (itch.io): a single Gerstner function drives both the shader and the physics. Includes multi-probe buoyancy and wakes. Targets Godot 4.7.

### 4.5 Tooling and assets for a Claude-built, stylized project
- **Free (CC0) stylized characters and animation**
  - **Quaternius Universal Base Characters:** six models (superhero, regular and teen proportions, each male and female) on a humanoid rig, with a Godot 4.3 source project.
  - **Quaternius Universal Animation Library:** 120+ animations on the same rig in the paid "Source" tier, and 45 in the free tier. Covers 8-direction locomotion, jog, sprint, push, crawl, swim, sit and deaths. A second volume exists but wasn't reviewed.
  - **KayKit Adventurers:** 4–5 characters, 75 animations, 25+ accessories.
  - **KayKit Series 4 and 5:** 15 and 14 characters.
  - **KayKit Character Animations:** 133 humanoid animations, on KayKit's own rigs.

  All of these are CC0. KayKit asks that unmodified copies not be resold.
- **Kenney:** CC0 environment, UI and audio packs. Well known, but not reviewed in this pass. **C**
- **Test tools**
  - **GdUnit4 v6.0.0** (Oct 2025, built on Godot 4.5 mono). C# support comes through gdUnit4Net, with a VSTest adapter, a command-line runner, JUnit XML output and a GitHub Action.
  - **Chickensoft GoDotTest 2.0.44:** a C# test runner run with `--run-tests` and `--quit-on-finish`, with coverage support.
  - **Chickensoft GodotTestDriver:** simulates input and UI interaction.

  Godot 4.7 support is unverified for all three.
- **Capturing frames for review.** Godot's Movie Maker mode, `--write-movie <path> --fixed-fps 30`, records deterministic frames, including from exported builds. Relative output paths resolve against the project folder.
- **SpacetimeDB on Windows.** Install with the PowerShell one-liner (`iwr https://windows.spacetimedb.com -useb | iex`) and switch versions with `spacetime version use`. Check spacetimedb.com/install for the current instructions.
- **Hosting prices** are moving. Hetzner raised dedicated-server prices in April 2026 (for example, the AX42 went from €56.29 to €68.19 a month), and its listings changed again mid-2026. Check current offers before budgeting.

---

## 5. Other references
- **AAEmu** is an open-source ArcheAge server emulator in C# (LGPL-licensed) and isn't affiliated with XL Games. It runs against the original client, and its data comes from the original game, so it can't serve as a data source for an original game.

---

## Sources

**ArcheAge**
- [Wikipedia: ArcheAge](https://en.wikipedia.org/wiki/ArcheAge)
- [Engadget: 120 class combinations](https://www.engadget.com/2014-04-20-archeage-boasts-120-class-combinations.html) · [Ten Ton Hammer: skillset basics](https://www.tentonhammer.com/guide/archeage/archeage-skillset-class-basics-guide) · [The Games Wiki: original class system](https://thegameswiki.com/archeage-chronicles/wiki/class-system-original-archeage)
- [mein-mmo: labor regeneration problem](https://mein-mmo.de/en/archeage-why-archeages-main-problem-remains-unresolved-for-now,18892) · [mein-mmo: Unchained tips](https://mein-mmo.de/en/5-tips-about-archeage-unchained-that-i-wish-i-had-known-earlier,386893) · [Massively OP: labor change](https://massivelyop.com/?p=2642)
- [The Games Wiki: crafting and proficiencies](https://thegameswiki.com/archeage-chronicles/wiki/crafting-and-proficiencies)
- [ArcheAge Wiki: Property](https://archeage.fandom.com/wiki/Property) · [ArcheAge Wiki: Scarecrow garden](https://archeage.fandom.com/wiki/Scarecrow_garden) · [mein-mmo: building a house](https://mein-mmo.de/en/archeage-how-to-build-a-house,659624/) · [GameSkinny: first 16×16 farm](https://www.gameskinny.com/tips/archeage-guide-how-to-get-your-first-16x16-farm-improved-scarecrow-farm/)
- [Scan: ArcheAge overview (farming)](https://www.scan.co.uk/info/guide/archeage-new-mmo) · [TechRaptor review](https://techraptor.net/content/archeage-review)
- [ArcheAge Wiki: Trade Pack](https://archeage.fandom.com/wiki/Trade_Pack) · [Ten Ton Hammer: trade pack guide](https://www.tentonhammer.com/guide/archeage/archeage-trade-pack-guide) · [Massively OP: 2017 trade revamp](https://massivelyop.com/2017/06/07/archeage-erenor-eternal-update-dev-tour-trading-revamp-and-new-zones-live-tonight/)
- [The Games Wiki: naval combat](https://thegameswiki.com/archeage-chronicles/wiki/naval-combat)
- [ArcheAge Wiki: Justice System](https://archeage.gamepedia.com/Justice_System) · [The Games Wiki: crime and justice](https://thegameswiki.com/archeage-chronicles/wiki/crime-and-justice-system) · [PCGamesN: on trial](https://www.pcgamesn.com/archeage/i-was-put-on-trial-in-archeage-my-first-punishment-was-to-get-naked)
- [ArcheAge Wiki: PvP](https://archeage.fandom.com/wiki/PvP) · [Magic Game World: PvP regions](https://www.magicgameworld.com/archeage-unchained-pvp-regions-and-how-they-work/) · [GamePressure: PvP](https://www.gamepressure.com/archeage/9-pvp/z969ee)
- [Magic Game World: upgrading gear](https://www.magicgameworld.com/archeage-unchained-upgrading-gear/) · [Massively OP: regrade event](https://massivelyop.com/2016/06/09/archeage-makes-it-temporarily-safer-to-regrade-gear-drama-llamas-come-out-to-play/)
- [Massively OP: Chronicles early access this year](https://massivelyop.com/2026/08/11/kakao-q2-2026-archeage-chronicles-will-enter-early-access-this-year-as-kakao-centers-mmos-and-older-gamers/) · [Massively OP: June focus test](https://massivelyop.com/2026/07/03/archeage-chronicles-shares-player-activity-from-a-recent-june-focus-group-test/) · [GuildOrder: launch readiness](https://guildorder.com/games/archeage_chronicles/guides/launch-readiness) · [Sportskeeda: Jake Song report](https://sportskeeda.com/mmo/news-archeage-chronicles-still-planned-2026-release-lead-developer-jake-song-reportedly-steps-out)
- [AAEmu fork](https://github.com/Coyenn/AAEmu)

**ArcheAge, final version (2016–2024)**
- [Massively OP: Western sunset](https://massivelyop.com/2024/04/25/kakaos-western-version-of-archeage-is-sunsetting-in-june/) · [KitGuru: shutdown](https://www.kitguru.net/gaming/matthew-wilson/archeage-is-shutting-down-in-the-us-and-europe/)
- [MMOs.com: 3.0 Revelation](https://mmos.com/?p=144383) · [Massively OP: Reign Part 1](https://massivelyop.com/?p=174969)
- [DualShockers: Erenor Eternal](https://www.dualshockers.com/archeage-erenor-eternal-expansion-release-date-june-7-2017/)
- [mein-mmo: 5.0 Relics of Hiram](https://mein-mmo.de/archeage-5-0-relics-of-hieram/) · [gamigo: Echoes of Hiram](https://press.gamigo.com/the-echoes-of-hiram-ring-through-archeage) · [Steam discussion: gear upgrading](https://steamcommunity.com/app/1147660/discussions/0/1639801448908053060)
- [gamigo: Ipnysh Sanctuary out now](https://press.gamigo.com/archeages-garden-of-the-gods-the-ipnysh-sanctuary-out-now) · [Massively OP: Ipnysh Sanctuary live](https://massivelyop.com/?p=296102)
- [gamigo: Gunslinger](https://corporate.gamigo.com/en/presse/archeage-introduces-the-gunslinger-skillset/) · [mein-mmo: 286 combinations](https://mein-mmo.de/mmorpg-archeage-bringt-gunslinger-bald-286-klassen-kombos/) · [Massively OP: Akasch Invasion](https://massivelyop.com/?p=326282)
- [Game Developer: Kakao relaunch, Dec 2021](https://www.gamedeveloper.com/press-release/archeage-kicks-off-final-month-of-2021-with-fresh-servers-and-in-game-content-under-kakao-games-publishing-banner)
- [Massively OP: Great Prairie interview](https://massivelyop.com/?p=393138) · [Massively OP: Daru and flag war](https://massivelyop.com/?p=443961) · [mmorpg.gg: June 2023 update](https://mmorpg.gg/june-update-for-archeage-brings-new-features/)
- [Massively OP: Mistmerrow returns, Dec 2023](https://massivelyop.com/2023/12/14/archeage-releases-its-first-big-patch-post-unchained-as-the-battle-of-mistmerrow-returns/) · [Sportskeeda: 2024 roadmap](https://www.sportskeeda.com/mmo/archeage-2024-update-roadmap-faction-improvements-fairy-race)
- [Magic Game World: ancestral skills](https://www.magicgameworld.com/archeage-unchained-ancestral-skills/) · [Magic Game World: ancestral level](https://www.magicgameworld.com/archeage-unchained-character-level-ancestral-level/)

**Tooling and assets**
- [Quaternius: Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html) · [Quaternius: Universal Base Characters](https://quaternius.itch.io/universal-base-characters)
- [KayKit Adventurers](https://kaylousberg.itch.io/kaykit-adventurers) · [KayKit Character Animations](https://kaylousberg.itch.io/kaykit-character-animations)
- [GdUnit4](https://store.godotengine.org/asset/mikeschulze/gdunit4-unit-testing-framework/) · [Chickensoft.GoDotTest](https://www.nuget.org/packages/Chickensoft.GoDotTest)
- [Godot: creating movies (Movie Maker)](https://docs.godotengine.org/de/4.4/_sources/tutorials/animation/creating_movies.rst.txt)
- [Deskmodder: Hetzner price increase, April 2026](https://www.deskmodder.de/blog/?p=210160)

**SpacetimeDB**
- [Docs index (llms.md)](https://spacetimedb.com/llms.md) · [Releases](https://github.com/clockworklabs/SpacetimeDB/releases) · [License](https://github.com/clockworklabs/SpacetimeDB/blob/master/LICENSE.txt)
- [Godot tutorial part 1](https://spacetimedb.com/docs/tutorials/godot/part-1) · [part 2](https://spacetimedb.com/docs/tutorials/godot/part-2) · [NuGet: SpacetimeDB.ClientSDK.Godot](https://www.nuget.org/packages/SpacetimeDB.ClientSDK.Godot/) · [GDScript SDK (unofficial)](https://store.godotengine.org/asset/plaught-armor/spacetimedb-sdk/)
- [Sync nearby entities](https://spacetimedb.com/docs/how-to/games/sync-nearby-entities) · [Fixed-rate tick](https://spacetimedb.com/docs/how-to/games/fixed-rate-game-tick) · [Hide information](https://spacetimedb.com/docs/how-to/games/hide-information)
- [Schedule tables](https://spacetimedb.com/docs/tables/schedule-tables) · [Event tables](https://spacetimedb.com/docs/tables/event-tables) · [Views](https://spacetimedb.com/docs/functions/views) · [Procedures](https://spacetimedb.com/docs/functions/procedures)
- [Subscriptions](https://spacetimedb.com/docs/clients/subscriptions) · [Subscription semantics](https://spacetimedb.com/docs/clients/subscriptions/semantics) · [Performance](https://spacetimedb.com/docs/tables/performance) · [Benchmark essay](https://spacetimedb.com/essays/spacetime/03-performance-that-changes-the-design.md)
- [Pricing](https://spacetimedb.com/pricing) · [Limits](https://spacetimedb.com/limits) · [Self-hosting](https://spacetimedb.com/docs/how-to/deploy/self-hosting)
- [RuntimeWire: Oct 31 scaling features](https://runtimewire.com/article/spacetimedb-october-31-scaling-plan-single-thread)
- [BitCraftPublic](https://github.com/clockworklabs/BitCraftPublic) · [BitCraft open-sourcing update](https://bitcraftonline.com/news/bitcraft-open-sourcing-update)

**Godot**
- [Godot 4.6 release](https://godotengine.org/releases/4.6/) · [GameFromScratch: 4.6](https://gamefromscratch.com/godot-4-6-released/) · [Godot 4.7 RC1](https://godotengine.org/article/release-candidate-godot-4-7-rc-1/) · [GameDev.net: 4.7 released](https://gamedev.net/news/godot-47-released-r4011/) · [OpenSourceForU: 4.7.2](https://www.opensourceforu.com/2026/08/godot-4-7-2-released/)
- [Terrain3D: double precision and world size](https://terrain3d.readthedocs.io/en/latest/docs/double_precision.html) · [Godot: large world coordinates](https://docs.godotengine.org/en/4.4/tutorials/physics/large_world_coordinates.html) · [Forum: C# double-precision build](https://forum.godotengine.org/t/c-api-not-compiling-for-double-precision/70686)
- [godot4-oceanfft](https://github.com/tessarakkt/godot4-oceanfft) · [OceanPro](https://forum.godotengine.org/t/oceanpro-1-0-gpu-fft-ocean-system-for-godot-4-7/144004) · [Ocean3D](https://ocean3d.itch.io/ocean3d)
