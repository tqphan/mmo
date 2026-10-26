# Spike 4: world scale

Status: done, 2026-10-26 (Radeon 610M runs added the same day). PLAN §11, M0 spike 4. Spec: docs/design/worldstream.md. Decisions: ADR 0017 (own terrain instead of Terrain3D), ADR 0004 (placeholder look).

## Question

Can the client show the whole 16 × 16 km world, with terrain, sea and streaming, within budget at mid-range settings? And can the server hold the whole map's terrain?

**Done when:** at 1080p the terrain and sea take at most 4 ms of a 16.7 ms frame on a mid-range GPU (around a GTX 1660 or RX 6600), no frame goes over 33 ms while streaming at sailing speed, and terrain uses less than 300 MB of video memory (docs/design/worldstream.md §2).

## What changed on the way

1. **Terrain3D was dropped** (ADR 0017). Its 1.0.2 source showed that adding or removing a region rebuilds the texture arrays of every loaded region, and removed regions stay in memory until saved (RESEARCH, Terrain3D). It also ships as native code, which the user preferred not to run. The client now draws terrain itself from the server's `.mmoh` file.
2. **Workers take tiles in batches.** At first each worker was given one tile per frame, so streaming speed followed the frame rate: on the Radeon 610M at 55 fps the first view took 9–10 s, against 0.8 s on the RTX. Each worker now takes up to 16 of the nearest tiles, and the first view streams in 0.5–0.7 s on both GPUs.
3. **Tiles shrank from 512 m to 256 m.** With 512 m tiles, walking drew 2.8 M triangles, because a whole tile went to full detail once any corner was within 400 m. Swapping in one full-detail tile took up to 11.6 ms on the main thread. 256 m tiles at full detail within 200 m fixed both. That's still about 10 pixels per 2 m quad at 1080p.

## Method

- **Map:** `cargo xtask bot world-gen` writes a placeholder 16 km world laid out as PLAN §9 sketches it (`sidecars/loadbot/src/world.rs`): two continents, a central sea with three islands, the north, and outer ocean, with mountains up to about 400 m. 8001² samples, 128 MB, 40% land; 5 s to generate.
- **Client:** `cargo xtask client --world-test target/world.mmoh --route R --no-vsync --secs S`, 1920 × 1080, Forward+, 2× MSAA, a shadowed sun (200 m), fog, glow and filmic tonemapping. The first 5 s are left out of the summary.
- **Hardware:** the Linux laptop's two GPUs.
  - RTX 5070 Ti Laptop GPU (NVIDIA 615.78), well above mid-range.
  - Radeon 610M integrated GPU (Mesa RADV), well below mid-range: 2 compute units sharing the system's DDR5. Run with `godot --path client --display-driver wayland --gpu-index 1 -- --world-test …`. Under X11 (Godot's default here, through XWayland) the Radeon can't create a swapchain: every frame fails with `VkResult -3` and nothing is drawn.
- **Server:** the full file uploaded with `loadbot terrain-upload` to a local SpacetimeDB 2.13.0, measuring the host process's resident memory before and after.

## Results

**Client, 256 m tiles, RTX 5070 Ti** (whole-scene GPU time, so terrain, sea, sky, shadows and post-processing together):

| Route | Frame p50 / p99 / max | GPU p50 / p99 | Peak triangles | Draw calls | Terrain meshes | Video memory |
|---|---|---|---|---|---|---|
| `sail`, 4 m above the sea at 11 m/s, 60 s | 1.11 / 1.44 / 11.9 ms | 1.05 / 1.36 ms | 0.18 M | 675 | 3 MB | 283 MB |
| `walk`, on the ground at 5.4 m/s, 60 s | 1.50 / 1.94 / 12.9 ms | 1.45 / 1.87 ms | 0.87 M | 567 | 18 MB | 294 MB |
| `cross`, 250 m up at 150 m/s, the whole map, 110 s | 1.56 / 2.06 / 12.0 ms | 1.50 / 1.96 ms | 0.58 M | 879 | 19 MB | 316 MB |
| `high`, circling from 3 km up, 60 s | 1.38 / 1.79 / 15.2 ms | 1.32 / 1.64 ms | 0.07 M | 983 | 19 MB | 312 MB |

**Client, 256 m tiles, Radeon 610M** (same settings: 1080p, 2× MSAA, shadows, glow):

