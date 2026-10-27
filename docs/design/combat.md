# Combat

Status: v0.2, 2026-10-27. M1 task 1, first half ("combat A"): vitals, targeting, skills with casts, cooldowns and impacts, death and respawn. The second half ("combat B", BACKLOG M1) adds statuses, crowd control and combos, levels 1–15, skillset picks and the rest of each skillset. Code: `content/combat.toml`, `server/sim/src/combat.rs`, `server/world/src/combat.rs`; `tools/worldtool/src/shrines.rs`, `world/shrines.tsv`; `sidecars/loadbot/src/combat.rs`; client `client/src/Sim/Combat.cs`, `Gameplay/Combat.cs`, `Gameplay/CombatModel.cs`, `UI/CombatHud.cs`, `UI/CombatDemo.cs`. Related: PLAN §4.2, §4.4, §5.2–5.4; docs/design/trade.md (packs dropped on death); ADR 0028.

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

## 2. Our rules

### Content (`content/combat.toml`)
- **`[combat]`** rules, all ours:
  - `hp_max` (1,000) and `mp_max` (800): every character's, until levels and gear (combat B).
  - `hp_regen_out`, `hp_regen_in` (20 and 4 per second) and `mp_regen_out`, `mp_regen_in` (16 and 6 per second): regeneration out of combat and in it.
  - `combat_s` (6): how long a character stays in combat after it last hit or was hit.
  - `gcd_ms` (1,000): the global cooldown, started by every skill.
  - `cast_move_m` (0.5): moving this far from where a cast began interrupts it.
  - `cast_leeway_m` (3): how far beyond its range a target may have gone when a cast finishes.
  - `respawn_first_s` (5), `respawn_step_s` (25) and `respawn_max_s` (180, B): the wait before respawning, below.
  - `death_stack_s` (600, B): how long a death counts toward the next wait.
  - `respawn_pct` (50): HP and mana on respawning, in percent.
  - `dummy_hp` (5,000): a dev Straw Dummy's HP.
- **`[[skillset]]`**: `wire` (0, 1, 2), `id`, `name`.
- **`[[skill]]`**: `wire` (100 to 999, stored in casts and events; Emberlore 1xx, Ironvow 2xx, Wellspring 3xx), `id`, `name`, `skillset`, `target` (`enemy`, `ally` or `caster`), `cast_ms` (0 is instant), `cooldown_ms`, `range_m`, `cost_mp`, `speed_mps` (0 lands at once, otherwise a projectile that lands after distance ÷ speed), `facing` (the target must be in front), and `effects`, a list of `{ kind, amount }` where `kind` is `damage` or `heal`.
- **`[[shrine]]`**: Wayshrine kinds (`wire`, `id`, `name`), each placed once from `world/shrines.tsv`.
- Contentgen checks: ids, names and wires; skill wires 100–999 and unique; each skill's skillset exists; targets and effect kinds known; at least one effect, amounts 1–100,000; an `enemy` skill deals damage and an `ally` or `caster` skill heals; ranges 0–60 m (0 only for `caster`), casts and cooldowns up to 60 s, costs from 0 to `mp_max`, speeds 0 or 5–200 m/s; the rules in range, `respawn_first_s ≤ respawn_max_s`. All of it is part of the content hash.

| Skillset | Skill | Target | Cast | Cooldown | Range | Mana | Effect |
|---|---|---|---|---|---|---|---|
| Emberlore (fire and frost) | Cinder Dart | enemy | 1.0 s | – | 25 m | 30 | 110 damage, a 30 m/s bolt |
| | Frostspike | enemy | instant | 8 s | 20 m | 45 | 80 damage |
| | Flare Burst | enemy | 2.0 s | 12 s | 25 m | 80 | 240 damage |
| Ironvow (weapons and fury) | Threefold Cut | enemy | instant | – | 4 m | 20 | 70 damage |
| | Rending Blow | enemy | instant | 6 s | 4 m | 40 | 140 damage |
| | Sundering Cleave | enemy | 1.5 s | 10 s | 5 m | 60 | 230 damage |
| Wellspring (healing) | Mending Word | ally | 1.5 s | – | 25 m | 60 | 260 heal |
| | Quick Balm | ally | instant | 6 s | 25 m | 50 | 120 heal |
| | Brine Lash | enemy | 1.0 s | – | 20 m | 30 | 80 damage |

