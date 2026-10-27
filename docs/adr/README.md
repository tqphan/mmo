# Decision records

One file per decision: `NNNN-short-title.md`, with status, context, decision and consequences. Superseded records stay, marked as such, with a link to the one that replaces them.

| # | Decision | Status |
|---|---|---|
| [0001](0001-fidelity-target.md) | Recreate the systems of the final Western version (June 2024) | Accepted |
| [0002](0002-one-seamless-world.md) | One seamless 16×16 km world | Accepted |
| [0003](0003-hobby-donations-no-monetization.md) | Hobby project, donations pay upkeep, nothing for sale | Accepted |
| [0004](0004-art-direction.md) | Stylized art, but placeholder geometry until the game systems are built; the look is chosen then | Accepted |
| [0005](0005-claude-builds-repo-is-memory.md) | Claude builds, the user directs; the repo is the memory; standing commit approval | Accepted |
| [0006](0006-spacetimedb-rust-server.md) | SpacetimeDB 2.13.0, Rust module, one self-hosted production instance | Accepted |
| [0007](0007-godot-dotnet-client.md) | Godot 4.7.x .NET client in C# | Accepted |
| [0008](0008-two-dev-machines-xtask.md) | Develop on Linux and Windows; dev tasks via `cargo xtask`; workspace at the repo root | Accepted |
| [0009](0009-lockfile-seeded-from-spacetimedb.md) | Seed `Cargo.lock` from SpacetimeDB's own lockfile | Accepted |
| [0010](0010-server-metrics-from-the-host.md) | Server timings come from the host's Prometheus metrics, not a module table | Accepted |
| [0011](0011-batched-movement-and-gzip.md) | Movement is published once per tick; clients use Gzip | Accepted |
| [0012](0012-content-generated-and-committed.md) | Content is generated into committed files; clients send a content hash | Accepted |
| [0013](0013-server-terrain-lenient-checks.md) | Server terrain: 2 m `u16` chunks per cell; checks take the kinder of both triangulations | Accepted |
| [0014](0014-ships-and-moving-decks.md) | Ships are stepped by the server; passengers move in the ship's frame | Accepted |
| [0015](0015-remote-entities-interpolate-then-dead-reckon.md) | Other entities are drawn 200 ms back, interpolated, then dead-reckoned | Accepted |
| [0016](0016-assets-inside-the-godot-project.md) | Third-party and generated assets live in `client/assets/`; the ledger stays in `assets/` | Accepted |
| [0017](0017-client-terrain-from-the-server-heightmap.md) | The client draws terrain from the server's `.mmoh` heightmap in LOD tiles; no Terrain3D | Accepted |
| [0018](0018-world-files-in-the-repo-and-a-terrain-hash.md) | The world's files live in the repo as compressed regions; clients send a terrain hash | Accepted |
| [0019](0019-client-ships-at-the-present-player-in-the-ships-frame.md) | The client draws ships at the present, and keeps a passenger in the ship's frame | Accepted |
| [0020](0020-reconnect-and-connection-keyed-despawn.md) | The client reconnects through the SDK and enters again; only the entering connection despawns a character | Accepted |
| [0021](0021-public-game-clock-and-account-labor.md) | A public `game_clock` gives clients game time; labor is one pool per account | Accepted |
| [0022](0022-item-rows-by-holder-and-place.md) | Item rows are addressed by holder and place; item definitions have permanent wire numbers | Accepted |
| [0023](0023-resource-nodes-public-rows-timed-by-a-scheduled-job.md) | Resource nodes are public rows with a regrowth timestamp; a gather is a scheduled job | Accepted |
| [0024](0024-crafting-one-unit-per-scheduled-run.md) | A craft is a private scheduled job that makes one unit per run and reschedules itself | Accepted |
| [0025](0025-plot-standing-from-tax-timestamp.md) | A plot's standing comes from its tax timestamp; demolition is one timer per plot; land use goes to the server | Accepted |
