// The world's ground (docs/design/art.md): colour from height, slope and the ground masks
// (GroundMasks.cs), broken up by world-space noise so wide areas aren't one flat tone. Grass in
// patches, rock with strata on steep ground and high up, sand only where the open sea is near,
// mud and reeds round inland water, dirt roads and towns with soft, ragged edges.
shader_type spatial;
render_mode cull_disabled, diffuse_lambert_wrap, specular_schlick_ggx;

#include "res://shaders/noise.gdshaderinc"

// R road; G town (1) or housing (0.39); B inland water and A open sea nearby. A texel per 4 m
// over the whole map (GroundMasks.cs).
uniform sampler2D masks : filter_linear_mipmap, repeat_disable;
uniform float world_half = 8000.0;

uniform vec4 grass_lush : source_color = vec4(0.30, 0.56, 0.20, 1.0);
uniform vec4 grass_light : source_color = vec4(0.55, 0.68, 0.26, 1.0);
uniform vec4 grass_dark : source_color = vec4(0.18, 0.40, 0.17, 1.0);
uniform vec4 grass_high : source_color = vec4(0.36, 0.50, 0.27, 1.0);
uniform vec4 sand_dry : source_color = vec4(0.96, 0.84, 0.56, 1.0);
uniform vec4 sand_wet : source_color = vec4(0.66, 0.58, 0.42, 1.0);
uniform vec4 seabed : source_color = vec4(0.52, 0.55, 0.42, 1.0);
uniform vec4 mud : source_color = vec4(0.42, 0.37, 0.25, 1.0);
uniform vec4 reeds : source_color = vec4(0.50, 0.55, 0.27, 1.0);
uniform vec4 rock_light : source_color = vec4(0.62, 0.56, 0.46, 1.0);
uniform vec4 rock_dark : source_color = vec4(0.50, 0.44, 0.36, 1.0);
uniform vec4 dirt : source_color = vec4(0.55, 0.42, 0.28, 1.0);
uniform vec4 packed_earth : source_color = vec4(0.66, 0.56, 0.42, 1.0);
uniform vec4 housing_tint : source_color = vec4(0.95, 0.85, 0.55, 1.0);

varying vec3 world;
varying vec3 world_normal;

// Bump mapping without tangents (Mikkelsen 2010): tilt the view-space normal by the screen-space
// slope of a height in metres.
vec3 bump(vec3 n, vec3 pos, float height) {
	vec3 dpdx = dFdx(pos);
	vec3 dpdy = dFdy(pos);
	vec3 r1 = cross(dpdy, n);
	vec3 r2 = cross(n, dpdx);
	float det = dot(dpdx, r1);
	vec3 grad = sign(det) * (dFdx(height) * r1 + dFdy(height) * r2);
	return normalize(abs(det) * n - grad);
}

void vertex() {
	world = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	// Tiles are only moved, never turned.
	world_normal = NORMAL;
}

