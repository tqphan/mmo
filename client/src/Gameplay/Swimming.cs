using System;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>
    /// When the local player swims (docs/design/client.md). The server allows swim mode anywhere
    /// from the sea floor up to 1 m above the surface (docs/design/terrain.md). The client swims
    /// where the sea is deeper than a body's chest, floating with its feet <see cref="FloatM"/>
    /// under the surface, and walks again where it's shallower, with some hysteresis so wading at
    /// the edge doesn't flicker between the two.
    /// </summary>
    public static class Swimming
    {
        /// <summary>How far under the surface a swimmer's feet hang: head and shoulders out.</summary>
        public const float FloatM = 1.2f;
        /// <summary>Start swimming where the sea floor is deeper than this.</summary>
        public const float EnterDepthM = 1.5f;
        /// <summary>Stop where it's shallower than this, and walk out.</summary>
        public const float LeaveDepthM = 1.3f;
        /// <summary>How fast a swimmer rises to the surface, or settles into the water from a fall, m/s.</summary>
        public const float RiseMps = 2.0f;
        public const float SinkMps = 4.0f;

        /// <summary>The height a swimmer's feet float at.</summary>
        public const float FloatY = Movement.SeaLevelM - FloatM;

        /// <summary>
        /// Whether to swim, given whether we are, the sea floor's height under us and our feet's.
        /// Falling in, you start swimming once you've come down to the float height.
        /// </summary>
        public static bool Swims(bool swimming, float seabed, float feet) =>
            swimming
                ? seabed < Movement.SeaLevelM - LeaveDepthM
                : seabed < Movement.SeaLevelM - EnterDepthM && feet <= FloatY + 0.05f;

        /// <summary>The vertical speed that brings a swimmer's feet to the float height.</summary>
        public static float VerticalSpeed(float feet) => Math.Clamp((FloatY - feet) * 4.0f, -SinkMps, RiseMps);
    }
}
