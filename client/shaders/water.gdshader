// The sea (docs/design/art.md, PLAN §2 "Water"): four Gerstner waves plus scrolling ripples, the
// seabed seen through shallow water, deep colour further out, the sky reflected at grazing angles,
// and foam that laps the shore and catches the wave crests. Waves and foam belong to the open sea
// (the A channel of the ground masks, GroundMasks.cs): waves die down over the last few tens of
// metres to the shore, and inland pools at sea level lie calm. Past
// the waves a flat ring of the same material reaches the horizon. Gameplay ignores waves;
// cosmetic buoyancy will sample the same sum (Water.cs).
shader_type spatial;
render_mode blend_mix, cull_back, depth_draw_always, diffuse_lambert, specular_schlick_ggx;

#include "res://shaders/noise.gdshaderinc"

uniform vec4 shallow : source_color = vec4(0.16, 0.72, 0.70, 1.0);
uniform vec4 deep : source_color = vec4(0.04, 0.22, 0.42, 1.0);
uniform vec4 sky_tint : source_color = vec4(0.44, 0.61, 0.79, 1.0);
uniform vec4 foam : source_color = vec4(0.95, 0.98, 1.0, 1.0);
// Each wave: direction (x, z), steepness, wavelength in metres. Kept in step with Water.cs.
uniform vec4 wave_a = vec4(1.0, 0.3, 0.10, 31.0);
uniform vec4 wave_b = vec4(-0.4, 1.0, 0.09, 17.0);
uniform vec4 wave_c = vec4(0.7, -0.8, 0.08, 11.0);
uniform vec4 wave_d = vec4(0.2, 0.9, 0.06, 6.5);
// Off for the flat ring out to the horizon (Water.cs).
uniform bool waves = true;
// Waves die down between these distances from the centre of the wavy square (Water.cs sets them
// from the mesh size, and moves the centre when the sea follows the camera).
uniform vec2 fade = vec2(300.0, 500.0);
uniform vec2 centre = vec2(0.0);
// Faceted: one normal per triangle, so the waves read as low-poly panels (the style test).
uniform bool faceted = false;
// The ground masks; without them everything counts as open sea.
uniform sampler2D masks : filter_linear, repeat_disable;
uniform bool has_masks = false;
uniform float world_half = 8000.0;
uniform sampler2D depth_tex : hint_depth_texture, filter_nearest;
uniform sampler2D screen_tex : hint_screen_texture, filter_linear_mipmap;

varying vec3 world;
varying vec3 world_normal;
varying float crest;
varying float open_sea;
varying float swell;

vec3 gerstner(vec4 wave, vec2 p, float t, inout vec3 tangent, inout vec3 binormal) {
	float k = TAU / wave.w;
	float c = sqrt(9.8 / k);
	vec2 d = normalize(wave.xy);
	float f = k * (dot(d, p) - c * t);
	float a = wave.z / k;
	tangent += vec3(-d.x * d.x * wave.z * sin(f), d.x * wave.z * cos(f), -d.x * d.y * wave.z * sin(f));
	binormal += vec3(-d.x * d.y * wave.z * sin(f), d.y * wave.z * cos(f), -d.y * d.y * wave.z * sin(f));
	return vec3(d.x * a * cos(f), a * sin(f), d.y * a * cos(f));
}

float sea_at(vec2 p) {
	if (!has_masks) {
		return 1.0;
	}
	return textureLod(masks, (p + world_half) / (2.0 * world_half), 0.0).a;
}

void vertex() {
	vec3 p = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world = p;
	world_normal = vec3(0.0, 1.0, 0.0);
	crest = 0.0;
	float sea = sea_at(p.xz);
	open_sea = smoothstep(0.15, 0.45, sea);
	swell = smoothstep(0.62, 0.95, sea);
	if (waves) {
		vec3 tangent = vec3(1.0, 0.0, 0.0);
		vec3 binormal = vec3(0.0, 0.0, 1.0);
		vec3 offset = gerstner(wave_a, p.xz, TIME, tangent, binormal)
			+ gerstner(wave_b, p.xz, TIME, tangent, binormal)
			+ gerstner(wave_c, p.xz, TIME, tangent, binormal)
			+ gerstner(wave_d, p.xz, TIME, tangent, binormal);
		// Calm towards the edge of the wavy square, to meet the flat ring without a seam, and
		// away from the open sea.
		float calm = (1.0 - smoothstep(fade.x, fade.y, max(abs(p.x - centre.x), abs(p.z - centre.y)))) * swell;
		offset *= calm;
		VERTEX += offset;
		world_normal = normalize(mix(vec3(0.0, 1.0, 0.0), cross(binormal, tangent), calm));
		crest = offset.y;
		world = p + offset;
	}
}

// Ripples: the slope of two scrolling noise layers, as an offset to the normal's x and z.
vec2 ripples(vec2 p, float t) {
	vec2 a = p * 0.25 + vec2(t * 0.06, t * 0.04);
	vec2 b = p * 0.7 + vec2(-t * 0.05, t * 0.1);
	const float e = 0.15;
	float na = fbm3(a), nb = vnoise(b);
	vec2 ga = vec2(fbm3(a + vec2(e, 0.0)) - na, fbm3(a + vec2(0.0, e)) - na) / e;
	vec2 gb = vec2(vnoise(b + vec2(e, 0.0)) - nb, vnoise(b + vec2(0.0, e)) - nb) / e;
	return ga * 0.07 + gb * 0.04;
}