void fragment() {
	vec2 p = world.xz;
	float h = world.y;
	float up = normalize(world_normal).y;
	vec2 uv = (p + world_half) / (2.0 * world_half);
	vec4 m = texture(masks, uv);
	// Roads from the full-size level only: a smaller level smears a 4 m road into a wide blob.
	m.r = textureLod(masks, uv, 0.0).r;

	// Noise from wide to fine. The fine layers fade out where a pixel covers more than they do,
	// so the ground doesn't shimmer in the distance.
	float wide = fbm3(p * 0.012);
	float mid = fbm3(p * 0.07);
	float fine = vnoise(p * 0.8);
	float grain = vnoise(p * 4.0);
	float metres_per_pixel = length(fwidth(p));
	float fine_fade = 1.0 - smoothstep(0.4, 1.5, metres_per_pixel);
	float mid_fade = 1.0 - smoothstep(6.0, 18.0, metres_per_pixel);
	float grain_fade = 1.0 - smoothstep(0.08, 0.3, metres_per_pixel);
	float speckle = mix(0.5, fine * 0.55 + mix(0.5, grain, grain_fade) * 0.45, fine_fade);
	float patch = mix(0.5, mid, mid_fade);

	// Grass: wide, soft drifts of lush and light, darker hollows, cooler higher up.
	vec3 grass = mix(grass_lush.rgb, grass_light.rgb, smoothstep(0.3, 0.75, wide));
	grass = mix(grass, grass_dark.rgb, smoothstep(0.45, 0.8, patch) * 0.35);
	grass = mix(grass, grass_high.rgb, smoothstep(25.0, 90.0, h + (wide - 0.5) * 20.0));
	grass *= 0.84 + 0.32 * speckle;

	// Rock: soft strata that run along the contour.
	float strata = vnoise(vec2(h * 0.35 + wide * 3.0, (p.x + p.y) * 0.01));
	vec3 rock = mix(rock_dark.rgb, rock_light.rgb, smoothstep(0.1, 0.9, strata) * 0.7 + speckle * 0.3);
	rock *= 0.9 + 0.2 * speckle;
	float slope = 1.0 - up;
	float rocky = smoothstep(0.10, 0.19, slope + (patch - 0.5) * 0.08);
	rocky = max(rocky, smoothstep(110.0, 170.0, h + (wide - 0.5) * 40.0) * smoothstep(0.02, 0.06, slope));

	// Low ground: a beach where the open sea is near, wet sand at the waterline and sand turning
	// to weedy seabed below it; reeds and mud on the low flats away from the sea, and round
	// inland water; grass elsewhere.
	float sea_near = smoothstep(0.01, 0.12, m.a + (patch - 0.5) * 0.04);
	float pool_near = smoothstep(0.02, 0.25, m.b + (patch - 0.5) * 0.1) * (1.0 - sea_near);
	float edge = 1.8 + (wide - 0.5) * 1.6 + (patch - 0.5) * 0.8;
	float low = 1.0 - smoothstep(edge - 0.05, edge + 0.05, h);
	vec3 sand = mix(sand_wet.rgb, sand_dry.rgb, smoothstep(-0.1, 0.35, h + (speckle - 0.5) * 0.1));
	sand = mix(sand, seabed.rgb, smoothstep(-1.5, -6.0, h + (patch - 0.5) * 2.0));
	sand *= 0.92 + 0.16 * speckle;
	vec3 marsh = mix(mud.rgb, reeds.rgb, smoothstep(0.35, 0.6, patch + h * 0.25));
	marsh = mix(marsh, grass_dark.rgb, smoothstep(0.55, 0.8, wide) * 0.5);
	marsh *= 0.9 + 0.2 * speckle;
	// Flats only well back from the sea, past the reach of its mask.
	float flats = (1.0 - smoothstep(1.0, 1.6, h + (patch - 0.5) * 0.6)) * (1.0 - smoothstep(0.0, 0.01, m.a));
	float boggy = max(pool_near * (1.0 - smoothstep(0.8, 2.2, h)), flats) * (1.0 - sea_near);
	// A beach also reaches about 20 m back from the waterline, with a crisp edge, however gently
	// the ground rises (the mask is about 0.5 at the waterline and falls 0.01 a metre inland).
	float beach = smoothstep(0.29, 0.31, m.a + (patch - 0.5) * 0.1 + (speckle - 0.5) * 0.02) * (1.0 - smoothstep(3.0, 4.0, h));
	float sandy = max(low * smoothstep(0.03, 0.05, m.a + (patch - 0.5) * 0.04), beach);
	vec3 ground = mix(grass, sand, sandy);
	ground = mix(ground, marsh, boggy);
	ground = mix(ground, rock, rocky);

	// Roads, towns and housing land, with ragged edges.
	// Roads are a single 4 m texel wide in the mask, so the threshold is low to keep them whole.
	float road = smoothstep(0.12, 0.4, m.r + (speckle - 0.5) * 0.12);
	ground = mix(ground, dirt.rgb * (0.88 + 0.24 * speckle), road * 0.9);
	float town = smoothstep(0.6, 0.85, m.g + (patch - 0.5) * 0.2);
	float housing = smoothstep(0.2, 0.35, m.g) * (1.0 - smoothstep(0.5, 0.6, m.g));
	// Towns: trodden grass, worn to earth in patches.
	vec3 yard = mix(grass * vec3(1.08, 1.04, 0.92), packed_earth.rgb, smoothstep(0.4, 0.75, patch) * 0.7);
	ground = mix(ground, yard * (0.92 + 0.16 * speckle), town);
	ground *= mix(vec3(1.0), housing_tint.rgb, housing * 0.2);

	ALBEDO = ground;
	float lumps = (fine * 0.12 + grain * 0.05 * grain_fade) * fine_fade * (1.0 + rocky * 2.0);
	NORMAL = bump(NORMAL, VERTEX, lumps);
	ROUGHNESS = mix(0.95, 0.55, (1.0 - smoothstep(-0.1, 0.6, h)) * max(sea_near, boggy));
	SPECULAR = 0.25;
}
