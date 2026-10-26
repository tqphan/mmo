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
        private ShipInput? ownInput;
        private double ownInputAt;

        public ShipView(ulong id, HullData hull)
        {
            Id = id;
            Hull = hull;
            Predictor = new ShipPredictor(hull);
            Name = $"Ship{id}";
            var size = new Vector3(hull.DeckHalfWidthM * 2, hull.DeckHeightM + hull.DraftM, hull.DeckHalfLengthM * 2);
            AddChild(Views.Ship(size));
            var body = new StaticBody3D { Name = "HullBody" };
            body.AddChild(new CollisionShape3D { Shape = new BoxShape3D { Size = size }, Position = new Vector3(0, -size.Y / 2, 0) });
            AddChild(body);
            AddChild(Views.Helm(new Vector3(hull.HelmX, 0, hull.HelmZ)));
            foreach (var m in hull.Mounts)
                AddChild(Views.Cannon(new Vector3(m.X, 0, m.Z), Mathf.DegToRad(m.FacingDeg)));
            label = new Label3D
            {
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                Position = new Vector3(0, 6.0f, 0),
                FixedSize = true,
                FontSize = 28,
                PixelSize = 0.0012f,
                OutlineSize = 6,
                NoDepthTest = true,
                Text = $"{hull.Id} #{id}",
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
            Predictor.Row(now, t.X, t.Z, Yaw.FromU16(t.Yaw), t.Speed / 100.0f);

        public void SetVehicle(Vehicle v, double now)
        {
            Vehicle = v;
            var input = new ShipInput(v.Throttle, v.Rudder);
            if (ownInput is ShipInput own && (input == own || now - ownInputAt > OwnInputS))
                ownInput = null;
            if (ownInput is null)
                Predictor.SetInput(now, input);
            label.Text = $"{Hull.Id} #{Id}  {v.Hp:N0}/{Hull.Hp:N0}";
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
            GlobalTransform = new Transform3D(new Basis(Vector3.Up, Pose.Yaw), new Vector3(Pose.X, Pose.Y, Pose.Z));
        }
    }
}
