# 0026: Crops, trees and livestock are public rows staged from timestamps; harvests are instant; gone plantings are cleared lazily

- **Status:** Accepted, 2026-10-26
- **Context:**
  - Farming (M1 task 6) needs growth from planting time and climate, with the test clock moving it and nothing ticked (CLAUDE.md, BACKLOG acceptance).
  - Plantings are many and small: a farm holds dozens, and anyone may plant on open land. Each one that needed a timer would cost a scheduled row and a reducer run.
  - Crops left ripe should wither, and plantings on a plot should go when the plot does, but the plot's demolition and a crop's withering happen at times no one is watching.
  - Theft off plots must be possible now and become a crime in a later task.
- **Decision:**
  - **One public `crop` table for crops, trees and livestock** in the Static tier (PLAN's `placed`, narrowed to what grows). A row stores `grown_from` and `ripe_at`; `sim::farm::stage` gives growing, ripe and withered from them and game time, on the server and, through the C# port, the client. The ideal climate is decided once at planting and stored as `ideal`, so regrowth after a harvest needs no land lookup.
  - **No timers at all.** Withering is a stage, not an event. A planting is **gone** when it has withered or its plot is missing or demolished by game time; gone plantings can't be harvested, don't block planting, and are deleted by the planting that needs their place, or by anyone uprooting them. Demolishing or abandoning a plot deletes its plantings in the same transaction.
  - **Harvest is instant**: one transaction checks the stage, rights, labor and bag room, spends, grants and regrows or removes. Any refusal returns `Err`, which rolls the whole call back.
  - **Rights are computed, not stored**: the plot's owner and access for plantings on a plot, and for others the planter's account, with anyone allowed to take a ripe one (theft, logged at one hook for the crime task) and to clear a withered one.
  - Seeds are items of a new kind, `seed`, and each plants exactly one kind (`crop_by_seed`).
- **Consequences:**
  - Planting, harvesting and uprooting cost one reducer call each and nothing in between, however long things grow.
  - Withered or orphaned rows can linger where nobody plants or uproots. They're small, public and harmless; a sweep can come later if the playtest shows them piling up.
  - The original's short harvest cast isn't modelled. If the playtest wants it, a harvest can become a scheduled job like gathering's (ADR 0023).
  - Care (water, feed, medicine) can be added as bits on the row that change `ripe_at` or the withering rule when set, without changing this design.
  - The schema change needs `cargo xtask publish --clear` and `cargo xtask world` once.
