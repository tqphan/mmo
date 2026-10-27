using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Wayshrines in the Static tier (docs/design/combat.md): one placeholder view per <c>shrine</c>
    /// row, with its name floating above it. The dead respawn at the nearest.
    /// </summary>
    public partial class ShrineRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, Node3D> views = new();

        public ShrineRegistry(Session session)
        {
            this.session = session;
            Name = "Shrines";
        }

        public int Count => views.Count;

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Shrine.OnInsert += (_, s) => Receive(s);
            db.Shrine.OnDelete += (_, s) =>
            {
                if (views.Remove(s.Id, out var v))
                    v.QueueFree();
            };
        }

        private void Receive(Shrine s)
        {
            if (views.ContainsKey(s.Id) || GameContent.Shrine(s.Kind) is not ShrineData kind)
                return;
            var view = ShrineView(kind);
            view.Position = new Vector3(s.X, s.Y, s.Z);
            view.Rotation = new Vector3(0, Sim.Yaw.FromU16(s.Yaw), 0);
            views[s.Id] = view;
            AddChild(view);
        }

        /// <summary>A standing stone on a stepped plinth, with a pale glow at its crown.</summary>
        private static Node3D ShrineView(ShrineData kind)
        {
            var root = new Node3D { Name = $"Shrine{kind.Id}" };
            var stone = new StandardMaterial3D { AlbedoColor = new Color(0.55f, 0.57f, 0.6f), Roughness = 0.95f };
            var glow = new StandardMaterial3D { AlbedoColor = new Color(0.7f, 0.95f, 1.0f), EmissionEnabled = true, Emission = new Color(0.5f, 0.85f, 1.0f), EmissionEnergyMultiplier = 1.5f };
            MeshInstance3D Box(Vector3 size, float y, Material m) => new() { Mesh = new BoxMesh { Size = size, Material = m }, Position = new Vector3(0, y, 0) };
            root.AddChild(Box(new Vector3(2.4f, 0.3f, 2.4f), 0.15f, stone));
            root.AddChild(Box(new Vector3(1.6f, 0.3f, 1.6f), 0.45f, stone));
            root.AddChild(Box(new Vector3(0.6f, 2.6f, 0.4f), 1.9f, stone));
            root.AddChild(new MeshInstance3D { Mesh = new SphereMesh { Radius = 0.22f, Height = 0.44f, Material = glow }, Position = new Vector3(0, 3.45f, 0) });
            root.AddChild(new Label3D
            {
                Text = kind.Name,
                Position = new Vector3(0, 4.0f, 0),
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                FixedSize = true,
                FontSize = 32,
                PixelSize = 0.0007f,
                OutlineSize = 8,
                VisibilityRangeEnd = 80,
                Modulate = new Color(0.8f, 0.95f, 1.0f),
            });
            return root;
        }
    }
}
