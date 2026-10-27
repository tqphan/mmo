# Land: gardens, farms, tax and demolition

Status: v0.1, 2026-10-26. M1 task 5 (BACKLOG M1 #1). Code: `content/land.toml` (`[property]`, `[[plot]]`), `server/sim/src/land.rs`, `server/world/src/land.rs`; `sidecars/loadbot/src/land.rs`, `world-upload` (land use); client `client/src/Sim/Land.cs`, `World/PlotRegistry.cs`, `Gameplay/Placement.cs`, `UI/PlotWindow.cs`, `UI/LandDemo.cs`. Related: docs/design/world.md (the land-use raster), ADR 0025.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Houses and farms were placed freely on open-world land, only inside designated areas; nothing was instanced | A | RESEARCH §2.4 |
| The first property was an 8 × 8 scarecrow garden from an early quest, which could be upgraded to a 16 × 16 farm | B | RESEARCH §2.4; [GameSkinny: first 16×16 farm](https://www.gameskinny.com/tips/archeage-guide-how-to-get-your-first-16x16-farm-improved-scarecrow-farm/) |
| Tax was paid weekly in tax certificates. The 8 × 8 garden cost at least 5 a week, rising with the number of buildings owned on the server, with a 10-certificate deposit to place it | B | [Fandom wiki: Scarecrow garden](https://archeage.fandom.com/wiki/Scarecrow_garden) (search extract, 2026-10-26) |
| The 16 × 16 scarecrow cost 10 certificates a week, from when it was placed | B | [Fandom wiki: Large Scarecrow Design](https://archeage.fandom.com/wiki/Large_Scarecrow_Design) (search extract) |
| The rate rose with the number of properties owned, steeply after two | B | RESEARCH §2.4 |
| Unpaid tax gave the owner about a week's notice; then the property entered a demolition phase and was removed at a fixed time of day, freeing the land. Players camped plots about to open | B | RESEARCH §2.4; [Magic Game World: finding a spot](https://www.magicgameworld.com/?p=123910) |
| A garden removed for unpaid tax had its design returned to the owner, 22 hours late | B | Fandom wiki: Scarecrow garden (search extract) |
| Owners could set who may use a property: private, family, guild or public | B | PLAN §8.1, common player knowledge |
| How far ahead tax could be prepaid, the exact demolition hour, and the property limit | **Unknown** | not found (searched 2026-10-26) |

## 2. Our rules

### Content (`content/land.toml`)
- **`[property]`** rules:
  - `tax_item` (`levy_seal`): the item tax is paid in.
  - `week_s` (604,800): one tax period.
  - `grace_s` (604,800): how long a plot stays up after its tax runs out (B: about a week).
  - `demolish_hour_utc` (20): demolition happens at the first 20:00 UTC of game time at least `grace_s` after the tax ran out (B: a fixed time of day; the hour is ours).
  - `prepay_weeks` (4): tax may be paid up to four weeks ahead (ours).
  - `heavy_tax_pct` ([100, 100, 200, 300, 400]): the weekly tax is multiplied by the entry for the number of plots the account owns, so the third plot doubles every plot's tax. Its length is the most plots an account may own (5). The shape follows "steeply after two" (B); the numbers are ours.
  - `reach_m` (8): how far from a plot's edge a character may place it, pay its tax or change it.
- **`[[plot]]`** kinds, each with `wire` (stored in plot rows), `id`, `name`, `deed` (an item of kind `deed`), `size_m` (a multiple of 4 m, so it covers whole land texels), `max_rise_m` (the most the ground may rise across it) and `tax` (seals a week before the multiplier).

| Plot | Deed | Size | Max rise | Tax a week |
|---|---|---|---|---|
| Garden | Garden Deed | 8 × 8 m | 2 m | 5 Levy Seals (B) |
| Farm | Farm Deed | 16 × 16 m | 3 m | 10 Levy Seals (B) |

- New items: **Garden Deed**, **Farm Deed** (kind `deed`, stack 10) and **Levy Seal** (material). New recipes: Levy Seal at the Bloomery (Tin Ingot 1, Charcoal 1 → 5, 10 labor, 4 s); Garden Deed at the Joiner's Bench (Driftpine Plank 6, Saltreed Twine 4 → 1, 25 labor, 8 s); Farm Deed there too (Hollowoak Plank 8, Hollowoak Beam 2, Saltrope 2 → 1, 50 labor, 12 s). Our numbers.
- Contentgen checks: the tax item and every deed exist and deeds are of kind `deed`, each deed is used by one plot kind, sizes are multiples of 4 from 4 to 64, rises from 0.25 to 16 m, tax 1 to 1,000, `heavy_tax_pct` has 1 to 10 entries from 100 to 10,000 that never fall, the periods from 60 s to 30 days, the hour 0–23, `prepay_weeks` 1–52, `reach_m` 1–64. All of it is part of the content hash.

### The footprint
- A plot is a square `size_m` on a side centred on `(x, z)`. The centre snaps to the 2 m height grid (`sim::land::snap`), so with sizes that are multiples of 4 its corners fall on height samples. `yaw` (0–3 quarter turns) only turns the marker; the square is the same either way.
- **Site checks** (`sim::land::check_site`, ported to C# for the preview), in order, each with its own refusal:
  1. The square is on the map.
  2. Every 4 m land texel it covers is a buildable use (`housing`). A cell with no land data is wild.
  3. Every 2 m height sample in it is known and at least 0.5 m above sea level.
  4. The highest sample is at most `max_rise_m` above the lowest.
  5. It doesn't overlap another standing plot. Plots may share an edge.
- The placer must be in the world, on land (not aboard a ship or swimming) and within `reach_m` of the square (inside counts).

### Plots
- The public **`plot`** table: `id`, `cell` (indexed, the centre's), `kind`, `owner` (`char_id`, indexed), `x`, `y` (the lowest ground, for drawing), `z`, `yaw`, `access`, `placed_at` and `tax_paid_until`. In the **Static tier**, like nodes and stations.
- Nothing about a plot's state is ticked. From `tax_paid_until` and game time, `sim::land::standing` gives:
  - **Paid** while `now < tax_paid_until`.
  - **Overdue** from then until `demolish_at(tax_paid_until)`: the first `demolish_hour_utc` at least `grace_s` later. Everyone can see the demolition time (the public marker).
  - **Demolished** from `demolish_at` on. A demolished plot counts as gone for every check, even before its row is removed.
- `place_plot(item_id, x, z, yaw)`: the item must be a deed in the caller's bag. It also takes the **first week's tax** from the bag, at the rate the account will pay with this plot counted, and refuses if the account already owns the most plots allowed. It removes any demolished plot under the square first, so a land rush is decided by whichever call runs first: the loser is told the land is taken. The new plot is paid until `now + week_s`.
- `pay_tax(plot)`: the owner's account (any of its characters, standing within reach) pays one week at the current rate (base × the multiplier for the plots the account owns now). It refuses once tax is paid `prepay_weeks` ahead or the plot is demolished. Overdue plots can be paid; each payment adds a week to `tax_paid_until`, not to now, so arrears are paid first.
- `set_plot_access(plot, access)`: the owner picks `private` (0) or `public` (1). Family and guild come with those systems (M2). Access only matters once crops and buildings stand on plots (farming).
- `abandon_plot(plot)`: the owner gives the plot up within reach; the deed goes back to the bag (refused if there's no room). Paid tax isn't refunded.
- **Demolition** is a one-shot scheduled job per plot (`plot_demolition`), due at its demolition time. It's rescheduled whenever tax is paid and whenever the dev clock moves. When it runs and the plot is demolished by game time, it removes the row and returns the deed: to the owner's bag, or, with no room, to a held `returns` place that mail will deliver (M2). If the plot was paid in the meantime it reschedules instead.

### Land use on the server
- Private **`land_chunk`**: one row per cell, 16 × 16 texels (256 bytes, rows of increasing z), for cells with any texel that isn't wild and temperate; a missing row reads as all wild and temperate.
- Admins upload with `admin_put_land` and `admin_clear_land`. `terrain_info` gains `land_chunks` and `land_hash`: the wrapping sum of `sim::land::land_chunk_hash` over the rows, FNV-1a over the cell key and texels, finished like `chunk_hash`.
- `cargo xtask world` (`loadbot world-upload`) uploads the world's land use after the heights and checks the server's land hash against its own.

### The client
- Plots are drawn from the Static tier as a flat outline on the ground with a corner post and a sign: `Garden · <owner>` and, when overdue, `Demolished in 3d 4h` in red.
- **Placing:** right-click a deed in the bag to start. A square follows a point 6 m in front of the player, snapped like the server's, green where `check_site` passes and red with the reason in the HUD. R turns the marker, F or a left click places, Esc or a right click cancels.
- Right-clicking any stack in the bag now uses it; only deeds have a use so far.
- **The plot window:** F within reach of a plot opens it: kind, owner, standing and its times, the weekly tax at the current rate with the seals held, and buttons for Pay tax, Access and Abandon (owner only, Abandon confirmed).
- `--land-demo` (dev builds) finds the nearest housing site (3.8 km west of spawn on the placeholder world), teleports beside it, grants a Garden Deed and seals, places a garden through the placement mode, pays a week, sets it public, and then moves the dev clock past the demolition time so the plot disappears and the deed comes back.

## 3. Deviations and not yet done
- **No deposit.** Placing pays the first week instead of a deposit plus tax.
- **No upgrade from garden to farm**, and no houses (M2). Farms are their own deed.
- **No 22-hour delay and no mail.** A demolished plot's deed returns at once; one that doesn't fit is held for mail.
- **Access has two settings**, private and public, until families and guilds exist.
- **No distance from roads and hubs** beyond the land-use classes: towns and roads aren't buildable, which covers it while the world has no roads.
- **No property limit by plot size or land quotas per region**, only per account.
- Tax is paid in Levy Seals crafted from materials and labor; there's no gold yet. Seals may be bought with gold once trade packs bring it.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `[property]` and `[[plot]]` parse and reach both outputs and the hash. Unknown or wrong-kind deeds, a deed used twice, a missing tax item, sizes that aren't multiples of 4, rises, taxes, periods, hour, reach and a falling or empty `heavy_tax_pct` are reported together | `server/contentgen` |
| sim | `snap`; rectangles overlap but sharing an edge doesn't; each site refusal (off the map, not housing, no data, under water, too steep, overlap) and a pass; `demolish_at` lands on the hour at least the grace later, on and just past the hour; `standing` at each boundary; `weekly_tax` by plots owned and past the limit; `land_chunk_hash` depends on the cell and every texel | `server/sim/src/land.rs` |
| sim → C# | `land_site`, `land_demolish_at`, `land_standing` and `land_tax` vectors | `client/tests/SimVectorTests.cs` |
| module | Placing takes the deed and a week's tax and sets `tax_paid_until`. Each refusal changes nothing: not in the world, ship, swimming, not a deed, someone else's item, out of reach, every site refusal, no seals, plot limit. **Land rush:** two placements over the same ground, the second refused. Neighbours sharing an edge are fine. Pay: one week added, arrears first, the rate rising with plots owned, prepay limit, not the owner, out of reach. Access and abandon (deed returned; refused with a full bag). By the dev clock: paid → overdue → demolished; demolition removes the row and returns the deed (or holds it); paying while overdue reschedules; a demolished plot's ground can be claimed before its job runs. Land upload: put, replace, clear and the hash; non-admins refused. `demolish_plot` can't be called by a client | `server/world/src/tests/land.rs` |
| worldtool | Land chunks cut from the world: wild-and-temperate cells skipped, 256 texels each, the hash matches `sim` | `tools/worldtool` |
| C# | The placement preview on the client's own readers (`LandRaster.Texel`, `HeightMap.GridSample`, which reads samples cell by cell as the server does), and the countdown text | `client/tests/LandTests.cs` |
| scenario | `loadbot land`: two bots by test ground the scenario uploads; both place a garden over the same spot at once and exactly one wins; refusals; tax paid ahead to the limit; the dev clock moves to overdue and then past demolition; the plot is gone, the deed is back, and the other bot claims the spot. Then it cleans up | `cargo xtask bot land` |
| client | `--land-demo` against a live server, with captures of the preview, a placed garden and an overdue sign | manual, with a capture |
