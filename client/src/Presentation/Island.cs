using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// A procedural island for the art style test (docs/design/art.md): hills, a ridge, beaches and a
    /// sea floor, sampled on the server's 2 m terrain grid. Heights are metres above sea level (y = 0).
    /// </summary>
    public sealed class Island
    {
        /// <summary>The island's mesh spans [-HalfSize, HalfSize] on x and z.</summary>
        public const float HalfSize = 180.0f;
        public const float Step = 2.0f;

        private readonly FastNoiseLite shape = new()
        {
            Seed = 7,
            NoiseType = FastNoiseLite.NoiseTypeEnum.Simplex,
            Frequency = 0.006f,
            FractalType = FastNoiseLite.FractalTypeEnum.Fbm,
            FractalOctaves = 3,
        };
        private readonly FastNoiseLite hills = new()
        {
            Seed = 11,
            NoiseType = FastNoiseLite.NoiseTypeEnum.Simplex,
            Frequency = 0.012f,
            FractalType = FastNoiseLite.FractalTypeEnum.Ridged,
            FractalOctaves = 4,
        };

        private readonly FastNoiseLite tint = new() { Seed = 13, Frequency = 0.08f };

        /// <summary>A little light and dark in the ground colour, so wide areas aren't one flat tone.</summary>
        private Color Tinted(Color c, Vector3 p)
        {
            float t = tint.GetNoise2D(p.X, p.Z) * 0.07f;
            return t > 0 ? c.Lightened(t) : c.Darkened(-t);
        }

        public float Height(float x, float z)
        {
            // Roughly round, with a wobbly coast; 0 at the centre, 1 at the edge of the land.
            float d = new Vector2(x, z).Length() / 125.0f + 0.35f * shape.GetNoise2D(x, z);
            float land = Mathf.SmoothStep(1.05f, 0.45f, d);
            // Gentle near the coast, hills and ridges inland.
            float relief = 2.0f + 18.0f * Mathf.Max(0.0f, hills.GetNoise2D(x, z) * 0.5f + 0.5f) * land * land * land;
            return Mathf.Lerp(-7.0f, relief, land);
        }

        public Vector3 Normal(float x, float z)
        {
            const float e = 0.5f;
            return new Vector3(Height(x - e, z) - Height(x + e, z), 2 * e, Height(x, z - e) - Height(x, z + e)).Normalized();
        }

        /// <summary>
        /// The terrain mesh. <paramref name="faceted"/> gives every triangle its own normal and one
        /// colour band; otherwise normals are smooth and colours blend between vertices.
        /// </summary>
        public ArrayMesh Mesh(bool faceted, Palette palette)
        {
            int n = (int)(2 * HalfSize / Step);
            var st = new SurfaceTool();
            st.Begin(Godot.Mesh.PrimitiveType.Triangles);
            Vector3 P(int i, int j)
            {
                float x = -HalfSize + i * Step, z = -HalfSize + j * Step;
                return new Vector3(x, Height(x, z), z);
            }
            void Vertex(Vector3 p, Color c, Vector3 normal)
            {
                st.SetColor(c);
                st.SetNormal(normal);
                st.AddVertex(p);
            }
            void Triangle(Vector3 a, Vector3 b, Vector3 c)
            {
                if (faceted)
                {
                    var normal = (c - a).Cross(b - a).Normalized();
                    var centre = (a + b + c) / 3;
                    var color = Tinted(palette.Ground(centre.Y, normal.Y, banded: true), centre);
                    Vertex(a, color, normal);
                    Vertex(b, color, normal);
                    Vertex(c, color, normal);
                }
                else
                {
                    foreach (var p in new[] { a, b, c })
                    {
                        var normal = Normal(p.X, p.Z);
                        Vertex(p, Tinted(palette.Ground(p.Y, normal.Y, banded: false), p), normal);
                    }
                }
            }
            for (int i = 0; i < n; i++)
            {
                for (int j = 0; j < n; j++)
                {
                    // Alternate the diagonal so the facets don't line up in stripes.
                    Vector3 a = P(i, j), b = P(i + 1, j), c = P(i + 1, j + 1), d = P(i, j + 1);
                    if (((i + j) & 1) == 0)
                    {
                        Triangle(a, b, c);
                        Triangle(a, c, d);
                    }
                    else
                    {
                        Triangle(a, b, d);
                        Triangle(b, c, d);
                    }
                }
            }
            return st.Commit();
        }
    }
}