| Route | Frame p50 / p99 / max | GPU p50 / p99 | Draw calls |
|---|---|---|---|
| `sail`, 60 s | 17.4 / 18.2 / 20.0 ms | 17.1 / 17.9 ms | 704 |
| `walk`, 30 s | 21.0 / 24.0 / 24.2 ms | 20.7 / 23.6 ms | 567 |
| `cross`, 110 s | 23.2 / 27.8 / 28.5 ms | 22.9 / 27.5 ms | 877 |
| `high`, 60 s | 19.3 / 20.0 / 21.7 ms | 19.0 / 19.5 ms | 715 |

No frame went over 33 ms on any route on either GPU, and no run logged an error (apart from the X11 swapchain failure above). Triangles and memory are the same as on the RTX.

**For comparison, 512 m tiles:** `walk` drew 2.80 M triangles at 1.73 / 2.17 ms GPU, and `cross` 2.12 M at 2.06 / 2.64 ms, with up to 11.6 ms spent swapping in one mesh.

**Streaming:**
- At start, the first view (1,000–1,200 tiles out to 10 km) is built in **0.5–0.7 s** on 4 worker threads, on either GPU (0.8–1.3 s before batching).
- At 150 m/s the queue stays at 0 or 1: tiles are built as fast as they're needed. The 15.6 km jump when `cross` loops back to the west edge re-streams the whole view (about 900 meshes) in **under 1 s on both GPUs**. Teleports will behave the same.
- Swapping meshes in stays within the 2 ms budget on ordinary frames. The longest single swap, 8.3 ms, came during the teleport re-stream. The other long frames (12–15 ms) are rare and come with no streaming activity; garbage collection is the likely cause, but that isn't confirmed.
- 1,608 of the 3,969 tiles are wholly under deep water and are never built.

**Memory:**
- The client's peak resident memory is **854 MB**, against **732 MB** for the style test scene. The world adds about 122 MB, nearly all of it the heightmap. An earlier version read the file into a second buffer first and peaked at 978 MB.
- Video memory is 283–316 MB in all. The terrain meshes are at most 19 MB of that; the rest is the engine's framebuffers, shadow map and sky.

**Server:** all 62,500 chunks (the whole map, ocean included) uploaded in **3.6 s**. The host process went from 447 to **619 MB**, so +172 MB for the 136 MB of samples.

## Verdict

**Go.**
- Streaming and memory pass clearly: no hitches, sub-second re-streams after a teleport, 19 MB of meshes and 122 MB of heightmap.
- Frame time passes with room on both GPUs, and they bracket mid-range.
  - The RTX, well above mid-range, takes at most 2.0 ms for the whole scene at p99.
  - The Radeon 610M, an integrated GPU far below mid-range, still holds 36–60 fps at 1080p with no frame over 33 ms. Its whole-scene p99 is 18–28 ms, and terrain alone (walking minus sailing) is about 4–6 ms.
  - A GTX 1660 or RX 6600 has roughly 9–16 times the 610M's shader throughput (by specifications, not measured). On that scale the whole scene would take about 1.5–3 ms there, and terrain under 1 ms, inside the 4 ms budget.
- This is with placeholder art (ADR 0004). The art pass re-measures.

## Levers if it gets tight
- **The sea** is the largest single cost: a full-screen transparent pass that reads the depth buffer (`sail` is 1.05 ms with almost no terrain). Use coarser wave subdivision further out, or a cheaper shader past a few hundred metres.
- **Draw calls** (up to about 1,000) come from many small far tiles. Merge the coarsest levels into 1–2 km meshes, or use a quadtree so far tiles cover more ground each.
- **Height-aware levels:** count the camera's height above a tile in its distance, so high fliers and gliders draw coarser terrain.
- **Shadows:** only levels 0 and 1 cast them now. The sun's shadow distance (200 m) can drop at lower settings.

## Follow-ups
- Measure on the Windows PC's RX 6950 XT when it's set up (BACKLOG task 3), for a third data point.
- On the Radeon most of the frame isn't terrain: sailing with almost no terrain on screen already costs 17 ms. A low-settings preset (no MSAA, no glow, shorter shadows, a cheaper sea) is a lever for integrated GPUs when graphics settings exist.
- Put the tiles and their collision (`HeightMapShape3D` near the player) into the game client, so players walk on the real terrain the server checks against.
- From 3 km up, the far edge of the sea ring shows against the sky. Players don't get that high; the sky and sea get a proper horizon in the art pass.
