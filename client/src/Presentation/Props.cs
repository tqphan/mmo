using System;
using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// Procedural low-poly props for the style test: two kinds of tree and a rock, built from cones,
    /// prisms and lumpy icospheres with vertex colours (docs/design/art.md). Each mesh stands on its
    /// origin.
    /// </summary>
    public sealed class Props
    {
        private readonly bool faceted;
        private readonly Palette palette;
        private readonly RandomNumberGenerator rng = new() { Seed = 23 };
        private SurfaceTool st = null!;

        public Props(bool faceted, Palette palette)
        {
            this.faceted = faceted;
            this.palette = palette;
        }

        public ArrayMesh Pine() => Build(() =>
        {
            Prism(Vector3.Zero, 0.25f, 0.18f, 1.6f, 5, palette.Trunk);
            for (int k = 0; k < 3; k++)
                Cone(new Vector3(0, 1.0f + k * 1.4f, 0), 1.8f - k * 0.45f, 2.6f - k * 0.4f, 7, palette.Pine.Lightened(k * 0.06f));
        });

        public ArrayMesh Broadleaf(bool autumn) => Build(() =>
        {
            Prism(Vector3.Zero, 0.3f, 0.2f, 2.2f, 5, palette.Trunk);
            var leaves = autumn ? palette.LeavesAutumn : palette.Leaves;
            Blob(new Vector3(0, 3.2f, 0), new Vector3(1.9f, 1.6f, 1.9f), 0.25f, leaves);
            Blob(new Vector3(0.9f, 2.6f, 0.4f), new Vector3(1.1f, 1.0f, 1.1f), 0.25f, leaves.Darkened(0.08f));
        });

        public ArrayMesh Rock() => Build(() =>
            Blob(new Vector3(0, 0.3f, 0), new Vector3(1.4f, 0.9f, 1.1f), 0.35f, palette.Rock.Lightened(rng.Randf() * 0.15f)));

        private ArrayMesh Build(Action shapes)
        {
            st = new SurfaceTool();
            st.Begin(Mesh.PrimitiveType.Triangles);
            shapes();
            // Unindexed triangles get one normal per face; indexing first shares vertices, so the
            // normals come out smooth.
            if (!faceted)
                st.Index();
            st.GenerateNormals();
            return st.Commit();
        }

        /// <summary>A triangle, wound so its front faces away from <paramref name="inside"/>.</summary>
        private void Tri(Vector3 a, Vector3 b, Vector3 c, Vector3 inside, Color color)
        {
            // Godot's front faces are clockwise, so (c - a) × (b - a) is the outward normal.
            if ((c - a).Cross(b - a).Dot((a + b + c) / 3 - inside) < 0)
                (b, c) = (c, b);
            st.SetColor(color);
            st.AddVertex(a);
            st.AddVertex(b);
            st.AddVertex(c);
        }

        private static Vector3 Ring(float r, int k, int segments, float y, float turn = 0) =>
            new(r * Mathf.Cos(Mathf.Tau * (k + turn) / segments), y, r * Mathf.Sin(Mathf.Tau * (k + turn) / segments));

        private void Cone(Vector3 bottom, float radius, float height, int segments, Color color)
        {
            var apex = bottom + new Vector3(0, height, 0);
            var inside = bottom + new Vector3(0, height * 0.3f, 0);
            float turn = rng.Randf();
            for (int k = 0; k < segments; k++)
            {
                var a = bottom + Ring(radius, k, segments, 0, turn);
                var b = bottom + Ring(radius, k + 1, segments, 0, turn);
                Tri(a, b, apex, inside, color);
                Tri(a, b, bottom, inside + new Vector3(0, height, 0), color.Darkened(0.2f));
            }
        }

        private void Prism(Vector3 bottom, float r0, float r1, float height, int segments, Color color)
        {
            var inside = bottom + new Vector3(0, height / 2, 0);
            for (int k = 0; k < segments; k++)
            {
                var a = bottom + Ring(r0, k, segments, 0);
                var b = bottom + Ring(r0, k + 1, segments, 0);
                var c = bottom + Ring(r1, k + 1, segments, height);
                var d = bottom + Ring(r1, k, segments, height);
                Tri(a, b, c, inside, color);
                Tri(a, c, d, inside, color);
            }
        }

        /// <summary>An icosphere subdivided once, squashed to <paramref name="size"/> and pushed in and out by up to <paramref name="lumps"/>.</summary>
        private void Blob(Vector3 centre, Vector3 size, float lumps, Color color)
        {
            float t = (1 + Mathf.Sqrt(5)) / 2;
            Vector3[] v =
            {
                new(-1, t, 0), new(1, t, 0), new(-1, -t, 0), new(1, -t, 0),
                new(0, -1, t), new(0, 1, t), new(0, -1, -t), new(0, 1, -t),
                new(t, 0, -1), new(t, 0, 1), new(-t, 0, -1), new(-t, 0, 1),
            };
            int[] faces =
            {
                0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
                3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
            };
            // One displacement per direction, so shared edges stay closed.
            var pushed = new System.Collections.Generic.Dictionary<Vector3, Vector3>();
            Vector3 Point(Vector3 dir)
            {
                dir = dir.Normalized();
                var key = (dir * 1000).Round();
                if (!pushed.TryGetValue(key, out var p))
                {
                    p = centre + dir * (1 + lumps * (rng.Randf() - 0.5f)) * size;
                    pushed[key] = p;
                }
                return p;
            }
            for (int f = 0; f < faces.Length; f += 3)
            {
                Vector3 a = v[faces[f]], b = v[faces[f + 1]], c = v[faces[f + 2]];
                Vector3 ab = (a + b) / 2, bc = (b + c) / 2, ca = (c + a) / 2;
                foreach (var (p, q, r) in new[] { (a, ab, ca), (b, bc, ab), (c, ca, bc), (ab, bc, ca) })
                    Tri(Point(p), Point(q), Point(r), centre, color);
            }
        }
    }
}
