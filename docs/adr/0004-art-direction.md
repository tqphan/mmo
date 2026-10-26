# 0004: Stylized art; placeholders until the game systems are built

- **Status:** Accepted, 2026-10-25 (user). Amended 2026-10-26 (user): the look is deferred.
- **Context:** Claude can make or assemble low-poly art (CC0 kits, procedural props). Hand-painted texturing needs far more custom work. The M0 style test (docs/design/art.md) showed flat and gradient low-poly. Having seen it, the user chose to put art aside: "we will use simple geometries as placeholder and worry about arts after all the game systems are built."
- **Decision:** The art stays stylized, and the final look (flat or gradient low-poly, or hand-painted; outlines; character proportions) is **chosen after the game systems are built**, not in M0. Until then the client uses **simple placeholder geometry**: capsules and boxes for characters, ships and props, and terrain coloured only by height and slope.
- **Consequences:**
  - M0 no longer waits on an art approval. Its exit is the spikes and the plan.
  - Don't spend time on art: no models, textures, animation or shader polish beyond what makes a placeholder readable (which side of a ship is the bow, water versus land, a nameplate).
  - The style test scene and its KayKit assets stay in the repo as a reference for the later decision.
  - Performance budgets measured with placeholders are lower bounds. The art pass re-measures them.
