# 0037: Quest givers are static public rows; each character's quests are private rows counted in the transaction of the act

- **Status:** Accepted, 2026-10-31
- **Context:**
  - M1 needs about ten quests round Hearthshore that walk a new character through the loop (PLAN §11), with givers in the towns, objectives for killing, gathering, crafting, harvesting and selling, and experience and item rewards.
  - The original let quests be turned in early at about half done and overachieved to about 150% (docs/design/quests.md §1), and party members shared objectives.
  - Who has taken what is private (CLAUDE.md), and nothing should be ticked.
- **Decision:**
  - **Quests are content** (`content/quests.toml`): givers, quests, objectives as act/target/count, rewards. Contentgen checks the references and that chains can't loop, and generates `sim::content` and C#.
  - **Givers are public `quest_giver` rows in the Static tier**, placed from `world/givers.tsv` like Wayshrines, so the client draws them and the server checks reach against them.
  - **Each taken quest is a private `quest_log` row** with a count per objective, read through `my_quests`. Turned-in rows stay as the record of what's done, which `after` chains and "you've done that quest" read.
  - **Progress is counted by the act itself**, in its transaction: `quests::count` from gathering's finish, crafting's finish, `harvest` and `sell_pack`, and `quests::killed` from an NPC's death for everyone who shares the kill. Nothing scans or ticks.
  - **Scores and pay are `sim::quest`**: counts cap at `over_pct`, the score is the least advanced objective's share, turning in needs `early_pct` and pays the score's share of the experience with the items in full. The client ports it against vectors for the window, markers and tracker.
  - **Experience goes through `combat::grant_xp`**, factored out of the kill path, so levels and events behave the same.
- **Consequences:**
  - New schema (two tables, a view, six reducers and a dev one) and content: run `cargo xtask publish` (no `--clear` needed) and `cargo xtask world` to upload the givers. `cargo xtask generate` has been run.
  - Every system that grants by an act calls one line into `quests`; a new kind of objective means a new act in contentgen, `sim::quest` and the hook.
  - Bots place their own givers beside their site and remove them; the world's givers are untouched.
  - The pay for early and overachieved quests is ours (linear) until the original's is found.
