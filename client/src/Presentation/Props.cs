using System;
using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// Procedural low-poly props, built from cones, tubes, blades and lumpy icospheres with vertex
    /// colours (docs/design/art.md): the style test's trees and rock, and the world's flora
    /// (<see cref="Kind"/>). Each mesh stands on its origin. The seed picks a variant.
    /// </summary>
    public sealed class Props
    {
        private readonly bool faceted;
        private readonly Palette palette;
        private readonly RandomNumberGenerator rng;
        private SurfaceTool st = null!;

        public Props(bool faceted, Palette palette, ulong seed = 23)
        {
            this.faceted = faceted;
            this.palette = palette;
            rng = new RandomNumberGenerator { Seed = seed };
        }

        /// <summary>A mesh for each kind of world flora, in its <paramref name="variant"/>.</summary>
        public ArrayMesh Kind(World.Flora kind, int variant) => kind switch
        {
            World.Flora.Pine => Pine(3 + variant),
            World.Flora.Oak => Oak(palette.Leaves.Lerp(palette.Pine, variant * 0.2f)),
            World.Flora.Birch => Birch(),
            World.Flora.Autumn => Oak(new[] { palette.LeavesAutumn, new Color(0.82f, 0.36f, 0.16f), new Color(0.92f, 0.78f, 0.25f) }[variant % 3]),
            World.Flora.DeadTree => DeadTree(),
            World.Flora.Bush => Bush(variant == 2),
            World.Flora.Boulder => Build(() => Blob(new Vector3(0, 0.7f, 0), new Vector3(1.5f, 1.1f, 1.3f), 0.45f, palette.Rock.Lerp(palette.RockLight, rng.Randf() * 0.5f))),
            World.Flora.Rock => Build(() => Blob(new Vector3(0, 0.2f, 0), new Vector3(0.7f, 0.4f, 0.55f), 0.4f, palette.Rock.Lerp(palette.RockLight, rng.Randf()))),
            World.Flora.Driftwood => Build(() =>
            {
                var c = new Color(0.62f, 0.56f, 0.48f);
                Tube(new Vector3(-1.6f, 0.15f, 0), new Vector3(1.5f, 0.2f, 0.2f), 0.18f, 0.12f, 5, c);
                Tube(new Vector3(0.4f, 0.18f, 0.05f), new Vector3(1.0f, 0.4f, 0.7f), 0.07f, 0.04f, 4, c.Darkened(0.1f));
            }),
            World.Flora.Grass => Tuft(9, 0.45f, palette.GrassBlade.Lerp(palette.GrassLow, variant * 0.3f)),
            World.Flora.Flowers => Flowers(new[] { new Color(0.95f, 0.85f, 0.25f), new Color(0.6f, 0.45f, 0.9f), new Color(0.97f, 0.95f, 0.92f) }[variant % 3]),
            World.Flora.Reeds => Reeds(),
            World.Flora.Pebbles => Build(() =>
            {
                for (int k = 0; k < 4; k++)
                    Blob(new Vector3(rng.RandfRange(-0.4f, 0.4f), 0.03f, rng.RandfRange(-0.4f, 0.4f)), new Vector3(0.12f, 0.07f, 0.1f) * rng.RandfRange(0.6f, 1.4f), 0.3f, palette.Rock.Lerp(palette.RockLight, rng.Randf()));
            }),
            _ => Rock(),
        };

        public ArrayMesh Pine(int tiers = 3) => Build(() =>
        {
            float step = 4.2f / tiers;
            Prism(Vector3.Zero, 0.25f, 0.18f, 1.6f, 5, palette.Trunk);
            for (int k = 0; k < tiers; k++)
            {
                float t = k / (float)tiers;
                Cone(new Vector3(0, 1.0f + k * step, 0), (1.9f - t * 1.3f) * rng.RandfRange(0.9f, 1.1f), 2.6f - t * 1.2f, 7, palette.Pine.Lightened(k * 0.05f));
            }
        });

        private ArrayMesh Oak(Color leaves) => Build(() =>
        {
            Prism(Vector3.Zero, 0.35f, 0.24f, 2.4f, 6, palette.Trunk);
            Tube(new Vector3(0, 1.8f, 0), new Vector3(1.1f, 2.9f, 0.3f), 0.14f, 0.09f, 5, palette.Trunk);
            Blob(new Vector3(0, 3.6f, 0), new Vector3(2.1f, 1.7f, 2.0f), 0.3f, leaves);
            for (int k = 0; k < 3; k++)
            {
                float a = rng.Randf() * Mathf.Tau;
                Blob(new Vector3(Mathf.Cos(a) * 1.3f, 3.0f + rng.Randf() * 0.9f, Mathf.Sin(a) * 1.3f), new Vector3(1.2f, 1.0f, 1.2f) * rng.RandfRange(0.8f, 1.15f), 0.3f, leaves.Darkened(rng.Randf() * 0.12f).Lightened(rng.Randf() * 0.06f));
            }
        });

        private ArrayMesh Birch() => Build(() =>
        {
            var bark = new Color(0.88f, 0.86f, 0.80f);
            Prism(Vector3.Zero, 0.16f, 0.1f, 4.6f, 5, bark);
            Tube(new Vector3(0, 2.6f, 0), new Vector3(-0.7f, 3.6f, 0.2f), 0.06f, 0.04f, 4, bark.Darkened(0.1f));
            var leaves = new Color(0.55f, 0.74f, 0.30f);
            Blob(new Vector3(0, 4.4f, 0), new Vector3(1.1f, 1.7f, 1.1f), 0.3f, leaves);
            Blob(new Vector3(-0.5f, 3.4f, 0.3f), new Vector3(0.8f, 1.0f, 0.8f), 0.3f, leaves.Darkened(0.08f));
        });

        private ArrayMesh DeadTree() => Build(() =>
        {
            var wood = new Color(0.45f, 0.40f, 0.34f);
            Prism(Vector3.Zero, 0.28f, 0.12f, 3.6f, 5, wood);
            for (int k = 0; k < 3; k++)
            {
                float a = rng.Randf() * Mathf.Tau, y = 1.6f + k * 0.7f;
                Tube(new Vector3(0, y, 0), new Vector3(Mathf.Cos(a) * 1.2f, y + 0.9f, Mathf.Sin(a) * 1.2f), 0.08f, 0.03f, 4, wood);
            }
        });

        private ArrayMesh Bush(bool berries) => Build(() =>
        {
            var leaves = palette.Leaves.Darkened(0.15f);
            for (int k = 0; k < 3; k++)
                Blob(new Vector3(rng.RandfRange(-0.5f, 0.5f), 0.45f + rng.Randf() * 0.2f, rng.RandfRange(-0.5f, 0.5f)), new Vector3(0.7f, 0.55f, 0.7f) * rng.RandfRange(0.8f, 1.2f), 0.35f, leaves.Lightened(rng.Randf() * 0.1f));
            if (berries)
                for (int k = 0; k < 6; k++)
                {
                    float a = rng.Randf() * Mathf.Tau;
                    Blob(new Vector3(Mathf.Cos(a) * 0.75f, 0.5f + rng.Randf() * 0.4f, Mathf.Sin(a) * 0.75f), new Vector3(0.08f, 0.08f, 0.08f), 0.1f, new Color(0.75f, 0.12f, 0.2f));
                }
        });

        /// <summary>A tuft of <paramref name="blades"/> thin blades, dark at the root and light at the tip.</summary>
        private ArrayMesh Tuft(int blades, float height, Color color) => Build(() =>
        {
            for (int k = 0; k < blades; k++)
                Blade(rng.Randf() * Mathf.Tau, rng.RandfRange(0.0f, 0.18f), height * rng.RandfRange(0.6f, 1.2f), 0.05f, color);
        });

        private ArrayMesh Flowers(Color petals) => Build(() =>
        {
            for (int k = 0; k < 5; k++)
                Blade(rng.Randf() * Mathf.Tau, rng.RandfRange(0.0f, 0.15f), 0.35f, 0.04f, palette.GrassBlade);
            for (int k = 0; k < 5; k++)
            {
                float a = rng.Randf() * Mathf.Tau, r = rng.RandfRange(0.05f, 0.3f);
                var at = new Vector3(Mathf.Cos(a) * r, rng.RandfRange(0.25f, 0.4f), Mathf.Sin(a) * r);
                Tube(at - new Vector3(0, 0.3f, 0), at, 0.012f, 0.012f, 3, palette.GrassBlade);
                Blob(at, new Vector3(0.06f, 0.035f, 0.06f), 0.2f, petals);
            }
        });

        private ArrayMesh Reeds() => Build(() =>
        {
            var stalk = new Color(0.52f, 0.58f, 0.30f);
            for (int k = 0; k < 9; k++)
                Blade(rng.Randf() * Mathf.Tau, rng.RandfRange(0.0f, 0.25f), rng.RandfRange(0.9f, 1.5f), 0.035f, stalk);
            for (int k = 0; k < 3; k++)
            {
                float a = rng.Randf() * Mathf.Tau, r = rng.RandfRange(0.0f, 0.2f), h = rng.RandfRange(1.1f, 1.5f);
                var foot = new Vector3(Mathf.Cos(a) * r, 0, Mathf.Sin(a) * r);
                Tube(foot, foot + new Vector3(0, h, 0), 0.015f, 0.012f, 3, stalk);
                Tube(foot + new Vector3(0, h - 0.05f, 0), foot + new Vector3(0, h + 0.2f, 0), 0.04f, 0.035f, 5, new Color(0.40f, 0.26f, 0.15f));
            }
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
            // A vertex whose triangles cancel out (a hair-thin stem, a blade's root) gets no normal,
            // which lights as NaN and flares in the glow: point it up instead.
            var arrays = st.CommitToArrays();
            var normals = arrays[(int)Mesh.ArrayType.Normal].AsVector3Array();
            for (int i = 0; i < normals.Length; i++)
                if (!(normals[i].LengthSquared() > 0.25f))
                    normals[i] = Vector3.Up;
            arrays[(int)Mesh.ArrayType.Normal] = normals;
            var mesh = new ArrayMesh();
            mesh.AddSurfaceFromArrays(Mesh.PrimitiveType.Triangles, arrays);
            return mesh;
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

        /// <summary>A blade leaning out from the root at angle <paramref name="a"/>: a thin triangle pair, dark below and light at the tip.</summary>
        private void Blade(float a, float lean, float height, float width, Color color)
        {
            var dir = new Vector3(Mathf.Cos(a), 0, Mathf.Sin(a));
            var side = new Vector3(-dir.Z, 0, dir.X) * width;
            var root = dir * lean * 0.3f;
            var tip = root + dir * lean + new Vector3(0, height, 0);
            var mid = (root + tip) / 2;
            Color low = color.Darkened(0.35f), high = color.Lightened(0.15f);
            void V(Vector3 p, Color c)
            {
                st.SetColor(c);
                st.SetNormal(Vector3.Up);
                st.AddVertex(p);
            }
            V(root - side, low);
            V(mid + side * 0.7f, color);
            V(root + side, low);
            V(root - side, low);
            V(mid - side * 0.7f, color);
            V(mid + side * 0.7f, color);
            V(mid - side * 0.7f, color);
            V(tip, high);
            V(mid + side * 0.7f, color);
        }

        /// <summary>A tapered tube from <paramref name="from"/> to <paramref name="to"/>.</summary>
        private void Tube(Vector3 from, Vector3 to, float r0, float r1, int segments, Color color)
        {
            var axis = (to - from).Normalized();
            var u = Mathf.Abs(axis.Y) < 0.9f ? axis.Cross(Vector3.Up).Normalized() : axis.Cross(Vector3.Right).Normalized();
            var w = axis.Cross(u);
            var inside = (from + to) / 2;
            Vector3 P(Vector3 c, float r, int k) => c + (u * Mathf.Cos(Mathf.Tau * k / segments) + w * Mathf.Sin(Mathf.Tau * k / segments)) * r;
            for (int k = 0; k < segments; k++)
            {
                Vector3 a = P(from, r0, k), b = P(from, r0, k + 1), c = P(to, r1, k + 1), d = P(to, r1, k);
                Tri(a, b, c, inside, color);
                Tri(a, c, d, inside, color);
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
