using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using SpacetimeDB.Types;

namespace Mmo.World
{
    /// <summary>
    /// A ship: a hull-sized box that the local player can stand on and bump into, with its helm and
    /// cannons marked, drawn at the present by a <see cref="ShipPredictor"/> (ADR 0014). The hull is
    /// a static body moved every physics frame, before the player steps, so the deck is where the
    /// player's frame says it is.
    /// </summary>
    public partial class ShipView : Node3D
    {
        /// <summary>Our own helm input wins over <c>vehicle</c> rows for this long, until the server echoes it.</summary>
        private const double OwnInputS = 1.0;

        private readonly Label3D label;
        private readonly Node3D[] slots;
        private readonly ushort[] shownCargo;
        private ShipInput? ownInput;
        private double ownInputAt;
        /// <summary>When we first saw it at 0 HP, sinking (docs/design/vehicles.md §2B).</summary>
        private double? sinkingSince;

        /// <param name="ground">The ground, so the predictor stands the vehicle where the server would (docs/design/vehicles.md).</param>
        public ShipView(ulong id, HullData hull, System.Func<float, float, Ground?>? ground = null)
        {
            Id = id;
            Hull = hull;
            Predictor = new ShipPredictor(hull, ground);
            Name = $"Ship{id}";
            // A cart's bed is a thin box on wheels; a boat's hull reaches down past the waterline.
            var size = hull.Land
                ? new Vector3(hull.DeckHalfWidthM * 2, 0.25f, hull.DeckHalfLengthM * 2)
                : new Vector3(hull.DeckHalfWidthM * 2, hull.DeckHeightM + hull.DraftM, hull.DeckHalfLengthM * 2);
            AddChild(Views.Ship(size));
            if (hull.Land)
                AddChild(Views.Wheels(hull.DeckHalfWidthM, hull.DeckHalfLengthM, hull.DeckHeightM));
            slots = new Node3D[hull.Slots.Count];
            shownCargo = new ushort[hull.Slots.Count];
            for (int i = 0; i < slots.Length; i++)
            {
                slots[i] = Views.Slot(new Vector3(hull.Slots[i].X, 0, hull.Slots[i].Z));
                AddChild(slots[i]);
            }
            var body = new StaticBody3D { Name = "HullBody" };
            body.AddChild(new CollisionShape3D { Shape = new BoxShape3D { Size = size }, Position = new Vector3(0, -size.Y / 2, 0) });
            AddChild(body);
            AddChild(Views.Helm(new Vector3(hull.HelmX, 0, hull.HelmZ)));
            foreach (var m in hull.Mounts)
                AddChild(Views.Cannon(new Vector3(m.X, 0, m.Z), Mathf.DegToRad(m.FacingDeg)));
            label = new Label3D
            {
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                Position = new Vector3(0, hull.Land ? 2.5f : 6.0f, 0),
                FixedSize = true,
                FontSize = 28,
                PixelSize = 0.0012f,
                OutlineSize = 6,
                NoDepthTest = true,
                Text = $"{hull.Name} #{id}",
            };
            AddChild(label);
        }

        public ulong Id { get; }
        public HullData Hull { get; }
        public ShipPredictor Predictor { get; }
        public Vehicle? Vehicle { get; private set; }
        /// <summary>The pose the ship was last placed at.</summary>
        public ShipPose Pose { get; private set; }
        public float Speed { get; private set; }

        public Deck Deck => new(Id, Hull, Pose, Speed);

        public void Push(Transform t, double now) =>
            Predictor.Row(now, t.X, t.Z, Yaw.FromU16(t.Yaw), t.Speed / 100.0f, t.Y);

        public void SetVehicle(Vehicle v, double now)
        {
            Vehicle = v;
            var input = new ShipInput(v.Throttle, v.Rudder);
            if (ownInput is ShipInput own && (input == own || now - ownInputAt > OwnInputS))
                ownInput = null;
            if (ownInput is null)
                Predictor.SetInput(now, input);
            label.Text = v.Hp > 0 ? $"{Hull.Name} #{Id}  {v.Hp:N0}/{Hull.Hp:N0}" : $"{Hull.Name} #{Id}  {(Hull.Land ? "broken down" : "sinking")}";
            if (v.Hp <= 0)
                sinkingSince ??= now;
            for (int i = 0; i < slots.Length; i++)
            {
                ushort def = i < v.Cargo.Count ? v.Cargo[i] : (ushort)0;
                if (def == shownCargo[i])
                    continue;
                shownCargo[i] = def;
                if (slots[i].GetNodeOrNull<Node3D>("Pack") is { } old)
                {
                    slots[i].RemoveChild(old);
                    old.QueueFree();
                }
                if (def != 0)
                {
                    var pack = Views.Pack(def);
                    pack.Position = new Vector3(0, 0.08f, 0);
                    slots[i].AddChild(pack);
                }
            }
        }

        /// <summary>Our own helm's input: predict it at once rather than waiting for the server.</summary>
        public void SetOwnInput(ShipInput input, double now)
        {
            ownInput = input;
            ownInputAt = now;
            Predictor.SetInput(now, input);
        }

        /// <summary>Move to the predicted pose at <paramref name="now"/>.</summary>
        public void Place(double now)
        {
            if (!Predictor.HasRow)
                return;
            Pose = Predictor.Pose(now);
            Speed = Predictor.Speed(now);
            // A sinking ship settles until its deck is awash; a broken cart slumps a little. Anyone
            // aboard rides down with it, in its frame.
            if (sinkingSince is double since)
            {
                float k = Mathf.Clamp((float)(now - since) / Mathf.Max(1, GameContent.Vehicles.SinkS), 0, 1);
                float depth = Hull.Land ? 0.3f : Hull.DeckHeightM + 0.4f;
                Pose = Pose with { Y = Pose.Y - k * depth };
            }
            GlobalTransform = new Transform3D(new Basis(Vector3.Up, Pose.Yaw), new Vector3(Pose.X, Pose.Y, Pose.Z));
        }
    }
}
