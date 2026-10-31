# Combat

Status: v0.4, 2026-10-31 (NPCs: docs/design/npcs.md). M1 task 1: combat A (vitals, targeting, skills with casts, cooldowns and impacts, death and respawn) and combat B (statuses, crowd control with diminishing returns, combos, damage and healing over time, levels 1–15 with experience, skillset picks, eight skills per skillset). Code: `content/combat.toml`, `server/sim/src/combat.rs`, `server/world/src/combat.rs`; `tools/worldtool/src/shrines.rs`, `world/shrines.tsv`; `sidecars/loadbot/src/combat.rs`; client `client/src/Sim/Combat.cs`, `Gameplay/Combat.cs`, `Gameplay/CombatModel.cs`, `UI/CombatHud.cs`, `UI/CombatDemo.cs`. Related: PLAN §4.2, §4.4, §5.2–5.4; docs/design/trade.md (packs dropped on death); ADRs 0028 and 0029.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Tab-target combat with heavy crowd control. Combos: one skill applies a status, and other skills, often from another skillset, get bonus effects against it | A (system), C (specific pairs) | RESEARCH §2.2 |
| A character picks three of the skillsets (ten at launch, fourteen at the end); skillsets were chosen around levels 1, 5 and 10 | A / B | RESEARCH §2.2 |
| Skills had cast times (instant or timed), cooldowns, ranges and mana costs; the skillset pages list them per skill | A | [Fandom: Battlerage](https://archeage.fandom.com/wiki/Battlerage), [Fandom: Occultism](https://archeage.fandom.com/wiki/Occultism) |
| Battlerage's first skill, Triple Slash: melee 0–4 m, instant, 30 mana, no cooldown, up to three hits in a row | B | Fandom: Battlerage (via search, 2026-10-27) |
| Occultism's first skill, Mana Stars: 0–20 m, instant, 24 mana, no cooldown | B | Fandom: Occultism (via search, 2026-10-27) |
| A sorcery skill's cast time was raised from 3.8 to 4.8 s at rank 7, so long casts existed | C | [SteamDB patch notes](https://steamdb.info/patchnotes/7812456) |
| The target frame showed the target's buffs and debuffs and who it was targeting | B | [Gamepressure: Combat](https://www.gamepressure.com/archeage/3-combat/zc69e8) |
| A dead player resurrected at the last-visited or closest statue | B | [Fandom: Combat](https://archeage.fandom.com/wiki/Combat) (via search) |
| Each death gave a stacking 10-minute debuff that made the next wait to resurrect longer, up to 180 s | B | Fandom: Combat (via search) |
| Dying to a monster cost gear durability and combat experience; dying to a player cost nothing but the debuff | B | Fandom: Combat (via search) |
| A resurrection scroll revived at 5% of max HP and MP; a bound self-resurrection scroll at 80% on the spot, with a 30-minute cooldown | B | [Fandom: Scroll: Resurrection](https://archeage.fandom.com/wiki/Scroll:_Resurrection), [Codex: Bound Self-Resurrection Scroll](https://archeagecodex.com/us/item/34021/) |
| A trade pack could be lost on death, and whoever killed the carrier could pick it up | B | [Ten Ton Hammer: trade packs](https://www.tentonhammer.com/guide/archeage/archeage-trade-pack-guide) |
| The global cooldown's length, HP and mana regeneration rates, the HP at respawn and how long "in combat" lasted | **Unknown** | not found (searched 2026-10-27) |
| Statuses that combos key off: Shaken (slower, weaker, less healing), Snared, Stunned (no actions), Tripped, Impaled; e.g. Triple Slash trips the Snared and hits the Tripped 48% harder | B | [Fandom: Status effect](https://archeage.fandom.com/wiki/Status_effect), RESEARCH §2A.7 |
| Counters: a few skills end one or two kinds of control (Shrug It Off, Alarm Call) | B | Fandom: Status effect |
| Diminishing returns on control: disputed. Reports range from none, to about 1 s of immunity to a kind after it ends, to 30 s of immunity after three stuns | C | Forums, RESEARCH §2A.7 |
| Level cap 50 at launch, later 55; skillsets picked around levels 1, 5 and 10 | C / B | RESEARCH §2.2 |
| The experience table and experience per kill | **Unknown** | not found (searched 2026-10-31) |

## 2. Our rules

### Content (`content/combat.toml`)
- **`[combat]`** rules, all ours:
  - `hp_max` (1,000) and `mp_max` (800): a level-1 character's.
  - `hp_regen_out`, `hp_regen_in` (20 and 4 per second) and `mp_regen_out`, `mp_regen_in` (16 and 6 per second): regeneration out of combat and in it.
  - `combat_s` (6): how long a character stays in combat after it last hit or was hit.
  - `gcd_ms` (1,000): the global cooldown, started by every skill.
  - `cast_move_m` (0.5): moving this far from where a cast began interrupts it.
  - `cast_leeway_m` (3): how far beyond its range a target may have gone when a cast finishes.
  - `respawn_first_s` (5), `respawn_step_s` (25) and `respawn_max_s` (180, B): the wait before respawning, below.
  - `death_stack_s` (600, B): how long a death counts toward the next wait.
  - `respawn_pct` (50): HP and mana on respawning, in percent.
  - `dummy_hp` (5,000): a dev Straw Dummy's HP.
- **`[progress]`**, all ours: `xp_to_next` (14 entries, 100 to 2,500; its length plus 1 is the cap, 15), `skillset_at` (1, 5, 10), `hp_per_level` (100) and `mp_per_level` (40), `kill_xp` (40) and `kill_xp_per_level` (20) for a kill, less by `kill_xp_gap_pct` (25) per level the killer is above it, and `death_xp_pct` (5).
- **`[control]`**, ours: `window_s` (15) and `steps_pct` (100, 50, 25), below.
- **`[[status]]`**: `wire`, `id`, `name`, `debuff`, `control` (`none`, `stun`, `knockdown` or `snare`), `speed_pct` (a snare's share of the speed limit), `dealt_pct` and `taken_pct` (direct damage dealt and taken), `tick_ms` (how often it hurts, as a debuff, or heals, as a buff, by its effect's amount).
- **`[[skillset]]`**: `wire` (0, 1, 2), `id`, `name`.
- **`[[skill]]`**: `wire` (100 to 999, stored in casts and events; Emberlore 1xx, Ironvow 2xx, Wellspring 3xx), `id`, `name`, `skillset`, `level` (needed to use it), `target` (`enemy`, `ally` or `caster`), `cast_ms` (0 is instant), `cooldown_ms`, `range_m`, `cost_mp`, `speed_mps` (0 lands at once, otherwise a projectile that lands after distance ÷ speed), `facing` (the target must be in front), and `effects`, a list of `{ kind, amount, status, ms, when }`: `kind` is `damage`, `heal`, `status` (puts `status` on for `ms`, with `amount` per tick if it ticks) or `cleanse` (ends the target's controls), and `when` names a status the target must have for the effect to happen (a combo).
- **`[[shrine]]`**: Wayshrine kinds (`wire`, `id`, `name`), each placed once from `world/shrines.tsv`.
- Contentgen checks: ids, names and wires; skill wires 100–999 and unique; each skill's skillset exists and its level is within the cap; targets, effect kinds, statuses and conditions known; at least one effect; damage and heals 1–100,000 with no status; status effects 0.5–60 s, with an amount exactly when the status ticks; cleanses bare; an `enemy` skill deals damage or debuffs and never heals, buffs or cleanses, and an `ally` or `caster` skill the other way round; ranges 0–60 m (0 only for `caster`), casts and cooldowns up to 60 s, costs from 0 to `mp_max`, speeds 0 or 5–200 m/s; statuses: only debuffs control, `speed_pct` 1–99 for a snare and 100 otherwise, modifiers −90 to 200%, ticks 0 or 250 ms–60 s; `xp_to_next` 1–98 entries, `skillset_at` from 1 and rising, no longer than the skillsets; `steps_pct` 1–8 entries that never rise; the rules in range, `respawn_first_s ≤ respawn_max_s`. All of it is part of the content hash.

| Status | Kind | Does |
|---|---|---|
| Slowed | debuff, snare | 60% of the speed limit |
| Stunned | debuff, stun | no moving, skills or other actions |
| Felled | debuff, knockdown | as a stun, lying down |
| Soaked | debuff | 10% less damage dealt; a combo mark |
| Scorched | debuff | its amount of damage every second |
| Sundered | debuff | 15% more damage taken |
| Shaken | debuff | 20% less damage dealt |
| Fervor | buff | 20% more damage dealt |
| Bulwark | buff | 30% less damage taken |
| Renewal | buff | its amount of healing every second |

| Skillset | Skill | Level | Target | Cast | Cooldown | Range | Mana | Effects |
|---|---|---|---|---|---|---|---|---|
| Emberlore (fire and frost) | Cinder Dart | 1 | enemy | 1.0 s | – | 25 m | 30 | 110 damage, a 30 m/s bolt |
| | Frostspike | 1 | enemy | instant | 8 s | 20 m | 45 | 80 damage, Slowed 4 s; **Soaked: Stunned 2 s** |
| | Flare Burst | 3 | enemy | 2.0 s | 12 s | 25 m | 80 | 240 damage; Stunned: 120 more |
| | Kindle | 5 | enemy | instant | 10 s | 25 m | 40 | Scorched 8 s, 30 a second |
| | Rime Snare | 7 | enemy | 1.0 s | 15 s | 20 m | 50 | 40 damage, Slowed 6 s |
| | Ember Focus | 9 | caster | instant | 30 s | – | 60 | Fervor 10 s |
| | Searing Lance | 12 | enemy | 2.5 s | 20 s | 30 m | 100 | 320 damage, a 40 m/s lance; Scorched: 100 more |
| | Frostbind | 14 | enemy | instant | 30 s | 20 m | 70 | Stunned 3 s; Slowed: 80 damage |
| Ironvow (weapons and fury) | Threefold Cut | 1 | enemy | instant | – | 4 m | 20 | 70 damage; **Slowed: Felled 1.5 s** |
| | Rending Blow | 1 | enemy | instant | 6 s | 4 m | 40 | 140 damage; Felled: 100 more |
| | Sundering Cleave | 3 | enemy | 1.5 s | 10 s | 5 m | 60 | 230 damage, Sundered 8 s |
| | Shield Bash | 5 | enemy | instant | 15 s | 4 m | 50 | 50 damage, Stunned 2 s |
| | Hamstring | 7 | enemy | instant | 12 s | 4 m | 30 | 40 damage, Slowed 6 s |
| | War Cry | 9 | caster | instant | 30 s | – | 50 | Fervor 10 s |
| | Iron Hide | 12 | caster | instant | 45 s | – | 40 | Bulwark 8 s |
| | Overhead Smash | 14 | enemy | 2.0 s | 20 s | 5 m | 90 | 300 damage; Stunned: Felled 2 s |
| Wellspring (healing) | Mending Word | 1 | ally | 1.5 s | – | 25 m | 60 | 260 heal |
| | Quick Balm | 1 | ally | instant | 6 s | 25 m | 50 | 120 heal |
| | Brine Lash | 1 | enemy | 1.0 s | – | 20 m | 30 | 80 damage, Soaked 8 s |
| | Renewing Tide | 3 | ally | instant | 12 s | 25 m | 60 | Renewal 10 s, 40 a second |
| | Undertow | 5 | enemy | 1.5 s | 15 s | 20 m | 50 | 60 damage, Slowed 5 s; Soaked: Felled 1.5 s |
| | Cleansing Spring | 7 | ally | instant | 20 s | 25 m | 60 | ends controls, 60 heal |
| | Wavebreak | 9 | enemy | 2.0 s | 15 s | 20 m | 70 | 160 damage, Shaken 6 s |
| | Tidal Ward | 12 | ally | instant | 30 s | 25 m | 70 | Bulwark 6 s |

Enemy skills need the target in front; ally and caster skills don't. The two cross-skillset combos are in bold: Wellspring's Brine Lash soaks, and Emberlore's Frostspike stuns the Soaked; Emberlore's Frostspike (or Ironvow's Hamstring, or Rime Snare) slows, and Ironvow's Threefold Cut fells the Slowed. Every number is ours, for the playtest to tune.

### Vitals
- The public **`vitals`** table (Near tier), one row per fighting entity (characters and dummies): `entity_id`, `cell`, `hp`, `hp_max`, `mp`, `mp_max`, `at` (when `hp` and `mp` had those values), `combat_until`, `target`, the cast under way (`cast_skill`, 0 for none, `cast_target`, `cast_start`, `cast_end`), `dead_at` (none while alive), `level`, `statuses` and `resists` (below).
- **HP and mana come from timestamps** (`sim::combat::settle`): the stored value plus regeneration since `at`, at the in-combat rate until `combat_until` and the out-of-combat rate after, capped at the maximum. Nothing ticks. The dead don't regenerate. Every change settles first, then writes the new value with `at` = now.
- The private **`fighter`** table, one row per character (`char_id`): the global cooldown (`gcd_until`), each skill's cooldown (`cooldowns`, skill wire and ready time), death stacks (`deaths`, `last_death_at`), the vitals saved when it leaves the world (`hp`, `mp`, `at`, `dead_at`), restored on entering, and `level`, `xp` (into the level) and `skillsets` (in the order picked). The owner reads it through **`my_fighter`**.
- `vitals.cell` follows `entity.cell` wherever the entity moves (published moves, ships, teleports, respawns).

### Targeting
- **`set_target(entity_id)`** sets `vitals.target` (0 clears it). It must be an entity with vitals. Range doesn't matter for targeting. Others see whom we target.

### Using a skill
- **`use_skill(skill, target)`** refuses, in this order: not in the world, dead ("you are dead"), no such skill, stunned or knocked down ("you can't act right now"), its skillset not picked or the level too low ("you haven't learned that skill"), already casting, the global cooldown or the skill's cooldown ("not ready yet"), not enough mana; then the target:
  - `enemy`: an entity with vitals, not ourselves ("can't target yourself"), alive ("the target is dead").
  - `ally`: the target if it's a living character, otherwise ourselves. `caster`: ourselves.
  - Out of range (3D, between world positions, including on decks), out of line of sight (terrain, eyes to chest, as `dev_swing` did), not in front (enemy skills: the direction to the target within 90° of our facing; under 1 m always counts as in front).
- Using a skill stops gathering and crafting.
- It starts the global cooldown. An **instant** skill then **finishes** at once. A **timed** skill writes the cast into `vitals` and schedules a private **`cast_job`** for `cast_end`.
- **Finishing** checks again that the target is alive, in range plus `cast_leeway_m` and in sight (facing isn't checked again), then spends the mana and starts the skill's cooldown. Either the effects land now, or a projectile's private **`skill_impact`** row is scheduled for distance ÷ speed later. A failed finish spends nothing and clears the cast. The caster is told why in the log.
- **Interrupting:** moving more than `cast_move_m` from where the cast began, **`cancel_cast()`**, dying, or leaving the world ends the cast with nothing spent. Turning on the spot doesn't.
- **Landing:** a projectile homes; it lands if the target is still alive and in the world, wherever it went.
- **Effects,** in order, each only if its `when` status was on the target as the skill landed (before any of this skill's own effects, so a skill never combos off itself): `damage` lowers HP, scaled by the dealer's `dealt_pct` and the target's `taken_pct` (each sum at least −100%, rounded down); `heal` raises it, to the maximum; `status` puts a status on (below); `cleanse` ends the target's controls (stun, knockdown, snare), leaving other debuffs. Damage puts both sides in combat (`combat_until` = now + `combat_s`); a heal on someone in combat puts the healer in combat too. Each effect writes a **`combat_event`** to the target's cell: `source`, `target`, `skill_id` (the skill's wire), `amount`, `flags` (`HIT` 1 for damage, `HEAL` 2, `KILL` 4 when it killed, `STATUS` 8 with the duration in ms, `IMMUNE` 16 when diminishing returns stopped a control, `XP` 32 and `LEVEL` 64 to the source, `TICK` 128 for damage or healing over time) and `status`. Identical events in one transaction are one row (tables are sets), so a client may see one number for several equal ticks. Characters may attack other characters; harm to one that isn't Bloodmarked is a crime, which marks the attacker and leaves evidence (docs/design/crime.md).

### Statuses and crowd control
- A **status** is a slot in `vitals.statuses`: `status`, `source`, `start`, `until`, `amount`. It's active from `start` until `until`, by timestamps: nothing removes it, readers ignore it once past, and the next write drops it. Putting a status on replaces any slot of the same status (it refreshes; nothing stacks).
- **Over time:** a status with `tick_ms` gets a private **`periodic`** row (one per fighter and status) with its next due time in µs in a btree index. Each `world_tick` reads the rows due by now through the index and deals (debuffs) or heals (buffs) the amount for every tick due, ticks falling every `tick_ms` from `start` up to and including `until`; then it moves the row on or deletes it. A dev clock jump catches up every tick at once. Ticks aren't scaled by modifiers. A death, leaving the world or replacing the status ends the row.
- **Controls:** stunned or knocked down, a fighter can't use skills or any other action (gather, craft, plant, place, pick up, drop or sell packs, take the helm or fire), and its cast ends; moves are judged at speed 0, so moving is corrected (turning on the spot is fine). Snared, moves are judged at `speed_pct` of the limit. A control counts for a move only if it held before the move began and still holds at its end, so the client's first move after one lands, and the move it ends during, are judged without it. Any control ends the target's cast; a stun or knockdown also stops gathering and crafting. The client stops (or slows) itself the same way, and lies down when knocked down.
- **Diminishing returns** (`[control]`): stuns and knockdowns are one group, snares another. `vitals.resists` keeps, per group, how many landed in a row and when its window runs out. A control lands for `steps_pct[n]` of its duration, where `n` is how many of its group landed since the window last ran out: 100%, 50%, 25%, then immune (an `IMMUNE` event, nothing lands) until the window runs out. Each landing sets the window to `window_s` past the end of that control. Ours: the original's rule is disputed (§1).
- Dying and respawning clear statuses and resists; so does leaving the world (§3).

### Levels and skillsets
- Characters start at level 1, with `hp_max` + `hp_per_level` × (level − 1) HP and the same for mana; `vitals.level` and the maximums follow `fighter.level`. A Straw Dummy is level 1; an NPC is its kind's level (docs/design/npcs.md), which is what killing it is worth.
- **Experience** comes from killing anything but a character: `kill_xp` + `kill_xp_per_level` × (its level − 1), less `kill_xp_gap_pct`% per level the killer is above it, to nothing (no bonus for killing above your level). It carries over levels and stops at the cap. A level gained raises the maximums and adds what they grew to HP and mana. `XP` and `LEVEL` events tell the cell.
- **Deaths:** a death to anything but another character (an NPC, or the dev self-kill) loses `death_xp_pct`% of what the level needs, never below the level's start. Dying to a character costs nothing.
- **`pick_skillset(skillset)`** adds a skillset, one for each of `skillset_at` (levels 1, 5 and 10) the character has reached; refusals: "no such skillset", "already picked", "you can pick another skillset at level N", "every skillset slot is taken". Picks are kept; there's no swapping yet.
- A skill can be used only with its skillset picked and the character at its `level` or above.
- `dev_set_level(level)` (dev builds) sets the level with no experience into it and full HP and mana, forgetting the latest picks beyond its slots.

### Death
- HP reaching 0 kills. A dead character: `dead_at` set, HP 0, mana kept, cast cleared; gathering and crafting stopped; the helm let go; a carried pack dropped at its feet if it's on land, where anyone may pick it up (taking another's is theft, docs/design/trade.md). On a ship or in water the pack stays on the back until vehicles give packs somewhere to fall.
- It counts a death: if the last one was within `death_stack_s`, `deaths` goes up by one; otherwise it starts again at 1.
- The dead can't move (`move_event` refuses with "you are dead" and changes nothing), use skills, gather, craft, pick up, drop or sell packs, take the helm or fire.
- **`respawn()`** is allowed once `respawn_first_s + respawn_step_s × (deaths − 1)` seconds have passed since `dead_at`, capped at `respawn_max_s`. It puts the character 3 m in front of the nearest Wayshrine (clear of its stone; or at the world spawn if there is none), off any ship, on the ground, running, with `respawn_pct` of its HP and mana, out of combat, and tells its client to snap there.
- No experience or durability is lost yet: neither exists (combat B and M2).

### Wayshrines
- The public **`shrine`** table (Static tier): `id`, `cell`, `kind`, `x`, `y`, `z`, `yaw`, placed from **`world/shrines.tsv`** by `worldtool shrines` and `cargo xtask world`, like posts. Admins have `admin_put_shrines`, `admin_remove_shrines` and `admin_clear_shrines`.
- Four for now, one in each town: Gullhaven, Furrowdale, Cairnwick and Redfen.

### Straw Dummies (dev builds)
- **`dev_spawn_dummy(x, z)`** puts a Straw Dummy (entity kind 3) on the ground with `dummy_hp` HP; `dev_remove_dummy(id)` removes it. A dummy is an `enemy` target that never fights back. At 0 HP it is "killed" (a `KILL` event) and back to full at once. NPCs (docs/design/npcs.md) are the real foes; dummies stay for practice and tests.
- `dev_set_vitals(hp, mp)` sets the caller's own, for bots and demos; an HP of 0 kills as a hit would (a death to no character: it costs experience).

### The client
- **Targeting:** Tab cycles through living others within 40 m, those in front first, nearest first within each; F1 targets ourselves; Escape clears (it also closes windows). A ring marks the target.
- **Hotbar:** one row of eight per skillset picked, in the order picked, each its skills in table order: keys 1–8 use the first row, Shift+1–8 the second and Ctrl+1–8 the third, on the current target. The digits are read from key events, not input actions, so a modifier can't fire two rows. Skills above our level show their level, greyed. The client refuses what it can see is wrong (controls, unlearned skills, cooldowns, mana, range, a dead target) without asking, by the same rules (`sim::combat`, ported to C#), and the server decides.
- **Skillsets:** K opens a picker with a button per player skillset (not Wildcraft, which is NPCs'), enabled while a pick is free; a line by the bars says "K: pick a skillset" or the level of the next pick, and with no skillset the hotbar says to press K.
- **Facing:** when an enemy skill's target is behind us, the client turns us to face it and sends the turn as a movement event (a turn on the spot, which the move sender doesn't otherwise send) just before the skill, so the server judges the skill facing the new way. Reducer calls from one client arrive in order.
- **Frames:** our HP and mana bars, cast bar, level and experience, and our buffs and debuffs (a chip each, name and seconds left, buffs edged green before debuffs edged red); the target's name and level, HP bar, statuses the same way, and cast bar; and the hotbar with each skill's cooldown, all from `vitals` and `my_fighter` settled to the present.
- **Numbers:** each `combat_event` in view floats above the target: damage in orange (red on us), heals in green, "killed", a status's name (debuffs violet, buffs blue), "immune to …", "+40 xp" and "level 5!" in gold.
- **Death:** the dead lie down; we can't move, and a panel counts down the wait and then says "R: respawn".
- `--combat-demo` (dev builds): goes to the top level and picks its missing skillsets through the picker, spawns a Straw Dummy ahead, targets it, soaks it with Brine Lash and stuns it with Frostspike, buffs itself with War Cry and Iron Hide, scorches the dummy with Kindle, runs in and fells it with Hamstring and Threefold Cut, lets a cast be interrupted by stepping, heals (Renewing Tide over time), then kills itself with `dev_set_vitals(0, …)`, waits and respawns at a Wayshrine. Movie Maker captures run slower than real time while the server's statuses expire in real time, so short ones end early in captures.

## 3. Deviations and not yet done
- **Three skillsets, not fourteen**, and every character ends with all three by level 10. More skillsets (and class names for the combinations) come later; with more than three, picks become a real choice.
- **Level cap 15**, not 50–55, with our own experience table; no swapping skillsets yet (the original charged gold).
- **Diminishing returns are ours** (100/50/25%, then immune for 15 s past the last), since the original's are disputed.
- **Statuses end on logging out**, so logging out clears a debuff. Saving them in `fighter` is a later fix if the playtest finds it abused.
- **Ticks aren't scaled** by damage modifiers, and heals aren't scaled at all.
- **Flat numbers.** HP and mana grow by level; damage and heals are fixed until stats and gear.
- **Death penalties:** experience only (durability waits for gear in M2), no resurrection by others or by scrolls, no corpse runs.
- **Packs stay on the back** when their carrier dies on a ship or in water, until vehicles.
- Every character can attack every other: the crime rules (docs/design/crime.md) and, from M2, factions decide the consequences.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `combat.toml` parses; rules, progress, control, statuses, skillsets, skills with their effects and conditions, and shrines reach both outputs and the hash; every bad rule, wire, target, effect, condition, status, level, range, cast, cost, speed, progress table and step is reported together | `server/contentgen` |
| sim | `settle` across the in-combat boundary, capped, dead; `respawn_wait_s` by stacks and capped; `deaths_after`; `in_range`, `in_front` with the 1 m exemption, `moved_off`; `impact_micros`; `check` refusals in order (controlled, unlearned included); maximums and skillset slots by level, `knows`; `gain_xp` across levels and at the cap, `kill_xp` with the gap, `xp_after_death` floored; slots active from start to until, `controlled`, `move_scale` counting only controls that span the move; modifiers summed and `scaled_damage` floored; diminishing returns in a row, groups apart, immunity until the window ends; `next_tick` up to and including the end; eight skills per skillset, each with one at level 1 | `server/sim/src/combat.rs` |
| sim → C# | `combat_settle`, `combat_wait`, `combat_front`, `combat_check`, `combat_level`, `combat_slots`, `combat_dr`, `combat_damage`, `combat_tick` vectors | `client/tests/SimVectorTests.cs` |
| module | Targeting; instant damage spends mana and starts the cooldown and GCD; refusals (dead, casting, GCD, cooldown, mana, self, dead target, range, sight, facing); a timed cast finishes at `cast_end`, is interrupted by moving but not by turning, by `cancel_cast` and by leaving, and fails without spending when the target ran out of range; a projectile lands later and misses a target that left; heals cap and an ally skill on an enemy heals ourselves; combat regen by the dev clock; a kill: dead, events, pack dropped, moves refused, respawn too early refused and then at the nearest shrine with half HP; stacks lengthen the wait and expire; vitals survive leaving and entering; the dummy resets; admin shrines; the view shows only our own fighter. Combat B: skills need the skillset and the level, picks by level and their refusals; a status lands, lasts its time and refreshes; a snare judges moves slower after the first, a stun ends a cast, refuses skills and other actions and corrects moves; stuns diminish 100/50/25% then immune, knockdowns share the group and snares don't, the window resets; both cross-skillset combos, a combo bonus hit, and no combo off a skill's own status; dealt and taken modifiers; damage and healing over time from the due index, caught up after a clock jump, ended by death; a cleanse ends controls only; experience from dummy kills, a level up with its maximums, less for a lower foe, nothing for PvP, a loss on a PvE death | `server/world/src/tests/combat.rs` |
| worldtool | `shrines.tsv` round-trips; bad lines refused; the committed shrines stand in their towns, each kind once | `tools/worldtool/src/shrines.rs` |
| C# | The hotbar's rows, keys and local refusals (controlled, unlearned); the pick and level lines; status chips, buffs first; the control speed factor; the floating texts; the respawn countdown; Tab's target order | `client/tests/CombatTests.cs` |
| scenario | `loadbot combat`: two top-level bots with every skillset and a dummy. Out of range, not in front, GCD and cooldown refusals; instant and timed hits on the dummy and each other, seen as events by both; a cast interrupted by stepping; a projectile in flight; a self-heal; Brine Lash then Frostspike stunning as a combo, seen in B's vitals, with B refused as stunned; a second stun at half the time; a kill with a pack dropped and picked up by the killer; moves and skills refused while dead; respawn refused early, then at a Wayshrine after the wait; regeneration by the dev clock. Then it cleans up | `cargo xtask bot combat` |
| client | `--combat-demo` against a live server, with captures of the picker, buffs and debuffs on both frames, ticks, a knockdown, the hotbar rows, death and respawn | manual, captures `docs/captures/2026-10-31-combat-b-*.png` |
