# Labor

Status: v0.1, 2026-10-26. M1 task 1. Code: `content/labor.toml`, `server/sim/src/labor.rs`, `server/world/src/{labor,time}.rs`; client `client/src/Sim/Labor.cs`, `UI/Hud.cs`; `sidecars/loadbot/src/labor.rs`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Labor points are spent on gathering, harvesting, crafting, making trade packs and similar work | A | RESEARCH §2.3 |
| Free players: a 2,000-point pool, 5 points every 5 minutes, online only. Patrons (subscribers): a 5,000-point pool, 10 points every 5 minutes, online and offline | A | RESEARCH §2.3; [archive wiki: Labor Points](https://archeage-archive.fandom.com/wiki/Labor_Points) (search extract) |
| *Unchained* (2019, the buy-to-play server that lasted to the shutdown) gave everyone the same: 10 every 5 minutes, online or not, capped at 5,000. That's 2,880 a day | A/B | [Massively OP, 19 Aug 2019](https://massivelyop.com/2019/08/19/archeage-unchained/), [MMOs.com](https://mmos.com/news/trion-reveals-archeage-unchained-a-buy-to-play-server), [MMOGAH tips](https://www.mmogah.com/news/archeage/10-tips-for-players-in-archeage-unchained) |
| Tying regeneration to being online and paying drew complaints for years | A | RESEARCH §2.3 |
| The pool was shared by every character on an account | C | RESEARCH §2.3 |
| Proficiency ranks lowered labor costs, and spending labor raised proficiency | B | RESEARCH §2.3 |
| Labor potions restored points | C | common player knowledge |
| What a brand-new account started with | **Unknown** | not found |

## 2. Our rules

### The pool
- **One pool per account**, shared by all its characters (the original's rule, C), in the private `labor` table: `identity`, `points`, `at`. The owner reads it through the `my_labor` view.
- **Everyone regenerates like a Patron** (PLAN §1.2): `regen_points` (10) every `regen_period_s` (300 s), online and offline, up to `cap` (5,000). The numbers live in `content/labor.toml` and are part of the content hash.
- **Never ticked** (CLAUDE.md). The row holds the points as of `at`; the value at any time is computed by `sim::labor::Pool::settle`:
  - `periods = ⌊(now − at) / period⌋` (0 if `now` is before `at`)
  - below the cap: `points + periods × regen`, and `at` moves on by those whole periods only, so progress toward the next 10 points is kept
  - reaching the cap: `cap`, and `at = now`. A full pool builds no progress, so the next points come one full period after something is spent
- Only spending (and, later, grants) writes the row. Reading never does, so when a client looks can't change the outcome.
- **A new account starts with `start_points`** (5,000, a full pool). The original's starting value is unknown; starting full lets a new player try the work loop at once, which the M1 playtest needs.
- The row is created when the account is (`on_connect`), and for accounts made before labor existed, on their next connect.

### Spending
- `labor::spend(ctx, who, cost)` is for the reducers that do work (gathering, crafting and the rest, from M1 task 3 on). It settles the pool at `now(ctx)` and takes `cost` off in the same transaction, so a refused action costs nothing.
- `cost` must be above 0. Not enough labor → `Err("not enough labor: have H, need C")`, and nothing changes.
- There's no client-callable spend: labor is only ever spent by the server, as part of an action.
- Dev builds add `dev_spend_labor(cost)` (the same path) and `dev_set_labor(points)` (sets the pool to `points` as of now, up to the cap) for bots and tests.

### Game time on the client
- Labor, and later growth, tax and demand, are shown by computing from a timestamp, so the client needs the server's game time. That's wall-clock time plus the dev test clock's offset.
- The dev offset moved from a private dev-only table to a **public single-row `game_clock`** (`offset_micros`), in every build. Production builds never write it, so a client sees no row and uses offset 0. `now(ctx)` still ignores it outside dev builds (CLAUDE.md).
- The client computes `now = its UTC clock + offset`. Skew between the client's and server's clocks only moves when the next 10 points appear on screen; the server's value is the one that counts.

### Client
- `Mmo.Sim.Labor` ports `sim::labor` and is checked against generated vectors.
- The HUD shows `labor 4,980 / 5,000 (+10 in 2:31)`, or `(full)`, recomputed every frame from `my_labor` and `game_clock`.

## 3. Deviations and not yet done
- **Same rate for everyone, online or offline** (PLAN §1.2), as on *Unchained*. Free-to-play's lower rate and cap are not reproduced.
- **Starting full** is our choice; the original's starting value is unknown.
- Proficiency, cost reductions and labor-to-proficiency come in M2 (PLAN §8). Labor potions come with items, if at all.
- Labor isn't grouped with gold and XP on the character, as PLAN §5.2 sketched, because the pool is per account.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `labor.toml` parses; bad values (cap 0, regen above the cap, start above the cap, unknown fields) are reported; every value changes the hash; both outputs carry the rules | `server/contentgen/src/lib.rs` |
| sim | Whole periods only; progress kept below the cap; capped pools anchor at `now`; spending from a full pool starts a full period; a clock before `at` adds nothing; huge gaps saturate at the cap; refusals for too little and for non-positive costs; the next regen time | `server/sim/src/labor.rs` |
| sim → C# | `labor` and `labor_spend` vectors | `client/tests/SimVectorTests.cs` |
| module | A new account gets a full pool and one row; an old account gets one on connect; spending settles first and keeps progress; a refused spend changes nothing; `dev_set_labor`; the view shows only your own pool; the dev clock drives regen; `game_clock` follows `dev_advance_clock` | `server/world/src/tests/labor.rs` |
| scenario | `loadbot labor`: set 1,000, advance the dev clock 610 s, spend 1 → 1,019; advance 10 days, spend 1 → 4,999; a 5,000 spend is refused; the bot's own computation from `my_labor` and `game_clock` agrees with the server's; it restores a full pool at the end | `cargo xtask bot labor` |
| client | The HUD shows labor counting up against a live server with the dev clock advanced | manual, with a capture |
