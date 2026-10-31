# 0036: Hearthshore is laid out in worldtool's code; characters start at a content-marked Wayshrine with a starting bag from content

- **Status:** Accepted, 2026-10-31
- **Context:**
  - M1's content needs one coastal zone of about 3 × 3 km and an island outpost across the water (PLAN §11). Until now everything in `world/` came from the placeholder generator: an even plain on the west coast, nodes scattered one per cell, camps at hand-picked centres, and new characters at (0, 0) on the trade island.
  - There is no editor yet. M2 builds the world in Terrain3D (ADR 0032), so a hand-editing tool built now would be thrown away.
  - Farming needs seeds before seed merchants and gold exist (docs/design/farming.md §3).
- **Decision:**
  - **The zone's layout is Rust data in `tools/worldtool/src/coast.rs`**: landforms applied to the placeholder's heights (hills, a ridge and levelled areas, each with a noisy outline), towns, roads, stations, posts, Wayshrines, NPC camps, node fields and named areas. The generator and every placement step take it from there, so `cargo xtask world --gen` stays the one way to write `world/`, and its tests check the committed files against it.
  - **Roads are land use only**; they don't change the ground.
  - **Named areas are a new world file**, `world/areas.tsv`, read by the client for the HUD. The server doesn't load them.
  - **The start is a Wayshrine marked `start = true` in content.** `create_character` puts new characters in front of it, and uses the old spawn when the world has none, so tests and bare servers still work. No new table.
  - **The starting bag is content**: `[bag] start` in `items.toml`, granted by `create_character`.
  - **`worldtool map`** draws the layout as a PNG, so it can be checked without the client.
- **Consequences:**
  - The terrain changed in the zone's regions (terrain hash `690339fb43366241`), and content changed: run `cargo xtask publish --clear` and then `cargo xtask world` once. Existing characters keep their saved positions.
  - Every new character, bots included, starts at Furrowdale with three stacks in its bag. Module tests empty the bag in their `create` helper; loadbot checks it against `BAG_START` in `Bot::arrive` and then empties it.
  - Editing the zone means editing `coast.rs`, regenerating and committing the outputs. When M2 moves the world into Terrain3D, the layout moves into its regions and these constants go.
