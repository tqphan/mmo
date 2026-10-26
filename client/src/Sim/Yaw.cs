using System;

namespace Mmo.Sim
{
    /// <summary>
    /// Yaws are Godot's: a rotation about +y, 0 facing -z, positive turning left. A port of the yaw
    /// helpers in <c>sim::ship</c>, checked against generated vectors.
    /// </summary>
    public static class Yaw
    {
        private const float Tau = MathF.PI * 2.0f;

        /// <summary>A yaw on the wire: yaw / 2π × 65536, wrapping.</summary>
        public static ushort ToU16(float yaw)
        {
            float r = yaw % Tau;
            if (r < 0.0f)
                r += Tau;
            return (ushort)(uint)MathF.Round(r / Tau * 65536.0f, MidpointRounding.AwayFromZero);
        }

        public static float FromU16(ushort v) => v / 65536.0f * Tau;

        /// <summary>The horizontal unit vector (x, z) a yaw faces.</summary>
        public static (float X, float Z) Facing(float yaw) => (-MathF.Sin(yaw), -MathF.Cos(yaw));

        /// <summary>The yaw facing horizontal direction (x, z).</summary>
        public static float Of(float x, float z) => MathF.Atan2(-x, -z);

        /// <summary>The signed shortest turn from <paramref name="from"/> to <paramref name="to"/>, in (-π, π].</summary>
        public static float Delta(float from, float to)
        {
            float d = (to - from) % Tau;
            if (d > MathF.PI)
                d -= Tau;
            else if (d <= -MathF.PI)
                d += Tau;
            return d;
        }
    }
}
