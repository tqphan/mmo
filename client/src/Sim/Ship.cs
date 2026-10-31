using System;
using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>Where a ship is: the centre of its deck in world coordinates, and its yaw. A port of <c>sim::ship::Pose</c>.</summary>
    public readonly record struct ShipPose(float X, float Y, float Z, float Yaw)
    {
        /// <summary>Ship-local to world: Godot's <c>Basis(Vector3.UP, yaw)</c> and then the ship's position.</summary>
        public Vector3 ToWorld(Vector3 l)
        {
            var (s, c) = (MathF.Sin(Yaw), MathF.Cos(Yaw));
            return new Vector3(X + l.X * c + l.Z * s, Y + l.Y, Z - l.X * s + l.Z * c);
        }

        public Vector3 ToLocal(Vector3 w)
        {
            var (s, c) = (MathF.Sin(Yaw), MathF.Cos(Yaw));
            float dx = w.X - X, dz = w.Z - Z;
            return new Vector3(dx * c - dz * s, w.Y - Y, dx * s + dz * c);
        }

        /// <summary>A world direction turned into the ship's frame (or, with <paramref name="toWorld"/>, out of it).</summary>
        public Vector3 Turn(Vector3 v, bool toWorld)
        {
            var (s, c) = (MathF.Sin(toWorld ? Yaw : -Yaw), MathF.Cos(Yaw));
            return new Vector3(v.X * c + v.Z * s, v.Y, -v.X * s + v.Z * c);
        }
    }

    /// <summary>What the server's tick steps for each ship. Yaw stays within 0 to 2π; speed is negative astern.</summary>
    public readonly record struct ShipMotion(float X, float Z, float Yaw, float Speed)
    {
        /// <summary>The pose of a ship afloat. A land hull's deck height comes from <see cref="Ship.Stand"/> (<see cref="PoseAt"/>).</summary>
        public ShipPose Pose(HullData hull) => PoseAt(Movement.SeaLevelM + hull.DeckHeightM);

        public ShipPose PoseAt(float y) => new(X, y, Z, Yaw);
    }

    /// <summary>The helmsman's settings, each -100 to 100 percent. Positive rudder turns to starboard.</summary>
    public readonly record struct ShipInput(sbyte Throttle, sbyte Rudder);

    /// <summary>A cannon shot's flight time and where it comes down to sea level.</summary>
    public readonly record struct Flight(float Secs, Vector3 Landing);

    /// <summary>
    /// Ships: poses, motion under throttle and rudder, decks and cannon flight (docs/design/ships.md).
    /// A port of <c>sim::ship</c>, checked against generated vectors, so the client predicts ships
    /// with the server's own step.
    /// </summary>
    public static class Ship
    {
        public const sbyte InputMax = 100;
        /// <summary>Below this share of top speed, the rudder's authority falls off in proportion to speed.</summary>
        public const float FullAuthorityShare = 0.3f;
        public const float Gravity = 9.8f;
        /// <summary>Just outside the deck rectangle counts as aboard, with no ground under you.</summary>
        public const float RailM = 1.5f;
        /// <summary>How close to the helm or a cannon a player must stand to use it.</summary>
        public const float ReachM = 2.5f;
        /// <summary>How far a client's view of a ship may lag the server's, allowed for when changing frame.</summary>
        public const float PoseLagS = 0.5f;
        /// <summary>How close to a pack slot's point a character must be to load or unload it, horizontally and vertically.</summary>
        public const float CargoReachM = 3.0f;

        private const float Tau = MathF.PI * 2.0f;
        private const float RadsPerDeg = MathF.PI / 180.0f;

        /// <summary>Rust's <c>f32::rem_euclid</c>.</summary>
        public static float RemEuclid(float v, float m)
        {
            float r = v % m;
            return r < 0.0f ? r + MathF.Abs(m) : r;
        }

        /// <summary>The speed the throttle asks for, m/s.</summary>
        public static float TargetSpeed(HullData hull, sbyte throttle)
        {
            float share = throttle / (float)InputMax;
            return share * (throttle >= 0 ? hull.MaxSpeedMps : hull.AsternSpeedMps);
        }

        /// <summary>One step of <paramref name="dt"/> seconds: speed toward the throttle's target, then yaw by the rudder, then position along the new yaw.</summary>
        public static ShipMotion Step(ShipMotion m, HullData hull, ShipInput input, float dt)
        {
            float target = TargetSpeed(hull, input.Throttle);
            float rate = input.Throttle == 0 ? hull.DriftDecelMps2 : hull.AccelMps2;
            float change = Math.Clamp(target - m.Speed, -rate * dt, rate * dt);
            float speed = m.Speed + change;

            float authority = MathF.Min(MathF.Abs(speed) / (FullAuthorityShare * hull.MaxSpeedMps), 1.0f);
            float rudder = input.Rudder / (float)InputMax;
            // Rust's signum: +0 counts as positive.
            float sign = speed < 0.0f || (speed == 0.0f && float.IsNegative(speed)) ? -1.0f : 1.0f;
            float yawRate = -rudder * (hull.TurnRateDps * RadsPerDeg) * authority * sign;
            float yaw = RemEuclid(m.Yaw + yawRate * dt, Tau);

            var (fx, fz) = Yaw.Facing(yaw);
            return new ShipMotion(m.X + fx * speed * dt, m.Z + fz * speed * dt, yaw, speed);
        }

        /// <summary>Whether the water is deep enough for the hull at its bow, centre and stern; unknown terrain is deep water.</summary>
        public static bool Afloat(HullData hull, ShipMotion m, Func<float, float, Ground?> ground)
        {
            var pose = m.Pose(hull);
            float floor = Movement.SeaLevelM - hull.DraftM;
            foreach (float lz in new[] { -hull.DeckHalfLengthM, 0.0f, hull.DeckHalfLengthM })
            {
                var w = pose.ToWorld(new Vector3(0, 0, lz));
                if (ground(w.X, w.Z) is Ground g && g.Hi > floor)
                    return false;
            }
            return true;
        }

        /// <summary>
        /// Where a hull can be with motion <paramref name="m"/>, as its deck's height, or null where it
        /// can't (docs/design/vehicles.md): afloat for a sea hull; for a land hull, known ground at bow,
        /// centre and stern no deeper than its ford, no steeper than its gradient, and rising no more
        /// than that from stern to bow. A port of <c>sim::ship::stand</c>.
        /// </summary>
        public static float? Stand(HullData hull, ShipMotion m, Func<float, float, Ground?> ground)
        {
            if (!hull.Land)
                return Afloat(hull, m, ground) ? Movement.SeaLevelM + hull.DeckHeightM : null;
            var pose = m.PoseAt(0.0f);
            float l = hull.DeckHalfLengthM;
            Span<float> his = stackalloc float[3];
            int k = 0;
            foreach (float lz in new[] { -l, 0.0f, l })
            {
                var w = pose.ToWorld(new Vector3(0, 0, lz));
                if (ground(w.X, w.Z) is not Ground g || g.Hi < Movement.SeaLevelM - hull.DraftM || g.Gradient > hull.MaxGradient)
                    return null;
                his[k++] = g.Hi;
            }
            return MathF.Abs(his[0] - his[2]) <= hull.MaxGradient * 2.0f * l ? his[1] + hull.DeckHeightM : null;
        }

        /// <summary>A pack slot's point in world coordinates, on the deck; null for no such slot.</summary>
        public static Vector3? SlotPoint(HullData hull, ShipPose pose, int slot) =>
            slot >= 0 && slot < hull.Slots.Count ? pose.ToWorld(new Vector3(hull.Slots[slot].X, 0, hull.Slots[slot].Z)) : null;

        /// <summary>Whether a character at <paramref name="at"/> (world) can reach a slot at <paramref name="point"/> (world).</summary>
        public static bool CargoReach(Vector3 point, Vector3 at) =>
            MathF.Sqrt(MathF.Pow(point.X - at.X, 2) + MathF.Pow(point.Z - at.Z, 2)) <= CargoReachM && MathF.Abs(point.Y - at.Y) <= CargoReachM;

        /// <summary>How close to a shipyard's footprint a character must be to work on it, horizontally (docs/design/vehicles.md §2B).</summary>
        public const float YardReachM = 4.0f;
        /// <summary>How far beyond a vehicle's half-length from its centre it may be repaired from.</summary>
        public const float RepairReachM = 3.0f;

        /// <summary>Whether <paramref name="at"/> (world) is within reach of a shipyard's footprint: the deck of <paramref name="hull"/> placed at <paramref name="m"/>.</summary>
        public static bool YardReach(HullData hull, ShipMotion m, Vector3 at)
        {
            var l = m.PoseAt(0).ToLocal(new Vector3(at.X, 0, at.Z));
            float dx = MathF.Max(MathF.Abs(l.X) - hull.DeckHalfWidthM, 0), dz = MathF.Max(MathF.Abs(l.Z) - hull.DeckHalfLengthM, 0);
            return dx * dx + dz * dz <= YardReachM * YardReachM;
        }

        /// <summary>Whether <paramref name="at"/> (world) is close enough to a vehicle at <paramref name="m"/> to repair it from off its deck.</summary>
        public static bool RepairReach(HullData hull, ShipMotion m, Vector3 at) =>
            MathF.Sqrt(MathF.Pow(m.X - at.X, 2) + MathF.Pow(m.Z - at.Z, 2)) <= hull.DeckHalfLengthM + RepairReachM;

        /// <summary>A vehicle's HP after one repair: a share of its hull's HP more, up to full.</summary>
        public static int RepairedHp(HullData hull, int hp) =>
            (int)Math.Min((long)hp + (long)hull.Hp * GameContent.Vehicles.RepairPct / 100, hull.Hp);

        /// <summary>Whether a vehicle last hit at <paramref name="hitAt"/> (µs; 0 is never) is still under fire at <paramref name="now"/>.</summary>
        public static bool UnderFire(long hitAt, long now) => hitAt != 0 && now < hitAt + (long)GameContent.Vehicles.UnderFireS * 1_000_000;

        /// <summary>The radius of the circle a ship sails at full speed and full rudder.</summary>
        public static float TurningRadius(HullData hull) => hull.MaxSpeedMps / (hull.TurnRateDps * RadsPerDeg);

        /// <summary>Whether a ship-local point is on the deck rectangle grown by <paramref name="margin"/>.</summary>
        public static bool OnDeck(HullData hull, float x, float z, float margin) =>
            MathF.Abs(x) <= hull.DeckHalfWidthM + margin && MathF.Abs(z) <= hull.DeckHalfLengthM + margin;

        /// <summary>Horizontal distance on deck from a ship-local point to (x, z).</summary>
        public static float Reach(Vector3 from, float x, float z) =>
            MathF.Sqrt(MathF.Pow(from.X - x, 2) + MathF.Pow(from.Z - z, 2));

        /// <summary>Whether a cannon may be aimed <paramref name="yaw"/> radians off its mount's facing at <paramref name="pitch"/> radians.</summary>
        public static bool AimOk(CannonData cannon, float yaw, float pitch) =>
            float.IsFinite(yaw) && float.IsFinite(pitch)
            && MathF.Abs(yaw) <= cannon.ArcDeg * RadsPerDeg
            && pitch >= cannon.MinPitchDeg * RadsPerDeg && pitch <= cannon.MaxPitchDeg * RadsPerDeg;

        /// <summary>Where a mount's shot leaves from, in world coordinates, and the world yaw it flies along.</summary>
        public static (Vector3 From, float Yaw) Muzzle(HullData hull, ShipPose pose, Mount mount, float yaw) =>
            (pose.ToWorld(new Vector3(mount.X, hull.Cannon.MuzzleHeightM, mount.Z)), pose.Yaw + mount.FacingDeg * RadsPerDeg + yaw);

        /// <summary>A shot along world <paramref name="yaw"/>, <paramref name="pitch"/> above the horizontal, falling until it reaches sea level. Null from at or below the water.</summary>
        public static Flight? CannonFlight(Vector3 from, float yaw, float pitch, float speed)
        {
            float height = from.Y - Movement.SeaLevelM;
            if (height <= 0.0f || !(float.IsFinite(from.X) && float.IsFinite(from.Y) && float.IsFinite(from.Z)))
                return null;
            float vy = speed * MathF.Sin(pitch), vh = speed * MathF.Cos(pitch);
            float secs = (vy + MathF.Sqrt(vy * vy + 2.0f * Gravity * height)) / Gravity;
            var (fx, fz) = Yaw.Facing(yaw);
            return new Flight(secs, new Vector3(from.X + fx * vh * secs, Movement.SeaLevelM, from.Z + fz * vh * secs));
        }

        /// <summary>Whether a landing point hits a ship at <paramref name="pose"/>.</summary>
        public static bool Hits(HullData hull, ShipPose pose, Vector3 landing)
        {
            var l = pose.ToLocal(landing);
            return OnDeck(hull, l.X, l.Z, hull.Cannon.SplashM);
        }
    }

    /// <summary>The parts of <c>sim::movement</c>'s distance check the client needs to stay inside it.</summary>
    public static class Movement
    {
        public const float SeaLevelM = 0.0f;
        public const float SpeedTolerance = 0.15f;
        public const float DistanceSlackM = 0.5f;
        public const float MaxCreditedGapS = 2.0f;

        /// <summary>The furthest a character may travel horizontally in <paramref name="dt"/> seconds at the given limit.</summary>
        public static float AllowedDistance(float maxSpeedMps, float dt) =>
            maxSpeedMps * (1.0f + SpeedTolerance) * Math.Clamp(dt, 0.0f, MaxCreditedGapS) + DistanceSlackM;

        /// <summary>The allowance for boarding, stepping off or crossing between ships (<c>sim::ship::frame_change_allowance</c>).</summary>
        public static float FrameChangeAllowance(float ownSpeedMps, float shipSpeedMps, float dt) =>
            AllowedDistance(ownSpeedMps, dt) + MathF.Abs(shipSpeedMps) * Ship.PoseLagS;
    }
}
