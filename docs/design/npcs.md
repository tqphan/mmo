# NPCs

Status: v0.1, 2026-10-31. M1 task 1 (BACKLOG M1 #1): NPC kinds, spawn points that wake near players, a simple brain (wander, aggro, chase, attack, leash and return), threat, loot with first-hit tagging, and experience both ways. Code: `content/npcs.toml` (and the Wildcraft skills in `content/combat.toml`, the trophies in `items.toml`), `server/sim/src/npc.rs`, `server/world/src/npc.rs`; `tools/worldtool/src/spawns.rs`, `world/spawns.tsv`; `sidecars/loadbot/src/npc.rs`; client `World/EntityRegistry.cs`, `World/Views.cs`, `Gameplay/NpcModel.cs`, `Gameplay/Looting.cs`, `UI/NpcDemo.cs`. Related: docs/design/combat.md (skills, statuses, death, experience), docs/design/interest.md (the Near tier), PLAN §4.4 and §5; ADR 0030.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Monsters were fought with the same skills, statuses and crowd control as players; dying to one cost combat experience and gear durability | A / B | RESEARCH §2.2; docs/design/combat.md §1 |
| Loot went to whoever hit the monster first ("first tag"); others couldn't loot it. Grouping in a party was how credit was shared | B | [Official forums: How does mob tagging work](http://forums.archeagegame.com/archive/index.php/t-2092.html?s=26647972d8f6100d5c77af732bf52c6c) (player reports, 2014) |
| The first hitter also had to do enough of the damage, reportedly over half; otherwise nobody could loot until a timer (about a minute, reports vary) made the corpse free for all | C | Same thread; [Official forums: Loot Sharing?](http://forums.archeagegame.com/archive/index.php/t-23138.html?s=79f14951cc9df0e5787488d10876074b) |
| Some monsters attacked on sight and others only when attacked | B | general knowledge of the game; not sourced in detail |
| Leash range, reset behaviour, aggro radius, respawn times, monster stats and drop tables | **Unknown** | not found (searched 2026-10-31); never taken from client data or emulators |

Leashing is ours, after the common MMO rule: a monster dragged too far from home gives up, walks back and resets ([Warcraft Wiki: Leash](https://warcraft.wiki.gg/wiki/Leash) describes the convention).

## 2. Our rules

### Content (`content/npcs.toml`)
- **`[npc]`** rules, all ours:
  - `brain_ms` (200): how often the brain thinks, a multiple of the 100 ms tick.
  - `wake_m` (120): a spawn point with a living character within this distance (horizontal) has its NPC. The Near tier shows at least 120 m (docs/design/interest.md), so NPCs appear at the edge of view.
  - `sleep_m` (160): an NPC that is idle and stopped with no character within this distance goes away, until someone comes back. The gap from `wake_m` stops it flickering.
  - `leash_m` (40): an NPC this far from home, horizontally, gives up the fight.
  - `heartbeat_ms` (500): how often a moving NPC's position is published, as a client's heartbeat is.
  - `wander_min_s` (8) and `wander_max_s` (20): the pause between strolls; `walk_pct` (45): a stroll's share of the kind's speed; `return_pct` (150): a return home's share.
  - `corpse_s` (90): how long a corpse (with its loot) lies.
  - `free_s` (30): when a corpse's loot is open to anyone, after the death.
  - `tag_pct` (50): the share of the damage the first hitter must have done to own the loot.
  - `heal_threat_pct` (50): threat from healing someone an NPC is fighting, as a share of the HP healed; `status_threat` (20): threat from a debuff that deals no damage.
  - `loot_m` (4): how close a character must stand to loot a corpse.
- **`[[kind]]`**: `wire` (stored in `entity.ref_id`), `id`, `name`, `level`, `hp`, `speed_mps`, `aggro_m` (0 attacks only when attacked), `wander_m` (how far from home it strolls), `respawn_s` (from the death; at least `corpse_s`), `size_m` and `color` (the placeholder body's height and RGB), `skills` (Wildcraft skill ids, in the order it tries them) and `loot`, a list of `{ item, min, max, pct }`: each line drops `min` to `max` of `item` with chance `pct`%.
- Wildcraft skills are ordinary `[[skill]]`s in `combat.toml`, in the **Wildcraft** skillset, which is marked `npc = true`: characters can't pick it, the client's picker leaves it out, and its skills cost no mana. Their wires are 4xx.
- Contentgen checks: ids, names, wires; levels within the cap; HP 1–1,000,000; speeds 1–15 m/s; `aggro_m` 0–40 and `wander_m` 0–`leash_m`; `respawn_s` from `corpse_s` to a day; sizes 0.3–8 m; colours within 24 bits; one to eight skills, each an `enemy` skill in an `npc` skillset; loot items that exist, `1 ≤ min ≤ max ≤` the item's stack, `pct` 1–100; the rules in range (`brain_ms` a multiple of 100, `wake_m < sleep_m`, `free_s ≤ corpse_s`). A player skillset with an `npc` skill, or an `npc` skillset in `progress.skillset_at`'s count, is refused. All of it is part of the content hash.

| Kind | Level | HP | Speed | Aggro | Skills | Loot |
|---|---|---|---|---|---|---|
| Shore Skitter | 1 | 300 | 4.0 | – | Pinch | Skitter Shell 1 (70%), Saltreed 1–3 (40%) |
| Bristleback Boar | 2 | 500 | 5.0 | – | Gore | Coarse Hide 1 (60%), Boar Tusk 1 (40%) |
| Fen Croaker | 3 | 600 | 4.0 | 12 m | Bog Spit | Fen Gland 1 (60%) |
| Greymane Wolf | 4 | 800 | 6.0 | 15 m | Hamstring Bite, Bite | Coarse Hide 1 (60%), Wolf Fang 1 (50%) |
| Wreck Brigand | 6 | 1,100 | 5.2 | 15 m | Shield Knock, Cutlass Slash | Brigand's Token 1 (80%), Rustiron Nails 2–5 (40%) |
| Brine Hexer | 7 | 1,000 | 5.0 | 18 m | Sapping Hex, Brine Bolt | Brine Pearl 1 (40%), Emberleaf 1–3 (50%) |
| Cliff Stalker | 9 | 1,600 | 6.5 | 18 m | Pounce, Rake | Stalker Claw 1 (60%), Coarse Hide 1–2 (60%) |
| Mossback Tortoise | 10 | 2,600 | 3.0 | – | Shell Slam, Snap | Mossback Scute 1 (70%) |
| Ashwing Drone | 11 | 1,500 | 6.0 | 15 m | Searing Sting | Ashwing Venom 1 (60%) |
| Wreck Captain | 15 | 6,000 | 5.4 | 20 m | Grapeshot, Captain's Cleave | Captain's Compass 1 (25%), Rustiron Fittings 1–2 (60%), Brigand's Token 2–4 (100%) |

| Wildcraft skill | Cast | Cooldown | Range | Effects |
|---|---|---|---|---|
| Pinch | instant | 2 s | 3 m | 40 damage |
| Gore | instant | 2.5 s | 3 m | 60 damage |
| Bog Spit | 1.0 s | 4 s | 15 m | 50 damage, Soaked 6 s |
| Bite | instant | 2 s | 3 m | 70 damage |
| Hamstring Bite | instant | 12 s | 3 m | 40 damage, Slowed 4 s |
| Cutlass Slash | instant | 2 s | 4 m | 90 damage |
| Shield Knock | instant | 15 s | 4 m | 40 damage, Stunned 1.5 s |
| Brine Bolt | 1.5 s | 2.5 s | 25 m | 120 damage, a 25 m/s bolt |
| Sapping Hex | instant | 15 s | 20 m | Shaken 6 s |
| Rake | instant | 2 s | 3 m | 110 damage |
| Pounce | instant | 18 s | 8 m | 60 damage, Felled 1.5 s |
| Snap | instant | 2.5 s | 3 m | 90 damage |
| Shell Slam | 1.5 s | 14 s | 4 m | 200 damage, Stunned 2 s |
| Searing Sting | instant | 3 s | 3 m | 60 damage, Scorched 6 s at 15 a second |
| Captain's Cleave | instant | 2.5 s | 5 m | 180 damage, Sundered 6 s |
| Grapeshot | 2.0 s | 12 s | 20 m | 260 damage |

Every number is ours, for the playtest to tune.

### Spawn points
- The private **`spawn`** table: `id`, `cell`, `kind`, `x`, `y`, `z`, `yaw` (home), `npc` (the entity there now, or 0) and `ready_at` (the earliest it may spawn again). Placed from **`world/spawns.tsv`** (`kind x y z yaw-degrees`) by `worldtool spawns` and `cargo xtask world`, like Wayshrines. Admins have `admin_put_spawns`, `admin_remove_spawns` and `admin_clear_spawns`; removing a spawn point removes its NPC.
- The placeholder placement puts camps of each kind on wild land round Gullhaven and Furrowdale, rising in level with the distance from Gullhaven: Skitters on the island's beaches, Boars and Croakers inland, Wolves and Brigands across the sea, and the rest further out. The coast task lays them out by hand.

### Waking and sleeping
- Every `brain_ms` (`world_tick`, every second tick) the brain gathers the living characters in the world. A spawn point with no NPC, past its `ready_at` and with a character within `wake_m` **spawns** its kind at home: an `entity` (kind 4, `ref_id` the kind's wire), a `transform`, `vitals` (the kind's level and HP, no mana) and a private **`npc`** row.
- An NPC idle and stopped with no character (alive or dead) within `sleep_m` **goes away**: its rows are deleted and its spawn point is ready at once. So NPCs exist only near players, and nothing far from everyone costs anything.

### Moving
- An NPC moves in **segments** in its private `npc` row: from (x, z), a heading, a speed, a start and an end time. Its position at any time comes from those (never stepped), at the height of the ground. The brain plans a new segment only to start, stop or turn.
- The public `transform` (and the cells of `entity` and `vitals`) are written when a segment starts or ends and every `heartbeat_ms` while moving, with the heading as yaw and the speed, exactly like a client's moves, so other clients draw NPCs with the same buffer (ADR 0015).
- NPCs don't go into water: a stroll is only to ground above the sea, and a chase stops at the shore.
- Controls work on NPCs as on characters: stunned or knocked down it stops and does nothing; snared, it moves at `speed_pct` of its speed.

### The brain
- **Idle:** it strolls to a random point within `wander_m` of home at `walk_pct` of its speed, then pauses `wander_min_s` to `wander_max_s`. An aggressive kind (`aggro_m` > 0) notices the nearest living character within `aggro_m` in its line of sight: that character gains 1 threat and the NPC fights.
- **Fighting:** it targets the living character in the world with the most threat (ties to the earlier entry) and shows it in `vitals.target`. If it is casting, it stands until the cast is due and then finishes it (below). Otherwise it tries its skills in order and uses the first that is off cooldown (and the global cooldown) with the target in range and in sight, facing it. If none is in range, it runs towards the target until it is within 80% of its shortest skill range.
- **Using a skill:** as a character's (docs/design/combat.md), but with no mana and no facing check. An instant skill lands at once (or sends its projectile). A timed one writes the cast into `vitals`; the brain finishes it at or after `cast_end` (up to `brain_ms` late), checking range plus `cast_leeway_m` and sight again; a control interrupts it as a character's. Cooldowns and the global cooldown are kept in the `npc` row.
- **Leash:** when it or its target is more than `leash_m` from home (so a target out at sea, which it can't follow, is given up too), or with no one left to fight, it **returns**: its threat is cleared, it targets no one and runs home at `return_pct` of its speed, taking no threat on the way. At home it is back to full HP with no statuses and goes idle.

### Threat
- The private **`threat`** table: one row per NPC and entity, with `amount` and `damage` (direct and over time, after modifiers).
- Damage to an NPC adds the damage as threat and to `damage`. Each debuff that lands adds `status_threat`. Healing someone an NPC has threat on adds `heal_threat_pct`% of the HP healed to the healer, for each such NPC. Being noticed adds 1. Any of these makes an idle NPC fight; a returning one ignores them.
- A character that dies or leaves the world is dropped from every threat list.

### Death and loot
- At 0 HP an NPC dies as a character does (statuses and casts end, `dead_at` set), stops, and lies there for `corpse_s`. Then its rows go and its spawn point is ready `respawn_s` after the death.
- **Tagging:** the first character to damage or debuff an NPC tags it (`npc.tagger`). At death, the tagger **owns the loot** if it did at least `tag_pct`% of the threat rows' damage; otherwise nobody does.
- **Experience** (docs/design/combat.md's `kill_xp`) goes to the owner if there is one and it's in the world, and to the killer otherwise.
- **Loot:** each of the kind's loot lines rolls (`ctx.random`); what drops becomes rows of the public **`loot`** table (Near tier): `id`, `cell`, `corpse`, `owner` (a `char_id`, or 0), `free_at` (death + `free_s`), `def` and `count`. Public because anyone near can see a corpse has loot, as in the original.
- **`loot_corpse(corpse)`** takes every stack it can into the caller's bag. It refuses when not in the world, dead or controlled ("you can't act right now"), with no such corpse or nothing on it ("nothing to loot"), out of `loot_m` ("too far away"), when the loot isn't ours and not free yet ("that isn't yours to loot yet"), and when the bag can't take any of it ("your bag is full"). What doesn't fit stays on the corpse.
- PvE deaths cost experience as docs/design/combat.md says: an NPC is not a character.

### The client
- NPCs are drawn as a capsule of the kind's size and colour with a darker snout, labelled with the kind's name, red for aggressive kinds and pale gold for passive ones. Tab targets them like anyone else, and the target frame shows their name, level, HP, statuses and casts.
- `loot` rows come with the Near tier. A corpse with loot we may take shows a spinning gold sparkle, and the HUD says "F: loot the Greymane Wolf (Coarse Hide, Wolf Fang)" or "…: not yours to loot for 12 s". F within `loot_m` loots it (`loot_corpse`), before packs, gathering, stations and plots (placing and planting come first).
- `--npc-demo` (dev builds): goes to the top level with all three skillsets, spawns a Greymane Wolf 12 m ahead with `dev_spawn_npc`, lets it come and bite, fights it down through the hotbar, loots the corpse, and removes the spawn point.

### Dev
- `dev_spawn_npc(kind, x, z)` adds a spawn point there (its NPC appears on the next brain step) and `dev_remove_spawn(id)` removes one, for bots and demos.

## 3. Deviations and not yet done
- **No pathfinding.** NPCs walk straight lines and stop at water; a cliff in the way isn't checked. The `npc-brain` sidecar (PLAN §4.1) can bring paths when the coast's layout needs them.
- **No parties** (M1 task 4), so tagging and experience go to one character.
- **No gear or durability**, so a PvE death costs experience only; no gold drops until there is gold to drop.
- **Returning NPCs still take damage** (they ignore threat but aren't invulnerable); they heal fully at home anyway.
- **One NPC per spawn point**; no roaming patrols, groups that aggro together, elites or named rares yet.
- **Placement is a placeholder** until the coast task.
- The trophies (hides, fangs and so on) are materials with no recipes yet.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `npcs.toml` parses; rules and kinds with their skills and loot reach both outputs and the hash; the `npc` skillset flag; every bad rule, kind, skill reference, loot line and npc-skillset misuse is reported together | `server/contentgen` |
| sim | segment positions before, during and after, stopping short; `approach_m`; `pick_skill` by order, cooldown, GCD and range; `top_threat` ties; `loot_count` at the edges of the roll; `owns_loot` at the threshold; `wander_point` within the radius; ten kinds with Wildcraft skills and existing loot | `server/sim/src/npc.rs` |
| module | A spawn point wakes only with a character within `wake_m` and past `ready_at`; an idle NPC sleeps beyond `sleep_m`; an aggressive NPC notices, chases, attacks with events and starts cooldowns; a passive one doesn't until hit; threat from damage, debuffs and heals picks the target; a timed NPC cast finishes and is interrupted by a stun; leash returns home with full HP; death drops loot to the tagger with enough damage, none to a weak tagger; loot refusals (range, owner, free after `free_s`, full bag); experience to the owner; a PvE death costs experience and drops the victim from threat; the corpse goes after `corpse_s` and the NPC respawns after `respawn_s`; admin spawns | `server/world/src/tests/npc.rs` |
| worldtool | `spawns.tsv` round-trips; bad lines refused; the committed spawns stand on dry wild land, every kind placed | `tools/worldtool/src/spawns.rs` |
| C# | NPC kinds from `ref_id`, colours, aggression; who may take loot and when; the loot prompt and its wait | `client/tests/NpcTests.cs` |
| scenario | `loadbot npc`: a bot near a spawn point it places wakes a Greymane Wolf, which notices it, chases and bites; the bot (top level) kills it and gets experience and the loot; a second bot is refused the loot; the corpse goes and the wolf respawns by the dev clock; a bot that runs off is let go and the wolf goes home at full HP; a passive Skitter ignores a bot until hit; a level-1 bot killed by a Captain loses experience. Then it cleans up | `cargo xtask bot npc` |
| client | `--npc-demo` against a live server, with captures of the wolf's approach, the fight, the corpse's sparkle and the loot | manual, captures `docs/captures/2026-10-31-npcs-*.png` |
