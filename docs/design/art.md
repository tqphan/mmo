# Art pipeline and style test

Status: v0.2, 2026-11-01. M0 style test, and the world's ground and sea (§2, "The world's ground and sea"). Code: `client/src/Presentation/`, `client/shaders/`, `client/src/World/GroundMasks.cs`, assets in `client/assets/`. Decisions: ADR 0004 (the look, deferred), ADR 0016 (where assets live) and ADR 0040 (the ground and sea before the playtest).

> **2026-10-26: the look is deferred** (ADR 0004). After seeing this test, the user chose simple placeholder geometry until the game systems are built. The style test stays as a reference for that later decision.
>
> **2026-11-01: the ground and the sea get their look** (ADR 0040), in the style test's gradient low-poly. The user wanted them to look good before the M1 playtest. Characters, props and buildings are still placeholders.

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
- **Water** (`Water.cs`, `water.gdshader`; as first built, since reworked for the world, below): a 1 km square subdivided every 4 m with three Gerstner waves (22, 13 and 9 m long, about 0.7 m high in total). Its colour runs from turquoise to deep blue with the depth of water behind it, read from the depth buffer, and there's a wobbling foam line where that depth is under about 0.7 m. The waves calm towards the square's edge, and a flat ring of the same material carries the sea out to 6 km.
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

### The world's ground and sea (ADR 0040)
`Landscape.Add` puts the terrain, the sea, the sun and the environment in both the game and the world test.

**Ground masks** (`World/GroundMasks.cs`): one RGBA8 texture with a texel per 4 m over the whole map, on the land raster's grid, built from `world/` when the client loads (about 1.3 s).

| Channel | Holds |
|---|---|
| R | road land (255) |
| G | town (255) or housing (100) land |
| B | inland water nearby: texels below sea level that the open sea doesn't reach, spread by two 13-texel box blurs (about 50 m) |
| A | open sea nearby: texels below sea level joined side to side to the map's edge, spread the same way. About 0.5 at the waterline and 1 some 50 m out |

**Ground** (`shaders/terrain.gdshader`), in order:
- **Noise:** world-space value noise at four scales: wide drifts (80 m), patches (14 m), detail (1.25 m) and grain (25 cm). The finer layers fade out once a pixel covers more than they do, so the distance doesn't shimmer.
- **Grass:** lush and light drifts, darker hollows, cooler above 25–90 m, and the detail and grain for texture.
- **Rock:** where the slope passes about 25°, and on gentle ground above 110–170 m. It has soft strata that follow the contour.
- **Sand:** where the open sea is near and the ground is under about 1.8 m (ragged), and in a band about 20 m back from the waterline, with a crisp edge however gently the beach rises. Wet sand at the waterline, turning to weedy seabed below 1.5–6 m.
- **Marsh:** mud and reeds on flats under about 1.3 m more than 50 m from the sea, and round inland pools.
- **Land use:** roads in dirt from the full-resolution mask level (a smaller level smears them); towns as trodden grass worn to earth in patches; housing land slightly warmer.
- **Bump:** the normal is tilted by the detail noise (bump mapping without tangents), more on rock.

**Sea** (`shaders/water.gdshader`):
- **Waves:** four Gerstner waves of 31, 17, 11 and 6.5 m, which die down from about 40 m out to the waterline (the A mask from 0.95 to 0.62). Pools away from the open sea lie flat.
- **Ripples:** two scrolling noise layers tilt the normal, fading out by 400 m. The sky's reflection uses a calmer normal, because full ripples flare in rings at grazing angles.
- **Colour:** the seabed comes through from the screen texture, bent a little by the surface. Red is absorbed first, then green, so shallows turn turquoise and fade into the deep colour by depth. Water a few centimetres deep reads as wet sand.
- **Sky:** Schlick fresnel to a sky tint, up to half. The engine's own specular fades past 300 m, where the flat far sea would otherwise mirror the pale horizon and wash out.
- **Foam:** lapping bands within about 12 m of the waterline, measured by the A mask (beaches here fall about 2 cm a metre, so foam by depth spread over tens of metres), skipped on the thinnest film of water; streaks on the highest crests out at sea.

**Flora** (ADR 0041; `World/FloraPlan.cs`, `Presentation/Greenery.cs`, `Props.cs`, `shaders/flora.gdshader`):
- **Thirteen kinds, three variants each,** built from cones, tubes, blades and lumpy icospheres with vertex colours: pine (3–5 tiers), oak, birch, autumn tree (orange, red, gold), dead tree, bush (one with berries), boulder, rock, driftwood, grass tuft, flowers (yellow, violet, white), reeds with cattails, and pebbles.
- **Placement:** a jittered grid, one candidate every 5 m for the big layer and 0.9 m for ground cover, hashed from the cell so it's the same on every machine. Each candidate gets at most one piece, chosen by the ground under it:

