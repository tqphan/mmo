using System;
using System.Collections.Generic;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.Sim;
using Mmo.World;

namespace Mmo.Gameplay
{
    /// <summary>
    /// The helm and the cannons (docs/design/ships.md). Aboard, F at the helm takes it, if it's
    /// free, and F at a cannon mans it. At the helm, W and S step the throttle by a quarter and A
    /// and D hold the rudder over. At a cannon, A and D aim it within its arc, W and S raise and
    /// lower it, and Space fires. A preview shows where the shot will land. F again lets go.
    /// While at either, the player doesn't walk.
    ///
    /// The server holds the helm (<c>vehicle.helm</c>) and decides each shot. We predict our own
    /// ship with our input at once (<see cref="ShipView.SetOwnInput"/>), and draw our own shots
    /// along the flight the server computes. Other players' shots aren't drawn yet.
    ///
    /// Dev builds: F9 puts a clipper on the water 30 m ahead of the camera, F10 removes the nearest
    /// ship, and <c>--spawn-ship X,Z,YAW</c> replaces any ship near (X, Z) with a fresh one once
    /// we're close enough to see it.
    /// </summary>
    public partial class ShipControls : Node3D
    {
        public const sbyte ThrottleStep = 25;
        public const float AimRate = 0.5f;
        public const float PitchRate = 0.2f;
        /// <summary>Where the cannon starts, and where the autopilot keeps it: 5° up, straight out.</summary>
        public static readonly float DefaultPitch = Mathf.DegToRad(5.0f);
        public const float SpawnAheadM = 30.0f;
        public const float SpawnClearM = 30.0f;

        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly HeightMap map;
        private readonly ClientOptions options;
        private readonly MeshInstance3D marker;
        private readonly MeshInstance3D arc;
        private readonly ImmediateMesh arcMesh = new();
        private readonly List<Shell> shells = new();
        private readonly Dictionary<(ulong, byte), double> readyAt = new();
        private byte? cannon;
        private float aimYaw, aimPitch = DefaultPitch;
        private sbyte throttle, rudder;
        private bool spawned;
        /// <summary>How far the ships we were aboard turned, for the logs.</summary>
        private float turned;
        private float lastYaw = float.NaN;
        private double lastReport;

        private sealed class Shell
        {
            public System.Numerics.Vector3 From;
            public float Yaw, Pitch, Speed, Secs;
            public double FiredAt;
            public MeshInstance3D Ball = null!;
            public MeshInstance3D? Splash;
        }

        public ShipControls(Session session, PlayerController player, EntityRegistry entities, HeightMap map, ClientOptions options)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.map = map;
            this.options = options;
            Name = "ShipControls";
            var red = new StandardMaterial3D
            {
                AlbedoColor = new Color(1.0f, 0.25f, 0.15f, 0.6f),
                Transparency = BaseMaterial3D.TransparencyEnum.Alpha,
                ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded,
                NoDepthTest = true,
                // After the sea, which is transparent too and sorts closer, since it follows the camera.
                RenderPriority = (int)Material.RenderPriorityMax,
            };
            marker = new MeshInstance3D { Mesh = new TorusMesh { InnerRadius = 1.6f, OuterRadius = 2.0f, Material = red }, Visible = false };
            arc = new MeshInstance3D { Mesh = arcMesh, MaterialOverride = red, Visible = false };
            AddChild(marker);
            AddChild(arc);
        }

        /// <summary>The helm, when the server says we hold it, or the cannon we're manning.</summary>
        public Station? Manning
        {
            get
            {
                if (AtHelm)
                    return new Station(StationKind.Helm, 0);
                return cannon is byte m ? new Station(StationKind.Cannon, m) : null;
            }
        }

        /// <summary>The cannon we're manning can fire.</summary>
        public bool Loaded => cannon is byte m && Clock.Now >= readyAt.GetValueOrDefault((player.Parent, m));

        public int ShotsFired { get; private set; }

        /// <summary>The world yaw the manned cannon would fire along, for the camera to look along.</summary>
        public float? AimWorldYaw =>
            Aboard is { } ship && cannon is byte m ? Ship.Muzzle(ship.Hull, ship.Pose, ship.Hull.Mounts[m], aimYaw).Yaw : null;

        /// <summary>What the HUD says about ships.</summary>
        public string Prompt { get; private set; } = "";

        private ShipView? Aboard => player.Parent == 0 ? null : entities.Ship(player.Parent);

        private bool AtHelm => Aboard?.Vehicle is { } v && session.Me is { } me && v.Helm == me.EntityId;

