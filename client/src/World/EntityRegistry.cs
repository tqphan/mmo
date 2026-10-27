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
    /// Our own entity is drawn by the player controller, not here. Ships get a <see cref="ShipView"/>,
    /// fed by <c>transform</c> and <c>vehicle</c>, and are placed every physics frame before the
    /// player steps, since the player may be standing on one.
    /// </summary>
    public partial class EntityRegistry : Node3D
    {
        private readonly Session session;
        private readonly bool trace;
        private readonly Dictionary<ulong, RemoteView> views = new();
        private readonly Dictionary<ulong, ShipView> ships = new();
        private readonly List<Gameplay.Deck> decks = new();
        /// <summary>Rows deleted this frame. A row that moves between two subscribed cells can arrive as a delete and an insert.</summary>
        private readonly HashSet<ulong> gone = new();

        public EntityRegistry(Session session, bool trace)
        {
            this.session = session;
            this.trace = trace;
            Name = "Entities";
            // Before the player, which runs at the default priority.
            ProcessPhysicsPriority = -1;
        }

        public int Count => views.Count + ships.Count;

        public ShipView? Ship(ulong id) => ships.GetValueOrDefault(id);

        /// <summary>Another character or a dummy, as drawn.</summary>
        public RemoteView? View(ulong id) => views.GetValueOrDefault(id);

        public IEnumerable<RemoteView> Views => views.Values;

        /// <summary>Every ship in view, where it was placed this physics frame.</summary>
        public IReadOnlyList<Gameplay.Deck> Decks => decks;

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Transform.OnInsert += (_, t) => Receive(t);
            db.Transform.OnUpdate += (_, _, t) => Receive(t);
            db.Transform.OnDelete += (_, t) => gone.Add(t.EntityId);
            db.Vehicle.OnInsert += (_, v) => ships.GetValueOrDefault(v.EntityId)?.SetVehicle(v, Clock.Now);
            db.Vehicle.OnUpdate += (_, _, v) => ships.GetValueOrDefault(v.EntityId)?.SetVehicle(v, Clock.Now);
            db.Entity.OnInsert += (_, e) => views.GetValueOrDefault(e.Id)?.SetPack(e.Pack);
            db.Entity.OnUpdate += (_, _, e) => views.GetValueOrDefault(e.Id)?.SetPack(e.Pack);
            db.Vitals.OnInsert += (_, v) => views.GetValueOrDefault(v.EntityId)?.SetDead(v.DeadAt is not null);
            db.Vitals.OnUpdate += (_, _, v) => views.GetValueOrDefault(v.EntityId)?.SetDead(v.DeadAt is not null);
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
            if (ships.TryGetValue(t.EntityId, out var ship))
            {
                ship.Push(t, Clock.Now);
                return;
            }
            if (!views.TryGetValue(t.EntityId, out var view))
            {
                var e = session.Conn!.Db.Entity.Id.Find(t.EntityId);
                if (e?.Kind == EntityKind.Ship && e.RefId < (ulong)GameContent.Hulls.Count)
                {
                    ship = new ShipView(t.EntityId, GameContent.Hulls[(int)e.RefId]);
                    ships[t.EntityId] = ship;
                    AddChild(ship);
                    ship.Push(t, Clock.Now);
                    if (session.Conn.Db.Vehicle.EntityId.Find(t.EntityId) is Vehicle v)
                        ship.SetVehicle(v, Clock.Now);
                    ship.Place(Clock.Now);
                    session.Log($"sees ship {t.EntityId} ({ship.Hull.Id}) at ({t.X:F1}, {t.Z:F1})");
                    return;
                }
                view = new RemoteView(t.EntityId, e?.Kind ?? EntityKind.Character, e?.RefId ?? 0);
                views[t.EntityId] = view;
                AddChild(view);
                if (view.Kind == EntityKind.Character)
                    view.SetLabel(session.Conn.Db.CharacterPublic.CharId.Find(view.RefId)?.Name ?? $"#{t.EntityId}");
                else if (view.Kind == EntityKind.Dummy)
                    view.SetLabel("Straw Dummy");
                view.SetPack(e?.Pack ?? 0);
                view.SetDead(session.Conn.Db.Vitals.EntityId.Find(t.EntityId)?.DeadAt is not null);
                session.Log($"sees entity {t.EntityId} ({view.Label}) at ({t.X:F1}, {t.Z:F1})");
            }
            view.Push(t, Clock.Now);
        }

        public override void _PhysicsProcess(double delta)
        {
            double now = Clock.Now;
            decks.Clear();
            foreach (var s in ships.Values)
            {
                s.Place(now);
                if (s.Predictor.HasRow)
                    decks.Add(s.Deck);
            }
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
                ShipView? parent = null;
                if (v.ParentId != 0 && !ships.TryGetValue(v.ParentId, out parent))
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
            if (ships.Remove(id, out var ship))
            {
                session.Log($"lost ship {id}: {why}");
                ship.QueueFree();
                decks.RemoveAll(d => d.Ship == id);
            }
        }
    }

    /// <summary>Values of <c>entity.kind</c> (server/world/src/session.rs).</summary>
    public static class EntityKind
    {
        public const byte Character = 1;
        public const byte Ship = 2;
        /// <summary>A dev Straw Dummy (docs/design/combat.md).</summary>
        public const byte Dummy = 3;
    }

    /// <summary>Another player, drawn from its snapshot buffer, in its ship's frame when it's aboard one.</summary>
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
        private ushort pack;

        public RemoteView(ulong id, byte kind, ulong refId)
        {
            Name = $"Entity{id}";
            EntityId = id;
            Kind = kind;
            RefId = refId;
            body = kind == EntityKind.Dummy ? Views.Dummy() : Views.Capsule(Views.ColorFor(id));
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
        }

        public ulong EntityId { get; }
        public byte Kind { get; }
        public ulong RefId { get; }
        public ulong ParentId { get; private set; }
        public string Label => label.Text;

        public void SetLabel(string text) => label.Text = text;

        /// <summary>Lying down while dead (docs/design/combat.md).</summary>
        public void SetDead(bool value)
        {
            Dead = value;
            Views.Lie(body, 0, value);
        }

        public bool Dead { get; private set; }

        /// <summary>Show the trade pack on its back, or none (docs/design/trade.md).</summary>
        public void SetPack(ushort def) => Views.SetBackPack(body, def, ref pack);

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
        public void Place(double now, Node3D? parent)
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
