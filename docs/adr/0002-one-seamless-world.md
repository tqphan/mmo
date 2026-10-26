# 0002: One seamless 16×16 km world

- **Status:** Accepted, 2026-10-25 (user)
- **Context:** The original had continents and an ocean that ships cross. Instanced zones would break the "sail on the horizon" feel.
- **Decision:** One seamless map, 16×16 km, origin at the centre, single-precision coordinates. Dungeons, arenas, the prison and the courthouse are separate instances that share the cell scheme through an instance id.
- **Consequences:** Interest management uses 64 m cells (250×250) and tiered subscriptions (PLAN §4.3). Precision stays fine within ±8 km. Terrain3D regions of 1 km, about 256 in all.
