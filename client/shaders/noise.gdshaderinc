// Value noise shared by the terrain and water shaders (docs/design/art.md). Cheap and smooth;
// all inputs are world metres times a frequency.

// An integer hash of a lattice point, so it stays good thousands of metres from the origin
// (a fract-based hash loses its low bits out there).
float hash2(vec2 p) {
	uvec2 q = uvec2(ivec2(p) + 65536);
	uint h = q.x * 1597334677u ^ q.y * 3812015801u;
	h = (h ^ (h >> 16u)) * 2246822519u;
	h ^= h >> 13u;
	return float(h & 0xffffffu) / 16777215.0;
}

// 0..1, smooth between integer lattice points.
float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	float a = hash2(i);
	float b = hash2(i + vec2(1.0, 0.0));
	float c = hash2(i + vec2(0.0, 1.0));
	float d = hash2(i + vec2(1.0, 1.0));
	return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Three octaves, 0..1.
float fbm3(vec2 p) {
	float v = 0.5 * vnoise(p);
	p = mat2(vec2(0.8, -0.6), vec2(0.6, 0.8)) * p * 2.03;
	v += 0.3 * vnoise(p);
	p = mat2(vec2(0.8, -0.6), vec2(0.6, 0.8)) * p * 2.01;
	v += 0.2 * vnoise(p);
	return v;
}