Enemy skills need the target in front; ally skills don't. Every character can use all nine until skillset picks arrive (combat B). Every number is ours, for the playtest to tune.

### Vitals
- The public **`vitals`** table (Near tier), one row per fighting entity (characters and dummies): `entity_id`, `cell`, `hp`, `hp_max`, `mp`, `mp_max`, `at` (when `hp` and `mp` had those values), `combat_until`, `target`, the cast under way (`cast_skill`, 0 for none, `cast_target`, `cast_start`, `cast_end`) and `dead_at` (none while alive).
- **HP and mana come from timestamps** (`sim::combat::settle`): the stored value plus regeneration since `at`, at the in-combat rate until `combat_until` and the out-of-combat rate after, capped at the maximum. Nothing ticks. The dead don't regenerate. Every change settles first, then writes the new value with `at` = now.
- The private **`fighter`** table, one row per character (`char_id`): the global cooldown (`gcd_until`), each skill's cooldown (`cooldowns`, skill wire and ready time), death stacks (`deaths`, `last_death_at`), and the vitals saved when it leaves the world (`hp`, `mp`, `at`, `dead_at`), restored on entering. The owner reads it through **`my_fighter`**.
- `vitals.cell` follows `entity.cell` wherever the entity moves (published moves, ships, teleports, respawns).

### Targeting
- **`set_target(entity_id)`** sets `vitals.target` (0 clears it). It must be an entity with vitals. Range doesn't matter for targeting. Others see whom we target.

### Using a skill
- **`use_skill(skill, target)`** refuses, in this order: not in the world, dead ("you are dead"), no such skill, already casting, the global cooldown or the skill's cooldown ("not ready yet"), not enough mana; then the target:
  - `enemy`: an entity with vitals, not ourselves ("can't target yourself"), alive ("the target is dead").
  - `ally`: the target if it's a living character, otherwise ourselves. `caster`: ourselves.
  - Out of range (3D, between world positions, including on decks), out of line of sight (terrain, eyes to chest, as `dev_swing` did), not in front (enemy skills: the direction to the target within 90° of our facing; under 1 m always counts as in front).
- Using a skill stops gathering and crafting.
- It starts the global cooldown. An **instant** skill then **finishes** at once. A **timed** skill writes the cast into `vitals` and schedules a private **`cast_job`** for `cast_end`.
- **Finishing** checks again that the target is alive, in range plus `cast_leeway_m` and in sight (facing isn't checked again), then spends the mana and starts the skill's cooldown. Either the effects land now, or a projectile's private **`skill_impact`** row is scheduled for distance ÷ speed later. A failed finish spends nothing and clears the cast. The caster is told why in the log.
- **Interrupting:** moving more than `cast_move_m` from where the cast began, **`cancel_cast()`**, dying, or leaving the world ends the cast with nothing spent. Turning on the spot doesn't.
- **Landing:** a projectile homes; it lands if the target is still alive and in the world, wherever it went.
- **Effects:** `damage` lowers HP; `heal` raises it, to the maximum. Damage puts both sides in combat (`combat_until` = now + `combat_s`); a heal on someone in combat puts the healer in combat too. Each effect writes a **`combat_event`** to the target's cell: `source`, `target`, `skill_id` (the skill's wire), `amount` and `flags` (`HIT` 1 for damage, `HEAL` 2, `KILL` 4 when it killed). Characters may attack other characters; the crime task (BACKLOG M1) turns attacks into crime points.

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
- **`dev_spawn_dummy(x, z)`** puts a Straw Dummy (entity kind 3) on the ground with `dummy_hp` HP; `dev_remove_dummy(id)` removes it. A dummy is an `enemy` target that never fights back. At 0 HP it is "killed" (a `KILL` event) and back to full at once. NPCs replace them (BACKLOG M1).
- `dev_set_vitals(hp, mp)` sets the caller's own, for bots and demos; an HP of 0 kills as a hit would.

