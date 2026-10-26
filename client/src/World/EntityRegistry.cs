using System;
using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// Entity id → view node (PLAN §6.1). Spawns a view when a <c>transform</c> row arrives in a
    /// subscribed cell, feeds its snapshot buffer on updates, and despawns it when the row goes.
    /// Our own entity is drawn by the player controller, not here.
    /// </summary>
    public partial class EntityRegistry : Node3D
    {
        private readonly Session session;
        private readonly bool trace;
        private readonly Dictionary<ulong, RemoteView> views = new();
        /// <summary>Rows deleted this frame. A row that moves between two subscribed cells can arrive as a delete and an insert.</summary>
        private readonly HashSet<ulong> gone = new();

        public EntityRegistry(Session session, bool trace)
        {
            this.session = session;
            this.trace = trace;
            Name = "Entities";
        }

        public int Count => views.Count;

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Transform.OnInsert += (_, t) => Receive(t);
            db.Transform.OnUpdate += (_, _, t) => Receive(t);
            db.Transform.OnDelete += (_, t) => gone.Add(t.EntityId);
            db.CharacterPublic.OnInsert += (_, c) =>
            {
                foreach (var v in views.Values)
                    if (v.Kind == EntityKind.Character && v.RefId == c.CharId)
                        v.SetLabel(c.Name);
            };
        }

        private void Receive(Transform t)
        {
            gone.Remove(t.EntityId);
            if (t.EntityId == session.Me?.EntityId)
                return;
            if (!views.TryGetValue(t.EntityId, out var view))
            {
                var e = session.Conn!.Db.Entity.Id.Find(t.EntityId);
                view = new RemoteView(t.EntityId, e?.Kind ?? EntityKind.Character, e?.RefId ?? 0);
                views[t.EntityId] = view;
                AddChild(view);
                if (view.Kind == EntityKind.Character)
                    view.SetLabel(session.Conn.Db.CharacterPublic.CharId.Find(view.RefId)?.Name ?? $"#{t.EntityId}");
                session.Log($"sees entity {t.EntityId} ({view.Label}) at ({t.X:F1}, {t.Z:F1})");
            }
            view.Push(t, Clock.Now);
        }

        public override void _Process(double delta)
        {
            foreach (var id in gone)
                Despawn(id, "left view");
            gone.Clear();
            if (session.Me is Presence me && views.ContainsKey(me.EntityId))
                Despawn(me.EntityId, "is us");

            double now = Clock.Now;
            foreach (var v in views.Values)
            {
                RemoteView? parent = null;
                if (v.ParentId != 0 && !views.TryGetValue(v.ParentId, out parent))
                {
                    // On a ship we can't see: nothing to draw it relative to.
                    v.Visible = false;
                    continue;
                }
                v.Visible = true;
                v.Place(now, parent);
                if (trace)
                    session.Log(FormattableString.Invariant($"trace {v.Name} {now:F4} {v.GlobalPosition.X:F3} {v.GlobalPosition.Z:F3}"));
            }
        }

        private void Despawn(ulong id, string why)
        {
            if (views.Remove(id, out var view))
            {
                session.Log($"lost entity {id} ({view.Label}): {why}");
                view.QueueFree();
            }
        }
    }

    /// <summary>Values of <c>entity.kind</c> (server/world/src/session.rs).</summary>
    public static class EntityKind
    {
        public const byte Character = 1;
        public const byte Ship = 2;
    }

    /// <summary>Another player or a ship, drawn from its snapshot buffer.</summary>
    public partial class RemoteView : Node3D
    {
        /// <summary>How fast the drawn pose catches up with the buffer, per second.</summary>
        private const float SmoothRate = 15.0f;

        private readonly SnapshotBuffer buffer = new();
        private readonly Node3D body;
        private System.Numerics.Vector3? drawn;
        private float drawnYaw;
        private double lastPlaced;
        private readonly Label3D label;

        public RemoteView(ulong id, byte kind, ulong refId)
        {
            Name = $"Entity{id}";
            Kind = kind;
            RefId = refId;
            if (kind == EntityKind.Ship && refId < (ulong)GameContent.Hulls.Count)
            {
                var hull = GameContent.Hulls[(int)refId];
                body = Views.Ship(new Vector3(hull.DeckHalfWidthM * 2, hull.DeckHeightM + hull.DraftM, hull.DeckHalfLengthM * 2));
            }
            else
            {
                body = Views.Capsule(Views.ColorFor(id));
            }
            AddChild(body);
            label = new Label3D
            {
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                Position = new Vector3(0, 2.2f, 0),
                // The same size on screen at any distance, like a nameplate.
                FixedSize = true,
                FontSize = 28,
                PixelSize = 0.0012f,
                OutlineSize = 6,
                NoDepthTest = true,
            };
            AddChild(label);
            if (kind == EntityKind.Ship && refId < (ulong)GameContent.Hulls.Count)
            {
                label.Text = GameContent.Hulls[(int)refId].Id;
                label.Position = new Vector3(0, 6.0f, 0);
            }
        }

        public byte Kind { get; }
        public ulong RefId { get; }
        public ulong ParentId { get; private set; }
        public string Label => label.Text;

        public void SetLabel(string text) => label.Text = text;

        public void Push(Transform t, double now)
        {
            // Boarding or leaving a ship changes the frame the position is in.
            if (t.Parent != ParentId)
            {
                buffer.Clear();
                drawn = null;
            }
            ParentId = t.Parent;
            var pos = new System.Numerics.Vector3(t.X, t.Y, t.Z);
            buffer.Add(new Snapshot(now, pos, Sim.Yaw.FromU16(t.Yaw), t.Speed / 100.0f));
        }

        /// <summary>Draw at <paramref name="now"/>, in <paramref name="parent"/>'s frame when on a ship.</summary>
        public void Place(double now, RemoteView? parent)
        {
            if (buffer.Count == 0)
                return;
            var (target, targetYaw) = buffer.Sample(now);
            // Ease towards the buffer's answer, so the small steps when a late row arrives don't show.
            float k = 1.0f - Mathf.Exp(-SmoothRate * (float)(now - lastPlaced));
            if (drawn is not { } d || System.Numerics.Vector3.Distance(d, target) > SnapshotBuffer.SnapM)
                (drawn, drawnYaw) = (target, targetYaw);
            else
                (drawn, drawnYaw) = (System.Numerics.Vector3.Lerp(d, target, k), drawnYaw + Sim.Yaw.Delta(drawnYaw, targetYaw) * k);
            lastPlaced = now;
            var pos = drawn.Value;
            var local = new Godot.Transform3D(new Basis(Vector3.Up, drawnYaw), new Vector3(pos.X, pos.Y, pos.Z));
            GlobalTransform = parent is null ? local : parent.GlobalTransform * local;
        }
    }
}
