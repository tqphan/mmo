# Art pipeline and style test

Status: v0.1, 2026-10-26. M0 style test. Code: `client/src/Presentation/`, `client/shaders/water.gdshader`, assets in `client/assets/`. Decisions: ADR 0004 (the look, deferred) and ADR 0016 (where assets live).

> **2026-10-26: the look is deferred** (ADR 0004). After seeing this test, the user chose simple placeholder geometry until the game systems are built. The style test stays as a reference for that later decision; nothing below is the game's look yet.

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| The original was built on CryEngine 3 with a semi-realistic, painterly fantasy look, not low-poly | A | public trailers and store pages |
| Large open land and sea, with a horizon you could sail towards; water was a central feature | A | as above, RESEARCH §2 |

Nothing about the original's art carries over (CLAUDE.md, PLAN §1.3). The only lessons taken are about readability: the sea, coasts and land use have to read at a distance, because players sail and build there. Everything below is our own choice (PLAN §7).

## 2. Our rules

### Where assets live
- Godot can only load files inside its project, so third-party and generated assets live in `client/assets/<source>/` (ADR 0016). `assets/LEDGER.md` at the repo root records every one of them, as PLAN §4.8 says.
- Each source folder keeps its licence file next to the assets (for example `client/assets/kaykit/LICENSE.txt`).
- Commit the `.import` files Godot writes next to each asset. They hold the import settings, so both machines import the same way.
- `cargo xtask client` and `capture` run Godot's headless import before every run, which takes about 2 s when nothing changed. A new asset is picked up without opening the editor.

### The style test scene
`cargo xtask client --style-test flat|gradient [--outline] [--shot follow|wide]`. It runs offline, with no server, and is built in code by `Presentation/StyleTest.cs`:
- **Island** (`Island.cs`): a procedural island about 300 m across, on the server's 2 m terrain grid. Gentle near the coast and ridged inland, with a sea floor 7 m down. The diagonals alternate so facets don't line up in stripes.
- **Palette** (`Palette.cs`): a short list of bright colours. Ground colour comes from height and slope (sand, two grasses, two rocks, sea floor) with a little low-frequency tint so wide areas aren't one flat tone.
- **Props** (`Props.cs`): procedural pines, broadleaf trees (green and autumn) and rocks, made from cones, prisms and lumpy icospheres with vertex colours. They're scattered in noisy clumps on gentle grass and drawn with one `MultiMesh` per kind.
- **Water** (`Water.cs`, `water.gdshader`): a 1 km square subdivided every 4 m with three Gerstner waves (22, 13 and 9 m long, about 0.7 m high in total). Its colour runs from turquoise to deep blue with the depth of water behind it, read from the depth buffer, and there's a wobbling foam line where that depth is under about 0.7 m. The waves calm towards the square's edge, and a flat ring of the same material carries the sea out to 6 km.
- **Characters** (`CharacterModel.cs`): KayKit Adventurers (CC0). A knight walks a 6 m circle at walk speed (2.0 m/s), a rogue runs an 11 m circle at run speed (5.4 m/s), and another rogue stands idle. Each keeps only the props named for its hands, since the files come holding every weapon in the pack. Models are turned to face -z, like the rest of the client.
- **Light:** a warm sun with shadows to 140 m, sky ambient light, a filmic tonemap, light distance fog and a little glow.
- **Camera:** `follow` starts inland behind the knight looking out to sea and orbits slowly. `wide` circles the whole island from 85 m up.

### The two looks
| | `flat` | `gradient` |
|---|---|---|
| Terrain | one normal and one colour band per triangle | smooth normals, colours blended between vertices, wrapped diffuse light |
| Props | faceted | smooth normals |
| Water | faceted panels (normal from screen-space derivatives) | smooth wave normals |
| Characters | the same KayKit models in both | |

`--outline` adds a dark inverted-hull outline to the characters (a front-culled overlay grown by 2.5 cm). PLAN §6.3 leaves outlines to this test.

Hand-painted, the other option in ADR 0004, can't be shown with the tools we have. It needs painted textures and models made for them (PLAN §7). If the user picks it, that becomes its own task.

## 3. Deviations and not yet done
- **Assets live in `client/assets/`, not `assets/`** (ADR 0016). The ledger stays at `assets/LEDGER.md`.
- **The characters are chibi-proportioned**, because that's what KayKit makes. The character style is part of the decision; Quaternius's CC0 characters have more realistic proportions but aren't on a host the xtask can fetch from.
- **The style test is a dev scene.** The game client still draws capsules on a flat checkerboard. Terrain3D, real terrain and character models come with spike 4, the world exporter and M1.
- **No foot-sliding fix.** Animations play at their authored speed, so feet may slide a little at run speed.
- **No day/night cycle, sky dome art or foliage wind** yet (PLAN §6.3).

## 4. Test plan
| What | How | Result, 2026-10-26 |
|---|---|---|
| Builds with no warnings | `cargo xtask test` (the Godot project builds with warnings as errors) | passes |
| Runs with no engine errors | every capture's log is free of `ERROR` and `SHADER ERROR` | clean |
| Both looks, both shots, with and without outlines | `cargo xtask capture style-<look>-<shot> 240 --style-test <look> --shot <shot> [--outline]`, then look at the frames | `docs/captures/2026-10-26-style-*.png` |
| Characters animate and face the way they move | look at frames a few apart in a `follow` capture | walking and running poses change between frames; the runner faces its direction of travel |
| Water: depth colour, shore foam, no seam at the edge of the waves, no mesh edge at the horizon | the `wide` captures | turquoise shallows, white foam line, no seam |
| Cost | Movie Maker's GPU time per frame on the laptop | about 1 ms per frame on the RTX 5070 Ti Mobile; measuring a mid-range budget is spike 4 |
