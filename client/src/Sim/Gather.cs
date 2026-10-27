using System;
using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>
    /// Gathering's rules (docs/design/gathering.md): reach, readiness and progress. A port of
    /// <c>sim::gather</c>, checked against generated vectors; the client uses it for prompts and the
    /// progress bar, and the server decides.
    /// </summary>
    public static class Gather
    {
        private const long MicrosPerS = 1_000_000;

        /// <summary>A gather is cancelled once the gatherer stands further than this from where it started.</summary>
        public const float MoveCancelM = 0.5f;

        /// <summary>A character standing at <paramref name="at"/> (its feet) can reach a node at <paramref name="node"/>.</summary>
        public static bool InReach(Vector3 node, Vector3 at)
        {
            float dx = at.X - node.X, dz = at.Z - node.Z;
            float r = GameContent.GatherReachM;
            return dx * dx + dz * dz <= r * r && MathF.Abs(at.Y - node.Y) <= r;
        }

        /// <summary>A node whose <paramref name="readyAt"/> (µs since the Unix epoch) has come can be gathered.</summary>
        public static bool Ready(long readyAt, long now) => now >= readyAt;

        public static long GatherMicros(NodeData kind) => (long)Math.Round((double)kind.GatherS * MicrosPerS, MidpointRounding.AwayFromZero);

        /// <summary>How far a gather finishing at <paramref name="doneAt"/> has got at <paramref name="now"/>, from 0 to 1.</summary>
        public static float Progress(NodeData kind, long doneAt, long now)
        {
            long total = GatherMicros(kind);
            long left = doneAt - now;
            return (float)Math.Clamp(1.0 - (double)left / total, 0.0, 1.0);
        }

        /// <summary>The gatherer has moved off the spot it started from (horizontally).</summary>
        public static bool MovedOff(Vector3 start, Vector3 at)
        {
            float dx = at.X - start.X, dz = at.Z - start.Z;
            return dx * dx + dz * dz > MoveCancelM * MoveCancelM;
        }
    }
}
