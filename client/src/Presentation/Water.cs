using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// The style test's sea at y = 0, drawn by <c>shaders/water.gdshader</c>: a finely subdivided
    /// square with waves, inside a flat ring that carries the sea out to the horizon.
    /// </summary>
    public static class Water
    {
        public static Node3D Create(bool faceted, Palette palette, float size = 1000.0f, int subdivisions = 250, float horizon = 12000.0f)
        {
            var root = new Node3D { Name = "Water" };
            var shader = GD.Load<Shader>("res://shaders/water.gdshader");
            ShaderMaterial Material(bool waves)
            {
                var m = new ShaderMaterial { Shader = shader };
                m.SetShaderParameter("faceted", faceted);
                m.SetShaderParameter("waves", waves);
                m.SetShaderParameter("fade", new Vector2(size * 0.3f, size * 0.5f));
                m.SetShaderParameter("shallow", palette.WaterShallow);
                m.SetShaderParameter("deep", palette.WaterDeep);
                return m;
            }
            root.AddChild(new MeshInstance3D
            {
                Name = "Near",
                Mesh = new PlaneMesh
                {
                    Size = new Vector2(size, size),
                    SubdivideWidth = subdivisions,
                    SubdivideDepth = subdivisions,
                    Material = Material(true),
                },
                CastShadow = GeometryInstance3D.ShadowCastingSetting.Off,
            });
            root.AddChild(new MeshInstance3D
            {
                Name = "Far",
                Mesh = Ring(size / 2, horizon / 2),
                MaterialOverride = Material(false),
                CastShadow = GeometryInstance3D.ShadowCastingSetting.Off,
            });
            return root;
        }

        /// <summary>A flat square ring between half-widths <paramref name="inner"/> and <paramref name="outer"/>.</summary>
        private static ArrayMesh Ring(float inner, float outer)
        {
            var st = new SurfaceTool();
            st.Begin(Mesh.PrimitiveType.Triangles);
            st.SetNormal(Vector3.Up);
            Vector3[] corners = { new(-1, 0, -1), new(1, 0, -1), new(1, 0, 1), new(-1, 0, 1) };
            for (int k = 0; k < 4; k++)
            {
                // A trapezoid per side, wound clockwise seen from above.
                Vector3 a = corners[k] * outer, b = corners[(k + 1) % 4] * outer;
                Vector3 c = corners[(k + 1) % 4] * inner, d = corners[k] * inner;
                foreach (var p in new[] { a, b, c, a, c, d })
                    st.AddVertex(p);
            }
            return st.Commit();
        }
    }
}
