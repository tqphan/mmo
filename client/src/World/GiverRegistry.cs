using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Quest givers in the Static tier (docs/design/quests.md): one placeholder figure per
    /// <c>quest_giver</c> row, with its name and title above it and a marker over that which
    /// <see cref="Gameplay.Quests"/> sets: ! for a quest to take, ? for one to turn in.
    /// </summary>
    public partial class GiverRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, (QuestGiver Row, Node3D View, Label3D Marker)> views = new();

        public GiverRegistry(Session session)
        {
            this.session = session;
            Name = "Givers";
        }

        public IEnumerable<QuestGiver> Rows
        {
            get
            {
                foreach (var v in views.Values)
                    yield return v.Row;
            }
        }

        public QuestGiver? Find(ulong id) => views.TryGetValue(id, out var v) ? v.Row : null;

        /// <summary>Show <paramref name="text"/> over giver <paramref name="id"/>.</summary>
        public void SetMarker(ulong id, string text)
        {
            if (views.TryGetValue(id, out var v) && v.Marker.Text != text)
                v.Marker.Text = text;
        }

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.QuestGiver.OnInsert += (_, g) => Receive(g);
            db.QuestGiver.OnDelete += (_, g) =>
            {
                if (views.Remove(g.Id, out var v))
                    v.View.QueueFree();
            };
        }

        private void Receive(QuestGiver g)
        {
            if (views.ContainsKey(g.Id) || GameContent.Giver(g.Kind) is not GiverData kind)
                return;
            var (view, marker) = GiverView(kind);
            view.Position = new Vector3(g.X, g.Y, g.Z);
            view.Rotation = new Vector3(0, Sim.Yaw.FromU16(g.Yaw), 0);
            views[g.Id] = (g, view, marker);
            AddChild(view);
        }

        private static Label3D Label(string text, float y, int size, Color color) => new()
        {
            Text = text,
            Position = new Vector3(0, y, 0),
            Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
            FixedSize = true,
            FontSize = size,
            PixelSize = 0.0007f,
            OutlineSize = 8,
            VisibilityRangeEnd = 80,
            Modulate = color,
        };

        /// <summary>A robed figure in a giver's colour on a low step, its name and title, and the marker above.</summary>
        private static (Node3D, Label3D) GiverView(GiverData kind)
        {
            var root = new Node3D { Name = $"Giver{kind.Id}" };
            var robe = new StandardMaterial3D { AlbedoColor = Color.FromHsv(kind.Wire * 0.23f % 1.0f, 0.45f, 0.65f), Roughness = 0.9f };
            var skin = new StandardMaterial3D { AlbedoColor = new Color(0.86f, 0.7f, 0.56f), Roughness = 0.8f };
            var step = new StandardMaterial3D { AlbedoColor = new Color(0.45f, 0.38f, 0.3f), Roughness = 0.95f };
            root.AddChild(new MeshInstance3D { Mesh = new CylinderMesh { TopRadius = 0.7f, BottomRadius = 0.75f, Height = 0.15f, Material = step }, Position = new Vector3(0, 0.075f, 0) });
            root.AddChild(new MeshInstance3D { Mesh = new CylinderMesh { TopRadius = 0.22f, BottomRadius = 0.4f, Height = 1.35f, Material = robe }, Position = new Vector3(0, 0.82f, 0) });
            root.AddChild(new MeshInstance3D { Mesh = new SphereMesh { Radius = 0.19f, Height = 0.38f, Material = skin }, Position = new Vector3(0, 1.68f, 0) });
            root.AddChild(Label(kind.Name, 2.3f, 32, new Color(1.0f, 0.95f, 0.75f)));
            root.AddChild(Label(kind.Title, 2.12f, 24, new Color(0.85f, 0.85f, 0.8f)));
            var marker = Label("", 2.75f, 64, new Color(1.0f, 0.85f, 0.2f));
            root.AddChild(marker);
            return (root, marker);
        }
    }
}
