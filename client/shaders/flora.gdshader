// The world's flora and stones (docs/design/art.md): vertex colours times each instance's tint
// (MultiMesh colours), swaying in the wind by height. Ground cover is lit as if it faced straight
// up, like the ground it grows on, and shows both faces of its blades.
shader_type spatial;
render_mode cull_disabled, diffuse_lambert_wrap, specular_disabled;

// Metres of sway per metre of height squared; 0 for stones.
uniform float sway = 0.0;
uniform bool ground_cover = false;

void vertex() {
	vec3 origin = MODEL_MATRIX[3].xyz;
	float h = max(VERTEX.y, 0.0);
	float phase = origin.x * 0.21 + origin.z * 0.17;
	float gust = sin(TIME * 1.3 + phase) + 0.5 * sin(TIME * 2.9 + phase * 1.7);
	VERTEX.x += gust * sway * h * h;
	VERTEX.z += 0.6 * cos(TIME * 1.1 + phase) * sway * h * h;
}

void fragment() {
	// Vertex colours are sRGB (Props.cs); the instance tint is already folded in.
	ALBEDO = pow(clamp(COLOR.rgb, vec3(0.0), vec3(1.0)), vec3(2.2));
	ROUGHNESS = 0.9;
	if (ground_cover) {
		NORMAL = normalize((VIEW_MATRIX * vec4(0.0, 1.0, 0.0, 0.0)).xyz);
	} else if (!FRONT_FACING) {
		NORMAL = -NORMAL;
	}
}