void fragment() {
	float dist = length(VERTEX);
	vec3 n = world_normal;
	// The sky's reflection is worked out from a calmer normal: full ripples flare in rings
	// where they tip away from a low camera.
	vec3 calm_n = world_normal;
	if (faceted) {
		n = (INV_VIEW_MATRIX * vec4(normalize(cross(dFdy(VERTEX), dFdx(VERTEX))), 0.0)).xyz;
	} else {
		// Ripples fade out with distance, where they would only shimmer.
		vec2 r = ripples(world.xz, TIME) * (1.0 - smoothstep(60.0, 400.0, dist));
		n = normalize(n + vec3(-r.x, 0.0, -r.y));
		calm_n = normalize(world_normal + vec3(-r.x, 0.0, -r.y) * 0.3);
	}
	NORMAL = normalize((VIEW_MATRIX * vec4(n, 0.0)).xyz);

	// How much water lies behind this pixel, from the opaque depth buffer: along the view ray,
	// and straight down.
	float raw = texture(depth_tex, SCREEN_UV).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, raw, 1.0);
	behind.xyz /= behind.w;
	float thickness = max(VERTEX.z - behind.z, 0.0);
	float below = max(world.y - (INV_VIEW_MATRIX * vec4(behind.xyz, 1.0)).y, 0.0);

	// The seabed, bent by the surface, seen through clear shallow water. Where the bent sample
	// lands on something in front of the water, use the straight one.
	vec2 bend = n.xz * 0.04 * smoothstep(0.0, 2.0, thickness);
	vec2 uv = SCREEN_UV + bend;
	float raw_bent = texture(depth_tex, uv).r;
	vec4 behind_bent = INV_PROJECTION_MATRIX * vec4(uv * 2.0 - 1.0, raw_bent, 1.0);
	behind_bent.xyz /= behind_bent.w;
	if (behind_bent.z > VERTEX.z) {
		uv = SCREEN_UV;
	}
	vec3 bed = textureLod(screen_tex, uv, 0.0).rgb;
	// Red goes first, then green, so the bed tints turquoise and then fades into the deep colour.
	vec3 through = bed * exp(-thickness * vec3(0.55, 0.12, 0.08));
	float clear = exp(-thickness * 0.22);
	vec3 body = mix(deep.rgb, shallow.rgb, exp(-below * 0.18) * 0.85);

	// Sky at grazing angles (Schlick, water's 0.02 head-on).
	float facing = clamp(dot(normalize((VIEW_MATRIX * vec4(calm_n, 0.0)).xyz), VIEW), 0.0, 1.0);
	float fresnel = 0.02 + 0.98 * pow(1.0 - facing, 5.0);
	fresnel = min(fresnel, 0.5) * smoothstep(0.0, 0.1, below);

	// Foam: bands that lap in over the shallows, broken up by noise, and a little on the crests.
	// Distance out from the waterline, roughly in metres: the open-sea mask climbs from 0.5 at the
	// coast to 1 about 50 m out (two 13-texel box blurs of 4 m texels, GroundMasks.cs). Beaches
	// are too flat to measure it by depth.
	float out_m = max(sea_at(world.xz) - 0.5, 0.0) * 104.0;
	float wobble = vnoise(world.xz * 0.2 + TIME * 0.1) * 3.0;
	float lap = sin((out_m + wobble) * 1.1 - TIME * 1.3) * 0.5 + 0.5;
	float shore = (1.0 - smoothstep(3.0, 12.0, out_m + wobble)) * smoothstep(0.55, 0.9, lap + (1.0 - smoothstep(0.0, 3.0, out_m)) * 0.5);
	// Not on the thinnest film of water, whose edge follows the ground's triangles.
	shore *= smoothstep(0.0, 0.04, below);
	// Streaks along the crests, drifting with the wind.
	float streak = fbm3(world.xz * vec2(0.35, 0.9) + vec2(TIME * 0.25, -TIME * 0.1));
	float caps = smoothstep(0.6, 0.95, crest / 0.9 + (streak - 0.5) * 0.6) * 0.55;
	float froth = clamp(shore * smoothstep(0.25, 0.6, streak + 0.2) * 0.85 + caps * swell, 0.0, 1.0) * open_sea;

	// The bed comes through as light already lit; the water body and the foam are lit by the sun.
	ALBEDO = mix(body * (1.0 - clear) * (1.0 - fresnel), foam.rgb, froth);
	EMISSION = (through * clear * (1.0 - fresnel) + sky_tint.rgb * fresnel) * (1.0 - froth);
	ROUGHNESS = mix(0.06, 0.8, froth);
	// The engine's own sky reflection fades far out, where the flat sea would mirror the pale
	// horizon and wash out; the fresnel term above stands in for it.
	SPECULAR = mix(0.35, 0.2, froth) * mix(1.0, 0.3, smoothstep(300.0, 3000.0, dist));
	ALPHA = 1.0;
}
