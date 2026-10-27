using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Crafting stations in the Static tier (docs/design/crafting.md): one placeholder view per
    /// <c>station</c> row, with its name floating above it.
    /// </summary>
    public partial class StationRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, StationView> views = new();

        public StationRegistry(Session session)
        {
            this.session = session;
            Name = "Stations";
        }

        public int Count => views.Count;

        public IEnumerable<Station> Rows
        {
            get
            {
                foreach (var v in views.Values)
                    yield return v.Row;
            }
        }

        public Station? Find(ulong id) => views.TryGetValue(id, out var v) ? v.Row : null;

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Station.OnInsert += (_, s) => Receive(s);
            db.Station.OnUpdate += (_, _, s) => Receive(s);
            db.Station.OnDelete += (_, s) =>
            {
                if (views.Remove(s.Id, out var v))
                    v.QueueFree();
            };
        }

        private void Receive(Station s)
        {
            if (!views.TryGetValue(s.Id, out var view))
            {
                if (GameContent.Station(s.Kind) is not StationData kind)
                    return;
                view = new StationView(kind);
                views[s.Id] = view;
                AddChild(view);
            }
            view.Set(s);
        }
    }

    /// <summary>One station: placeholder geometry by kind until the look is chosen (ADR 0004).</summary>
    public partial class StationView : Node3D
    {
        public StationView(StationData kind)
        {
            Kind = kind;
            Name = $"Station{kind.Id}";
            AddChild(Build(kind));
            AddChild(new Label3D
            {
                Text = kind.Name,
                Position = new Vector3(0, 2.4f, 0),
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                // The same size on screen at any distance, and only nearby.
                FixedSize = true,
                FontSize = 32,
                PixelSize = 0.0007f,
                OutlineSize = 8,
                VisibilityRangeEnd = 40,
                Modulate = new Color(1.0f, 0.93f, 0.75f),
            });
        }

        public StationData Kind { get; }
        public Station Row { get; private set; } = null!;

        public void Set(Station s)
        {
            Row = s;
            Position = new Vector3(s.X, s.Y, s.Z);
            Rotation = new Vector3(0, Sim.Yaw.FromU16(s.Yaw), 0);
        }

        private static StandardMaterial3D Flat(Color c) => new() { AlbedoColor = c, Roughness = 0.9f };

        private static MeshInstance3D Box(Vector3 size, Vector3 at, Material m) => new() { Mesh = new BoxMesh { Size = size, Material = m }, Position = at };

        private static Node3D Build(StationData kind)
        {
            var root = new Node3D { Name = "Body" };
            var wood = Flat(new Color(0.55f, 0.38f, 0.22f));
            var stone = Flat(new Color(0.5f, 0.48f, 0.45f));
            var glow = new StandardMaterial3D
            {
                AlbedoColor = new Color(1.0f, 0.45f, 0.1f),
                EmissionEnabled = true,
                Emission = new Color(1.0f, 0.4f, 0.05f),
                EmissionEnergyMultiplier = 2.5f,
            };
            switch (kind.Id)
            {
                case "joiners_bench":
                    // A plank table on four legs, with a vice block at one end.
                    root.AddChild(Box(new Vector3(2.2f, 0.12f, 0.9f), new Vector3(0, 0.9f, 0), wood));
                    foreach (var (x, z) in new[] { (-0.95f, -0.35f), (0.95f, -0.35f), (-0.95f, 0.35f), (0.95f, 0.35f) })
                        root.AddChild(Box(new Vector3(0.12f, 0.9f, 0.12f), new Vector3(x, 0.45f, z), wood));
                    root.AddChild(Box(new Vector3(0.3f, 0.25f, 0.4f), new Vector3(0.85f, 1.08f, 0), Flat(new Color(0.35f, 0.25f, 0.15f))));
                    break;
                case "bloomery":
                    // A squat stone stack with a glowing mouth.
                    root.AddChild(new MeshInstance3D
                    {
                        Mesh = new CylinderMesh { TopRadius = 0.45f, BottomRadius = 0.85f, Height = 1.8f, RadialSegments = 8, Material = stone },
                        Position = new Vector3(0, 0.9f, 0),
                    });
                    root.AddChild(Box(new Vector3(0.45f, 0.4f, 0.2f), new Vector3(0, 0.35f, 0.75f), glow));
                    root.AddChild(new OmniLight3D { Position = new Vector3(0, 0.5f, 1.1f), LightColor = new Color(1.0f, 0.5f, 0.2f), LightEnergy = 1.5f, OmniRange = 4 });
                    break;
                case "packwright_bench":
                    // A low, heavy table with a crate and a coil of rope on it, and a stack of sacks beside.
                    root.AddChild(Box(new Vector3(2.4f, 0.16f, 1.2f), new Vector3(0, 0.7f, 0), wood));
                    foreach (var (x, z) in new[] { (-1.05f, -0.45f), (1.05f, -0.45f), (-1.05f, 0.45f), (1.05f, 0.45f) })
                        root.AddChild(Box(new Vector3(0.18f, 0.7f, 0.18f), new Vector3(x, 0.35f, z), wood));
                    root.AddChild(Box(new Vector3(0.6f, 0.5f, 0.5f), new Vector3(-0.5f, 1.03f, 0), Flat(new Color(0.65f, 0.45f, 0.26f))));
                    root.AddChild(new MeshInstance3D
                    {
                        Mesh = new TorusMesh { InnerRadius = 0.12f, OuterRadius = 0.25f, Material = Flat(new Color(0.75f, 0.65f, 0.4f)) },
                        Position = new Vector3(0.55f, 0.82f, 0),
                    });
                    root.AddChild(Box(new Vector3(0.7f, 0.45f, 0.5f), new Vector3(1.7f, 0.22f, 0), Flat(new Color(0.85f, 0.75f, 0.5f))));
                    break;
                default:
                    // A ring of stones round an ember.
                    for (int i = 0; i < 8; i++)
                    {
                        float a = i * Mathf.Tau / 8;
                        root.AddChild(new MeshInstance3D
                        {
                            Mesh = new SphereMesh { Radius = 0.22f, Height = 0.3f, RadialSegments = 6, Rings = 3, Material = stone },
                            Position = new Vector3(0.7f * Mathf.Cos(a), 0.1f, 0.7f * Mathf.Sin(a)),
                        });
                    }
                    root.AddChild(new MeshInstance3D { Mesh = new SphereMesh { Radius = 0.35f, Height = 0.3f, Material = glow }, Position = new Vector3(0, 0.08f, 0) });
                    root.AddChild(new OmniLight3D { Position = new Vector3(0, 0.6f, 0), LightColor = new Color(1.0f, 0.5f, 0.2f), LightEnergy = 1.2f, OmniRange = 4 });
                    break;
            }
            return root;
        }
    }
}