### The client
- **Targeting:** Tab cycles through living others within 40 m, those in front first, nearest first within each; F1 targets ourselves; Escape clears (it also closes windows). A ring marks the target.
- **Hotbar:** keys 1–9 use the nine skills in table order, on the current target. The client refuses what it can see is wrong (cooldowns, mana, range, a dead target) without asking, by the same rules (`sim::combat`, ported to C#), and the server decides.
- **Facing:** when an enemy skill's target is behind us, the client turns us to face it and sends the turn as a movement event (a turn on the spot, which the move sender doesn't otherwise send) just before the skill, so the server judges the skill facing the new way. Reducer calls from one client arrive in order.
- **Frames:** our HP and mana bars and cast bar, the target's name, HP bar and cast bar, and the hotbar with each skill's cooldown, all from `vitals` and `my_fighter` settled to the present.
- **Numbers:** each `combat_event` in view floats its amount above the target: damage in orange, heals in green, and "killed".
- **Death:** the dead lie down; we can't move, and a panel counts down the wait and then says "R: respawn".
- `--combat-demo` (dev builds): spawns a Straw Dummy ahead, targets it, casts through the hotbar, heals, lets a cast be interrupted by stepping, then kills itself with `dev_set_vitals(0, …)`, waits and respawns at a Wayshrine.

## 3. Deviations and not yet done
- **Combat B:** statuses, buffs, crowd control with diminishing returns, combos, threat, levels 1–15 and experience, skillset picks, and the rest of each skillset (about eight skills each, two cross-skillset combos). Skills from every skillset are usable by anyone until then.
- **Flat numbers.** HP, mana, damage and heals are fixed until levels, stats and gear.
- **No death penalties** (no experience or durability yet), no resurrection by others or by scrolls, no corpse runs.
- **Packs stay on the back** when their carrier dies on a ship or in water, until vehicles.
- **Dummies, not NPCs**, until the NPC task.
- Every character can attack every other: factions and the crime task decide the consequences later.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `combat.toml` parses; rules, skillsets, skills with their effects, and shrines reach both outputs and the hash; every bad rule, wire, target, effect, range, cast, cost and speed is reported together | `server/contentgen` |
| sim | `settle` across the in-combat boundary, capped, dead; `respawn_wait_s` by stacks and capped; `deaths_after`; `in_range`, `in_front` with the 1 m exemption, `moved_off`; `impact_micros`; `check` refusals in order | `server/sim/src/combat.rs` |
| sim → C# | `combat_settle`, `combat_wait`, `combat_front`, `combat_check` vectors | `client/tests/SimVectorTests.cs` |
| module | Targeting; instant damage spends mana and starts the cooldown and GCD; refusals (dead, casting, GCD, cooldown, mana, self, dead target, range, sight, facing); a timed cast finishes at `cast_end`, is interrupted by moving but not by turning, by `cancel_cast` and by leaving, and fails without spending when the target ran out of range; a projectile lands later and misses a target that left; heals cap and an ally skill on an enemy heals ourselves; combat regen by the dev clock; a kill: dead, events, pack dropped, moves refused, respawn too early refused and then at the nearest shrine with half HP; stacks lengthen the wait and expire; vitals survive leaving and entering; the dummy resets; admin shrines; the view shows only our own fighter | `server/world/src/tests/combat.rs` |
| worldtool | `shrines.tsv` round-trips; bad lines refused; the committed shrines stand in their towns, each kind once | `tools/worldtool/src/shrines.rs` |
| C# | The hotbar's local refusals and labels; the respawn countdown; Tab's target order | `client/tests/CombatTests.cs` |
| scenario | `loadbot combat`: two bots and a dummy. Out of range, not in front, GCD and cooldown refusals; instant and timed hits on the dummy and each other, seen as events by both; a cast interrupted by stepping; a projectile in flight; a self-heal; a kill with a pack dropped and picked up by the killer; moves and skills refused while dead; respawn refused early, then at a Wayshrine after the wait; regeneration by the dev clock. Then it cleans up | `cargo xtask bot combat` |
| client | `--combat-demo` against a live server, with captures of the hotbar and frames, a cast, floating numbers, death and respawn | manual, with a capture |
