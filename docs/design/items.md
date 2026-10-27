# Items and the bag

Status: v0.1, 2026-10-26. M1 task 2. Code: `content/items.toml`, `server/world/src/items.rs`; client `client/src/UI/BagWindow.cs`, `Gameplay/BagModel.cs`; `sidecars/loadbot/src/items.rs`.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| Each character had a bag and a warehouse, both starting at 50 slots | B | [Fandom wiki: Expansion Scroll](https://archeage.fandom.com/wiki/Expansion_Scroll) (search extract) |
| Each expansion added 10 slots, up to 150 for the bag and for the warehouse | B | same; [Saarith guide, 2019](https://saarith.com/archeage-expansion-scrolls-and-how-to-get-more-storage-space/) |
| Expansions came from scrolls sold in the cash shop and traded for gold; warehouses stopped expanding for gold after patch 1.2 | B | [Fandom wiki: Warehouse](https://archeage.fandom.com/wiki/Warehouse) (search extract); Saarith |
| Materials stacked high (to 1,000 for common ones), gear didn't stack | C | common player knowledge; no source found |
| Stacks were split with a count, merged by dropping one on another of the same item, and items were destroyed with a confirmation | C | common to the genre; not found for the original |

## 2. Our rules

### Definitions
- Items are content, in `content/items.toml`: `wire` (the number stored in item rows), `id`, `name`, `kind` and `stack` (the most in one slot).
- **`wire` values are permanent.** A row in the database refers to its definition by `wire`, so a wire value never changes and is never reused, even after its item is removed. Gaps are allowed, unlike the other content tables. Wire 0 is reserved.
- Kinds are `material`, `consumable` and `gear`. Gear has a stack of 1. Stacks run from 1 to 10,000.
- Names are original and come from `docs/world/` (CLAUDE.md).
- Contentgen checks the ids, unique names and wires, the kinds and the stack limits. The definitions are part of the content hash.
- The bag's size is content too (`[bag] slots`, 50, the original's starting size).

### Storage
- The private `item` table has one row per stack: `id`, `holder`, `place`, `slot`, `def` (the wire value) and `count`.
- `place` says what kind of container holds the stack, and `holder` says whose it is. For now there is one place, the bag (`place = 1`), and its holder is a `char_id`. The warehouse, equipment, chests, ships' holds and mail join later as new places. A chest's holder will be its own id; `place` keeps the two kinds of id apart.
- A btree index on `(holder, place, slot)` serves both "the whole bag" (a prefix) and "what's in slot N".
- At most one stack per slot, and `1 ≤ count ≤ stack`. The reducers keep both rules; there's no unique constraint over two columns.
- **`my_inventory`** shows the owner the bag stacks of all their characters, found through `character_private`, never `presence`. Presence changes on every move, and a view that read it would be recomputed just as often. The client filters by its own `char_id`.

### Operations
All of these act on the caller's character in the world (its `presence`); otherwise "not in the world". An item that isn't in that character's bag is "no such item", whether it belongs to someone else or doesn't exist, so a refusal doesn't reveal other players' item ids.

- **`move_item(item_id, to_slot)`**
  - Slots run from 0 to the bag's size − 1; any other slot is refused.
  - Moving a stack to its own slot does nothing.
  - To an empty slot, the stack moves.
  - Onto a stack of the same item with room, as much as fits merges into it. The rest stays where it was, and an emptied stack is deleted.
  - Otherwise the two stacks swap slots. That includes dropping onto a full stack of the same item.
- **`split_item(item_id, count, to_slot)`**
  - Takes `count`, from 1 to the stack's count − 1, into an empty slot as a new stack, or onto a stack of the same item with room for all of it.
  - Anything else is refused ("slot N is taken" or "no room for C in slot N"), and nothing changes.
- **`destroy_item(item_id, count)`**
  - Destroys `count`, from 1 to the stack's count. Destroying them all deletes the row.
  - The client asks for confirmation first; the server doesn't.
- **`items::grant(ctx, char_id, def, count)`** is for gathering, loot and crafting.
  - It tops up existing stacks of the item, lowest slot first, then fills empty slots from the lowest.
  - **All or nothing:** if everything doesn't fit, it returns "bag is full: room for R of C" and changes nothing.
  - Gathering decides what happens when the bag is full (M1 task 3).
- Dev builds add **`dev_grant_item(item, count)`**, which grants by content id to the caller's character in the world.

### Client
- A bag window opens and closes with I or B. It has 50 slots in a 10-wide grid. Each slot shows a short label, its count and a colour by kind, with the full name as a tooltip.
- Clicking works by picking up:
  - Click a stack to pick it up, then click a slot to move it there (the server merges or swaps). Clicking the same slot, or pressing Esc, puts it down.
  - Shift-click picks up half a stack (rounded down) to split.
  - Ctrl + right-click a stack asks to destroy it.
- The window draws what the server says. Nothing is predicted: a pick-up only highlights the slot, and the rows change when the server commits.
- The click logic (`Gameplay/BagModel.cs`) has no Godot dependency and is covered by xUnit tests.
- `--bag-demo` (dev builds): on entering, grants some items and works through a move, a merge, a split, a swap and a destroy using the same click logic. It logs each step and leaves the window open for captures.

## 3. Deviations and not yet done
- **No bag expansion yet.** All bags have 50 slots. Expansion (to 150, 10 at a time) comes with gold in M2. Expansion was sold for money in the original, and here it won't be (PLAN §1.2).
- No warehouse, equipment, chests, mail or ship holds yet; each is a new `place`.
- No item properties beyond the definition: grade, durability, binding, crafter and the rest come with gear (M2). Packs carry their origin and craft time in M1 task 6 and will need per-item data then.
- No sorting button, no dragging, no count dialog for splits (half only), and no dropping items on the ground.
- Stack sizes are ours (1,000 for materials, 100 for consumables); the original's exact sizes weren't found.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `items.toml` parses; duplicate wires, ids or names, wire 0, a bad kind, gear that stacks and stacks out of range are reported; each value changes the hash; both outputs carry the items | `server/contentgen/src/lib.rs` |
| module | grant tops up stacks then fills empty slots in order; all-or-nothing when full; unknown items and counts of 0 refused. Move to empty, merge with remainder, merge emptying the source, swap, swap with a full stack, own slot, out of range. Split to empty and onto a stack; refused onto another item or an overfull stack, for 0 or the whole stack. Destroy some and all; too many refused. Someone else's item is "no such item"; not in the world is refused; the view shows only your own characters' items | `server/world/src/tests/items.rs` |
| C# | `BagModel` builds the grid from rows for one character; click sequences give move, split and destroy intents; same-slot click and Esc cancel; a shift-click on a single item does nothing | `client/tests/BagTests.cs` |
| scenario | `loadbot items`: grant 1,500 of a material (two stacks), a gear item and a consumable; move, merge, split, swap and destroy, checking the bag after each; refusals for an unknown item id, a bad slot, and a grant that doesn't fit; empties the bag at the end | `cargo xtask bot items` |
| client | `--bag-demo` against a live server, with captures of the window | manual, with a capture |
