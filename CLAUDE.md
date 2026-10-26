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
5. **Commit.** Git isn't initialized yet; ask the user before the first commit.
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
To be filled in during M0: build, test, start the local server, publish the module, generate bindings, run the client, run bots, capture frames.

## Environment (checked 2026-10-25)
- **Machine:** Windows 11, Ryzen 9 7900X, 32 GB RAM, RX 6950 XT.
- **Installed:** Git 2.45, Rust 1.90 (still needs the `wasm32-unknown-unknown` target), .NET SDK 7.0 and 10.0, SpacetimeDB CLI **1.10** (upgrade to 2.13.x pending).
- **Not installed:** Godot, Blender.
