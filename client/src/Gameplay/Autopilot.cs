using System;
using System.Collections.Generic;
using System.Globalization;
using System.Numerics;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>What the autopilot sees each physics frame.</summary>
    /// <param name="World">Our horizontal world position (x, z).</param>
    /// <param name="Parent">The ship we're aboard, or 0.</param>
    /// <param name="Local">Our position in <paramref name="Parent"/>'s frame, when aboard.</param>
    /// <param name="Decks">The ships in view.</param>
    /// <param name="Manning">The station we're at: the helm when the server says we hold it, or the cannon we're manning.</param>
    /// <param name="Loaded">The cannon we're manning can fire.</param>
    public readonly record struct PilotView(double Now, Vector2 World, ulong Parent, Vector3 Local, IReadOnlyList<Deck> Decks, Station? Manning, bool Loaded);

    public enum PilotAction { None, Climb, Use, Fire }

    /// <summary>
    /// What to do: run along <see cref="Dir"/> (unit length or zero, in the ship's frame when
    /// <see cref="OnDeck"/>), maybe jump, maybe act, and with the helm, the input to set.
    /// </summary>
    public readonly record struct PilotOrder(Vector2 Dir, bool OnDeck = false, bool Jump = false, PilotAction Action = PilotAction.None, ShipInput? Helm = null);

    /// <summary>
    /// A scripted route that stands in for the keyboard, for captures and playtests without a person:
    /// <list type="bullet">
    /// <item><c>circle:X,Z,R</c> runs anticlockwise (seen from above) round a circle.</item>
    /// <item><c>pace:X,Z,LEN</c> runs back and forth along x.</item>
    /// <item><c>deck:X,Z,S</c> goes to the ship nearest (X, Z), climbs aboard and runs laps of its
    /// deck, jumping every 3 s; after S seconds aboard (0: never) it runs over the starboard side
    /// and stays in the water.</item>
    /// <item><c>helm:X,Z,R</c> boards the ship nearest (X, Z), takes the helm, waits 20 s for the
    /// crew to board, and sails at full throttle, putting the rudder to R percent 15 s later.</item>
    /// <item><c>gun:X,Z,M</c> boards the ship nearest (X, Z), mans cannon mount M and fires it each
    /// time it's loaded.</item>
    /// </list>
    /// The first two run to the route from wherever the player is. The ship routes wait 25 m from
    /// (X, Z) until a ship is in view within 60 m of it.
    /// </summary>
    public sealed class Autopilot
    {
        private enum Kind { Circle, Pace, Deck, Helm, Gun }

        public const float ShipSearchM = 60.0f;
        public const float WaitM = 25.0f;
        public const double JumpEveryS = 3.0;
        /// <summary>The helm route waits this long at the helm before sailing, for the others to board.</summary>
        public const double CrewWaitS = 20.0;
        public const double RudderAfterS = 15.0;
        /// <summary>How long to wait for the server before trying the helm or cannon again.</summary>
        public const double UseRetryS = 1.0;

        private readonly Kind kind;
        private readonly Vector2 center;
        private readonly float size;
        private bool paceEast = true;
        private double aboardSince = double.NaN, heldSince = double.NaN, lastJump, lastUse = double.NegativeInfinity;
        private int corner;
        private bool wentOver;

        private Autopilot(Kind kind, Vector2 center, float size)
        {
            this.kind = kind;
            this.center = center;
            this.size = size;
        }

        /// <summary>Whether this route boards ships; the others only steer.</summary>
        public bool Sails => kind is Kind.Deck or Kind.Helm or Kind.Gun;

        /// <summary>Parses the routes above, or <c>none</c> (null).</summary>
        public static Autopilot? Parse(string spec)
        {
            if (spec == "none")
                return null;
            var parts = spec.Split(':', 2);
            var nums = parts.Length == 2 ? parts[1].Split(',') : Array.Empty<string>();
            if (nums.Length != 3)
                throw new ArgumentException($"autopilot {spec}: expected circle:X,Z,R, pace:X,Z,LEN, deck:X,Z,S, helm:X,Z,R or gun:X,Z,M");
            float N(int i) => float.Parse(nums[i], CultureInfo.InvariantCulture);
            var kind = parts[0] switch
            {
                "circle" => Kind.Circle,
                "pace" => Kind.Pace,
                "deck" => Kind.Deck,
                "helm" => Kind.Helm,
                "gun" => Kind.Gun,
                _ => throw new ArgumentException($"autopilot {spec}: unknown route {parts[0]}"),
            };
            float n = N(2);
            bool ok = kind switch
            {
                Kind.Circle or Kind.Pace => n > 1.0f,
                Kind.Deck => n >= 0,
                Kind.Helm => n >= -100 && n <= 100,
                _ => n >= 0 && n == MathF.Floor(n) && n < 256,
            };
            if (!ok)
                throw new ArgumentException($"autopilot {spec}: {(kind is Kind.Circle or Kind.Pace ? "size must be over 1 m" : "bad last value")}");
            return new Autopilot(kind, new Vector2(N(0), N(1)), n);
        }

        /// <summary>The horizontal direction (x, z) to run in from <paramref name="pos"/>; unit length.</summary>
        public Vector2 Steer(Vector2 pos)
        {
            if (kind == Kind.Pace)
            {
                var target = center + new Vector2(paceEast ? size / 2 : -size / 2, 0);
                if (Vector2.Distance(pos, target) < 0.5f)
                {
                    paceEast = !paceEast;
                    target = center + new Vector2(paceEast ? size / 2 : -size / 2, 0);
                }
                return Vector2.Normalize(target - pos);
            }
            var offset = pos - center;
            float d = offset.Length();
            if (d < 0.01f)
                return Vector2.UnitX;
            var radial = offset / d;
            if (MathF.Abs(d - size) > 1.0f)
                return d > size ? -radial : radial;
            // Anticlockwise seen from above (+y), with x east and z south: tangent = (z, -x).
            var tangent = new Vector2(radial.Y, -radial.X);
            return Vector2.Normalize(tangent + radial * (size - d) / size * 4.0f);
        }

        /// <summary>Call every physics frame.</summary>
        public PilotOrder Drive(PilotView v)
        {
            if (!Sails)
                return new PilotOrder(Steer(v.World));
            Deck? aboard = null;
            foreach (var d in v.Decks)
                if (d.Ship == v.Parent && v.Parent != 0)
                    aboard = d;
            if (aboard is not Deck ship)
                return Approach(v);
            if (double.IsNaN(aboardSince))
                aboardSince = v.Now;
            var l = new Vector2(v.Local.X, v.Local.Z);
            var hull = ship.Hull;
            switch (kind)
            {
                case Kind.Helm:
                    if (v.Manning is { Kind: StationKind.Helm })
                    {
                        if (double.IsNaN(heldSince))
                            heldSince = v.Now;
                        double held = v.Now - heldSince;
                        sbyte throttle = held >= CrewWaitS ? (sbyte)100 : (sbyte)0;
                        sbyte rudder = held >= CrewWaitS + RudderAfterS ? (sbyte)size : (sbyte)0;
                        return new PilotOrder(Vector2.Zero, Helm: new ShipInput(throttle, rudder));
                    }
                    return GoAndUse(v, l, new Vector2(hull.HelmX, hull.HelmZ));
                case Kind.Gun:
                    int m = (int)size;
                    if (m >= hull.Mounts.Count)
                        return new PilotOrder(Vector2.Zero);
                    if (v.Manning is { Kind: StationKind.Cannon } at && at.Mount == m)
                        return new PilotOrder(Vector2.Zero, Action: v.Loaded ? PilotAction.Fire : PilotAction.None);
                    return GoAndUse(v, l, new Vector2(hull.Mounts[m].X, hull.Mounts[m].Z));
                default:
                    if (size > 0 && v.Now - aboardSince >= size)
                    {
                        // Over the starboard side.
                        wentOver = true;
                        return new PilotOrder(Vector2.UnitX, OnDeck: true);
                    }
                    float hx = hull.DeckHalfWidthM - 1.0f, hz = hull.DeckHalfLengthM - 1.0f;
                    // Laps anticlockwise seen from above, bow to port to stern to starboard.
                    Span<Vector2> corners = stackalloc Vector2[] { new(hx, -hz), new(-hx, -hz), new(-hx, hz), new(hx, hz) };
                    if (Vector2.Distance(l, corners[corner]) < 0.6f)
                        corner = (corner + 1) % 4;
                    bool jump = v.Now - lastJump >= JumpEveryS;
                    if (jump)
                        lastJump = v.Now;
                    return new PilotOrder(Toward(l, corners[corner]), OnDeck: true, Jump: jump);
            }
        }

        private PilotOrder Approach(PilotView v)
        {
            // Over the side and done; or knocked off the deck, which we try again.
            if (wentOver)
                return new PilotOrder(Vector2.Zero);
            aboardSince = heldSince = double.NaN;
            Deck? best = null;
            float bestD = ShipSearchM;
            foreach (var d in v.Decks)
            {
                float dist = Vector2.Distance(new Vector2(d.Pose.X, d.Pose.Z), center);
                if (dist <= bestD)
                    (best, bestD) = (d, dist);
            }
            if (best is Deck ship)
                return new PilotOrder(Toward(v.World, new Vector2(ship.Pose.X, ship.Pose.Z)), Action: PilotAction.Climb);
            return new PilotOrder(Vector2.Distance(v.World, center) > WaitM ? Toward(v.World, center) : Vector2.Zero);
        }

        /// <summary>Walk to a station on deck, and use it once there.</summary>
        private PilotOrder GoAndUse(PilotView v, Vector2 l, Vector2 station)
        {
            if (Vector2.Distance(l, station) > 1.0f)
                return new PilotOrder(Toward(l, station), OnDeck: true);
            if (v.Now - lastUse < UseRetryS)
                return new PilotOrder(Vector2.Zero, OnDeck: true);
            lastUse = v.Now;
            return new PilotOrder(Vector2.Zero, OnDeck: true, Action: PilotAction.Use);
        }

        private static Vector2 Toward(Vector2 from, Vector2 to)
        {
            var d = to - from;
            return d.LengthSquared() < 1e-6f ? Vector2.Zero : Vector2.Normalize(d);
        }
    }
}