        /// <summary>Called by the player each physics frame after it moves.</summary>
        public void Step(double now, PilotOrder order, bool interact)
        {
            float dt = (float)GetPhysicsProcessDeltaTime();
            DevSpawn();
            var ship = Aboard;
            if (ship is null)
            {
                cannon = null;
                lastYaw = float.NaN;
                Prompt = ClimbPrompt();
                return;
            }
            Report(ship, now);
            var station = Boarding.StationAt(ship.Hull, PlayerController.ToNumerics(player.Local));
            if (interact || order.Action == PilotAction.Use)
            {
                bool keyboard = options.Autopilot is null;
                if (AtHelm && keyboard)
                    session.Conn!.Reducers.LeaveHelm();
                else if (cannon is not null && keyboard)
                    cannon = null;
                else if (station is { Kind: StationKind.Helm } && !AtHelm)
                    session.Conn!.Reducers.TakeHelm();
                else if (station is { Kind: StationKind.Cannon } c && cannon is null)
                    (cannon, aimYaw, aimPitch) = (c.Mount, 0, DefaultPitch);
            }
            if (AtHelm)
            {
                cannon = null;
                Steer(ship, now, order);
            }
            else
            {
                (throttle, rudder) = (ship.Vehicle?.Throttle ?? 0, ship.Vehicle?.Rudder ?? 0);
            }
            if (cannon is byte m)
                Gun(ship, m, now, dt, order);
            Prompt = Describe(ship, station);
        }

        public override void _ExitTree() =>
            session.Log($"ships: turned {Mathf.RadToDeg(turned):F0}° aboard, fired {ShotsFired} shots");

        /// <summary>Every 25 s aboard (not a whole turn of the clipper's 30 s circle), where the ship is and how far it has turned.</summary>
        private void Report(ShipView ship, double now)
        {
            if (!float.IsNaN(lastYaw))
                turned += Mathf.Abs(Yaw.Delta(lastYaw, ship.Pose.Yaw));
            lastYaw = ship.Pose.Yaw;
            if (now - lastReport < 25)
                return;
            lastReport = now;
            var p = ship.Pose;
            session.Log($"aboard ship {ship.Id} at ({p.X:F1}, {p.Z:F1}), {ship.Speed:F1} m/s, heading {Mathf.RadToDeg(p.Yaw):F0}°, turned {Mathf.RadToDeg(turned):F0}°; on deck at ({player.Local.X:F1}, {player.Local.Y:F2}, {player.Local.Z:F1}); {session.Me?.Corrections ?? 0} corrections");
        }

        private void Steer(ShipView ship, double now, PilotOrder order)
        {
            sbyte t = throttle, r = rudder;
            if (options.Autopilot is null)
            {
                if (Input.IsActionJustPressed("forward"))
                    t = (sbyte)Math.Min(Ship.InputMax, t + ThrottleStep);
                if (Input.IsActionJustPressed("back"))
                    t = (sbyte)Math.Max(-Ship.InputMax, t - ThrottleStep);
                // Positive rudder is to starboard (right).
                r = (sbyte)(((Input.IsActionPressed("right") ? 1 : 0) - (Input.IsActionPressed("left") ? 1 : 0)) * Ship.InputMax);
            }
            else if (order.Helm is ShipInput want)
            {
                (t, r) = (want.Throttle, want.Rudder);
            }
            if ((t, r) == (throttle, rudder))
                return;
            session.Conn!.Reducers.Helm(t, r);
            ship.SetOwnInput(new ShipInput(t, r), now);
            session.Log($"helm: throttle {t}%, rudder {r}%");
            (throttle, rudder) = (t, r);
        }

        private void Gun(ShipView ship, byte m, double now, float dt, PilotOrder order)
        {
            var c = ship.Hull.Cannon;
            bool fire;
            if (options.Autopilot is null)
            {
                aimYaw += ((Input.IsActionPressed("left") ? 1 : 0) - (Input.IsActionPressed("right") ? 1 : 0)) * AimRate * dt;
                aimPitch += ((Input.IsActionPressed("forward") ? 1 : 0) - (Input.IsActionPressed("back") ? 1 : 0)) * PitchRate * dt;
                fire = Input.IsActionJustPressed("jump");
            }
            else
            {
                fire = order.Action == PilotAction.Fire;
            }
            aimYaw = Mathf.Clamp(aimYaw, -Mathf.DegToRad(c.ArcDeg), Mathf.DegToRad(c.ArcDeg));
            aimPitch = Mathf.Clamp(aimPitch, Mathf.DegToRad(c.MinPitchDeg), Mathf.DegToRad(c.MaxPitchDeg));
            if (!fire || !Loaded || !Ship.AimOk(c, aimYaw, aimPitch))
                return;
            var (from, yaw) = Ship.Muzzle(ship.Hull, ship.Pose, ship.Hull.Mounts[m], aimYaw);
            if (Ship.CannonFlight(from, yaw, aimPitch, c.MuzzleSpeedMps) is not Flight f)
                return;
            session.Conn!.Reducers.Fire(m, aimYaw, aimPitch);
            readyAt[(ship.Id, m)] = now + c.ReloadS;
            ShotsFired++;
            session.Log($"fired cannon {m} of ship {ship.Id}: lands in {f.Secs:F2} s at ({f.Landing.X:F1}, {f.Landing.Z:F1})");
            var ball = new MeshInstance3D { Mesh = new SphereMesh { Radius = 0.18f, Height = 0.36f, Material = new StandardMaterial3D { AlbedoColor = new Color(0.1f, 0.1f, 0.1f) } } };
            AddChild(ball);
            shells.Add(new Shell { From = from, Yaw = yaw, Pitch = aimPitch, Speed = c.MuzzleSpeedMps, Secs = f.Secs, FiredAt = now, Ball = ball });
        }

