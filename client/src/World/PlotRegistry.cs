using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.Sim;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Plots in the Static tier (docs/design/land.md): one view per <c>plot</c> row, an outline on
    /// the ground with a post and a sign. A plot whose demolition time has passed is hidden even
    /// before its row goes, as the server treats it as gone.
    /// </summary>
    public partial class PlotRegistry : Node3D
    {
        private readonly Session session;
        private readonly HeightMap map;
        private readonly Dictionary<ulong, PlotView> views = new();

        public PlotRegistry(Session session, HeightMap map)
        {
            this.session = session;
            this.map = map;
            Name = "Plots";
        }

        public int Count => views.Count;

        /// <summary>Plots still standing (paid or overdue) by game time.</summary>
        public IEnumerable<Plot> Standing
        {
            get
            {
                long now = session.GameNowMicros;
                return views.Values.Select(v => v.Row).Where(p => Land.StandingOf(p.TaxPaidUntil.MicrosecondsSinceUnixEpoch, now) != Sim.Standing.Demolished);
            }
        }

        public Plot? Find(ulong id) => views.TryGetValue(id, out var v) ? v.Row : null;

        /// <summary>Whether one of our characters owns <paramref name="p"/>.</summary>
        public bool Mine(Plot p) => session.Conn?.Db.MyCharacters.Iter().Any(c => c.CharId == p.Owner) == true;

        public string OwnerName(Plot p) => session.Conn?.Db.CharacterPublic.CharId.Find(p.Owner)?.Name ?? $"#{p.Owner}";

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Plot.OnInsert += (_, p) => Receive(p);
            db.Plot.OnUpdate += (_, _, p) => Receive(p);
            db.Plot.OnDelete += (_, p) =>
            {
                if (views.Remove(p.Id, out var v))
                    v.QueueFree();
            };
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            foreach (var v in views.Values)
                v.Refresh(now, OwnerName(v.Row), Mine(v.Row));
        }

        private void Receive(Plot p)
        {
            if (!views.TryGetValue(p.Id, out var view))
            {
                if (GameContent.Plot(p.Kind) is not PlotData kind)
                    return;
                view = new PlotView(kind, map);
                views[p.Id] = view;
                AddChild(view);
            }
            view.Set(p);
        }
    }

    /// <summary>One plot: a ribbon round its edge over the ground, a corner post and a sign.</summary>
    public partial class PlotView : Node3D
    {
        private static readonly Color OursColor = new(0.45f, 0.9f, 0.5f);
        private static readonly Color TheirsColor = new(0.95f, 0.92f, 0.8f);
        private static readonly Color OverdueColor = new(1.0f, 0.35f, 0.3f);

        private readonly PlotData kind;
        private readonly HeightMap map;
        private readonly StandardMaterial3D material = new() { ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded, AlbedoColor = TheirsColor };
        private readonly MeshInstance3D ribbon = new() { Name = "Edge" };
        private readonly Node3D post = new() { Name = "Post" };
        private readonly Label3D sign = new()
        {
            Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
            FixedSize = true,
            FontSize = 32,
            PixelSize = 0.0007f,
            OutlineSize = 8,
            VisibilityRangeEnd = 60,
        };

        public PlotView(PlotData kind, HeightMap map)
        {
            this.kind = kind;
            this.map = map;
            Name = $"Plot{kind.Id}";
            ribbon.MaterialOverride = material;
            AddChild(ribbon);
            var wood = new StandardMaterial3D { AlbedoColor = new Color(0.5f, 0.36f, 0.22f), Roughness = 0.9f };
            post.AddChild(new MeshInstance3D { Mesh = new BoxMesh { Size = new Vector3(0.15f, 1.6f, 0.15f), Material = wood }, Position = new Vector3(0, 0.8f, 0) });
            post.AddChild(new MeshInstance3D { Mesh = new BoxMesh { Size = new Vector3(0.8f, 0.45f, 0.06f), Material = wood }, Position = new Vector3(0, 1.35f, 0.1f) });
            sign.Position = new Vector3(0, 2.1f, 0);
            post.AddChild(sign);
            AddChild(post);
        }

        public Plot Row { get; private set; } = null!;

        public void Set(Plot p)
        {
            Row = p;
            ribbon.Mesh = Edge(map, Sim.Rect.Plot(kind, p.X, p.Z), 0.3f);
            // The post stands at the corner the marker faces.
            float h = kind.SizeM / 2.0f - 0.3f;
            var (cx, cz) = (p.Yaw % 4) switch { 0 => (-h, -h), 1 => (h, -h), 2 => (h, h), _ => (-h, h) };
            float x = p.X + cx, z = p.Z + cz;
            post.Position = new Vector3(x, map.Height(x, z), z);
        }

        public void Refresh(long now, string owner, bool ours)
        {
            long paid = Row.TaxPaidUntil.MicrosecondsSinceUnixEpoch;
            var standing = Land.StandingOf(paid, now);
            Visible = standing != Sim.Standing.Demolished;
            if (!Visible)
                return;
            var color = standing == Sim.Standing.Overdue ? OverdueColor : ours ? OursColor : TheirsColor;
            material.AlbedoColor = color;
            sign.Modulate = color;
            string access = Row.Access == 1 ? " · public" : "";
            sign.Text = standing == Sim.Standing.Overdue
                ? $"{kind.Name} · {owner}{access}\ndemolished in {Land.Span(Land.DemolishAt(paid) - now)}"
                : $"{kind.Name} · {owner}{access}";
        }

        /// <summary>A flat ribbon <paramref name="width"/> wide just inside <paramref name="r"/>'s edge, laid over the ground in 1 m steps.</summary>
        public static ArrayMesh Edge(HeightMap map, Sim.Rect r, float width)
        {
            var st = new SurfaceTool();
            st.Begin(Mesh.PrimitiveType.Triangles);
            var corners = new[] { (r.X0, r.Z0), (r.X1, r.Z0), (r.X1, r.Z1), (r.X0, r.Z1) };
            float cx = (r.X0 + r.X1) / 2, cz = (r.Z0 + r.Z1) / 2;
            for (int k = 0; k < 4; k++)
            {
                var (ax, az) = corners[k];
                var (bx, bz) = corners[(k + 1) % 4];
                int steps = Mathf.Max(1, Mathf.CeilToInt(new Vector2(bx - ax, bz - az).Length()));
                for (int s = 0; s < steps; s++)
                {
                    Vector3 Outer(float t)
                    {
                        float x = Mathf.Lerp(ax, bx, t), z = Mathf.Lerp(az, bz, t);
                        return new Vector3(x, map.Height(x, z) + 0.08f, z);
                    }
                    Vector3 Inner(float t)
                    {
                        var o = Outer(t);
                        // Toward the centre by `width` on both axes, so the corners meet.
                        float x = o.X + Mathf.Sign(cx - o.X) * width, z = o.Z + Mathf.Sign(cz - o.Z) * width;
                        float ix = Mathf.Abs(o.X - r.X0) < 1e-3f || Mathf.Abs(o.X - r.X1) < 1e-3f ? x : o.X;
                        float iz = Mathf.Abs(o.Z - r.Z0) < 1e-3f || Mathf.Abs(o.Z - r.Z1) < 1e-3f ? z : o.Z;
                        return new Vector3(ix, map.Height(ix, iz) + 0.08f, iz);
                    }
                    float t0 = (float)s / steps, t1 = (float)(s + 1) / steps;
                    Vector3 a = Outer(t0), b = Outer(t1), c = Inner(t1), d = Inner(t0);
                    foreach (var v in new[] { a, b, c, a, c, d, a, c, b, a, d, c })
                        st.AddVertex(v);
                }
            }
            return st.Commit();
        }
    }
}
