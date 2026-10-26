using Godot;
using Mmo.Content;
using Mmo.Net;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// The local player: predicted on this client and reported to the server as movement events
    /// (PLAN §4.2). It snaps to the server's position on entering and on each correction. The body's
    /// origin is at its feet, where the server measures positions.
    /// </summary>
    public partial class PlayerController : CharacterBody3D
    {
        public const float Gravity = 15.0f;
        /// <summary>Apex about 0.8 m, well inside the server's 2 m jump allowance.</summary>
        public const float JumpSpeed = 5.0f;
        public const float Height = 1.8f;
        public const float Radius = 0.35f;

        private readonly Session session;
        private readonly ClientOptions options;
        private readonly MoveSender sender = new();
        private Node3D body = null!;
        private float heading;
        private double enteredAt = double.NaN;

        public PlayerController(Session session, ClientOptions options)
        {
            this.session = session;
            this.options = options;
            Name = "Player";
        }

        /// <summary>The camera's yaw; keyboard movement is relative to it.</summary>
        public float CameraYaw { get; set; }

        public int MovesSent => sender.Sent;

        public override void _Ready()
        {
            AddChild(new CollisionShape3D
            {
                Shape = new CapsuleShape3D { Radius = Radius, Height = Height },
                Position = new Vector3(0, Height / 2, 0),
            });
            body = Views.Capsule(new Color(0.95f, 0.75f, 0.25f));
            AddChild(body);
            FloorSnapLength = 0.3f;
            Visible = false;
            SetPhysicsProcess(false);
            session.Entered += Snap;
            session.Corrected += Snap;
        }

        public override void _ExitTree() =>
            session.Log($"quitting: {sender.Sent} moves sent, {session.Me?.Corrections ?? 0} corrections");

        private void Snap(Presence p)
        {
            GlobalPosition = new Vector3(p.X, p.Y, p.Z);
            Velocity = Vector3.Zero;
            heading = Sim.Yaw.FromU16(p.Yaw);
            body.Rotation = new Vector3(0, heading, 0);
            sender.Reset(p.LastSeq, State(0, true), Clock.Now);
            Visible = true;
            SetPhysicsProcess(true);
            if (double.IsNaN(enteredAt))
                enteredAt = Clock.Now;
        }

        private MoveState State(float speed, bool grounded) =>
            new(new System.Numerics.Vector3(GlobalPosition.X, GlobalPosition.Y, GlobalPosition.Z), heading, speed, (byte)(options.Walk ? MoveMode.Walk : MoveMode.Run), grounded);

        public override void _PhysicsProcess(double delta)
        {
            float dt = (float)delta;
            var wish = Wish();
            float maxSpeed = (options.Walk ? MoveMode.Walk : MoveMode.Run).Data().MaxSpeedMps;
            var v = Velocity;
            v.X = wish.X * maxSpeed;
            v.Z = wish.Y * maxSpeed;
            bool jumped = false;
            if (IsOnFloor())
            {
                if (options.Autopilot is null && Input.IsActionJustPressed("jump"))
                {
                    v.Y = JumpSpeed;
                    jumped = true;
                }
            }
            else
            {
                v.Y -= Gravity * dt;
            }
            Velocity = v;
            MoveAndSlide();
            if (options.TestCheat && Clock.Now - enteredAt > 3.0)
            {
                options.TestCheat = false;
                GlobalPosition += new Vector3(0, 0, -30);
                session.Log($"test cheat: jumped to ({GlobalPosition.X:F1}, {GlobalPosition.Z:F1})");
            }

            float speed = new Vector2(Velocity.X, Velocity.Z).Length();
            if (speed > MoveSender.StillMps)
            {
                heading = Sim.Yaw.Of(Velocity.X, Velocity.Z);
                body.Rotation = new Vector3(0, heading, 0);
            }
            var state = State(speed, IsOnFloor() && !jumped);
            if (sender.Update(Clock.Now, state, jumped) is MoveMsg m)
            {
                session.SendMove(m);
                session.Near?.Update(m.Pos.X, m.Pos.Z);
            }
        }

        /// <summary>Where the player wants to go: a horizontal (x, z) direction, at most unit length.</summary>
        private Vector2 Wish()
        {
            if (options.Autopilot is Autopilot pilot)
            {
                var steer = pilot.Steer(new System.Numerics.Vector2(GlobalPosition.X, GlobalPosition.Z));
                return new Vector2(steer.X, steer.Y);
            }
            var input = Input.GetVector("left", "right", "forward", "back");
            // Rotate the input from camera space into the world: forward is the camera's -z.
            return input.Rotated(-CameraYaw).LimitLength(1.0f);
        }
    }
}
