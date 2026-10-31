# 0034: Evidence is public and its offender private; reports add crime points; the attacker's mark is a status

- **Status:** Accepted, 2026-10-31
- **Context:**
  - Crime (BACKLOG M1, docs/design/crime.md) turns the thefts already logged (packs, crops, vehicle cargo) and attacks on characters into evidence, crime points and reports. Trials, juries and prison are M2.
  - In the original, theft left footprints (3 points when reported), assault a small bloodstain (1) and murder a large one (10). Anyone could report evidence, which added crime points and the same infamy to the offender; at 50 crime points the next PvP death sent the player to trial. An attacker got a 30 s debuff that let others fight back without crime. Same-faction attacks needed a hostile flag. Evidence faded unreported after about a day.
  - PLAN §5.2 already put evidence in a public table and the offender in a private one, revealed only through `report`.
- **Decision:**
  - **Evidence** is a public Near-tier row (`evidence`: kind, world position, `made_at`) that says what and where. Who did it, to whom and for how many points is in a private **`evidence_secret`** with no view. Evidence fades `evidence_s` after it was made by a private scheduled `evidence_fade` row, rescheduled when the dev clock moves, as plot demolitions are.
  - **Crimes are found where harm and theft already happen:** `combat::harm` (damage and damage over time) and `combat::afflict` (debuffs) call `crime::harmed` before the harm lands; the pack, crop and cargo theft hooks call `crime::theft`. Nothing is refused for being a crime.
  - **The attacker's mark is a combat status** (`bloodmarked`, named in `[crime]` in `combat.toml`), set in `vitals` without diminishing returns or an event. Harm to a marked character is no crime. A bout leaves one Bloodstain: harm by an attacker who is already marked adds none. A kill of an unmarked character leaves a Blood Pool.
  - **Reports** (`report_evidence`) need reach and an unfaded piece, refuse the offender's own account, delete the evidence, and add its points to a private **`crime_record`** (`my_crime`): crime points and Notoriety (our infamy) and a report count. Nobody learns who reported.
  - **Sought** (≥ `sought_points`) is a public `vitals.sought`, set on entering the world and whenever the record changes.
- **Consequences:**
  - The schema changed (`vitals.sought`, the `evidence`, `evidence_secret`, `evidence_fade` and `crime_record` tables, the `my_crime` view): run `cargo xtask publish --clear` and then `cargo xtask world` once.
  - With no factions or hostile flag yet, every character can harm every other, and the crime is the only cost. M2's factions add the flag; its justice turns Sought into a trial at the next death to a character.
  - Every theft leaves footprints (the original's chance is unknown). The mark ends with death or leaving the world, as every status does.
  - Evidence from a deck stays at the world point where it happened, behind a moving ship.
