# Plan: an ArcheAge-faithful sandbox MMO on SpacetimeDB + Godot

Status: v0.2, 2026-10-25. The research is in [RESEARCH.md](RESEARCH.md); references like R§2A point to its sections. Day-to-day progress will be tracked in `docs/STATUS.md` and `docs/BACKLOG.md`, which get created in milestone M0.

## Decisions so far

| Decision | Value | Decided by |
|---|---|---|
| Fidelity target | The NA/EU live game as it was at shutdown: final content patch 25 Apr 2024, servers closed 27 Jun 2024 | User, 2026-10-25 |
| World | One seamless 16×16 km map | User |
| Project type | Hobby project; donations pay for server upkeep | User |
| Art | Stylized | User |
| Who builds it | Claude, across many sessions. The user directs, playtests and approves. | User |
| Server | SpacetimeDB 2.x with Rust modules | Plan v0.1 |
| Client | Godot 4.7 .NET (C#) | Plan v0.1 |

---

## 0. Summary

**What it is.** A hobby MMO that recreates the systems of ArcheAge's final Western version and the way they fit together. The world, names, story and art are original, and the art is stylized. The game has:
- open-world land that players claim and can lose
- labor as the limit on all work
- trade goods that physically travel over land and sea, by pack, cargo and ship
- player-built ships and naval combat
- faction war cycles
- crime and jury justice
- castles in the north
- the late-game layers: ancestral levels, deep gear progression, arenas, battlegrounds, raids and transformations

**How it's built.**
- The server is a single SpacetimeDB database holding the whole world, written in Rust and self-hosted on one machine. That fits both the license and a donation budget.
- The client is the .NET build of Godot 4.7.
- The server decides every outcome except the player's own movement, which the client predicts and the server checks.

**How the work happens.** Claude builds the game in small tasks with tests, keeping its memory in the repository: `CLAUDE.md`, a status file, a backlog, design specs and decision records. The user steers, plays each milestone and decides on taste and money.

**Roadmap.** Milestones, not dates:
- **M0** Foundations
- **M1** "One Coast" core loop
- **M2** "Two Continents"
- **M3** "Open Sea & North"
- **M4** "Final Version" layer
- **M5** Public hobby launch

Every milestone ends with something playable. Reaching full parity with the final version is a multi-year hobby effort.

**Biggest risks.**
- The scope of the final version
- Keeping continuity across sessions
- Checking whether it's fun without human playtests
- Producing enough character and animation art
- Intellectual property, which matters more because the project accepts donations

---

## 1. Fidelity

### 1.1 Scope: the systems live at shutdown

The target is "everything the game had in June 2024" (R§2, R§2A). Content volume gets compressed (fewer zones, dungeons and items); the systems don't. The milestones are defined in §11.

| Area | What we reproduce | Milestone |
|---|---|---|
| Races | Six, three per continental faction. The two later races fight with transformation-based racial skills. | M1: 1 · M2: 4 · M4: 6 |
| Classes | Fourteen skillsets; each character picks three (364 combinations). Skills from different skillsets combo off each other. | M1: 3 · M2: 12 · M4: 14 |
| Leveling | Levels 1–55, then ancestral levels that unlock skill morphs | M2 · M4 |
| Labor and proficiencies | Labor drives all work. About 22 proficiencies with ranks, and a limit on how many you can max. | M1 · M2 |
| Land | Gardens, farms and houses on open-world land, plus housing provinces and guild residences. Weekly tax, then demolition if unpaid. | M1 · M2 · M4 |
| Farming | Crops, trees and livestock; climate zones; upkeep; theft; public farms. Farms on water if confirmed. | M1 · M2 |
| Trade | Regional packs sold at trade outposts (inland, ports, overseas). Cargo for overseas routes, with value decay. A special currency for ships. | M1: packs and outposts · M3: cargo |
| Travel | Carts and wagons; mounts with levels and skills; gliders; submarines | M1 · M2 · M3 |
| Ships | Rowboat, clipper, harpoon clipper, merchant ship, warships, fishing boat. Cannons and boarding. | M1 · M2 · M3 |
| Fishing and diving | Sport fishing from boats; diving and sunken treasure | M2 |
| Factions and PvP | West, East and Pirates. Zone war cycles, honor, a permanent war state on the open ocean. | M2 · M3 |
| Crime and justice | Evidence, reports, trial and jury, prison, infamy, becoming a pirate | M1 · M2 · M3 |
| The north | Castles, sieges and nations. The north turns into a conflict zone while raid monsters are up. | M3 |
| Gear progression | Grades up to a top "eternal" grade, with item level and sockets. Regrading with a visible success chance. Tempering, gems, synthesis. A crafted tier you evolve. An awakening tier. Set bonuses that modify skills. Cloaks. | M2 basics · M4 full |
| Group PvE | Dungeons, including power-scaled ones; raids; world bosses twice a day; sea bosses; story chapters | M2 · M3 · M4 |
| Instanced and event PvP | Arenas (free-for-all, 3v3), 10v10 flag war, a scramble mode, PvE battlegrounds (tower defense), a scheduled large faction battle | M3 · M4 |
| Lifestyle transformation | A trade-and-gathering form: faster pack hauling with a pack animal, water breathing, faster swimming, its own bag. No combat skills and no instances. | M4 |
| Social | Party, raid, guild (levels, residences), family, chat, mail, auction house | M1 → M4 |

**Left out:**
- every monetization system: cash shop, prestige shop, paid pass tracks, Patron perks, loot boxes
- the fairy race, which was announced but never shipped in the West
- the cross-region arena, since we have a single server

**Unknowns become research tasks**, done before the related system is built: the final patch contents, "Gods' Authorities", farms on water, the maximum ancestral level, the grade ladder above Divine, and the exact rules for each PvP mode (R§2A.3).

### 1.2 Deliberate changes
- **Everyone gets what paying players got.** Labor regenerates at the Patron rate (about 10 points per 5 minutes, online and offline), and every account can own land.
- **Donations buy nothing in-game:** no items, no cosmetics, no priority. A thank-you page outside the game is fine.
- **Built for a smaller population.** One server sized for a hobby (§4.6). Zone caps, siege sizes and world-boss tuning are scaled to match.

### 1.3 Must be original
- **Expressive content:** the game's name, map, lore, and every name of a race, faction, place, NPC, item, skill or currency. Also all text, art, models, animation, UI art, music and sound. Mechanics generally aren't protected by copyright, but all of this is, and "ArcheAge" is a trademark.
- **Data:** nothing from the original client or from anything derived from it (emulator databases, private-server data), not even numbers tables.
- **Branding:** the game and its donation page must present it as its own game, with no ArcheAge name, logo or art. Taking money raises the stakes of an IP complaint, so get a lawyer's opinion before going public. This plan isn't legal advice.

### 1.4 How fidelity is checked
- **A spec before every system.** Claude writes `docs/design/<system>.md`, covering:
  - how the original worked, with sources and confidence levels
  - our rules and numbers, under our own names
  - deliberate deviations, and why
  - the test plan
- **Sources:** archived wikis, period guides and patch notes. The user's memories of how something felt count too, and should be recorded in the spec.
- **Playtests.** At each milestone the user checks feel against memory, and the notes go into `docs/playtests/`.

---

## 2. Design pillars

| # | Pillar | Proof in the first playable slice |
|---|---|---|
| P1 | **Land you can lose** | Claim an open-world plot by placing a design. Pay weekly tax, or the plot enters a demolition window and frees up. |
| P2 | **Labor is the universal limit** | Gathering, harvesting, crafting and making packs all cost labor, which regenerates slowly and at the same rate for everyone. |
| P3 | **Goods travel physically** | Packs made from regional produce are carried slowly to outposts. They're worth more the farther they go, and they can be stolen. |
| P4 | **The sea is a place** | Player-built ships with holds, cannons and boarding. A crossing takes minutes, and you can see threats coming. |
| P5 | **Build your class** | Pick 3 of N skillsets. Skills from different skillsets combo off each other. Tab-target combat with crowd control. |
| P6 | **Politics with teeth** | Zone war cycles. Crimes against your own faction leave evidence and lead to trial. Pirates, castles and nations follow. |
| P7 | **A long climb** *(final version)* | Ancestral levels and gear tiers you build up over months, with real risk of failure along the way. |

---

## 3. Technology decisions

| Area | Decision | Why | Revisit if |
|---|---|---|---|
| Server | SpacetimeDB, pinned to 2.13.x | Database, logic and replication in one system. Transactions run one at a time, so contested actions (land claims, pack pickups, auction purchases, loot) can't race. BitCraft shows the platform can run an MMO. | Phase M0 load tests miss their targets after mitigations |
| Module language | Rust | Each database runs on one thread, so the CPU cost of each transaction is the capacity limit, and lean code keeps the hosting bill small. BitCraft's server is Rust code we can read. | — |
| Client | Godot 4.7.x .NET, C#, with the official `SpacetimeDB.ClientSDK.Godot` | The official SDK is C#. | — |
| Shared rules | A pure Rust `sim` crate. C# tests check the client's copies of its formulas against generated test vectors. | One source of truth for the few formulas the client predicts: movement speed, ship motion, cannon arcs. | Keeping the copies in sync gets painful. Then build `sim` as a native library and call it from C#. |
| Art | Stylized low-poly with a bright palette and readable silhouettes. Uses CC0 kits plus procedurally generated props (§7). | Achievable by Claude and free to use. Light on the GPU. | The user picks a hand-painted direction (Q1) |
| Terrain | Terrain3D with stylized materials | One file per region, editor tools, foliage instancer | — |
| Water | A stylized Gerstner-wave shader. Cosmetic buoyancy samples the same function. | Matches the art style, simpler than FFT waves, and gameplay ignores waves anyway | — |
| World scale | 16×16 km, origin at the center, single precision | Precision problems start 10–40 km from the origin | — |
| Physics | Jolt (Godot's default), client only | The server uses simple motion math, a heightmap and obstacle outlines. | — |
| Tests | Rust unit tests and SpacetimeDB in-memory module tests; scripted bot scenarios against a local server; xUnit for pure C# code; GoDotTest or GdUnit4 in-engine (check 4.7 support in M0); Movie Maker frame captures for visual review | Lets Claude verify its own work (§10.4) | — |
| Auth | SpacetimeAuth (supports Steam tickets) or any OIDC provider | Stable identity across devices | — |
| Hosting | One self-hosted SpacetimeDB instance on a rented server for production. Development on the user's PC. | The license allows one production instance, and it's the cheapest option (§4.7). | Playtests need a shared server before M5. Then use Maincloud's free or Pro tier temporarily. |
| Content | Text files in `content/` run through a generator that emits Rust statics and C# data | One source of truth. Definitions never live in database rows. | — |
| Sidecars | Rust programs using SpacetimeDB's Rust client SDK | Pathfinding and load-test bots run off the database thread. | — |

---

## 4. Architecture

### 4.1 Topology

```
 Players                    SpacetimeDB (one self-hosted instance)
 ┌──────────────┐  WS   ┌───────────────────────────────────────────┐
 │ Godot client │──────▶│ world  (Rust module, the whole world)      │
 │  C# + SDK    │◀──────│   all gameplay tables and reducers         │
 └──────────────┘       │   10 Hz world_tick · one-shot timers       │
                        └───────▲────────────────────────▲──────────┘
                                │ reducers, service id   │ admin reducers
                     ┌──────────┴─────────┐   ┌──────────┴─────────┐
                     │ npc-brain (Rust)   │   │ GM / ops web panel │
                     │ paths, AI choices  │   │ (TypeScript SDK)   │
                     ├────────────────────┤   └────────────────────┘
                     │ loadbot (Rust)     │
                     └────────────────────┘
 Offline: Godot editor + world exporter → content/ and world data
          → compiled into the module or uploaded to tables, and packed into the client
```

### 4.2 Who simulates what

| What | Simulated by | What the server checks or owns | Why |
|---|---|---|---|
| Player's own movement: walk, run, jump, swim, climb | The client predicts it and sends **movement events** (start, stop, turn, jump, mode change) plus a 2 Hz heartbeat with velocity. Other clients extrapolate. | Speed limit for each movement mode and active buff; vertical rules against the heightmap; obstacle outlines; teleport detection, answered with a correction | Running physics for every player on a single-threaded database doesn't scale; checking their reports does. Events cost far less than fixed-rate samples. |
| Gliding, mounts | Client | Glide-ratio and speed limits. Both are disabled while carrying a pack. | Same as above |
| NPC movement | Server, as **timed segments** (start point, end point, start time, speed). Clients interpolate along them. | Paths come from `npc-brain`. | The server writes only when a segment changes. |
| Combat: skills, damage, heals, crowd control, buffs, threat | Server | Global cooldown, skill cooldowns, resources, range, line of sight, facing, status conditions | Never trust the client with outcomes |
| Projectiles and cannonballs | Server computes the trajectory with a formula and schedules the impact as a one-shot timer | Hit test at impact time | No per-tick projectile simulation |
| Ships, carts, wagons, siege engines | Server tick, driven by the driver's throttle and rudder input. Positions are written at 5 Hz and clients extrapolate. | Water depth, coastline, speed, collisions | Several players share, fight over and board these |
| Economy, items, labor, crafting, land, farms, trade, justice, progression | Server | Everything | — |
| Waves, ship roll, cloth, foliage, weather effects | Client | — | Cosmetic only |

**Moving decks.** Players on a ship move in the ship's local coordinates: their `transform.parent` is the ship, and the server checks and stores local positions. The helmsman's client can predict its own ship with the shared `sim` formulas; everyone else interpolates. This gets tested in M0.

### 4.3 World partitioning and interest management

- **Cells.** The map is split into 64 m cells, 250×250 in total. Each cell key packs the instance id, the cell's y index and its x index into one `u64`. Instance 0 is the open world. Dungeons, arenas, the prison and the courthouse each get their own instance id, so they're separated for free.
- **Every public table with a world position has an indexed `cell` column.** Data that hangs off an entity goes inside a row that already has `cell` (for example, the buff list lives inside `status`), or is fetched with a join subscription if the pinned version supports it (verify in M0).
- **Cell keys are laid out row by row.** Each row of a subscription window is then one indexed range query. Benchmark this against per-cell equality queries in M0.
- **Subscription tiers:**

  | Tier | Contents | Radius | Write rate |
  |---|---|---|---|
  | Self | The player's own private data through views: inventory, cooldowns, quests, labor, mail, crime record | — | On change |
  | Near | `entity`, `transform`, `status`, `combat_event` | 5×5 cells, about ±160 m | Movement events, 2 Hz heartbeat |
  | Static | Plots, buildings, crops, trees, livestock, furniture, resource nodes | 9×9 cells, about ±290 m | Rarely |
  | Far | `far_marker` (ships, world bosses, siege engines) on a 512 m grid | 5×5 cells, about ±1.3 km | 1 Hz |
  | Global | Zone war states, world events, clock, weather | Whole world | Rarely |

  The Far tier is what puts a sail on the horizon.
- **Switching cells.** Subscribe to the new cells before dropping the old ones, with hysteresis so a player on a cell border doesn't keep resubscribing.
- **Hot and cold tables are separate.** Write rows only when something changed, using small types.
- **Public positions are readable by modified clients**, so map hacks are possible. Protect only what matters for gameplay (§4.5).

### 4.4 Simulation timing

- **`world_tick` runs every 100 ms (10 Hz)** with measured `dt`. Its budget is 20 ms or less.
  - It runs NPC logic only in cells near players.
  - It applies buff, damage-over-time and heal-over-time ticks that are due, found through a due-time index.
  - It moves vehicles and handles regeneration during combat.
  - Anything heavy is spread across several ticks.
- **Never tick what can be computed from timestamps:** growth, labor, trade demand, tax due dates, cargo value decay, cooldown displays. Store a start time and a rate, then compute the current value when the row is read.
- **One-shot timers** for discrete events: a cast finishing, a projectile landing, a respawn, a demolition, a zone changing phase, a trial phase, a prisoner's release, a siege, a world-boss spawn, an arena match ending.
- **Event tables** for short-lived things: the combat log, damage numbers, effect triggers, public chat.
- **Game time.** Every reducer reads time through `now(ctx)`, which is `ctx.timestamp` plus a dev-only offset. Tests use an admin reducer to fast-forward days in seconds. Production builds compile the offset out.
- **Clock.** A one-row public `world_clock` table is updated every second. Day/night and weather are derived from server time.

### 4.5 Hidden information and anti-cheat
- **Private data** lives in private tables, exposed through per-player `my_*` views: inventory, bank, mail, whispers, cooldowns, quests, crime record, jury votes, auction bids.
- **Stealth.** Positions of stealthed entities live in a private table. A per-player view returns them only to observers who can detect them.
- **Crime evidence.** The bloodstain or footprint row is public. Who did it is stored privately and revealed only through `report`.
- **Whispers** go into a private inbox table read through a view, not through a public event table.
- **Ownership and rate limits.** Every reducer checks that `ctx.sender` owns the character it's acting for. A private table holds a per-identity rate limit (token bucket).
- **Randomness for valuable rolls** (regrades, tempering, rare drops). The built-in RNG is predictable, so mix a rotating server secret and a per-call counter into a keyed generator, and log every roll.
- **Anomaly logging.** Movement anomalies go to a `suspicion` table, and the commit log keeps a full history.
- **No reliance on row-level security** while it's still experimental.

### 4.6 Capacity (hobby scale, measured in M0)
- **Target:** 100–300 concurrent players on one server. The design still leaves room for about 1,000.
- **Movement load.** With movement events plus a heartbeat, each player sends about 2–3 updates a second, so 300 players is under 1,000 small transactions a second. The real cost is fan-out to nearby subscribers, which the vendor's benchmark doesn't measure (R§3.4). M0 measures it.
- **Crowded fights.** In sieges and world-boss fights, lower the send rate where players are dense, update far-away players less often, and cap participants per side. A relay sidecar is the last resort.

### 4.7 Hosting, license and cost
- **Production** runs as one self-hosted SpacetimeDB instance. The BSL 1.1 license allows one production instance as long as it isn't a database service. Development and test instances on the user's PC are non-production.
- **Expected cost:** a dedicated server or large VPS at roughly €50–150 a month at 2026 prices (prices rose in 2026, so check current offers), plus off-site backups and a domain. M0 measures CPU and memory per player, which determines the server size.
- **Why not Maincloud for production?** Its usage-based pricing works out to roughly $0.0000003 per call, which is about $2k a month for movement alone at 1,000 players. At hobby scale that's still more than a flat-rate server. Maincloud's free tier is fine for early shared playtests.
- **Backups:** automated snapshots to off-site storage, with a restore drill before launch (M5).
- **Donations** cover the server, backups and domain. If there's surplus, it can go to commissioned art. Use a public ledger page to keep it transparent.

### 4.8 Repository layout

```
mmo/
├── CLAUDE.md        Claude's standing instructions (loaded every session)
├── docs/            PLAN.md, RESEARCH.md, STATUS.md, BACKLOG.md
│   ├── adr/         decision records (NNNN-title.md)
│   ├── design/      one spec per system
│   ├── world/       world bible: our names, races, factions, lore, map
│   └── playtests/   notes from each playtest
├── content/         game data source of truth (items, skills, recipes, NPCs, quests, routes)
├── assets/          third-party and generated assets + LEDGER.md (source, author, license)
├── server/          Cargo workspace
│   ├── world/       SpacetimeDB module (the whole world)
│   ├── sim/         pure rules: formulas, motion, combat math (no SpacetimeDB dependency)
│   └── contentgen/  turns content/ into Rust statics, C# data and a content hash
├── sidecars/
│   ├── npc-brain/   pathfinding and AI decisions, sent as reducer calls
│   └── loadbot/     headless bots for load and scenario tests
├── client/          Godot 4.7 .NET project
│   ├── addons/      Terrain3D, …
│   ├── net/         generated bindings (spacetime generate --lang csharp)
│   └── src/         Net · World · Gameplay · Presentation · UI
├── tools/           world exporter plugin, procedural asset generators, GM web panel
└── ops/             deploy scripts, backup jobs, configuration
```

---

## 5. Server design (Rust module `world`)

### 5.1 Code organization

| File or folder | Covers |
|---|---|
| `lib.rs` | Init and lifecycle reducers |
| `time.rs` | `now(ctx)` and the test clock |
| `tick.rs` | The world tick |
| `spatial.rs` | Cells and subscription helpers |
| `session.rs` | Connecting, characters, entering the world |
| `movement.rs` | Movement events and validation |
| `combat/` | Skills, effects, buffs, threat, death |
| `npc.rs` | NPC behavior and spawns |
| `items/` | Items and containers |
| `progression/` | Levels, ancestral levels, regrade, temper, sockets, synthesis, evolve, awaken |
| `labor.rs`, `gathering.rs`, `crafting.rs` | Labor and production |
| `land/` | Plots, buildings, tax |
| `farming.rs` | Crops, trees, livestock |
| `trade.rs` | Packs, outposts, cargo |
| `vehicles/` | Ships, carts, cannons, seats |
| `factions.rs` | Zone war cycles and honor |
| `justice.rs` | Crime, evidence, trials, prison |
| `pvp/` | Arenas, flag war, battlegrounds, sieges |
| `social/` | Party, raid, guild, family, chat, mail |
| `market.rs` | Auction house |
| `quests.rs` | Quests and story chapters |
| `events.rs` | World bosses and scheduled events |
| `admin.rs` | GM and admin reducers |

All game math goes in `sim`, so it can be unit-tested without a database.

### 5.2 Core schema sketch

This uses SpacetimeDB 2.x attribute style. It's a sketch to show the shape; check the exact syntax against the pinned version.

```rust
// ── identity ─────────────────────────────────────────────
#[table(accessor = account)]                         // private
pub struct Account { #[primary_key] identity: Identity, created_at: Timestamp, gm_level: u8 }

#[table(accessor = character_public, public)]        // what others may see
pub struct CharacterPublic {
    #[primary_key] char_id: u64,
    #[unique] name: String,
    race: u8, faction: u8, level: u8, ancestral_level: u8,
    guild_id: Option<u64>,
    appearance: Vec<u8>,                             // compact character-creator params
}

#[table(accessor = character_private)]               // read through the `my_character` view
pub struct CharacterPrivate {
    #[primary_key] char_id: u64,
    #[index(btree)] owner: Identity,
    skillsets: Vec<u8>,                              // the 3 picks
    xp: u64, gold: u64,
    labor: i32, labor_at: Timestamp,                 // regen computed lazily from labor_at
    crime_points: u32, infamy: u32, honor: u64,
}

// ── spatial: cold / hot / warm ──────────────────────────
#[table(accessor = entity, public)]                  // cold: what to spawn
pub struct Entity { #[primary_key] id: u64, #[index(btree)] cell: u64, kind: u8, ref_id: u64 }

#[table(accessor = transform, public)]               // hot: keep it tiny
pub struct Transform {
    #[primary_key] entity_id: u64,
    #[index(btree)] cell: u64,
    parent: u64,                                     // 0 = world, else a vehicle (moving decks)
    x: f32, y: f32, z: f32,
    yaw: u16, speed: i16,                            // velocity for extrapolation
    mode: u8,                                        // walk / swim / glide / mount / carry / transformed / dead
}

#[table(accessor = status, public)]                  // warm
pub struct Status {
    #[primary_key] entity_id: u64,
    #[index(btree)] cell: u64,
    hp: i32, hp_max: i32, mp: i32, mp_max: i32,
    target: u64, cast_skill: u32, cast_end: Option<Timestamp>,
    buffs: Vec<BuffSlot>,                            // embedded, so buffs need no cell bookkeeping
}

// ── land and placed objects (static tier) ───────────────
#[table(accessor = parcel, public)]
pub struct Parcel {
    #[primary_key] #[auto_inc] id: u64,
    #[index(btree)] cell: u64,
    owner_char: u64, design_id: u32,
    x: f32, z: f32, rot: u16, half_w: u16, half_d: u16,
    build_stage: u8, access: u8,                     // private / family / guild / public
    tax_paid_until: Timestamp, demolish_at: Option<Timestamp>,
}

#[table(accessor = placed, public)]                  // crops, trees, livestock, furniture, workstations, dropped packs
pub struct Placed {
    #[primary_key] #[auto_inc] id: u64,
    #[index(btree)] cell: u64,
    parcel_id: Option<u64>, owner_char: u64, def_id: u32,
    x: f32, y: f32, z: f32, rot: u16,
    planted_at: Timestamp, care: u8, harvests_left: u8,
}

// ── items (private; each container exposed through a view) ──
#[table(accessor = item)]
pub struct Item {
    #[primary_key] #[auto_inc] id: u64,
    #[index(btree)] container: u64,                  // packed (kind, owner): bag, bank, equip, chest, hold, mail
    slot: u16, def_id: u32, count: u32,
    grade: u8, item_level: u16, temper: u8, durability: u16,
    sockets: Vec<u32>,                               // gem def ids
    extra: Vec<u8>,                                  // pack origin + crafted_at, awakening/evolve state, locked stats, …
}

// ── trade, vehicles, politics ───────────────────────────
#[table(accessor = trade_demand, public)]
pub struct TradeDemand { #[primary_key] key: u64 /* (outlet, origin zone) */, pct: u16, at: Timestamp }

#[table(accessor = vehicle, public)]
pub struct Vehicle {
    #[primary_key] entity_id: u64, def_id: u32, owner_char: u64,
    hp: i32, throttle: i8, rudder: i8,
    cargo: Vec<CargoSlot>,
}

#[table(accessor = zone_state, public)]
pub struct ZoneState { #[primary_key] zone_id: u16, phase: u8, score: u32, phase_ends_at: Option<Timestamp> }

#[table(accessor = evidence, public)]
pub struct Evidence { #[primary_key] #[auto_inc] id: u64, #[index(btree)] cell: u64, kind: u8, x: f32, z: f32, expires_at: Timestamp }

#[table(accessor = evidence_secret)]                 // private: who did it
pub struct EvidenceSecret { #[primary_key] evidence_id: u64, offender: u64, crime: u8, points: u16 }

// ── timers and events ───────────────────────────────────
#[table(accessor = world_tick_timer, scheduled(world_tick))]
pub struct WorldTickTimer { #[primary_key] #[auto_inc] scheduled_id: u64, scheduled_at: ScheduleAt }

#[table(accessor = timer, scheduled(on_timer))]      // one-shots: casts, impacts, respawns, demolition, …
pub struct Timer { #[primary_key] #[auto_inc] scheduled_id: u64, scheduled_at: ScheduleAt, kind: u8, subject: u64 }

#[table(accessor = combat_event, public, event)]
pub struct CombatEvent { cell: u64, source: u64, target: u64, skill_id: u32, amount: i32, flags: u16 }
```

### 5.3 Reducers by system

| System | Reducers clients can call | Timers |
|---|---|---|
| Session | `create_character`, `enter_world`, `leave_world` (plus the connect and disconnect lifecycle reducers) | Logout after idling or combat |
| Movement | `move_event(seq, kind, x, y, z, yaw, speed, mode)` | — |
| Combat | `set_target`, `use_skill(skill, target, ground_pos)`, `cancel_cast` | `cast_done`, `impact`, `world_tick` |
| Items | `move_item`, `split`, `equip`, `use_item`, `loot`, `destroy` | — |
| Labor and crafting | `gather(node)`, `craft(recipe, station, n)` | `craft_done` |
| Land | `place_design(item, x, z, rot)`, `contribute(parcel, items)`, `pay_tax(parcel)`, `set_access`, `demolish` | `tax_due`, `demolition` |
| Farming | `plant(seed, x, z)`, `care(obj, kind)`, `harvest(obj)`, `uproot(obj)` | None; growth is computed from timestamps |
| Trade | `craft_pack(recipe, station)`, `pick_up(obj)`, `drop_pack`, `load(vehicle, slot)`, `unload`, `sell_pack(outpost)`, `convert_to_cargo(tower)`, `deliver_cargo(port)` | None; demand and decay are computed from timestamps |
| Vehicles and naval | `build_ship(design, shipyard)`, `summon`, `board(seat)`, `leave_seat`, `helm(throttle, rudder)`, `fire(seat, yaw, pitch)`, `repair` | `impact`, `sink` |
| Factions | `set_hostile_flag` | `zone_phase` |
| Justice | `report(evidence)`, `plead`, `join_jury`, `vote(trial, sentence)`, `escape_attempt` | `trial_phase`, `release` |
| Progression | `pick_skillset`, `ancestral_choose(skill, morph)`, `regrade(item, scroll, charm)`, `temper(item, kind)`, `socket(item, gem)`, `synthesize(item, fodder)`, `evolve(item, fodder)`, `awaken(item, infusions)` | — |
| Instanced PvP | `queue(mode)`, `leave_queue`, `ready` | `match_start`, `match_end` |
| Transformations | `transform(kind)`, `revert` | — |
| Social | `chat`, `whisper`, party, raid, guild and family actions, `send_mail`, `take_mail` | Mail expiry |
| Market | `list_item`, `buy`, `cancel_listing` | Listing expiry |
| Sieges | `declare`, `bid`, `deploy_siege_engine` | `siege_start`, `siege_end` |

### 5.4 Content pipeline
- **Source files.** `content/` holds human-editable TOML, with CSV for large tables. `contentgen` checks every reference and produces:
  1. Rust `static` tables compiled into the module
  2. C# data for the client
  3. a `CONTENT_HASH` on both sides, so mismatched clients are refused
- **Skills are data:** effect lists plus conditions. Combos come from conditions such as "bonus if the target has status X", and ancestral morphs are alternate effect lists. Names are placeholders:

  ```toml
  [[skill]]
  id = "ember_bolt"
  skillset = "flame"
  cast_ms = 1000
  range_m = 25
  cost_mp = 40
  effects = [
    { kind = "damage", school = "fire", coeff = 1.2 },
    { kind = "bonus_if_target_status", status = "chilled", damage_mult = 1.5 },  # cross-skillset combo
  ]
  [[skill.morph]]                  # ancestral variant
  id = "ember_bolt_scorch"
  replace_effects = [ { kind = "damage", school = "fire", coeff = 1.0 }, { kind = "dot", school = "fire", coeff = 0.4, ms = 6000 } ]
  ```
- **World data** comes out of the Godot editor through an exporter plugin. One source feeds both the server's checks and the client.

  | Data | Notes |
  |---|---|
  | Heightmap | 2–4 m resolution, u16 per sample, stored in a private `terrain_chunk` table |
  | Land-use raster | Where housing is allowed, no-build areas, roads, water depth, climate band |
  | Obstacle outlines | Walls, gates, cliffs |
  | Placements | Spawn points, resource nodes, workstations, outposts, ports |

---

## 6. Client design (Godot 4.7 .NET)

### 6.1 Layers

| Layer | Responsibilities |
|---|---|
| Net | `DbConnection` and `STDBUpdateManager`. `SubscriptionDirector` manages the tiers and cell sets. `ReducerGateway` provides typed calls, rate limits, pending actions and error toasts. `ServerClock`. Reconnect and resync. |
| World | `EntityRegistry` (entity id → view node). Spawns and despawns from row callbacks. Interpolation and extrapolation buffers. Applies server corrections to the local player. |
| Gameplay | `PlayerController` (CharacterBody3D, prediction, ship-local mode). Tab-targeting, action bars, cast bar, cooldown prediction. Build mode with ghost placement. Vehicle, helm and cannon controls. Glider. Transformations. |
| Presentation | Characters (shared rig, modular parts, AnimationTree). Visual and sound effects triggered by event tables. Nameplates and damage numbers. |
| UI | Window framework, HUD, and the inventory, character, skills, crafting, progression, map, mail, market, guild, trial and arena windows |
| WorldStream | Terrain3D regions, per-cell prop scenes loaded on background threads, foliage instancing, impostors, water, sky and weather |

### 6.2 Sync rules
- **Row callbacks.** A row inserted in a subscribed cell spawns an entity; a deleted row despawns it with a fade. A `transform` update feeds the interpolation buffer. The local player reconciles only when the server sends a correction.
- **Sending movement.** Send movement events on start, stop, turn, jump and mode change, plus a 2 Hz heartbeat, numbered so the server can order them.
- **Optimistic updates.** Only movement and cosmetic UI update before the server confirms.

### 6.3 Rendering and performance
- **Stylized pipeline:** the Forward+ renderer, a toon-ish lighting ramp, outlines on characters (optional, decide in the art test), a stylized sky, distance fog and a day/night cycle.
- **Budgets.** Set them in M0 against a mid-range GPU, not just the development PC. For example: 60 fps at 1080p in a town with 100 characters, and at least 30 fps in a 150-player fight.
- **Techniques:** lower-detail models and slower animation updates at distance, impostors for crowds, visibility ranges for props, MultiMesh foliage.

### 6.4 Characters and animation
- **One shared humanoid rig.** All races are built on the Quaternius Universal Base Characters rig and animated from the Universal Animation Library (CC0). Godot's retargeting (SkeletonProfileHumanoid) makes it easy to bring in other humanoid animation.
- **Races come from variation:** body proportions and scale, colors, attachable parts (ears, tails, horns, beards, hair), and gear. Each race should be recognizable by silhouette.
- **Creator:** blend shapes where the meshes have them, bone scales, part swaps and color pickers, all saved into `appearance`.
- **Animation gaps** (weapon-specific attacks, skill casts, pack carrying, helm and cannon work, gliding, crafting) get filled by:
  - CC0 libraries first
  - then procedural or IK-driven poses using the IK framework from Godot 4.6
  - then commissions, if donations allow

---

## 7. Art and audio pipeline

- **Direction.** Stylized low-poly with flat or gradient shading, a bright and readable palette, chunky silhouettes, and strong color coding by faction and region. This is the default; confirm with Q1. M0 produces a style test scene with screenshots for approval.
- **Sources, in order of preference:**
  1. **CC0 kits:** Quaternius (characters, animation, nature, buildings), KayKit (characters, props, dungeon kits) and Kenney (environment, UI, audio).
  2. **Procedural generation by Claude:** Godot `@tool` scripts, or Blender's Python API run headless if Blender gets installed. Good fits are modular houses and farms, ship hulls (lofted from profiles), carts, trade packs and crates, crop growth stages, rocks and cliffs, and castle wall segments.
  3. **CC-BY assets**, with credits.
  4. **Commissions**, paid for by donations.
- **Not allowed:** anything non-commercial or no-derivatives, assets with unclear licenses, or anything from ArcheAge. AI-generated art only with the user's approval and clear terms.
- **Asset ledger.** `assets/LEDGER.md` records every third-party file: source URL, author, license and date. Claude adds a row whenever it imports something.
- **Terrain and water.** Terrain3D with a small palette of stylized ground materials. The water shader uses Gerstner waves, shoreline foam, toon highlights and wakes.
- **UI.** Built on a Godot Theme, with Kenney UI as the starting point, then made our own.
- **Audio.** Kenney and other CC0 sound effects, and CC0 or CC-BY music with credits.

---

## 8. How each system gets built

Milestones: **M1** core loop · **M2** two continents · **M3** open sea and the north · **M4** final-version layer.

| System | Milestone | Notes |
|---|---|---|
| Characters and races | M1: 1 · M2: 4 · M4: 6 | See §8.8 |
| Skillsets and combat | M1: 3 · M2: 12 · M4: 14 + ancestral morphs | Effects are data; combos are conditions; diminishing returns on crowd control; threat |
| NPCs and spawns | M1 | A simple state machine for cells near players; `npc-brain` supplies paths |
| Quests and story chapters | M1: about 10 · M2: levels 1–55 · M4: chronicles | Kill, gather, craft and deliver reducers report quest progress |
| Labor and proficiency | M1: labor · M2: ranks and the limit on maxed proficiencies | Regeneration from timestamps, at the Patron rate for everyone |
| Gathering and crafting | M1 | Timed actions use one-shot timers |
| Land and housing | M1: garden and farm · M2: houses, furniture, a housing province · M4: guild residences | See §8.1 |
| Farming and husbandry | M1: crops, trees, 1 animal · M2: climates, breeding · M4: water farms if confirmed | See §8.2 |
| Trade | M1: packs and outposts · M3: cargo, overseas routes, ship currency | See §8.3 |
| Mounts, gliders, carts, submarine | M1: cart · M2: mounts and gliders · M3: submarine | Client-predicted within limits; carts are vehicles |
| Ships and naval | M1: rowboat and clipper · M2: merchant ship · M3: warships, harpoon clipper, sea bosses | See §8.4 |
| Fishing and diving | M2 | Sport-fishing minigame; breath, depth, treasure |
| Factions and zone cycle | M2 | See §8.5 |
| Crime and justice | M1: evidence and crime points · M2: trial, jury, prison · M3: pirates | See §8.6 |
| The north | M3 | See §8.7 |
| Gear progression | M2: grades, regrading, crafted tiers · M4: the full stack | See §8.9 |
| Dungeons and raids | M2: 3 dungeons · M3: world bosses twice a day · M4: power-scaled dungeon, raids | Separated by `instance_id` |
| Instanced and event PvP | M3: battlegrounds, scheduled faction battle · M4: arenas, flag war, scramble | See §8.10 |
| Lifestyle transformation | M4 | See §8.8 |
| Social | M1: party, chat · M2: guild, family, mail, market · M4: guild levels and residences | Two buyers can't both win the same listing |

### 8.1 Land and housing
- **Placement checks.** `place_design` checks that:
  - the land-use raster allows building there
  - the footprint doesn't overlap other plots
  - the slope is acceptable
  - it keeps its distance from roads and hubs
  - the account's property limit isn't exceeded

  Because transactions run one at a time, land rushes resolve correctly.
- **Construction.** Placing a design creates a scaffold. Contributing materials and labor advances `build_stage`.
- **Tax and demolition.** Weekly tax rises with the number of properties owned. When `tax_paid_until` passes, a `tax_due` timer sets `demolish_at`. At demolition time, the contents are mailed to the owner and the land frees up. A public marker shows the plot is about to open.
- **Access.** Each plot is private, family, guild or public.
- **Later layers:** housing provinces (M2), and guild residences unlocked by guild level (M4).

### 8.2 Farming
- **Growth.** A crop's stage is calculated from its planting time, its growth time and the climate multiplier at that spot (from the land-use raster). It's never ticked.
- **Care.** Watering, feeding and medicine set bits in `care`. Neglect is resolved when the crop is next evaluated.
- **Theft.** On a plot, only players with access can harvest. Off a plot, anyone can harvest or uproot once the crop matures, which leaves footprints and adds crime points.

### 8.3 Trade (final-version model)
- **Packs.** A pack is an item that records its origin zone and when it was crafted. It goes in a back slot. While carrying it, the player is slower, can't glide and can only use certain mounts; the server enforces this. A dropped pack becomes a world object, and theft rules apply.
- **Selling at outposts.** Packs sell at trade outposts inland, at ports and overseas. The price is the origin's base value multiplied by a route factor, the outpost's demand (which recovers toward its cap over time) and a freshness factor. Each sale lowers demand.
- **Cargo (M3).** At trade towers, packs convert into cargo bound for overseas ports. Cargo loses value over time, down to a floor. Some ports pay in a special currency that buys ships.

### 8.4 Ships and naval combat
- **Movement.** Throttle and rudder input set each hull's acceleration, turn rate and top speed. Ships collide with the coast using a distance field built from the heightmap, and with each other using hull circles. They can't enter water shallower than their draft.
- **Seats.** Helm, cannons and passengers. A ship with no one at the helm drifts.
- **Cannons.** The client aims, previewing the arc with the shared formula. The server schedules the `impact` and checks for hits against hull boxes at impact time.
- **Building.** A shipyard design is placed on a shore, built up through material stages, then launched.
- **Sinking.** At 0 HP a `sink` timer starts. What happens to the cargo will be decided in the spec, after checking the original.
- **Visibility.** Ships appear in the Far tier, so you can see them coming.

### 8.5 Factions and the zone cycle
- **States.** Each contested zone has a `zone_state` row. PvP deaths add to its score, and crossing thresholds raises the tension level. Maximum tension leads to war for a fixed period, then peace, then tension builds again. The open ocean is always at war.
- **Rewards.** Honor in wartime, and war events on phase changes.
- **Zone caps** per faction, scaled to the server's population.

### 8.6 Crime and justice
- **Committing crimes.** Attacking your own faction requires a hostile flag. Attacks, kills and theft write evidence and add crime points and infamy.
- **Reporting.** A witness's `report` reveals the offender to the justice system only, not to the witness.
- **Trial and prison.** At or above the threshold, a PvP death sends the offender to the courthouse instance. There they either:
  - plead guilty and get a shorter sentence, or
  - face a jury of five volunteers, who vote on a sentence.

  Prison is a separate instance where combat, mounts and pets are disabled. Work tasks shorten the sentence, and there's an escape route.
- **Preventing abuse.** The defendant never learns who the jurors are, jurors can't be contacted during the trial, and each juror has a reputation score.
- **Pirates.** High infamy moves a player to the pirate faction (M3).

### 8.7 The north: castles, sieges, nations (M3)
- **Castles.** Zones where castles can be claimed. A guild completes the crystal objective to claim one. Castle buildings are plots whose obstacle outlines block movement and line of sight.
- **Sieges.** Weekly: declarations or bids, then a scheduled siege window. Siege engines are vehicles, and each side has a participant cap.
- **Nations.** Castle owners can found nations; this gets its own spec.
- **Raid-monster conflict.** While raid monsters are up, the north becomes a conflict zone (the 2022 behavior).

### 8.8 Races and transformations
- **Races.** Six races, three per continental faction. Each has racial skills; the two races added later (M4) fight through transformations, such as a short-lived combat form.
- **Lifestyle transformation (M4).** A trade-and-gathering form:
  - hauls packs faster with a pack animal
  - breathes underwater and swims faster
  - has its own bag
  - can't use racial or combat skills or enter instances

  Modeled as a transformed movement mode with its own speed limits and skill restrictions.

### 8.9 Gear progression (final-version stack)
- **Grades.** A grade ladder topped by an "eternal" grade. Higher grades raise item level and add socket slots.
- **Regrading.** Uses scrolls, with optional charms. It shows the success chance, and has three difficulty tiers by item category. Failure can mean no change, demotion or destruction, depending on the tier.
- **Tempering.** Stacks of improvement that can fail and fall back at high levels.
- **Gems** in sockets.
- **Synthesis.** Feed items into another to gain grade or experience; the safe path.
- **Crafted evolving tier.** You craft an item, "uncloak" it into a variant of your choice (no randomness), then evolve it by feeding it other gear.
- **Awakening tier.** Upgraded with infusions gathered from dailies and drops. Cheap in gold but heavy in labor. Its first stats lock at a set tier.
- **Set bonuses** that modify skills, and **cloak tiers**.
- **Implementation.** Everything is data-driven. Every roll uses the keyed random generator and is logged. Telemetry tracks how far players get, for balancing.

### 8.10 Instanced and event PvP
- **Battlegrounds (M3):** islands where players defend towers with skills, mines and artillery against waves, for honor and materials.
- **Scheduled large faction battle (M3):** a timed world event in a dedicated zone.
- **Arenas (M4):** free-for-all and 3v3, built on the matchmaking-queue pattern from the SpacetimeDB docs. Matches run in their own instance.
- **10v10 flag war and a scramble mode (M4):** queued instances with their own rules specs.

---

## 9. World plan (the final version, compressed)

One 16×16 km map, with sea level at 0 and the origin at the center. All names are placeholders until the world bible (`docs/world/`) is written.

| Region | Approx. size | Contents |
|---|---|---|
| West continent | 5×9 km | Three race homelands with starter areas, a capital, contested middle zones, a prairie region, endgame mountains, and housing areas plus a housing province |
| East continent | 5×9 km | Three race homelands, a capital, contested middle zones, a raid ridge, endgame mountains, and housing areas |
| Central sea | About 6 km wide | A neutral trade island (special currency, cargo port), a battleground island with a harbor, and a sanctum island with a story-gated dungeon |
| The north | About 10×3 km along the top edge | Castle territories, raid-monster spawns, nations |
| Outer ocean | The edges | Always at war; sea bosses; sunken treasure |
| Separate instances | Not on the map | Dungeons, raids, arenas, prison, courthouse |

- **Travel times (tunable).** An inland pack route takes 5–10 minutes. Crossing the central sea takes about 10 minutes at roughly 10 m/s, and a run to the north about 12–15 minutes: long enough for risk and interception, short enough to repeat.
- **Climate bands** from north to south feed farming.
- **Terrain3D** with 1 km regions gives about 256 regions, well within its limits. Open ocean needs only a seabed depth in the server's depth map.

---

## 10. How Claude builds this

### 10.1 Roles

| Claude | User |
|---|---|
| Writes specs from research, the code and tests, tools and procedural assets. Writes original lore, names and quests. Runs bots, load tests and frame captures. Keeps every doc current. | Sets direction and priorities. Approves installs, money and big decisions. Downloads assets that need a browser or an account. Playtests and reports how it feels. Handles hosting, the domain, donations and legal advice. |

### 10.2 Continuity: the repository is the memory
- **`CLAUDE.md`**: standing rules, a map of the docs, commands, and the workflow. Loaded every session.
- **`docs/STATUS.md`**: the current milestone, what's done, what's in progress, what's next, and blockers.
- **`docs/BACKLOG.md`**: tasks in order, each small enough for one session, each with acceptance criteria.
- **`docs/adr/`**: one record per decision, with its context and consequences.
- **`docs/design/<system>.md`**: the spec for each system.
- **`docs/world/`**: the world bible.
- **`docs/playtests/`**: notes from each playtest.
- **Git:** a commit per finished task, with a clear message (once approved, Q2). Optionally push to a private remote as off-site backup.

### 10.3 Session loop
1. Read `CLAUDE.md`, `docs/STATUS.md` and the top of `docs/BACKLOG.md`.
2. If the task's system has no spec yet, write one first, including research with sources.
3. Build the smallest end-to-end slice: server, client and tests.
4. Verify it (§10.4).
5. Update the status file, the backlog and any decision records, then commit.
6. Report back: what changed, how to try it, and what needs a decision.

### 10.4 Verifying without a human in the loop
- **Rules:** `sim` crate unit tests, plus golden test vectors shared with C#.
- **Module:** SpacetimeDB in-memory unit tests for reducers.
- **Scenarios:** a local server plus scripted Rust bots that play a loop and check the database at each step. For example: plant, fast-forward the test clock, harvest, craft a pack, carry it, sell it.
- **Client:** xUnit for pure C# logic, in-engine tests with GoDotTest or GdUnit4, and simulated input with GodotTestDriver.
- **Visuals:** scripted camera paths rendered in Movie Maker mode, giving PNG frames that Claude inspects. Before-and-after captures for every visual change.
- **Performance:** frame-time logs from scripted flythroughs, and server tick timings from a metrics table.
- **People:** a user playtest at the end of every milestone, with friends if possible.

### 10.5 Definition of done
- The behavior matches the spec, and it's covered by tests that pass.
- No new warnings.
- Docs are updated, including the status file.
- Visual changes come with captures.
- New third-party assets are recorded in the asset ledger.
- The user can see how to try it.

---

## 11. Milestones

Relative sizes: M0 medium · M1 large · M2 very large · M3 large · M4 very large · M5 medium. There are no dates; the pace depends on how often sessions run.

### M0: Foundations and risk spikes
**Deliverables:**
- **Toolchain** at the pinned versions (§13).
- **Repository setup:** `CLAUDE.md`, the status file and backlog, and decision records for everything decided so far.
- **Scripts** to build, test, run locally, publish, generate bindings, start bots and capture frames.
- **Basic play:** sessions, movement and cell subscriptions, with two clients that can see each other.
- **Infrastructure:** the test clock, a first version of `contentgen`, and a first world exporter (heightmap and land-use).
- **Art style test scene:** stylized terrain, water, and a CC0 character walking around. Screenshots go to the user for approval.

**Spikes, each with a go/no-go result:**
1. **Throughput.** 500 bots on this PC, sending movement events and heartbeats, with 30% in combat.
   - Go if the tick takes 20 ms or less, reducer latency at the 99th percentile is 100 ms or less, and each client in a crowd receives about 40 KB/s or less.
   - Also measures CPU and memory per player, to size the production server.
2. **Mass fight.** 150 bots in a 3×3-cell area. Output: a send-rate and level-of-detail policy.
3. **Ship and moving deck.** A server-driven clipper with two cannons and four players walking and fighting on deck while it turns.
4. **World scale.** A 16 km stylized map with Terrain3D, water and streaming, within budget at mid-range settings.
5. **Server-side terrain.** Movement checks and line of sight against the uploaded heightmap and obstacles, with no false positives in a 30-minute session.

**Exit:** every spike passes or has a written mitigation, the art style is approved, and the plan is updated.

### M1: "One Coast" (the core loop)
- **Content:** one coastal zone of about 3×3 km plus a small island outpost across the water; one race; levels 1–15; three skillsets with about 8 skills each and at least two cross-skillset combos; about 10 NPC types and 10 quests.
- **Systems:**
  - labor; three gathering types; two workstations and about 20 recipes
  - an 8×8 garden and a 16×16 farm, with tax and demolition
  - six crops, two trees and one animal
  - two pack recipes and three outposts, one across water, with demand
  - a cart, a rowboat and a clipper built at a shipyard
  - evidence and crime points
  - party and local chat
- **Exit:** a user playtest where the whole loop (gather, farm, craft a pack, carry it, sail it, sell it) fits in one evening, and being robbed at sea feels tense, not tedious.

### M2: "Two Continents"
**Players and combat:**
- 2 factions and 4 races
- 12 skillsets (pick 3), levels 1–55

**World and PvP:**
- zone war cycles and honor
- full justice: trial, jury, prison
- 3 dungeons

**Economy and living:**
- houses, furniture and a housing province
- mail and the auction house
- proficiency ranks
- gear grades, regrading and crafted tiers

**Travel and leisure:**
- mounts and gliders
- the merchant ship
- sport fishing and diving

**Social:**
- guilds and families

**Optional:** a public test server from here on, if a community helps.

### M3: "Open Sea & North"
**Sea:**
- cargo and overseas routes, and the ship currency
- open-ocean PvP and the pirate faction
- warships, the harpoon clipper and the submarine
- sea bosses and sunken treasure

**The north:**
- castles, sieges and nations
- conflict while raid monsters are up

**Events:**
- world bosses twice a day
- battleground islands
- the scheduled large faction battle

### M4: "Final Version"
**Races and classes:**
- the two later races with transformation racial skills
- the last two skillsets (a rifle damage set and a dance healer set), for 14 in total
- ancestral levels and morphs

**Gear:**
- the full gear stack (§8.9)

**PvP:**
- arenas, the flag war and the scramble mode

**PvE:**
- the power-scaled dungeon, raids and story chapters

**Living:**
- guild levels and residences
- the lifestyle transformation
- farms on water, if confirmed

### M5: Public hobby launch
- **Server:** the production server (one instance), automated off-site backups with a restore drill, and monitoring and alerts.
- **Community and money:** GM tools, a rules, terms and privacy page, a donation page with a cost ledger, and a community channel.
- **Process:** a patch process.
- **Legal:** a legal review before announcing.

---

## 12. Risks

L is likelihood and I is impact: H = high, M = medium, L = low.

| Risk | L | I | Mitigation |
|---|---|---|---|
| Final-version scope | H | H | Milestones with exit criteria. Systems before content. A cut list for each milestone. |
| Losing context between sessions | H | M | `CLAUDE.md`, status file, backlog, decision records and specs. Small tasks. Tests act as executable memory. |
| Fun can't be verified by tests | H | M | A user playtest every milestone. Tuning lives in data. Telemetry. |
| Character and animation art | H | H | One shared CC0 rig and library, retargeting, procedural variation, commissions funded by donations |
| IP complaint, more likely because of donations | M | H | Original IP. No names, assets or data from the original. Legal review before going public. |
| Asset license mistakes | M | M | CC0 first. The asset ledger, checked on every import. |
| Mass PvP throughput | M | H | M0 spikes, adaptive send rates, caps |
| SpacetimeDB API changes | M | M | Pin versions. Each upgrade is its own task with a decision record. Avoid experimental features. |
| Hosting cost exceeds donations | M | M | One self-hosted instance. Cost per player measured in M0. Player caps if needed. |
| Cheating | H | M | Server validation and server authority |
| Data loss | L | H | Off-site backups, restore drills, and pushes to a git remote |

---

## 13. Next steps

### Environment check (this PC, 2026-10-25)

| Item | Found | Action |
|---|---|---|
| Hardware | Ryzen 9 7900X, 32 GB RAM, Radeon RX 6950 XT, 714 GB free | More than enough for development, bots and captures |
| Git | 2.45.1 | OK |
| Rust | 1.90.0 | Add the `wasm32-unknown-unknown` target |
| .NET SDK | 7.0.203, 10.0.401 | Check that Godot 4.7 .NET builds with SDK 10; otherwise add the .NET 8 SDK |
| SpacetimeDB CLI | **1.10.0** | Upgrade to 2.13.x. 2.0 broke the 1.x API. |
| Godot | Not installed | Install 4.7.x .NET |
| Blender | Not installed | Optional; only needed for procedural assets |

### What needs your OK
1. **The toolchain changes** above.
2. **Git:** `git init`, a commit per finished task, and optionally a private GitHub repository as off-site backup.

### Then Claude starts M0
1. Scaffold the repository (§4.8). Write the status file, the backlog (M0 tasks), and decision records 0001–0007 for the decisions so far.
2. Build the server `world` module and the Godot client skeleton, with a bindings script and build, test and run scripts.
3. Sessions, movement, cells and the Near tier, with two clients that can see each other.
4. The test clock and a `loadbot` skeleton.
5. The art style test scene, with captures for your approval.

---

## 14. Open questions
1. **Look:** low-poly with flat or gradient shading (recommended; free CC0 kits fit it), or hand-painted textures (needs much more custom art)?
2. **Setup:** OK to make the toolchain changes and initialize git? Do you want a private GitHub remote?
3. **Taste:** is there anything from the final version you'd rather leave out, or anything you especially want early? For example, the depth of gear progression, or the jury system.
