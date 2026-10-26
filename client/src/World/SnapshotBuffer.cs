using System;
using System.Collections.Generic;
using System.Numerics;
using Mmo.Sim;

namespace Mmo.World
{
    /// <summary>One <c>transform</c> row as received: local receive time in seconds, position, yaw and speed (m/s).</summary>
    public readonly record struct Snapshot(double Time, Vector3 Pos, float Yaw, float Speed);

    /// <summary>
    /// Where to draw another player (PLAN §6.2, ADR 0011). Rows arrive once per 100 ms tick, and only
    /// when the player sent a move: on start, stop, turn and jump, and every 0.5 s while moving.
    /// So the buffer draws <see cref="Delay"/> in the past, interpolating between the two rows either
    /// side of that time, and past the newest row it carries on along that row's yaw at its speed
    /// for at most <see cref="MaxExtrapolateS"/>.
    /// </summary>
    public sealed class SnapshotBuffer
    {
        /// <summary>Two world ticks: a row is at most this late even when the tick that carried it was full.</summary>
        public const double Delay = 0.2;

        /// <summary>A heartbeat interval, plus a tick, plus slack.</summary>
        public const double MaxExtrapolateS = 0.8;

        /// <summary>A jump between rows this much further than their speeds explain is a teleport or correction: snap.</summary>
        public const float SnapM = 8.0f;

        private const int Capacity = 16;
        private readonly List<Snapshot> rows = new(Capacity);

        public int Count => rows.Count;

        public Snapshot? Newest => rows.Count > 0 ? rows[^1] : null;

        public void Clear() => rows.Clear();

        public void Add(Snapshot s)
        {
            // Rows that arrive in the same frame share a time; keep the newest.
            if (rows.Count > 0 && s.Time <= rows[^1].Time)
                s = s with { Time = rows[^1].Time + 1e-6 };
            // A teleport: forget the path that led here, so nothing interpolates across the gap.
            if (rows.Count > 0 && Teleported(rows[^1], s))
                rows.Clear();
            rows.Add(s);
            if (rows.Count > Capacity)
                rows.RemoveAt(0);
        }

        private static bool Teleported(Snapshot a, Snapshot b)
        {
            float explained = Math.Max(a.Speed, b.Speed) * (float)(b.Time - a.Time + Delay);
            return Vector3.Distance(a.Pos, b.Pos) > explained + SnapM;
        }

        /// <summary>Position and yaw to draw at local time <paramref name="now"/>.</summary>
        public (Vector3 Pos, float Yaw) Sample(double now)
        {
            if (rows.Count == 0)
                throw new InvalidOperationException("no rows");
            double t = now - Delay;
            // Forget rows that render time has passed, keeping the one before it.
            while (rows.Count >= 2 && rows[1].Time <= t)
                rows.RemoveAt(0);
            var a = rows[0];
            if (t < a.Time)
                return (a.Pos, a.Yaw);
            if (rows.Count >= 2)
            {
                var b = rows[1];
                float f = (float)((t - a.Time) / (b.Time - a.Time));
                return (Vector3.Lerp(a.Pos, b.Pos, f), a.Yaw + Yaw.Delta(a.Yaw, b.Yaw) * f);
            }
            float ahead = (float)Math.Min(t - a.Time, MaxExtrapolateS);
            var (fx, fz) = Yaw.Facing(a.Yaw);
            return (a.Pos + new Vector3(fx, 0, fz) * a.Speed * ahead, a.Yaw);
        }
    }
}
