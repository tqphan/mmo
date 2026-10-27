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
    /// Crops, trees and livestock in the Static tier (docs/design/farming.md): one placeholder view
    /// per <c>crop</c> row, grown, ripe or withered by game time every frame. Nothing ticks on the
    /// server for this. Only the planting in <see cref="Focus"/> wears a label, as rows of crops a
    /// metre apart would print over each other.
    /// </summary>
    public partial class CropRegistry : Node3D
    {
        private readonly Session session;
        private readonly PlotRegistry plots;
        private readonly Dictionary<ulong, CropView> views = new();

        public CropRegistry(Session session, PlotRegistry plots)
        {
            this.session = session;
            this.plots = plots;
            Name = "Crops";
        }

        public int Count => views.Count;

        /// <summary>The planting whose label shows: the one the HUD is talking about, or none.</summary>
        public ulong Focus { get; set; }

        public IEnumerable<Crop> Rows => views.Values.Select(v => v.Row);

        public Crop? Find(ulong id) => views.TryGetValue(id, out var v) ? v.Row : null;

        /// <summary>Whether one of our characters planted <paramref name="c"/>.</summary>
        public bool Mine(Crop c) => session.Conn?.Db.MyCharacters.Iter().Any(ch => ch.CharId == c.Owner) == true;

        /// <summary>The standing plot a planting is on, if it's on one that's still standing.</summary>
        public Plot? PlotOf(Crop c) => c.Plot == 0 ? null : plots.Standing.FirstOrDefault(p => p.Id == c.Plot);

        public Stage StageOf(Crop c) =>
            GameContent.Crop(c.Kind) is { } k ? Farm.StageOf(k, c.RipeAt.MicrosecondsSinceUnixEpoch, session.GameNowMicros) : Stage.Withered;

        /// <summary>Withered, or its plot is gone: the server treats it as gone for planting.</summary>
        public bool Gone(Crop c) => StageOf(c) == Stage.Withered || (c.Plot != 0 && PlotOf(c) is null);

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Crop.OnInsert += (_, c) => Receive(c);
            db.Crop.OnUpdate += (_, _, c) => Receive(c);
            db.Crop.OnDelete += (_, c) =>
            {
                if (views.Remove(c.Id, out var v))
                    v.QueueFree();
            };
        }

        private void Receive(Crop c)
        {
            if (!views.TryGetValue(c.Id, out var view))
            {
                if (GameContent.Crop(c.Kind) is not CropData kind)
                    return;
                view = new CropView(kind);
                views[c.Id] = view;
                AddChild(view);
            }
            view.Set(c);
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            foreach (var v in views.Values)
                v.Refresh(now, v.Row.Id == Focus);
        }
    }

    /// <summary>One planting: placeholder geometry by kind, until the look is chosen (ADR 0004).</summary>
    public partial class CropView : Node3D
    {
        private static readonly Color Dead = new(0.45f, 0.35f, 0.22f);

        private readonly Node3D body = new() { Name = "Body" };
        private readonly Node3D ripe = new() { Name = "Ripe" };
        private readonly StandardMaterial3D leaf;
        private readonly Label3D label = new()
        {
            Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
            FixedSize = true,
            FontSize = 26,
            PixelSize = 0.0006f,
            OutlineSize = 8,
            VisibilityRangeEnd = 18,
        };
        private Stage shown = (Stage)255;
        private float shownScale = -1;

        public CropView(CropData kind)
        {
            Kind = kind;
            Name = $"Crop{kind.Id}";
            leaf = Flat(Tint(kind));
            Build();
            AddChild(body);
            body.AddChild(ripe);
            label.Position = new Vector3(0, kind.Kind == CropKind.Tree ? 5.0f : 1.6f, 0);
            AddChild(label);
        }

        public CropData Kind { get; }
        public Crop Row { get; private set; } = null!;

        public void Set(Crop c)
        {
            Row = c;
            Position = new Vector3(c.X, c.Y, c.Z);
            body.Rotation = new Vector3(0, Sim.Yaw.FromU16(c.Yaw), 0);
        }

        public void Refresh(long now, bool labelled)
        {
            long from = Row.GrownFrom.MicrosecondsSinceUnixEpoch, at = Row.RipeAt.MicrosecondsSinceUnixEpoch;
            var stage = Farm.StageOf(Kind, at, now);
            // Trees and livestock keep their size once grown; only their fruit or fleece comes again.
            float grown = Row.Harvests > 0 ? 1 : Farm.Progress(from, at, now);
            float min = Kind.Kind switch { CropKind.Tree => 0.15f, CropKind.Animal => 0.55f, _ => 0.2f };
            float scale = min + (1 - min) * grown;
            if (stage != shown || Mathf.Abs(scale - shownScale) > 0.01f)
            {
                body.Scale = Vector3.One * scale;
                ripe.Visible = stage == Stage.Ripe;
                leaf.AlbedoColor = stage == Stage.Withered ? Dead : Tint(Kind);
                body.RotationDegrees = new Vector3(stage == Stage.Withered ? 18 : 0, body.RotationDegrees.Y, 0);
                shown = stage;
                shownScale = scale;
            }
            label.Visible = labelled;
            if (!labelled)
                return;
            label.Text = $"{Kind.Name}\n{Farm.StageText(Kind, from, at, now)}";
            label.Modulate = stage switch { Stage.Ripe => new Color(1.0f, 0.9f, 0.35f), Stage.Withered => new Color(0.75f, 0.6f, 0.45f), _ => Colors.White };
        }

        private static StandardMaterial3D Flat(Color c) => new() { AlbedoColor = c, Roughness = 0.9f };

        private static Color Tint(CropData kind) => kind.Id switch
        {
            "hearthgrain" => new Color(0.55f, 0.7f, 0.3f),
            "whiteflax" => new Color(0.45f, 0.65f, 0.55f),
            "sunroot" => new Color(0.3f, 0.6f, 0.2f),
            "cinderpepper" => new Color(0.35f, 0.5f, 0.2f),
            "dustbean" => new Color(0.5f, 0.6f, 0.3f),
            "frostkale" => new Color(0.4f, 0.55f, 0.6f),
            "amberpear_tree" => new Color(0.3f, 0.5f, 0.2f),
            "driftpine_tree" => new Color(0.16f, 0.36f, 0.24f),
            "woolback" => new Color(0.9f, 0.88f, 0.8f),
            _ => new Color(0.8f, 0.2f, 0.8f),
        };

        /// <summary>The ripe parts' colour: grain heads, fruit, or a full fleece.</summary>
        private static Color Fruit(CropData kind) => kind.Id switch
        {
            "hearthgrain" => new Color(0.92f, 0.78f, 0.35f),
            "whiteflax" => new Color(0.75f, 0.8f, 1.0f),
            "sunroot" => new Color(0.95f, 0.6f, 0.2f),
            "cinderpepper" => new Color(0.85f, 0.2f, 0.12f),
            "dustbean" => new Color(0.7f, 0.6f, 0.4f),
            "frostkale" => new Color(0.6f, 0.75f, 0.9f),
            "amberpear_tree" => new Color(0.95f, 0.7f, 0.2f),
            "driftpine_tree" => new Color(0.4f, 0.28f, 0.18f),
            _ => new Color(0.97f, 0.96f, 0.92f),
        };

        private void Build()
        {
            var fruit = Flat(Fruit(Kind));
            var bark = Flat(new Color(0.35f, 0.24f, 0.15f));
            void Add(Node3D to, Mesh m, Vector3 at, Vector3? rot = null) =>
                to.AddChild(new MeshInstance3D { Mesh = m, Position = at, RotationDegrees = rot ?? Vector3.Zero });
            switch (Kind.Kind)
            {
                case CropKind.Crop:
                    // Five stalks, each with a head when ripe.
                    for (int i = 0; i < 5; i++)
                    {
                        float a = i * Mathf.Tau / 5;
                        var at = new Vector3(0.18f * Mathf.Cos(a), 0, 0.18f * Mathf.Sin(a));
                        var tilt = new Vector3(10 * Mathf.Cos(a), 0, 10 * Mathf.Sin(a));
                        Add(body, new CylinderMesh { TopRadius = 0.02f, BottomRadius = 0.05f, Height = 0.9f, Material = leaf }, at + new Vector3(0, 0.45f, 0), tilt);
                        Add(ripe, new SphereMesh { Radius = 0.08f, Height = 0.22f, Material = fruit }, at * 1.15f + new Vector3(0, 0.95f, 0));
                    }
                    break;
                case CropKind.Tree:
                    Add(body, new CylinderMesh { TopRadius = 0.16f, BottomRadius = 0.26f, Height = 2.6f, Material = bark }, new Vector3(0, 1.3f, 0));
                    if (Kind.Id == "driftpine_tree")
                        Add(body, new CylinderMesh { TopRadius = 0.0f, BottomRadius = 1.3f, Height = 3.4f, Material = leaf }, new Vector3(0, 3.8f, 0));
                    else
                    {
                        Add(body, new SphereMesh { Radius = 1.5f, Height = 2.6f, Material = leaf }, new Vector3(0, 3.4f, 0));
                        // Hanging on the crown's surface, all the way round.
                        for (int i = 0; i < 8; i++)
                        {
                            float a = i * Mathf.Tau / 8 + 0.3f, y = i % 2 == 0 ? 2.9f : 3.6f;
                            float r = 1.5f * Mathf.Sqrt(1 - Mathf.Pow((y - 3.4f) / 1.3f, 2)) + 0.05f;
                            Add(ripe, new SphereMesh { Radius = 0.2f, Height = 0.4f, Material = fruit }, new Vector3(r * Mathf.Cos(a), y, r * Mathf.Sin(a)));
                        }
                    }
                    break;
                case CropKind.Animal:
                    var skin = Flat(new Color(0.3f, 0.25f, 0.22f));
                    Add(body, new BoxMesh { Size = new Vector3(0.6f, 0.5f, 1.0f), Material = leaf }, new Vector3(0, 0.6f, 0));
                    Add(body, new BoxMesh { Size = new Vector3(0.3f, 0.3f, 0.35f), Material = skin }, new Vector3(0, 0.85f, -0.6f));
                    foreach (var (x, z) in new[] { (-0.2f, -0.35f), (0.2f, -0.35f), (-0.2f, 0.35f), (0.2f, 0.35f) })
                        Add(body, new CylinderMesh { TopRadius = 0.05f, BottomRadius = 0.05f, Height = 0.4f, Material = skin }, new Vector3(x, 0.2f, z));
                    // A full fleece when it can be shorn.
                    Add(ripe, new SphereMesh { Radius = 0.5f, Height = 0.75f, RadialSegments = 8, Rings = 4, Material = fruit }, new Vector3(0, 0.7f, 0.05f));
                    break;
            }
        }
    }
}
