using System;
using System.Linq;
using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>
    /// Crafting's rules (docs/design/crafting.md): reach, progress and how many units the bag and
    /// labor cover. A port of <c>sim::craft</c>, checked against generated vectors; the client uses
    /// it for the craft window and the progress bar, and the server decides.
    /// </summary>
    public static class Craft
    {
        private const long MicrosPerS = 1_000_000;

        /// <summary>A character standing at <paramref name="at"/> (its feet) can use a station at <paramref name="station"/>.</summary>
        public static bool InReach(Vector3 station, Vector3 at)
        {
            float dx = at.X - station.X, dz = at.Z - station.Z;
            float r = GameContent.CraftReachM;
            return dx * dx + dz * dz <= r * r && MathF.Abs(at.Y - station.Y) <= r;
        }

        public static long CraftMicros(RecipeData r) => (long)Math.Round((double)r.CraftS * MicrosPerS, MidpointRounding.AwayFromZero);

        /// <summary>How far a unit finishing at <paramref name="doneAt"/> has got at <paramref name="now"/>, from 0 to 1.</summary>
        public static float Progress(RecipeData r, long doneAt, long now)
        {
            long total = CraftMicros(r);
            long left = doneAt - now;
            return (float)Math.Clamp(1.0 - (double)left / total, 0.0, 1.0);
        }

        /// <summary>How many units the inputs held (<paramref name="have"/>, by item wire) and <paramref name="labor"/> cover, up to the most one craft may ask for. Room in the bag isn't counted.</summary>
        public static uint Affordable(RecipeData r, Func<ushort, uint> have, int labor)
        {
            uint byLabor = r.Labor > 0 ? (uint)(Math.Max(labor, 0) / r.Labor) : GameContent.CraftMaxCount;
            return Math.Min(r.Inputs.Aggregate(byLabor, (n, i) => Math.Min(n, have(i.Item) / i.Count)), GameContent.CraftMaxCount);
        }
    }
}
