using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.Sim;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Shipyards in the Static tier (docs/design/vehicles.md §2B): one placeholder view per
    /// <c>shipyard</c> row, a scaffold the size of its hull's deck with the hull rising in it stage
    /// by stage, and a label with its name and next stage.
    /// </summary>
    public partial class ShipyardRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, YardView> views = new();

        public ShipyardRegistry(Session session)
        {
            this.session = session;
            Name = "Shipyards";
        }

        public IEnumerable<Shipyard> Rows
        {
            get
            {
                foreach (var v in views.Values)
                    yield return v.Row;
            }
        }

        /// <summary>Every yard in sight, as the rules see it.</summary>
        public IEnumerable<ShipyardModel.Seen> Seen(ulong me)
        {
            foreach (var y in Rows)
                if (GameContent.Shipyard(y.Kind) is { } kind)
                    yield return new ShipyardModel.Seen(y.Id, kind, new Sim.ShipMotion(y.X, y.Z, y.Yaw, 0), y.Stage, me != 0 && y.Owner == me);
        }

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Shipyard.OnInsert += (_, y) => Receive(y);
            db.Shipyard.OnUpdate += (_, _, y) => Receive(y);
            db.Shipyard.OnDelete += (_, y) =>
            {
                if (views.Remove(y.Id, out var v))
                    v.QueueFree();
            };
        }

        private void Receive(Shipyard y)
        {
            if (!views.TryGetValue(y.Id, out var view))
            {
                if (GameContent.Shipyard(y.Kind) is not ShipyardData kind)
                    return;
                view = new YardView(kind);
                views[y.Id] = view;
                AddChild(view);
            }
            view.Set(y);
        }
    }

    /// <summary>One shipyard: posts and rails round the footprint and a hull that grows with the stages.</summary>
    public partial class YardView : Node3D
    {
        private readonly ShipyardData kind;
        private readonly MeshInstance3D hull;
        private readonly Label3D label;

        public YardView(ShipyardData kind)
        {
            this.kind = kind;
            Name = $"Yard{kind.Id}";
            var h = kind.Hull.Data();
            float hw = h.DeckHalfWidthM + 0.8f, hl = h.DeckHalfLengthM + 0.8f, top = h.DeckHeightM + 3.0f;
            var wood = new StandardMaterial3D { AlbedoColor = new Color(0.55f, 0.42f, 0.28f), Roughness = 0.9f };
            // Posts from the seabed (well below) to above the deck, along both sides.
            for (float z = -hl; z <= hl + 0.01f; z += hl / 3)
                foreach (float x in new[] { -hw, hw })
                    AddChild(new MeshInstance3D
                    {
                        Mesh = new BoxMesh { Size = new Vector3(0.3f, top + 6, 0.3f), Material = wood },
                        Position = new Vector3(x, top - (top + 6) / 2, z),
                    });
            foreach (float x in new[] { -hw, hw })
                AddChild(new MeshInstance3D { Mesh = new BoxMesh { Size = new Vector3(0.2f, 0.2f, hl * 2), Material = wood }, Position = new Vector3(x, top, 0) });
            for (float z = -hl; z <= hl + 0.01f; z += hl)
                AddChild(new MeshInstance3D { Mesh = new BoxMesh { Size = new Vector3(hw * 2, 0.2f, 0.2f), Material = wood }, Position = new Vector3(0, top, z) });
            hull = new MeshInstance3D { Mesh = new BoxMesh { Size = new Vector3(h.DeckHalfWidthM * 2, 1, h.DeckHalfLengthM * 2), Material = new StandardMaterial3D { AlbedoColor = new Color(0.62f, 0.5f, 0.36f) } } };
            AddChild(hull);
            label = new Label3D
            {
                Position = new Vector3(0, top + 1.2f, 0),
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                FixedSize = true,
                FontSize = 28,
                PixelSize = 0.0012f,
                OutlineSize = 6,
                NoDepthTest = true,
            };
            AddChild(label);
        }

        public Shipyard Row { get; private set; } = null!;

        public void Set(Shipyard y)
        {
            Row = y;
            var h = kind.Hull.Data();
            // The hull rises from below the water to its full height by the last stage.
            float share = (float)y.Stage / kind.Stages.Count;
            float full = h.DeckHeightM + h.DraftM;
            float height = Mathf.Max(0.2f, full * share);
            hull.Scale = new Vector3(Mathf.Max(0.3f, share), height, 1);
            hull.Position = new Vector3(0, -h.DraftM + height / 2, 0);
            hull.Visible = y.Stage > 0;
            label.Text = ShipyardModel.StageLine(kind, y.Stage);
            GlobalTransform = new Transform3D(new Basis(Vector3.Up, y.Yaw), new Vector3(y.X, Movement.SeaLevelM, y.Z));
        }
    }
}
