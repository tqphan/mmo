// Stylized sea for the art style test (docs/design/art.md, PLAN §2 "Water"): three Gerstner waves,
// colour from the depth of water behind the surface, and a foam line at the shore. Past the waves
// a flat ring of the same material reaches the horizon. Gameplay ignores
// waves; cosmetic buoyancy will sample the same sum (Water.cs).
shader_type spatial;
render_mode blend_mix, cull_back, depth_draw_always, specular_schlick_ggx;

uniform vec4 shallow : source_color = vec4(0.25, 0.85, 0.80, 1.0);
uniform vec4 deep : source_color = vec4(0.07, 0.35, 0.60, 1.0);
uniform vec4 foam : source_color = vec4(0.96, 0.98, 1.0, 1.0);
// Each wave: direction (x, z), steepness, wavelength in metres. Kept in step with Water.cs.
uniform vec4 wave_a = vec4(1.0, 0.3, 0.12, 22.0);
uniform vec4 wave_b = vec4(-0.4, 1.0, 0.10, 13.0);
uniform vec4 wave_c = vec4(0.7, -0.8, 0.08, 9.0);
// Off for the flat ring out to the horizon (Water.cs).
uniform bool waves = true;
// Waves die down between these distances from the centre of the wavy square (Water.cs sets them
// from the mesh size, and moves the centre when the sea follows the camera).
uniform vec2 fade = vec2(300.0, 500.0);
uniform vec2 centre = vec2(0.0);
// Faceted: one normal per triangle, so the waves read as low-poly panels.
uniform bool faceted = true;
uniform sampler2D depth_tex : hint_depth_texture, filter_nearest;

varying vec3 world;

vec3 gerstner(vec4 wave, vec3 p, float t, inout vec3 tangent, inout vec3 binormal) {
	float k = TAU / wave.w;
	float c = sqrt(9.8 / k);
	vec2 d = normalize(wave.xy);
	float f = k * (dot(d, p.xz) - c * t);
	float a = wave.z / k;
	tangent += vec3(-d.x * d.x * wave.z * sin(f), d.x * wave.z * cos(f), -d.x * d.y * wave.z * sin(f));
	binormal += vec3(-d.x * d.y * wave.z * sin(f), d.y * wave.z * cos(f), -d.y * d.y * wave.z * sin(f));
	return vec3(d.x * a * cos(f), a * sin(f), d.y * a * cos(f));
}

void vertex() {
	vec3 p = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world = p;
	if (waves) {
		vec3 tangent = vec3(1.0, 0.0, 0.0);
		vec3 binormal = vec3(0.0, 0.0, 1.0);
		vec3 offset = gerstner(wave_a, p, TIME, tangent, binormal)
			+ gerstner(wave_b, p, TIME, tangent, binormal)
			+ gerstner(wave_c, p, TIME, tangent, binormal);
		// Calm towards the edge of the wavy square, to meet the flat ring without a seam.
		float calm = 1.0 - smoothstep(fade.x, fade.y, max(abs(p.x - centre.x), abs(p.z - centre.y)));
		offset *= calm;
		VERTEX += offset;
		NORMAL = normalize(mix(vec3(0.0, 1.0, 0.0), cross(binormal, tangent), calm));
		world = p + offset;
	}
}

void fragment() {
	if (faceted) {
		// View space, with screen y growing downwards.
		NORMAL = normalize(cross(dFdy(VERTEX), dFdx(VERTEX)));
	}
	// How much water lies behind this pixel, from the opaque depth buffer.
	float raw = texture(depth_tex, SCREEN_UV).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, raw, 1.0);
	behind.xyz /= behind.w;
	float depth = max(VERTEX.z - behind.z, 0.0);

	vec3 color = mix(shallow.rgb, deep.rgb, smoothstep(0.0, 7.0, depth));
	float shore = 0.7 + 0.25 * sin(world.x * 0.35 + TIME * 1.3) * sin(world.z * 0.31 - TIME);
	float edge = 1.0 - step(shore, depth);
	ALBEDO = mix(color, foam.rgb, edge);
	// Opaque once deep, so whether terrain is drawn under deep water never shows.
	ALPHA = mix(0.75, 1.0, smoothstep(0.0, 6.0, depth));
	ALPHA = max(ALPHA, edge);
	ROUGHNESS = 0.2;
	SPECULAR = 0.4;
}