        public override void _Process(double delta)
        {
            double now = Clock.Now;
            DrawAim();
            for (int i = shells.Count - 1; i >= 0; i--)
            {
                var s = shells[i];
                float t = (float)(now - s.FiredAt);
                if (s.Splash is { } splash)
                {
                    // A white column that rises and fades over 0.8 s.
                    float k = (t - s.Secs) / 0.8f;
                    if (k >= 1)
                    {
                        splash.QueueFree();
                        shells.RemoveAt(i);
                        continue;
                    }
                    splash.Scale = new Vector3(1 + k, 0.2f + k * 2, 1 + k);
                    ((StandardMaterial3D)((CylinderMesh)splash.Mesh).Material).AlbedoColor = new Color(1, 1, 1, 0.8f * (1 - k));
                    continue;
                }
                var at = Along(s.From, s.Yaw, s.Pitch, s.Speed, Math.Min(t, s.Secs));
                s.Ball.GlobalPosition = at;
                if (t >= s.Secs)
                {
                    s.Ball.QueueFree();
                    s.Splash = new MeshInstance3D
                    {
                        Mesh = new CylinderMesh
                        {
                            TopRadius = 0.6f,
                            BottomRadius = 1.0f,
                            Height = 3.0f,
                            Material = new StandardMaterial3D { Transparency = BaseMaterial3D.TransparencyEnum.Alpha, AlbedoColor = new Color(1, 1, 1, 0.8f), RenderPriority = (int)Material.RenderPriorityMax },
                        },
                    };
                    AddChild(s.Splash);
                    s.Splash.GlobalPosition = new Vector3(at.X, 0.5f, at.Z);
                }
            }
        }

        /// <summary>A shot's position <paramref name="t"/> seconds after it left <paramref name="from"/>, as <see cref="Ship.CannonFlight"/> flies it.</summary>
        private static Vector3 Along(System.Numerics.Vector3 from, float yaw, float pitch, float speed, float t)
        {
            var (fx, fz) = Yaw.Facing(yaw);
            float vh = speed * MathF.Cos(pitch), vy = speed * MathF.Sin(pitch);
            return new Vector3(from.X + fx * vh * t, from.Y + vy * t - Ship.Gravity / 2 * t * t, from.Z + fz * vh * t);
        }

        private void DrawAim()
        {
            var ship = Aboard;
            bool show = ship is not null && cannon is not null;
            marker.Visible = arc.Visible = show;
            if (!show)
                return;
            var c = ship!.Hull.Cannon;
            var (from, yaw) = Ship.Muzzle(ship.Hull, ship.Pose, ship.Hull.Mounts[cannon!.Value], aimYaw);
            if (Ship.CannonFlight(from, yaw, aimPitch, c.MuzzleSpeedMps) is not Flight f)
                return;
            marker.GlobalPosition = new Vector3(f.Landing.X, 0.05f, f.Landing.Z);
            arcMesh.ClearSurfaces();
            arcMesh.SurfaceBegin(Mesh.PrimitiveType.LineStrip);
            const int n = 32;
            for (int i = 0; i <= n; i++)
                arcMesh.SurfaceAddVertex(Along(from, yaw, aimPitch, c.MuzzleSpeedMps, f.Secs * i / n));
            arcMesh.SurfaceEnd();
        }

        private string ClimbPrompt()
        {
            foreach (var d in entities.Decks)
                if (Boarding.ClimbTarget(d, PlayerController.ToNumerics(player.GlobalPosition)) is not null)
                    return $"{d.Hull.Id} #{d.Ship} alongside: F to climb aboard{(player.ClimbNote is { } note && note != "nothing to climb here" ? $" ({note})" : "")}";
            return "";
        }

