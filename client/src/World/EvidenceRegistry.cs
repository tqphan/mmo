using System.Collections.Generic;
using Godot;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.Sim;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Evidence in the Near tier (docs/design/crime.md): one flat placeholder decal per
    /// <c>evidence</c> row, hidden once it has faded. It says what happened and where, never who.
    /// </summary>
    public partial class EvidenceRegistry : Node3D
    {
        private readonly Session session;
        private readonly Dictionary<ulong, (EvidenceInfo Info, Node3D View)> views = new();

        public EvidenceRegistry(Session session)
        {
            this.session = session;
            Name = "Evidence";
        }

        /// <summary>Every piece in view, faded or not.</summary>
        public IEnumerable<EvidenceInfo> All
        {
            get
            {
                foreach (var (info, _) in views.Values)
                    yield return info;
            }
        }

        public int Count => views.Count;

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Evidence.OnInsert += (_, e) => Receive(e);
            db.Evidence.OnDelete += (_, e) =>
            {
                if (views.Remove(e.Id, out var v))
                    v.View.QueueFree();
            };
        }

        private void Receive(Evidence e)
        {
            if (views.ContainsKey(e.Id))
                return;
            var kind = (EvidenceKind)e.Kind;
            var info = new EvidenceInfo(e.Id, kind, new System.Numerics.Vector3(e.X, e.Y, e.Z), e.MadeAt.MicrosecondsSinceUnixEpoch);
            var view = View(kind, e.Id);
            view.Position = new Vector3(e.X, e.Y + 0.03f, e.Z);
            views[e.Id] = (info, view);
            AddChild(view);
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            foreach (var (info, view) in views.Values)
                view.Visible = !Sim.Crime.Faded(info.MadeAt, now);
        }

        /// <summary>Dark boot prints in a short trail, a small red splash, or a wide dark-red pool.</summary>
        private static Node3D View(EvidenceKind kind, ulong id)
        {
            var root = new Node3D { Name = $"Evidence{id}", Rotation = new Vector3(0, (id % 16) * Mathf.Tau / 16, 0) };
            StandardMaterial3D Mat(Color c) => new() { AlbedoColor = c, Roughness = 0.6f };
            MeshInstance3D Disc(float r, Material m, Vector3 at, float squash = 1) => new()
            {
                Mesh = new CylinderMesh { TopRadius = r, BottomRadius = r, Height = 0.02f, RadialSegments = 16, Material = m },
                Position = at,
                Scale = new Vector3(squash, 1, 1),
            };
            switch (kind)
            {
                case EvidenceKind.Footprints:
                    var mud = Mat(new Color(0.2f, 0.15f, 0.1f));
                    for (int i = 0; i < 6; i++)
                        root.AddChild(Disc(0.13f, mud, new Vector3(i % 2 == 0 ? -0.14f : 0.14f, 0, i * 0.42f - 1.05f), 0.6f));
                    break;
                case EvidenceKind.Bloodstain:
                    var blood = Mat(new Color(0.6f, 0.05f, 0.05f));
                    root.AddChild(Disc(0.35f, blood, Vector3.Zero));
                    root.AddChild(Disc(0.12f, blood, new Vector3(0.45f, 0, 0.1f)));
                    root.AddChild(Disc(0.08f, blood, new Vector3(-0.2f, 0, 0.42f)));
                    break;
                default:
                    var pool = Mat(new Color(0.35f, 0.02f, 0.03f));
                    root.AddChild(Disc(0.95f, pool, new Vector3(0, 0.005f, 0), 1.3f));
                    root.AddChild(Disc(0.3f, pool, new Vector3(1.2f, 0.005f, 0.3f)));
                    break;
            }
            return root;
        }
    }
}
