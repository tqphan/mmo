using System;
using System.Globalization;
using System.Numerics;

namespace Mmo.Gameplay
{
    /// <summary>
    /// A scripted route that stands in for the keyboard, for captures and playtests without a person:
    /// <c>circle:X,Z,R</c> runs anticlockwise (seen from above) round a circle, and <c>pace:X,Z,LEN</c>
    /// runs back and forth along x. Either first runs to the route from wherever the player is.
    /// </summary>
    public sealed class Autopilot
    {
        private enum Kind { Circle, Pace }

        private readonly Kind kind;
        private readonly Vector2 center;
        private readonly float size;
        private bool paceEast = true;

        private Autopilot(Kind kind, Vector2 center, float size)
        {
            this.kind = kind;
            this.center = center;
            this.size = size;
        }

        /// <summary>Parses <c>circle:X,Z,R</c>, <c>pace:X,Z,LEN</c> or <c>none</c> (null).</summary>
        public static Autopilot? Parse(string spec)
        {
            if (spec == "none")
                return null;
            var parts = spec.Split(':', 2);
            var nums = parts.Length == 2 ? parts[1].Split(',') : Array.Empty<string>();
            if (nums.Length != 3)
                throw new ArgumentException($"autopilot {spec}: expected circle:X,Z,R or pace:X,Z,LEN");
            float N(int i) => float.Parse(nums[i], CultureInfo.InvariantCulture);
            var kind = parts[0] switch
            {
                "circle" => Kind.Circle,
                "pace" => Kind.Pace,
                _ => throw new ArgumentException($"autopilot {spec}: unknown route {parts[0]}"),
            };
            if (!(N(2) > 1.0f))
                throw new ArgumentException($"autopilot {spec}: size must be over 1 m");
            return new Autopilot(kind, new Vector2(N(0), N(1)), N(2));
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
    }
}
