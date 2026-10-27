using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Resource nodes in the Static tier (docs/design/gathering.md): one placeholder view per
    /// <c>resource_node</c> row, drawn full when it's ready and as a stump while it regrows, worked
    /// out every frame from <c>ready_at</c> and game time. Nothing ticks on the server for this.
    /// </summary>
    public partial class NodeRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, NodeView> views = new();

        public NodeRegistry(Session session)
        {
            this.session = session;
            Name = "Nodes";
        }

        public int Count => views.Count;

        public IEnumerable<ResourceNode> Rows
        {
            get
            {
                foreach (var v in views.Values)
                    yield return v.Row;
            }
        }

        public ResourceNode? Find(ulong id) => views.TryGetValue(id, out var v) ? v.Row : null;

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.ResourceNode.OnInsert += (_, n) => Receive(n);
            db.ResourceNode.OnUpdate += (_, _, n) => Receive(n);
            db.ResourceNode.OnDelete += (_, n) =>
            {
                if (views.Remove(n.Id, out var v))
                    v.QueueFree();
            };
        }

        private void Receive(ResourceNode n)
        {
            if (!views.TryGetValue(n.Id, out var view))
            {
                if (GameContent.Node(n.Kind) is not NodeData kind)
                    return;
                view = new NodeView(kind);
                views[n.Id] = view;
                AddChild(view);
            }
            view.Set(n);
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            ulong me = session.Me?.EntityId ?? 0;
            foreach (var v in views.Values)
                v.Refresh(now, me);
        }
    }

    /// <summary>One node: placeholder geometry by category and colour by kind, until the look is chosen (ADR 0004).</summary>
    public partial class NodeView : Node3D
    {
        private readonly Node3D full;
        private readonly Node3D stump;
        private readonly MeshInstance3D busy;

        public NodeView(NodeData kind)
        {
            Kind = kind;
            Name = $"Node{kind.Id}";
            (full, stump) = Build(kind);
            AddChild(full);
            AddChild(stump);
            busy = new MeshInstance3D
            {
                Mesh = new TorusMesh { InnerRadius = 0.5f, OuterRadius = 0.65f, Material = Flat(new Color(1.0f, 0.85f, 0.2f)) },
                Position = new Vector3(0, kind.Kind == NodeKind.Tree ? 6.0f : 2.0f, 0),
                Visible = false,
            };
            AddChild(busy);
        }

        public NodeData Kind { get; }
        public ResourceNode Row { get; private set; } = null!;

        public void Set(ResourceNode n)
        {
            Row = n;
            Position = new Vector3(n.X, n.Y, n.Z);
            Rotation = new Vector3(0, Sim.Yaw.FromU16(n.Yaw), 0);
        }

        /// <summary>Show it ready or regrowing at game time <paramref name="now"/>, and mark it while someone else gathers it.</summary>
        public void Refresh(long now, ulong me)
        {
            bool ready = Sim.Gather.Ready(Row.ReadyAt.MicrosecondsSinceUnixEpoch, now);
            full.Visible = ready;
            stump.Visible = !ready;
            busy.Visible = Row.Gatherer != 0 && Row.Gatherer != me;
            if (busy.Visible)
                busy.RotateY(0.05f);
        }

        private static StandardMaterial3D Flat(Color c) => new() { AlbedoColor = c, Roughness = 0.9f };

        private static Color Tint(NodeData kind) => kind.Id switch
        {
            "saltreed_clump" => new Color(0.72f, 0.75f, 0.45f),
            "emberleaf_patch" => new Color(0.75f, 0.22f, 0.15f),
            "duskberry_bush" => new Color(0.18f, 0.35f, 0.2f),
            "tinstone_vein" => new Color(0.62f, 0.64f, 0.66f),
            "rustvein_outcrop" => new Color(0.55f, 0.3f, 0.2f),
            "driftpine" => new Color(0.16f, 0.36f, 0.24f),
            "hollowoak" => new Color(0.3f, 0.48f, 0.2f),
            _ => new Color(0.8f, 0.2f, 0.8f),
        };

        /// <summary>The node ready, and what's left while it regrows.</summary>
        private static (Node3D Full, Node3D Stump) Build(NodeData kind)
        {
            var tint = Flat(Tint(kind));
            var full = new Node3D { Name = "Full" };
            var stump = new Node3D { Name = "Stump" };
            var bark = Flat(new Color(0.35f, 0.24f, 0.15f));
            switch (kind.Kind)
            {
                case NodeKind.Plant:
                    // A tuft of three blades.
                    for (int i = 0; i < 3; i++)
                    {
                        float a = i * Mathf.Tau / 3;
                        full.AddChild(new MeshInstance3D
                        {
                            Mesh = new CylinderMesh { TopRadius = 0.0f, BottomRadius = 0.18f, Height = 0.9f, Material = tint },
                            Position = new Vector3(0.22f * Mathf.Cos(a), 0.45f, 0.22f * Mathf.Sin(a)),
                            RotationDegrees = new Vector3(12 * Mathf.Cos(a), 0, 12 * Mathf.Sin(a)),
                        });
                    }
                    if (kind.Id == "duskberry_bush")
                    {
                        var berry = Flat(new Color(0.35f, 0.12f, 0.45f));
                        foreach (var p in new[] { new Vector3(0.25f, 0.6f, 0.1f), new Vector3(-0.2f, 0.5f, -0.15f), new Vector3(0.0f, 0.75f, 0.2f) })
                            full.AddChild(new MeshInstance3D { Mesh = new SphereMesh { Radius = 0.09f, Height = 0.18f, Material = berry }, Position = p });
                    }
                    stump.AddChild(new MeshInstance3D { Mesh = new CylinderMesh { TopRadius = 0.35f, BottomRadius = 0.4f, Height = 0.05f, Material = Flat(new Color(0.4f, 0.32f, 0.2f)) }, Position = new Vector3(0, 0.03f, 0) });
                    break;
                case NodeKind.Ore:
                    full.AddChild(new MeshInstance3D
                    {
                        Mesh = new SphereMesh { Radius = 0.8f, Height = 1.1f, RadialSegments = 7, Rings = 4, Material = tint },
                        Position = new Vector3(0, 0.35f, 0),
                    });
                    full.AddChild(new MeshInstance3D
                    {
                        Mesh = new SphereMesh { Radius = 0.45f, Height = 0.7f, RadialSegments = 6, Rings = 3, Material = tint },
                        Position = new Vector3(0.7f, 0.2f, 0.3f),
                    });
                    stump.AddChild(new MeshInstance3D
                    {
                        Mesh = new SphereMesh { Radius = 0.5f, Height = 0.4f, RadialSegments = 6, Rings = 3, Material = Flat(Tint(kind).Darkened(0.4f)) },
                        Position = new Vector3(0, 0.1f, 0),
                    });
                    break;
                case NodeKind.Tree:
                    full.AddChild(new MeshInstance3D { Mesh = new CylinderMesh { TopRadius = 0.18f, BottomRadius = 0.28f, Height = 3.0f, Material = bark }, Position = new Vector3(0, 1.5f, 0) });
                    Mesh crown = kind.Id == "driftpine"
                        ? new CylinderMesh { TopRadius = 0.0f, BottomRadius = 1.4f, Height = 3.6f, Material = tint }
                        : new SphereMesh { Radius = 1.8f, Height = 3.0f, Material = tint };
                    full.AddChild(new MeshInstance3D { Mesh = crown, Position = new Vector3(0, kind.Id == "driftpine" ? 4.2f : 4.0f, 0) });
                    stump.AddChild(new MeshInstance3D { Mesh = new CylinderMesh { TopRadius = 0.26f, BottomRadius = 0.3f, Height = 0.5f, Material = bark }, Position = new Vector3(0, 0.25f, 0) });
                    break;
            }
            return (full, stump);
        }
    }
}
