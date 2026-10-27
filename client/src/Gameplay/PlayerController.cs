using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.Sim;
using Mmo.World;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// The local player: predicted on this client and reported to the server as movement events
    /// (PLAN §4.2). It snaps to the server's position on entering and on each correction. The body's
    /// origin is at its feet, where the server measures positions. It walks on the same heightmap
    /// the server checks against, kept under it by <see cref="TerrainCollision"/>.
    ///
    /// On a ship (docs/design/ships.md) it keeps its position in the ship's frame. Each physics
    /// frame it's carried to where that puts it on the ship's predicted pose, then steps in world
    /// space against the hull, and the step is turned back into the ship's frame. The body's own
    /// velocity never includes the ship's, so stepping off leaves the ship behind (ADR 0014).
    /// </summary>
    public partial class PlayerController : CharacterBody3D
    {
        public const float Gravity = 15.0f;
        /// <summary>Apex about 0.8 m, well inside the server's 2 m jump allowance.</summary>
        public const float JumpSpeed = 5.0f;
        public const float Height = 1.8f;
        public const float Radius = 0.35f;
        /// <summary>A climb waits this long for the server's allowance before giving up.</summary>
        private const double ClimbWaitS = 1.0;

        private readonly Session session;
        private readonly ClientOptions options;
        private readonly TerrainCollision ground;
        private readonly EntityRegistry entities;
        private readonly MoveSender sender = new();
        private Node3D body = null!;
        /// <summary>The heading in the current frame: the world's, or the ship's.</summary>
        private float heading;
        private Vector3 local;
        private double enteredAt = double.NaN;
        private double climbUntil = double.NegativeInfinity;
        /// <summary>Physics frames to wait after a snap built new ground, so the engine has it before we fall.</summary>
        private int settle;
        /// <summary>The furthest the body stood from the heightmap's bilinear height, to check the collision lines up.</summary>
        private float worstGap;
        private int boardings, landings;
        private bool swimming;

        public PlayerController(Session session, ClientOptions options, TerrainCollision ground, EntityRegistry entities)
        {
            this.session = session;
            this.options = options;
            this.ground = ground;
            this.entities = entities;
            Name = "Player";
        }

        /// <summary>The camera's yaw; keyboard movement is relative to it.</summary>
        public float CameraYaw { get; set; }

        public int MovesSent => sender.Sent;

        /// <summary>The ship we're aboard, or 0.</summary>
        public ulong Parent { get; private set; }

        /// <summary>Our position in <see cref="Parent"/>'s frame, when aboard.</summary>
        public Vector3 Local => local;

        /// <summary>The yaw of the frame we're in: the ship's, or 0 in the world.</summary>
        public float FrameYaw => entities.Ship(Parent)?.Pose.Yaw ?? 0;

        /// <summary>Swimming, walking or running.</summary>
        public MoveMode Mode => swimming ? MoveMode.Swim : options.Walk ? MoveMode.Walk : MoveMode.Run;

        /// <summary>The helm and cannons; set by <see cref="Main"/>.</summary>
        public ShipControls? Controls { get; set; }

        /// <summary>Gathering from resource nodes; set by <see cref="Main"/>. F on land gathers before it climbs.</summary>
        public Gathering? Gathering { get; set; }

        /// <summary>Why the last climb didn't happen, for the HUD.</summary>
        public string? ClimbNote { get; private set; }

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
            // The ship's motion is ours to apply, not the engine's (ADR 0014).
            PlatformOnLeave = PlatformOnLeaveEnum.DoNothing;
            Visible = false;
            SetPhysicsProcess(false);
            session.Entered += Snap;
            session.Corrected += Snap;
            // Out of the world until we enter again, which snaps us to where the server saved us.
            session.Dropped += () => SetPhysicsProcess(false);
        }

        public override void _ExitTree() =>
            session.Log($"quitting: {sender.Sent} moves sent, {session.Me?.Corrections ?? 0} corrections, standing at most {worstGap:F2} m off the heightmap, boarded {boardings}x, went ashore or overboard {landings}x");

        private void Snap(Presence p)
        {
            Parent = p.Parent;
            var at = new Vector3(p.X, p.Y, p.Z);
            local = at;
            if (Parent == 0)
                GlobalPosition = at;
            else if (entities.Ship(Parent) is ShipView ship)
                GlobalPosition = ToGodot(ship.Pose.ToWorld(ToNumerics(at)));
            Velocity = Vector3.Zero;
            if (ground.Follow(GlobalPosition.X, GlobalPosition.Z))
                settle = 2;
            heading = Yaw.FromU16(p.Yaw);
            body.Rotation = new Vector3(0, FrameYaw + heading, 0);
            sender.Reset(p.LastSeq, State(0, true), Clock.Now);
            Visible = true;
            SetPhysicsProcess(true);
            if (double.IsNaN(enteredAt))
                enteredAt = Clock.Now;
        }

        private MoveState State(float speed, bool grounded) =>
            new(ToNumerics(Parent == 0 ? GlobalPosition : local), heading, speed, (byte)Mode, grounded, Parent);

        public override void _PhysicsProcess(double delta)
        {
            double now = Clock.Now;
            var ship = Parent == 0 ? null : entities.Ship(Parent);
            if (Parent != 0 && ship is null)
                return; // aboard a ship we can't see yet: nothing to stand on
            // Carried by the ship: where our place on deck is now.
            var carried = ship is null ? GlobalPosition : ToGodot(ship.Pose.ToWorld(ToNumerics(local)));
            GlobalPosition = carried;
            body.Rotation = new Vector3(0, FrameYaw + heading, 0);
            // The window only moves once we're 8 m into a cell it already covers, so the ground under
            // us is always there.
            ground.Follow(GlobalPosition.X, GlobalPosition.Z);
            if (settle > 0)
            {
                settle--;
                return;
            }
            float dt = (float)delta;
            swimming = Parent == 0 && Swimming.Swims(swimming, ground.Map.Height(GlobalPosition.X, GlobalPosition.Z), GlobalPosition.Y);
            var order = options.Autopilot?.Drive(View(now, ship)) ?? new PilotOrder(System.Numerics.Vector2.Zero);
            bool interact = options.Autopilot is null && Input.IsActionJustPressed("interact");
            if (interact && Parent == 0 && Gathering?.Interact() == true)
                interact = false;
            // F climbs aboard from the water, and works the helm or a cannon once aboard; not both at once.
            bool interactAboard = interact && Parent != 0;
            var wish = Controls?.Manning is not null ? Vector2.Zero : Wish(order, ship);
            float maxSpeed = Mode.Data().MaxSpeedMps;
            var v = Velocity;
            v.X = wish.X * maxSpeed;
            v.Z = wish.Y * maxSpeed;
            bool jumped = false;
            if (swimming)
            {
                v.Y = Swimming.VerticalSpeed(GlobalPosition.Y);
            }
            else if (IsOnFloor())
            {
                bool jump = options.Autopilot is null ? Controls?.Manning is null && Input.IsActionJustPressed("jump") : order.Jump;
                if (jump)
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
            if (options.TestCheat && now - enteredAt > 3.0)
            {
                options.TestCheat = false;
                GlobalPosition += new Vector3(0, 0, -30);
                session.Log($"test cheat: jumped to ({GlobalPosition.X:F1}, {GlobalPosition.Z:F1})");
            }

            if (Parent == 0 && IsOnFloor() && !swimming)
                worstGap = Mathf.Max(worstGap, Mathf.Abs(GlobalPosition.Y - ground.Map.Height(GlobalPosition.X, GlobalPosition.Z)));

            float speed = new Vector2(Velocity.X, Velocity.Z).Length();
            if (speed > MoveSender.StillMps)
                heading = Yaw.Of(Velocity.X, Velocity.Z) - FrameYaw;

            if ((interact && Parent == 0) || order.Action == PilotAction.Climb)
                climbUntil = now + ClimbWaitS;
            if (Parent == 0 && now < climbUntil && TryClimb(now, maxSpeed))
                climbUntil = double.NegativeInfinity;
            else
                Reframe(ship, carried);

            // A swimmer is held at the surface, not falling, so it counts as grounded.
            var state = State(speed, swimming || (IsOnFloor() && !jumped));
            if (sender.Update(now, state, jumped) is MoveMsg m)
                session.SendMove(m);
            // Every frame, not only when we move: a ship carries us across cells while we stand still.
            session.Near?.Update(GlobalPosition.X, GlobalPosition.Z);
            session.Static?.Update(GlobalPosition.X, GlobalPosition.Z);
            Controls?.Step(now, order, interactAboard);
        }

        /// <summary>
        /// After a step: carry the step into the ship's frame, or change frame if we boarded,
        /// went over the side or jumped across (<see cref="Boarding.FrameAfter"/>).
        /// </summary>
        private void Reframe(ShipView? ship, Vector3 carried)
        {
            ulong next = Boarding.FrameAfter(Parent, ToNumerics(GlobalPosition), IsOnFloor(), entities.Decks);
            if (next == Parent)
            {
                // Only the step itself is turned into the ship's frame, so standing still on deck
                // doesn't wander with float error far from the origin.
                if (ship is not null)
                    local += ToGodot(ship.Pose.Turn(ToNumerics(GlobalPosition - carried), toWorld: false));
                return;
            }
            float worldHeading = heading + FrameYaw;
            var was = Parent;
            Parent = next;
            heading = worldHeading - FrameYaw;
            if (entities.Ship(next) is ShipView aboard)
            {
                local = ToGodot(aboard.Pose.ToLocal(ToNumerics(GlobalPosition)));
                boardings++;
                session.Log($"boarded ship {next} at ({local.X:F1}, {local.Y:F2}, {local.Z:F1}) on deck");
            }
            if (next == 0 || was != 0)
            {
                landings++;
                session.Log($"left ship {was} at ({GlobalPosition.X:F1}, {GlobalPosition.Y:F1}, {GlobalPosition.Z:F1}), moving {Velocity.Length():F1} m/s of our own");
            }
        }

        /// <summary>
        /// Dev builds: move to (x, z) on the ground at once with the GM teleport, which the server
        /// applies before any move we send after it, and carry on from there.
        /// </summary>
        public void DevTeleport(float x, float z)
        {
            session.Conn?.Reducers.DevTeleport(x, z);
            Parent = 0;
            swimming = false;
            GlobalPosition = new Vector3(x, ground.Map.Height(x, z), z);
            Velocity = Vector3.Zero;
            if (ground.Follow(x, z))
                settle = 2;
            sender.Reset(0, State(0, true), Clock.Now);
            session.Log($"dev teleport to ({x:F1}, {z:F1})");
        }

        /// <summary>Climb aboard the ship alongside, if the server would allow the jump to its deck now.</summary>
        private bool TryClimb(double now, float maxSpeed)
        {
            Deck? best = null;
            System.Numerics.Vector3 target = default;
            float bestD = float.MaxValue;
            var here = ToNumerics(GlobalPosition);
            foreach (var d in entities.Decks)
            {
                if (Boarding.ClimbTarget(d, here) is not System.Numerics.Vector3 t)
                    continue;
                float dist = System.Numerics.Vector3.Distance(d.Pose.ToWorld(t), here);
                if (dist < bestD)
                    (best, target, bestD) = (d, t, dist);
            }
            if (best is not Deck deck)
            {
                ClimbNote = "nothing to climb here";
                return false;
            }
            var to = deck.Pose.ToWorld(target);
            if (!Boarding.WithinAllowance(SentWorld(), to, (float)(now - sender.LastAt), maxSpeed, deck.Speed))
            {
                ClimbNote = "climbing…";
                return false;
            }
            ClimbNote = null;
            swimming = false;
            float worldHeading = heading;
            GlobalPosition = ToGodot(to);
            Velocity = Vector3.Zero;
            Parent = deck.Ship;
            local = ToGodot(target);
            heading = worldHeading - deck.Pose.Yaw;
            boardings++;
            session.Log($"climbed aboard ship {deck.Ship} at ({target.X:F1}, {target.Z:F1}) on deck");
            return true;
        }

        /// <summary>Where the server has us, in world coordinates: the last position we sent.</summary>
        private System.Numerics.Vector3 SentWorld()
        {
            var last = sender.Last;
            if (last.Parent == 0)
                return last.Pos;
            return entities.Ship(last.Parent) is ShipView s ? s.Pose.ToWorld(last.Pos) : ToNumerics(GlobalPosition);
        }

        private PilotView View(double now, ShipView? ship) =>
            new(now, new System.Numerics.Vector2(GlobalPosition.X, GlobalPosition.Z), Parent, ToNumerics(local), entities.Decks,
                Controls?.Manning, Controls?.Loaded ?? false);

        /// <summary>Where the player wants to go: a horizontal (x, z) world direction, at most unit length.</summary>
        private Vector2 Wish(PilotOrder order, ShipView? ship)
        {
            if (options.Autopilot is not null)
            {
                var dir = new System.Numerics.Vector3(order.Dir.X, 0, order.Dir.Y);
                if (order.OnDeck && ship is not null)
                    dir = ship.Pose.Turn(dir, toWorld: true);
                return new Vector2(dir.X, dir.Z);
            }
            var input = Input.GetVector("left", "right", "forward", "back");
            // Rotate the input from camera space into the world: forward is the camera's -z.
            return input.Rotated(-CameraYaw).LimitLength(1.0f);
        }

        public static System.Numerics.Vector3 ToNumerics(Vector3 v) => new(v.X, v.Y, v.Z);

        public static Vector3 ToGodot(System.Numerics.Vector3 v) => new(v.X, v.Y, v.Z);
    }
}
