# 0012: Content is generated into committed files, and clients send a content hash

- **Status:** Accepted, 2026-10-25
- **Context:** Game data in `content/` (PLAN §5.4) has to reach the Rust module, which builds to wasm, and the Godot C# client. A mismatched client would predict with the wrong rules, for example the wrong speed limits, and get corrected constantly.
- **Decision:**
  - `contentgen` turns `content/*.toml` into `server/sim/src/content.rs` and `client/content/Content.g.cs`. Both outputs are committed, rather than generated in a `build.rs`.
    - Reviews and diffs show what changed in the game's data.
    - The client and the module build without running Rust tooling first, and `build.rs` shouldn't write outside its own output folder.
    - `contentgen check` in `cargo xtask test`, plus a unit test, catch stale outputs.
  - The content hash is 64-bit FNV-1a over a canonical form of the data, embedded as a constant on both sides. `enter_world` takes the client's hash and refuses a different one.
- **Consequences:**
  - After editing `content/`, run `cargo xtask content` and commit the outputs with it.
  - Changing `enter_world`'s arguments changed the bindings; loadbot passes `sim::content::CONTENT_HASH`.
  - The hash detects honest mismatches only. It isn't a security measure; the server still decides outcomes.
  - A comparison of committed and regenerated text ignores CRLF, so a Windows checkout with `autocrlf` passes.
