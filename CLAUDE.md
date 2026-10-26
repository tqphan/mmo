# CLAUDE.md

This is a hobby MMO that recreates the **systems** of ArcheAge's final Western version (as of its June 2024 shutdown). Everything else is **original**: the world, the names, the story and the art. The art is stylized low-poly.
- **Server:** SpacetimeDB 2.13.x, with the module written in Rust.
- **Client:** Godot 4.7.x .NET, written in C#.
- **Who does what:** Claude builds it over many sessions. The user directs, playtests and approves.

## Start of every session
1. Read `docs/STATUS.md` (where things stand) and the top of `docs/BACKLOG.md` (what's next). Both get created in milestone M0. Until then, use `docs/PLAN.md` §13.
2. Read the parts of `docs/PLAN.md` that apply to the task, and `docs/design/<system>.md` if that system has a spec.

## Rules
- **Original content only.** Never use ArcheAge's names (game, places, races, factions, NPCs, items, skills, currencies), text, art, models, audio, UI or music. Never use data from its client or from emulators or private servers. Mechanics and numbers taken from public sources are fine. All names come from `docs/world/`; if a name doesn't exist yet, invent one and add it there.
- **The server decides outcomes.** The only exception is the player's own movement, which the client predicts and reducers check.
- **Never tick what can be computed from timestamps.** Growth, labor, demand, taxes and decay are calculated from a start time and a rate.
- **Private by default.** Any client can read a public table. Hidden information goes in private tables and is exposed through views.
- **All game time goes through `now(ctx)`.** It includes a test-clock offset that only exists in dev builds.
- **Pin versions.** Upgrading SpacetimeDB or Godot is its own task, recorded in an ADR in `docs/adr/`.
- **Asset licenses.** Prefer CC0. CC-BY is allowed with credits. Anything else needs the user's OK. Record every third-party asset in `assets/LEDGER.md`.
- **No monetization.** Donations never buy anything in-game.

## Workflow for each task
1. **Spec first.** If the system has no `docs/design/<system>.md`, write one. It covers how the original worked (with sources and confidence), our rules and names, any deviations, and a test plan.
2. **Build** the smallest end-to-end slice: server, client and tests.
3. **Verify** with tests and bot scenarios. For visual changes, capture frames in Movie Maker mode and look at them.
4. **Update docs.** Update `docs/STATUS.md` and `docs/BACKLOG.md`, and record any decisions as ADRs.
5. **Commit.** The repository is https://github.com/tqphan/mmo (public, branch `main`). Ask before committing or pushing unless the user has given standing approval, and record any such approval here. (Standing approval was given; see below.)
6. **Report** to the user: what changed, how to try it, and what needs their decision.

## Definition of done
- Behavior matches the spec, and tests cover it and pass.
- No new warnings.
- Docs are updated.
- Visual changes have captures.
- New assets are in the ledger.
- The user knows how to try it.

## Docs
- `docs/PLAN.md`: plan, architecture, milestones
- `docs/RESEARCH.md`: research, with sources
- `docs/design/`: one spec per system
- `docs/adr/`: decision records
- `docs/world/`: world bible
- `docs/playtests/`: playtest notes

## Commands
Run these from the repo root. They work on Linux and Windows (ADR 0008).
- `cargo xtask content`: regenerate the content outputs from `content/` (Rust `sim::content`, C# `client/content`) and the client's test vectors from `sim` (`client/tests/vectors`). Run after editing `content/` or the `sim` code the client ports, and commit the outputs too.
- `cargo xtask test`: content and vector staleness checks, unit and module tests, the Godot client's C# build and xUnit tests, then clippy with warnings as errors (with and without `dev`)
- `cargo xtask server`: local SpacetimeDB on 127.0.0.1:3000, in the foreground (data in `target/stdb-data`)
- `cargo xtask publish [--prod] [--clear]`: build the module to wasm and publish it as `mmo-world` on the local server
- `cargo xtask generate`: regenerate bindings (Rust → `sidecars/loadbot/src/module_bindings`, C# → `client/net`). Run after any schema or reducer change.
- `cargo xtask bot <scenario> [args]`: run a loadbot scenario against the local server (`bot metrics` prints server timings; `bot swarm --bots N --area M` is the throughput test, docs/spikes/throughput.md; `bot terrain --bots N --secs S` walks bots over uploaded test terrain in the map's south-west corner and clears it afterwards, docs/design/terrain.md; `bot terrain-gen` and `bot terrain-upload --file F` write and upload `.mmoh` height files; `bot ship --secs S` sails a clipper with bots walking its deck and firing its cannon, docs/design/ships.md; `bot visitor --secs S [--expect N]` runs circles near spawn as company for client tests)
- `cargo xtask check`: test, then publish with `--clear` and run every scenario (needs `server` running)
- `cargo xtask client [args]`: build and run the Godot client; args such as `--profile NAME` and `--autopilot circle:X,Z,R` go to it (docs/design/client.md)
- `cargo xtask capture <name> <frames> [args]`: the client in Movie Maker mode at 30 fps, PNGs in `target/captures/<name>/`. Look at them with Read; keep the ones worth keeping in `docs/captures/`.

Gotchas:
- `spacetime call` connects and disconnects around each call, and disconnecting despawns the character. Use loadbot for anything that needs a session.
- Never run a blanket `cargo update` (ADR 0009).
- View rows (`my_presence`, `my_characters`) have no primary key, so the C# SDK never fires `OnUpdate` for them. A change arrives as an insert and then a delete (docs/design/client.md).
- SpacetimeDB 2.13 accepts `#[reducer(update)]` but never runs it on publish; it becomes an ordinary reducer that clients can call. Reducers also have no clock, so timings come from the host's `/v1/metrics` (ADR 0010).

## Environment (checked 2026-10-25)
Development happens on two machines, so keep everything cross-platform (ADR 0008). Details are in `docs/STATUS.md`.
- **Linux laptop (Arch):** Rust 1.99.1 with the wasm32 target, SpacetimeDB CLI 2.13.0 and Godot 4.7.2 .NET in `~/.local/bin` (not on PATH; the xtask finds them). .NET SDKs from pacman. No Blender.
- **Windows PC:** Rust 1.90 (needs the wasm32 target), .NET SDK 7.0 and 10.0, SpacetimeDB CLI 1.10 (needs upgrading to 2.13.0). No Godot or Blender. The xtask looks for Godot in `$GODOT`, on PATH, or in `%LOCALAPPDATA%\Godot\godot.exe`.

## Standing approvals
- 2026-10-25: Claude may commit **and push** to `main` after each finished task without asking (ADR 0005).
- 2026-10-25: installing the Rust wasm target and the SpacetimeDB CLI on the Linux machine.
- 2026-10-25: installing the client toolchain on the Linux machine: Godot 4.7.x .NET (into `~/.local`) and the .NET 8 and 10 SDKs (pacman; needs the user's sudo password).
