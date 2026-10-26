# 0018: The world's files live in the repo as compressed regions; clients send a terrain hash

- **Status:** Accepted, 2026-10-26 (M0 "World building v0").
- **Context:** Until now the world was `target/world.mmoh`, generated on each machine and never committed. Nothing checked that the server's terrain came from the same file as the client's, so a mismatch showed up as a stream of corrections. The raw heightmap is 128 MB, which GitHub won't take as one file, and every edit would add another 128 MB to history. Uploads also used the dev-only `dev_set_gm`, so a `--prod` build couldn't be given terrain at all.
- **Decision:**
  - The world's source is `world/` in the repo: a manifest (`world.toml`) and 625 region files of 640 m (`.mmor`). Each region holds its heights as zig-zag differences from a planar prediction (`left + above − above-left`), split into byte planes and compressed with zlib, and its 4 m land-use raster, also compressed. The placeholder world is 9.4 MB. An edit only touches the regions it changes. Our own tool, `tools/worldtool`, writes them: the generator for now, editor tooling later (docs/design/world.md).
  - The **terrain hash** is the wrapping sum of a per-chunk hash: FNV-1a 64 over the cell key and samples, then the splitmix64 finaliser. It doesn't depend on order, so the server keeps it in a public `terrain_info` row and updates it on each upload. `enter_world` takes the client's hash and refuses a mismatch, as it does for content (ADR 0012). The client computes the hash from what it loaded rather than reading it from the manifest.
  - Land-use classes and climates are content (`content/land.toml`, through contentgen), so Rust and C# share them and they're part of the content hash.
  - Uploads run as the publisher: `cargo xtask world` reads the CLI's token with `spacetime login show --token` and passes it to loadbot in an environment variable. The publisher is always an admin, so this also works on production builds.
  - zlib, not zstd or Brotli: `flate2`'s default backend is pure Rust and already in the lockfile (ADR 0009), and .NET has `ZLibStream` built in. LZMA was only 14% smaller.
- **Consequences:**
  - A fresh clone has the world. `cargo xtask client` no longer generates it, and loading takes about 0.2 s in the client.
  - Regenerating with `cargo xtask world --gen` overwrites hand edits, once there are any. The generator stays only until editor tooling replaces it.
  - Bots enter with the hash they read from `terrain_info`, since they test the server, not a world file. `bot visitor`, which walks the world's ground, checks the hash first.
  - Land use isn't uploaded until housing and farming need it (M2). It will get its own private table and hash.
