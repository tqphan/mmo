# 0006: SpacetimeDB 2.13.0 with a Rust module, one self-hosted production instance

- **Status:** Accepted, 2026-10-25
- **Context:** See PLAN §3 and RESEARCH §3. Each database is single-threaded, so the CPU cost of each transaction is the capacity limit. The BSL 1.1 license allows one production instance.
- **Decision:**
  - The server is one SpacetimeDB database (`mmo-world`) running one Rust module (`server/world`). Pure rules live in `server/sim`, which has no SpacetimeDB dependency.
  - Pinned to **SpacetimeDB 2.13.0**: the CLI, `spacetimedb = "=2.13.0"` and `spacetimedb-sdk = "=2.13.0"`. Upgrading is its own task, with an ADR.
  - The module's `dev` cargo feature adds the test-clock offset and dev-only reducers (`dev_advance_clock`, open to any caller). **Production builds never enable `dev`** (`cargo xtask build --prod`).
  - Production is one self-hosted instance. Development runs on a local server.
- **Consequences:** Reducer tests use the in-memory `test-utils` datastore (`TestContext`). Scenario tests use Rust bots built on the client SDK (`sidecars/loadbot`).