        private string Describe(ShipView ship, Station? station)
        {
            var v = ship.Vehicle;
            string head = $"aboard {ship.Hull.Id} #{ship.Id}  hp {v?.Hp ?? ship.Hull.Hp:N0}  {ship.Speed:F1} m/s  throttle {v?.Throttle ?? 0}%  rudder {v?.Rudder ?? 0}%";
            if (AtHelm)
                return head + "\nat the helm: W/S throttle, hold A/D for the rudder, F to let go";
            if (cannon is byte m)
            {
                var c = ship.Hull.Cannon;
                var (from, yaw) = Ship.Muzzle(ship.Hull, ship.Pose, ship.Hull.Mounts[m], aimYaw);
                var f = Ship.CannonFlight(from, yaw, aimPitch, c.MuzzleSpeedMps);
                float range = f is Flight fl ? new Vector2(fl.Landing.X - from.X, fl.Landing.Z - from.Z).Length() : 0;
                return head + $"\ncannon {m}: aim {Mathf.RadToDeg(aimYaw):F0}°, elevation {Mathf.RadToDeg(aimPitch):F1}°, {range:F0} m"
                    + $"  {(Loaded ? "loaded" : "reloading")}\nA/D aim, W/S elevation, Space fire, F to leave";
            }
            return station switch
            {
                { Kind: StationKind.Helm } when v is { Helm: not 0 } => head + "\nsomeone has the helm",
                { Kind: StationKind.Helm } => head + "\nF to take the helm",
                { Kind: StationKind.Cannon } => head + "\nF to man the cannon",
                _ => head,
            };
        }

        /// <summary>F9, F10 and <c>--spawn-ship</c>: dev reducers, refused by production builds.</summary>
        private void DevSpawn()
        {
            if (session.Conn is not { } conn || session.Me is null)
                return;
            var here = player.GlobalPosition;
            if (options.SpawnShip is { } spawn && !spawned && session.Near is { Ready: true }
                && new Vector2(here.X - spawn.X, here.Z - spawn.Z).Length() < Autopilot.ShipSearchM)
            {
                var (sx, sz, syaw) = spawn;
                spawned = true;
                foreach (var d in entities.Decks)
                {
                    if (new Vector2(d.Pose.X - sx, d.Pose.Z - sz).Length() < SpawnClearM)
                    {
                        conn.Reducers.DevRemoveShip(d.Ship);
                        session.Log($"removed ship {d.Ship} to make room");
                    }
                }
                conn.Reducers.DevSpawnShip((byte)Hull.Clipper, sx, sz, Mathf.DegToRad(syaw));
                session.Log($"spawned a clipper at ({sx:F0}, {sz:F0}), yaw {syaw:F0}°");
            }
            if (options.Autopilot is not null)
                return;
            if (Input.IsActionJustPressed("dev_spawn_ship"))
            {
                var (fx, fz) = Yaw.Facing(player.CameraYaw);
                var at = new Vector2(here.X + fx * SpawnAheadM, here.Z + fz * SpawnAheadM);
                // Side on to us, so it's easy to walk up to.
                float yaw = player.CameraYaw + Mathf.Pi / 2;
                var hull = Hull.Clipper.Data();
                var pose = new ShipPose(at.X, 0, at.Y, yaw);
                bool deep = true;
                foreach (float lz in new[] { -hull.DeckHalfLengthM, 0, hull.DeckHalfLengthM })
                {
                    var p = pose.ToWorld(new System.Numerics.Vector3(0, 0, lz));
                    // The server reads the higher of two triangulations; keep well clear of it.
                    deep &= map.Height(p.X, p.Z) < -hull.DraftM - 0.3f;
                }
                if (!deep)
                    session.Log("F9: the water 30 m ahead is too shallow for a clipper");
                else
                {
                    conn.Reducers.DevSpawnShip((byte)Hull.Clipper, at.X, at.Y, yaw);
                    session.Log($"F9: spawned a clipper at ({at.X:F0}, {at.Y:F0})");
                }
            }
            if (Input.IsActionJustPressed("dev_remove_ship"))
            {
                Deck? nearest = null;
                float best = 60.0f;
                foreach (var d in entities.Decks)
                {
                    float dist = new Vector2(d.Pose.X - here.X, d.Pose.Z - here.Z).Length();
                    if (dist < best)
                        (nearest, best) = (d, dist);
                }
                if (nearest is Deck n)
                {
                    conn.Reducers.DevRemoveShip(n.Ship);
                    session.Log($"F10: removed ship {n.Ship}");
                }
            }
        }
    }
}
