# 0009: Seed `Cargo.lock` from SpacetimeDB's own lockfile

- **Status:** Accepted, 2026-10-25
- **Context:** The module's tests need `spacetimedb`'s `test-utils` feature, which pulls in `spacetimedb-core` → `v8 145` → `temporal_rs 0.1.2`. That crate uses `icu_calendar`'s *unstable* API with a loose `2.1` requirement. A fresh resolve picks `icu_calendar 2.3`, and `temporal_rs` no longer compiles. The `icu_*` crates depend on each other, so pinning them one at a time fails.
- **Decision:** Our `Cargo.lock` starts from SpacetimeDB v2.13.0's lockfile (`raw.githubusercontent.com/clockworklabs/SpacetimeDB/v2.13.0/Cargo.lock`), trimmed by Cargo to our graph. Everything resolves to versions upstream tested, including `icu_calendar 2.1.1`.
- **Consequences:**
  - Don't run a blanket `cargo update`. Update named crates only.
  - When upgrading SpacetimeDB, re-seed from the new tag's lockfile.
  - If someone deletes `Cargo.lock`, the tests break; restore it from git.
