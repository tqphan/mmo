# 0021: A public game clock, and labor as one pool per account

- **Status:** Accepted, 2026-10-26
- **Context:**
  - Labor is the first value computed from a timestamp (CLAUDE.md: never tick what can be computed). Growth, tax, demand and decay follow. The client has to show these values counting, so it needs the server's game time, and in dev builds that includes the test clock's offset, which lived in a private dev-only table.
  - SpacetimeDB views have no clock, so a view can't hand out the current value. It can only hand out the stored row.
  - The original shared labor across an account's characters (confidence C), and PLAN §5.2 had sketched labor on the character.
- **Decision:**
  - **The test-clock offset lives in a public single-row `game_clock` table in every build.** Only dev builds write it (`dev_advance_clock`), and `now(ctx)` reads it only in dev builds. Clients compute game time as their UTC clock plus the offset, or plus 0 when there's no row.
  - **Labor is one pool per account** in a private `labor` table, read through `my_labor`, with `points` as of `at`. `sim::labor` settles it by whole periods, and the client's port is checked against vectors. Only spending writes the row (docs/design/labor.md).
- **Consequences:**
  - Production schemas carry an empty `game_clock` table.
  - The client trusts its own wall clock. Skew only shifts when the next points appear on screen, and the server's value decides every spend.
  - Renaming the dev clock's table needs `cargo xtask publish --clear` (and `cargo xtask world`) once.
