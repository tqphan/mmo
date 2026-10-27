using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Trade posts and dropped packs in the Static tier (docs/design/trade.md): one placeholder
    /// view per <c>trade_post</c> and <c>dropped_pack</c> row, each with its name floating above it.
    /// </summary>
    public partial class TradeRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, (TradePost Row, Node3D View)> posts = new();
        private readonly Dictionary<ulong, (DroppedPack Row, Node3D View)> packs = new();

        public TradeRegistry(Session session)
        {
            this.session = session;
            Name = "Trade";
        }

        public IEnumerable<TradePost> Posts
        {
            get
            {
                foreach (var p in posts.Values)
                    yield return p.Row;
            }
        }

        public IEnumerable<DroppedPack> Dropped
        {
            get
            {
                foreach (var p in packs.Values)
                    yield return p.Row;
            }
        }

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.TradePost.OnInsert += (_, p) => Receive(p);
            db.TradePost.OnUpdate += (_, _, p) => Receive(p);
            db.TradePost.OnDelete += (_, p) =>
            {
                if (posts.Remove(p.Id, out var v))
                    v.View.QueueFree();
            };
            db.DroppedPack.OnInsert += (_, d) => Receive(d);
            db.DroppedPack.OnDelete += (_, d) =>
            {
                if (packs.Remove(d.Id, out var v))
                    v.View.QueueFree();
            };
        }

        private void Receive(TradePost p)
        {
            if (!posts.TryGetValue(p.Id, out var entry))
            {
                if (GameContent.Post(p.Kind) is not PostData kind)
                    return;
                entry = (p, PostView(kind));
                AddChild(entry.View);
            }
            entry.Row = p;
            posts[p.Id] = entry;
            entry.View.Position = new Vector3(p.X, p.Y, p.Z);
            entry.View.Rotation = new Vector3(0, Sim.Yaw.FromU16(p.Yaw), 0);
        }

        private void Receive(DroppedPack d)
        {
            if (packs.ContainsKey(d.Id))
                return;
            var view = new Node3D { Name = $"Dropped{d.Id}", Position = new Vector3(d.X, d.Y, d.Z), Rotation = new Vector3(0, Sim.Yaw.FromU16(d.Yaw), 0) };
            view.AddChild(Views.Pack(d.Def));
            view.AddChild(Label(GameContent.Item(d.Def)?.Name ?? "Pack", 1.2f, new Color(0.95f, 0.9f, 0.7f)));
            packs[d.Id] = (d, view);
            AddChild(view);
        }

        private static Label3D Label(string text, float height, Color color) => new()
        {
            Text = text,
            Position = new Vector3(0, height, 0),
            Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
            // The same size on screen at any distance, and only nearby.
            FixedSize = true,
            FontSize = 32,
            PixelSize = 0.0007f,
            OutlineSize = 8,
            VisibilityRangeEnd = 60,
            Modulate = color,
        };

        private static StandardMaterial3D Flat(Color c) => new() { AlbedoColor = c, Roughness = 0.9f };

        /// <summary>A market stall: a counter under a striped awning on four posts, with crates stacked beside it.</summary>
        private static Node3D PostView(PostData kind)
        {
            var root = new Node3D { Name = $"Post{kind.Id}" };
            var wood = Flat(new Color(0.5f, 0.34f, 0.2f));
            var cloth = Flat(new Color(0.75f, 0.22f, 0.18f));
            var pale = Flat(new Color(0.92f, 0.86f, 0.7f));
            MeshInstance3D Box(Vector3 size, Vector3 at, Material m) => new() { Mesh = new BoxMesh { Size = size, Material = m }, Position = at };
            root.AddChild(Box(new Vector3(3.0f, 1.0f, 1.0f), new Vector3(0, 0.5f, 0), wood));
            foreach (var (x, z) in new[] { (-1.5f, -0.6f), (1.5f, -0.6f), (-1.5f, 0.9f), (1.5f, 0.9f) })
                root.AddChild(Box(new Vector3(0.14f, 2.6f, 0.14f), new Vector3(x, 1.3f, z), wood));
            // The awning in stripes, sloping towards the front.
            for (int i = 0; i < 6; i++)
            {
                var stripe = Box(new Vector3(0.55f, 0.06f, 1.9f), new Vector3(-1.375f + i * 0.55f, 2.65f, 0.15f), i % 2 == 0 ? cloth : pale);
                stripe.RotationDegrees = new Vector3(-12, 0, 0);
                root.AddChild(stripe);
            }
            root.AddChild(Box(new Vector3(0.6f, 0.5f, 0.5f), new Vector3(2.2f, 0.25f, 0.2f), Flat(new Color(0.62f, 0.42f, 0.24f))));
            root.AddChild(Box(new Vector3(0.5f, 0.45f, 0.45f), new Vector3(2.15f, 0.72f, 0.2f), Flat(new Color(0.55f, 0.37f, 0.2f))));
            root.AddChild(Label(kind.Name, 3.3f, new Color(1.0f, 0.93f, 0.75f)));
            return root;
        }
    }
}
