# Quests

Status: v0.1, 2026-10-31 (M1 "The coast", second half). Code: `content/quests.toml`, `server/sim/src/quest.rs`, `server/world/src/quests.rs` (hooks in `npc.rs`, `gathering.rs`, `crafting.rs`, `farming.rs`, `trade.rs`; experience through `combat::grant_xp`), `tools/worldtool/src/givers.rs` (layout in `coast.rs`), `sidecars/loadbot/src/quests.rs`, and in the client `src/Sim/Quest.cs`, `Gameplay/QuestModel.cs`, `Gameplay/Quests.cs`, `World/GiverRegistry.cs`, `UI/QuestWindow.cs`, `UI/QuestDemo.cs`. Data: `world/givers.tsv`. Related: docs/design/coast.md, docs/design/social.md (sharing kills), docs/design/combat.md (experience), ADR 0037.

M1 covers **about ten quests round Hearthshore** that walk a new character through the loop (PLAN §11): gather, craft, plant and harvest, fight up the camps, make a Grain Bale and carry it, sail to Gullhaven. Story chapters, daily and weekly quests, and the later levels come in M2 and after (PLAN §9).

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Quests came from NPC quest givers. Types: regular (levels 1 to the cap, the cheapest way to level, with coin and items), daily, story (a race's line, ending about level 30) and others. Objectives: kill, collect, talk to and deliver | B | [Fandom: Quest Giver](https://archeage.fandom.com/wiki/Quest_Giver), [Fandom: Quests](https://archeage.fandom.com/wiki/Quests) (both via search extracts, 2026-10-31; the pages sit behind a bot check) |
| Markers: **!** over an NPC with a quest to give, **?** over one to turn in to; a dark ? when the quest isn't fully done, a ? with pluses when it's been overachieved; a green ! for the main line | B | archeage.info "Questing: quest types and icons" (via search extract; the site no longer resolves), Orcz.com "ArcheAge: Tutorial – Quest Types" (via search extract) |
| **Early completion:** quests that repeat one objective could be turned in once about **half** was done ("[Comp. Early]"), for less experience and coin but every promised item | B | [Five Favorite Features: ArcheAge #2](https://fredelas.wordpress.com/2015/08/04/archeage-overachieving-quests/) (2015), [Wikipedia: ArcheAge](https://en.wikipedia.org/wiki/ArcheAge) ("early-bird system") |
| **Overachieving:** past the count the tracker showed "[Extra Prog.]", and at about **150%** "[Overachieve]", after which nothing more counted; turning in paid bonus experience (and, by some accounts, coin) | B | Five Favorite Features (above), [PCGamesN review](https://www.pcgamesn.com/archeage/archeage-review-progress) (via search) |
| The quest log held up to 50 ordinary quests; weekly quests (added later) up to 20 more, reset Mondays | C | [ArcheRage forum](https://na.archerage.to/forums/threads/after-level-52-few-level-quests-are-gone.8013/) (a private server's forum, read only for this player recollection of the official game), [ArcheAge Asia guide: weekly quests](https://asia.archeage.com/guide/738?page=7) |
| Abandoning an ordinary quest and taking it again worked; race quests couldn't be abandoned | C | [Steam: ArcheAge Unchained discussion](https://steamcommunity.com/app/1147660/discussions/1/1643178512739339219) (via search) |
| Party members shared quest objectives | B | docs/design/social.md §1 |
| How much less early completion paid, and how much more overachieving did | **Unknown** | not found |

## 2. Our rules

### Content (`content/quests.toml`)
- `[quests]`: `log` (50, C), `reach_m` (5), `early_pct` (50, B), `over_pct` (150, B).
- **Givers** (`[[giver]]`): `wire`, `id`, `name`, `title`. Four: Odda Fernwright, Reeve of Furrowdale; Brannoch Hale, Warden of Cairnwick; Yeva Mossgrove, Fenkeeper of Redfen; Quill Ambry, Harbourmaster of Gullhaven.
- **Quests** (`[[quest]]`): `wire`, `id`, `name`, `giver`, `to` (who it's turned in to; the giver when left out), `level`, `after` (quest ids turned in first), `text`, up to 4 `objectives`, `xp` and up to 4 item `rewards`.
- **Objectives** are `act` on `target`, `count` times:

  | Act | Target | Counts |
  |---|---|---|
  | `kill` | an NPC kind | each kill the character shares (below) |
  | `gather` | an item | items as they come from resource nodes |
  | `craft` | an item | items as crafting makes them; a pack counts 1 |
  | `harvest` | an item | items as they come from plantings, stolen ones too |
  | `sell` | a pack item | each sale, at any post or only at post kind `at` |

- A quest with no objectives is an **errand**: it's done as soon as it's taken, and turned in to another giver.
- Contentgen checks: rules in range (log 1–200, reach 1–20 m, early 1–100%, over 100–300%); ids and wires; names (item-name rules) and titles; text (1–240 printable ASCII, no quotes, backslashes or braces, so it prints as a literal in both languages); givers exist; levels within the cap; `after` names quests with lower wires (so chains can't loop); an errand goes to another giver; each objective's act, target and post exist (a kill target is an NPC kind, a gather target a node's item, a craft target a recipe's output, a harvest target a crop's product, a sell target a pack), counts 1–1,000, no objective twice; rewards exist, fit a stack and appear once; experience at most 1,000,000. All of it is part of the content hash.

### The coast's quests

| # | Quest | From → to | Level, after | Objectives | Pays |
|---|---|---|---|---|---|
| 0 | Pinchers on the Strand | Odda | 1 | 6 Shore Skitters | 120 xp, 2 Emberleaf Salve |
| 1 | First Furrows | Odda | 1 | harvest 20 Hearthgrain | 250 xp, 10 Hearthgrain Seed |
| 2 | Reed and Twine | Odda | 1 | gather 32 Saltreed, craft 2 Reedcloth | 220 xp, 2 Reedcloth |
| 3 | The Cairnwick Road | Odda → Brannoch | 2, after 0 | an errand | 100 xp |
| 4 | Bristle Downs Boars | Brannoch | 2, after 3 | 8 Bristleback Boars | 320 xp, 4 Emberleaf Tea |
| 5 | Wolves in Greymane Wood | Brannoch | 3, after 4 | 6 Greymane Wolves | 520 xp, a Felling Axe |
| 6 | Croakers in the Fen | Odda → Yeva | 2 | 8 Fen Croakers | 360 xp, 4 Duskberry Tart |
| 7 | The Hexers of the Tarn | Yeva | 5, after 6 | 4 Brine Hexers | 700 xp, 4 Emberleaf Salve |
| 8 | A Bale for Market | Odda | 2, after 1 and 2 | craft a Grain Bale, sell a Grain Bale | 450 xp, a Driftpine Skiff |
| 9 | Across the Water | Odda → Quill | 3, after 8 | sell a Grain Bale at Gullhaven Post | 800 xp, a Tin Lantern |

The loop: the starting seeds become First Furrows; Reed and Twine's Reedcloth (2 made, 2 given) covers two Grain Bales; the first sells inland and pays a skiff, which carries the second across to Gullhaven. The camps go out by level, ending at the Hexers' level 7 near Wreckers' Cove. Every quest at 150% pays under 2,000 experience (a `sim` test).

### Givers in the world
- `coast::GIVERS` puts each giver a few steps from its town's Wayshrine; `worldtool givers` (and `gen`) sets them on the ground in `world/givers.tsv` (`giver x y z yaw-degrees`), and `cargo xtask world` uploads them.
- A public **`quest_giver`** row per giver in the Static tier (`id`, `cell`, `kind`, position, `yaw`), placed by admins with `admin_put_givers`, `admin_remove_givers` and `admin_clear_givers`. More than one row may share a giver (bots place their own).

### The quest log
- A private **`quest_log`** row per quest a character has taken: `char_id`, `quest`, `done` (each objective's count), `taken_at`, and `turned_in_at` (none while open). Turned-in rows stay, as the record that the quest is done. Characters read theirs through **`my_quests`**.
- **`accept_quest(giver, quest)`**: alive and in the world ("not in the world"); the quest exists ("no such quest"); the giver row exists ("nobody there") and stands within `reach_m` horizontally and up or down ("too far from X"); it gives that quest ("X doesn't give that quest"); not already done ("you've done that quest") or open ("you're already on that quest"); the character's level ("you need to be level N"); every quest in `after` turned in ("finish X first"); and room in the log ("your quest log is full").
- **`turn_in_quest(giver, quest)`**: alive and in the world; on the quest ("you're not on that quest"); a giver in reach that it's turned in to ("turn that quest in to X"); far enough along ("that quest isn't far enough along"); every reward fits the bag, or nothing happens ("your bag is too full for the reward"). Then the experience, the items, and `turned_in_at`.
- **`abandon_quest(quest)`**: an open quest ("you're not on that quest"). Its row goes, with its progress; it can be taken again.
- Dev builds: **`dev_reset_quests`** drops all of the caller's character's rows.

### Counting, early and over
- `sim::quest`: an objective of 2 or more counts up to its **cap**, `count × over_pct` (9 of 6); a single act counts to 1. Counts only rise, and only on open quests.
- The **score** is the least advanced objective's share, `min(done, cap) × 100 / count`; an errand scores 100.
- A quest may be turned in at a score of `early_pct` or more. It pays **`xp × score / 100`** experience (so 50% to 150%) and **every reward item in full** (B for the items; the linear experience is ours).
- Its **standing**, for the log and tracker: *going* (under `early_pct`), *can turn in early* (under 100), *done*, *done, extra* (some objective past its count) and *overachieved* (every objective at its cap, one past its count).
- **Who counts:** a kill counts for every character that shares its experience (docs/design/social.md): the earner and party members alive and within `share_m`. Gathering, crafting, harvesting and selling count for the character that does them, in the same transaction as the act.
- Experience goes through `combat::grant_xp`, the same path as kills, with its levels and events.

### The client
- **Givers:** a robed placeholder figure on a step, with name and title, and a marker above: **?** when a quest to them is ready to turn in, **!** when they have one to take, **…** when one to them is under way.
- **F by a giver** (within reach, on land; before anything else F does) opens its panel at the top centre: quests to turn in to it (ready ones with what they pay now and a Turn in button), then its quests: each one we may take with its text, objectives, reward at full and a Take button, then the ones that wait, with why ("needs level 2", "after First Furrows"). It closes with F or Esc, or by walking away; it scrolls when long.
- **L** opens the log: each open quest with its text, objectives and who to turn it in to, and Abandon (pressed twice).
- **The tracker** on the right lists each open quest, its standing and its objectives' counts, while the log is shut.
- The server's answers (taken, turned in, or the refusal) show in the HUD's prompt lines for 5 s.
- `--quest-demo` (dev builds): walks to the nearest giver, opens its panel and takes what it offers, then summons Shore Skitters a few steps away and cuts them down until Pinchers on the Strand is done, walks back, turns it in and opens the log.

## 3. Deviations and not yet done
- **The early and overachieving pay is ours**: linear in the score. The original's amounts weren't found, and it paid coin too; we have no coin rewards yet.
- **Collect objectives count what's gathered, not what's held**, and nothing is taken on turning in. The original's collected quest items, and its deliver and talk-to objectives, aren't here; the errand stands in for a talk-to.
- **No quest items, no story chapters, no daily or weekly quests, no repeatable quests.** Out-leveled quests stay takeable.
- **One shared quest: kills.** Party members don't share gathering, crafting or sales.
- **Givers are placeholders** that don't move, speak or face the player. There's no map or compass marker, no quest text beyond one paragraph, and no hand-in dialogue.
- **No shared progress across a character's account.**

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | quests reach both outputs and the hash (kill targets as NPC wires, items otherwise, errands, posts); bad rules, titles, targets, acts, counts, rewards, errands, chains, posts, levels and text are reported together | `server/contentgen` |
| sim | caps (half again, a single act), tally; the least advanced objective sets the score, errands score 100; early turn-in at 50%, pay by the score; standings; an objective counts its own act, target and post; taking needs the level and the chain; the coast's quests hang together | `server/sim/src/quest.rs` |
| module | admins place, remove and clear givers; taking: unknown, nobody there, too far, wrong giver, level, chain, already on it, not in the world; objectives count their own acts up to their cap; gathering and crafting count, a half-done quest turns in for half with the items in full, done can't be taken, counted or turned in again; harvesting counts; packs count when crafted and sold, only at the named post, and errands and quests to another giver turn in there only; a reward that doesn't fit turns in nothing; abandoning; `my_quests` shows only our own; kills count for the party near the kill, overachieving pays 150%; the dead can't take or turn in | `server/world/src/tests/quests.rs` |
| worldtool | givers round-trip; the committed givers stand on gentle town ground beside a Wayshrine, each once, and are current | `tools/worldtool/src/givers.rs` |
| C# | `Sim.Quest` matches `sim` on generated vectors (scores, readiness, pay, standing, tally, who may take what); offers in order with why they wait; markers; objective lines, pay and the tracker | `client/tests/SimVectorTests.cs`, `QuestTests.cs` |
| scenario | `loadbot quests`: refusals for level and the wrong giver; Reed and Twine gathered and crafted to half and turned in for half the experience; the chain refusal; Shore Skitters killed with B partied and near, both at 9 of 6, both paid 150%; First Furrows planted and harvested by the dev clock; a Grain Bale crafted and sold on the isle for a skiff; a sale at the wrong post not counted, at Gullhaven Post counted, refused at the wrong giver and turned in to Quill; the errand and the four camps at top level; all ten turned in, taking one again refused. Pass: every step seen | `cargo xtask bot quests` |
| client | `--quest-demo` against a live server with the world uploaded, with captures of the offers, the tracker and markers, the turn-in and the log | `docs/captures/2026-10-31-quests-*.png` |
