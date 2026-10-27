# 0025: A plot's standing comes from its tax timestamp; demolition is one timer per plot; land use goes to the server

- **Status:** Accepted, 2026-10-26
- **Context:**
  - Land (M1 task 5) needs weekly tax, a public demolition time when it goes unpaid, and removal at a fixed hour, without ticking anything (CLAUDE.md).
  - Placement must follow the land-use raster (housing only), which until now only the client loaded (docs/design/world.md).
  - The dev clock moves game time ahead of the host's. Scheduled reducers fire on the host's clock, so a timer set for a game time can fire early, or far too late after the clock jumps.
  - There's no gold yet, and the original paid tax in certificates.
- **Decision:**
  - **A plot row stores `tax_paid_until` and nothing that changes with time.** Paid, overdue and demolished are computed by `sim::land::standing` from it and game time, and the demolition time by `demolish_at`: the first `demolish_hour_utc` at least `grace_s` after the tax ran out. The client computes the same from the public row, so the sign's countdown needs no updates.
  - **A demolished plot counts as gone everywhere at once**: for overlap, for the account's plot count, and for paying, even before its row is removed. Placement over one removes it in the same transaction, so a land rush is decided by whichever call runs first.
  - **One private `plot_demolition` row per plot**, scheduled for its demolition time measured in game time from now. It's rescheduled whenever tax is paid, and `dev_advance_clock` reschedules every plot's. When it fires, it checks game time; if the plot isn't due (paid since, or fired early) it reschedules instead of acting.
  - **Land use is uploaded to a private `land_chunk` table**, one 16 × 16 row per cell that isn't all wild and temperate; a missing row reads as wild. `terrain_info` gains `land_chunks` and `land_hash` (a wrapping sum of per-chunk FNV-1a/splitmix64 hashes, like the terrain hash), and `world-upload` checks it. It isn't checked by `enter_world`: a client with other land use only draws other tints.
  - **Tax is paid in Levy Seals**, a crafted item (tin and charcoal at the Bloomery), at the original's certificate rates for the two plot kinds, times a heavy-tax multiplier by plots owned. Placing pays the first week. Deeds are items of a new kind, `deed`.
  - Footprints are squares centred on the 2 m height grid, with sides a multiple of the 4 m land texel, so the site check reads whole samples and texels and the client's port agrees exactly.
- **Consequences:**
  - The server holds another ~256 bytes per non-wild cell (the placeholder world: 32,568 chunks, about 8 MB). Uploading adds a few seconds to `cargo xtask world`.
  - A deed returned when there's no room in the bag waits in a held `returns` place until mail (M2) delivers it.
  - Farming (next task) reads climate from the same `land_chunk` rows, and plot access from the plot row.
  - The new tables need `cargo xtask publish --clear` and `cargo xtask world` once.