| Ground | Big layer | Ground cover |
|---|---|---|
| sea, roads, towns | nothing | nothing (towns get a few tufts) |
| housing land | the odd bush | tufts |
| beach (the open-sea mask, as in the shader) | driftwood, rocks | pebbles, a few tufts |
| marsh (flats away from the sea, round pools) | dead trees, bushes | reeds and tufts |
| steep (slope over about 30°) | boulders, rocks, clinging pines | pebbles, tufts |
| elsewhere | forest by wide noise with clearings, thicker by area: Greymane Wood 32%, Wrackline Point 22% (mostly pine), the crags and scarp 20%, the downs and meadow 6%. Pines take over with height. Lone trees, bushes and boulders in the open | tufts in clumps, flowers in patches (twice as many on the downs and meadow) |

- **Drawing:** trees, bushes and stones in 256 m cells to 900 m with shadows, and ground cover in 32 m cells to 80 m without. Up to three worker threads plan cells nearest first, and the main thread spends at most 2 ms a frame making MultiMeshes. Each instance has a turn, a size and a tint.
- **Shader:** vertex colours (sRGB, clamped) times the instance tint. Sway grows with the square of height: little for trees, more for bushes, most for grass. Ground cover is lit as facing straight up, like the ground under it.

**Light** (`Landscape.cs`): sun at 38° in warm light, sky ambient 0.55, Filmic tonemap, thin fog with aerial perspective, a little glow, and saturation 1.12 and contrast 1.05.

## 3. Deviations and not yet done
- **Assets live in `client/assets/`, not `assets/`** (ADR 0016). The ledger stays at `assets/LEDGER.md`.
- **The characters are chibi-proportioned**, because that's what KayKit makes. The character style is part of the decision; Quaternius's CC0 characters have more realistic proportions but aren't on a host the xtask can fetch from.
- **The style test is a dev scene.** The game client still draws capsules on a flat checkerboard. Real terrain comes with spike 4 (our own tiles, ADR 0017); everything stays placeholder until the art pass (ADR 0004).
- **No foot-sliding fix.** Animations play at their authored speed, so feet may slide a little at run speed.
- **No day/night cycle, sky dome art or foliage wind** yet (PLAN §6.3).
- **Flora doesn't collide** (ADR 0041), and doesn't fade between distances: trees pop in at 900 m (in the fog) and tufts at 80 m.
- **Climate doesn't tint the ground** yet. Hearthshore is all temperate.
- **Roads have stair-stepped edges** from the 4 m land raster.

## 4. Test plan
| What | How | Result, 2026-10-26 |
|---|---|---|
| Builds with no warnings | `cargo xtask test` (the Godot project builds with warnings as errors) | passes |
| Runs with no engine errors | every capture's log is free of `ERROR` and `SHADER ERROR` | clean |
| Both looks, both shots, with and without outlines | `cargo xtask capture style-<look>-<shot> 240 --style-test <look> --shot <shot> [--outline]`, then look at the frames | `docs/captures/2026-10-26-style-*.png` |
| Characters animate and face the way they move | look at frames a few apart in a `follow` capture | walking and running poses change between frames; the runner faces its direction of travel |
| Water: depth colour, shore foam, no seam at the edge of the waves, no mesh edge at the horizon | the `wide` captures | turquoise shallows, white foam line, no seam |
| Cost | Movie Maker's GPU time per frame on the laptop | about 1 ms per frame on the RTX 5070 Ti Mobile; measuring a mid-range budget is spike 4 |

### The world's ground and sea, 2026-11-01
| What | How | Result |
|---|---|---|
| Ground masks: open sea joined to the edge, an enclosed pool isn't sea, the sea fades inland, land use in R and G | xUnit, `GroundMasksTests.cs` | passes |
| No shader errors, in the game, the world test and the style test | capture logs | clean |
| Before and after from the same five views (town, beach, fen, crags, sea) | `cargo xtask capture <name> 90 --world-test world --view X,Z,HEIGHT,YAW,PITCH`; the views are in docs/captures/2026-11-01-look-*.png | pale, flat ground and one flat sea before; patches, beaches, marsh, rock and a see-through sea after |
| Shallows on a near-level beach: no foam sheets or rings | the `shallows` view (−3100, 330, 6 m up) | seabed through clear water, foam only at the waterline |
| In the game | `cargo xtask capture` with a new profile at the Furrowdale Wayshrine | `2026-11-01-look-game-start.png` |
| Cost | the world test's `walk` and `sail` routes with `--no-vsync`, laptop RTX 5070 Ti Mobile | GPU p50 1.35 → 1.76 ms walking, 1.05 → 1.43 ms sailing |

### Flora, 2026-11-01
| What | How | Result |
|---|---|---|
| Same flora every time; nothing in the sea, on roads or in towns; trees inland of the beach, driftwood and pebbles on it; no reeds by the sea; a named wood has many more trees; steep ground gets stones | xUnit, `FloraPlanTests.cs` | passes |
| No flares from bad colours or normals | captures in the fen, the wood and the town | clean (vertex colours below zero made NaN in `pow`; they're clamped now) |
| Looks | the world test's views | `docs/captures/2026-11-01-flora-{wood,fen,shore,game-start}.png` |
| Cost | `walk`, `sail` and `cross` with `--no-vsync`, RTX 5070 Ti Mobile | GPU p50 2.41 / 1.44 / 2.49 ms, p99 3.08 / 1.76 / 2.97 ms; at most 2.6 M triangles and 1,199 draw calls; no frame over 33 ms |

