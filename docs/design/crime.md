# Crime

Status: v0.1, 2026-10-31. M1 task 1 (BACKLOG M1 #1). Code: `[crime]` and the `bloodmarked` status in `content/combat.toml`, `server/sim/src/crime.rs`, `server/world/src/crime.rs` (hooks in `combat.rs`, `trade.rs`, `farming.rs`, `vehicle.rs`); `sidecars/loadbot/src/crime.rs`; client `client/src/Sim/Crime.cs`, `World/EvidenceRegistry.cs`, `Gameplay/Crime.cs`, `Gameplay/CrimeModel.cs`, `UI/CrimeDemo.cs`. Related: docs/design/combat.md (harm and statuses), docs/design/trade.md, docs/design/farming.md and docs/design/vehicles.md (the thefts), ADR 0034.

M1 covers **evidence, crime points and reports** (PLAN §11). Trials, juries and prison come in M2, and becoming a pirate in M3 (PLAN §8.6).

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Crime points came from attacking or killing same-faction players (which required opting into a hostile flag), harvesting others' crops and stealing packs | B | RESEARCH §2.8 |
| Theft left **footprints**, reportable for 3 points. Theft left evidence only on the thief's home continent, and only by chance | B | [ArcheAge Archive: Crime and Punishment](https://archeage-archive.fandom.com/wiki/Crime_and_Punishment) (via search, 2026-10-31) |
| Assault always left a **small bloodstain** (1 point), and murder a **large bloodstain** (10 points) | B | same |
| An attacker of a friendly player got a 30 s debuff that let others defend themselves against them without earning crime points | B | same |
| Later patches changed the numbers: murder 50 then 20 points, a penalty for attacking lower levels of up to 100 points, small bloodstains reported automatically, and the large bloodstain prompting its victim to report | C | [ArcheAge Asia guide: Crime and Trial System](https://asia.archeage.com/guide/699?page=5&categories=system) (via search; the page didn't load) |
| Any player could click evidence to report it; unreported evidence faded, and the offender got nothing | B | [Adrift in ArcheAge, part III](http://nullsignifier.blogspot.com/2014/12/adrift-in-archeage-part-iii-crime-and.html) |
| Evidence lasted about 12 or 24 hours and could be reported once | C | [official forum archive: crime footprints](http://forums.archeagegame.com/archive/index.php/t-10042.html) (players disagree) |
| A report added the same number of crime points and infamy points. At 50 crime points or more, a "wanted" debuff sent the player to trial at their next PvP death. Jail cleared crime points; infamy stayed | B | Crime and Punishment; RESEARCH §2.8 |
| Crime points didn't fall with time; daily quests and a consumable lowered them | C | same (via search) |
| The footprint chance, the exact durations, and whether the victim or offender learned who reported | **Unknown** | not found |

## 2. Our rules

### Content (`[crime]` in `content/combat.toml`)
- `mark` (`"bloodmarked"`): the status an attacker gets (below). It must be a debuff with no control, no ticks and no modifiers.
- `mark_s` (30, B): how long it lasts.
- `evidence_s` (86,400): how long evidence lies before it fades. Ours: the longer of the forum's two figures.
- `reach_m` (4): how close a reporter must stand, horizontally and vertically, as for packs.
- `footprint_points` (3, B), `bloodstain_points` (1, B), `blood_pool_points` (10, B): what each kind of evidence is worth when reported. These are the older numbers, from before the later patches.
- `sought_points` (50, B): crime points at or above this make a character **Sought**.
- Contentgen checks the mark status exists and is a plain debuff, the times run 1 s to 7 days, the reach is 1 to 20 m, the points are 1 to 1,000, and `sought_points` is 1 to 100,000. All of it is part of the content hash.

### Who is a victim
- A **crime against a character** is harm by another character: a skill's damage, damage over time from its status, or a debuff it puts on them. Harm to NPCs and Straw Dummies is never a crime, and nor is healing, buffing or anything a character does to itself.
- **Bloodmarked** (the mark status, in `vitals.statuses` like any other) makes a character **fair game**: harm to a Bloodmarked character is no crime, whoever deals it. It's how a victim, or anyone nearby, fights back. A cleanse doesn't remove it (it isn't a control). Dying, and leaving the world, end it, as for every status.
- There are no factions yet, so every character counts as every other's own faction, and there's no hostile flag (§3).

### Committing crimes (`crime::harmed`, `crime::theft`)
- **Assault:** harm to a character that isn't Bloodmarked puts the mark on the attacker for `mark_s` (or refreshes it). If the attacker wasn't marked already, it also leaves a **Bloodstain** at the victim's feet. So a bout leaves one stain, not one per hit, and a new stain comes only after the attacker has gone `mark_s` without assaulting anyone.
- **Murder:** harm that kills a character that wasn't Bloodmarked leaves a **Blood Pool** where it fell, as well as the assault's mark and any stain.
- **Theft** leaves **Footprints**:
  - picking up a pack made by another account's character (from the ground or the water, docs/design/trade.md), at the pack;
  - harvesting or uprooting another account's ripe planting off a plot (docs/design/farming.md), at the planting;
  - unloading a pack made by another account's character from a vehicle (docs/design/vehicles.md), at the thief's feet.
  Every theft leaves footprints (§3).
- Nothing is refused for being a crime. The crime is the evidence.

### Evidence
- The public **`evidence`** table (Near tier, by cell): `id`, `cell`, `kind` (0 Footprints, 1 Bloodstain, 2 Blood Pool), `x`, `y`, `z` (world), `made_at`. It says what happened and where, never who.
- The private **`evidence_secret`** table, keyed by `evidence_id`: `offender` and `victim` (character ids) and `points`. No view shows it.
- Evidence fades at `made_at + evidence_s` of game time. A private scheduled **`evidence_fade`** row removes it then, and the dev clock reschedules every fade as it does demolitions. Faded evidence can't be reported even before its row goes.

### Reporting
- **`report_evidence(id)`**: the caller is in the world and alive, the evidence exists ("no such evidence") and hasn't faded ("this evidence has faded"), and is in reach ("out of reach"). The offender's own account can't report it ("you can't report your own crime"); that tells the offender nothing new. Anyone else may, the victim included.
- A report removes the evidence and adds its points to the offender's **crime points** and **Notoriety** (our name for the original's infamy). The reporter learns nothing about who did it, and nobody learns who reported.
- Each character has a private **`crime_record`** row (`char_id`, `owner`, `crime_points`, `notoriety`, `reports`, the times it has been reported), made on its first report. The owner reads theirs through **`my_crime`**.
- **Sought:** at `sought_points` crime points or more. Everyone can see it, as `vitals.sought` in the Near tier. It's set on entering the world and whenever the points change. In M1 being Sought changes nothing else: trial at the next death to a character comes with M2's justice. Nothing lowers crime points yet; Notoriety never falls.
- Dev builds: `dev_set_crime(crime_points, notoriety)` for the caller's character.

### The client
- Evidence is drawn from the Near tier as flat placeholder decals: dark boot prints for Footprints, a small red splash for a Bloodstain and a wide dark-red pool for a Blood Pool, hidden once faded.
- **J reports** the nearest unfaded evidence in reach; the prompt names it (`J: report Footprints`). The client can't tell whose it is, so the server's refusal of our own shows as the HUD's last message.
- The HUD shows crime points and Notoriety once there's a record, and "Sought" when we are. Frames and name tags of a Sought character say "Sought". Bloodmarked shows as a status chip like any other.
- `--crime-demo` (dev builds, with `bot visitor` nearby for a victim): picks a skillset if needed, hits the visitor (a Bloodstain at its feet, Bloodmarked on us), tries to report our own stain (refused), then `dev_set_crime` makes us Sought.

## 3. Deviations and not yet done
- **No factions and no hostile flag.** The original needed a flag to harm one's own faction. Until factions (M2) every character can hit every other, and the crime is what stops them.
- **Every theft leaves footprints.** The original's chance is unknown; ours is certain, so a theft is always risky.
- **No level-gap penalty, no automatic reports.** Those were later patches with uncertain numbers. Ours are the older ones.
- **Notoriety and Sought do nothing else yet.** Trials, juries, prison and clearing points are M2; pirates are M3.
- **Evidence at sea** stays at the world point where it happened, so a deck crime's stain is left in the water behind the ship.
- **No report reward and no limit on reports**, beyond each piece of evidence being reported once.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `[crime]` reaches both outputs and the hash; a missing section, an unknown mark, a mark that controls or ticks, and out-of-range times, reach and points are reported together | `server/contentgen` |
| sim | `faded` at the boundary; `in_reach` horizontally and vertically; `points` per kind; `sought` at the threshold | `server/sim/src/crime.rs` |
| module | Assault marks the attacker and leaves one Bloodstain per bout; a marked victim is fair game (no mark, no stain); a kill of an unmarked character leaves a Blood Pool, of a marked one nothing; a debuff alone is assault; damage over time counts; harm to dummies and NPCs and healing are no crime. Each theft leaves Footprints with the right offender and victim; taking one's own pack doesn't. Report: adds points and Notoriety and removes the evidence; refused for the offender's account, out of reach, dead, faded (by the dev clock), gone. Sought at the threshold, set on entering and in `vitals`. Fading by the dev clock removes rows. `my_crime` shows only our own records; `dev_set_crime` | `server/world/src/tests/crime.rs` |
| C# | Evidence kinds, names and points; fading; the report prompt (the nearest in reach, skipping faded and out-of-reach evidence); the HUD line and Sought tags | `client/tests/CrimeTests.cs` |
| scenario | `loadbot crime`: two bots. A hits B: B sees the Bloodstain and A Bloodmarked; B hits back with no new evidence; A kills B: a Blood Pool. A can't report; B reports both and A's record shows 11 points. A picks up B's dropped pack: Footprints; a third report. `dev_set_crime` makes A Sought, seen by B in `vitals`. Fading by the dev clock. Pass: every step seen, no corrections | `cargo xtask bot crime` |
| client | `--crime-demo` against a live server with `bot visitor` nearby, with captures of evidence, the prompt and Sought | manual, with a capture |
