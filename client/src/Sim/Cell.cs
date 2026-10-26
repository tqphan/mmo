using System;
using System.Collections.Generic;

namespace Mmo.Sim
{
    /// <summary>
    /// World partitioning: 64 m cells over a 16×16 km map centred on the origin. A port of
    /// <c>sim::cell</c>, checked against vectors generated from it (client/tests/vectors/sim.txt).
    /// </summary>
    public static class Cell
    {
        public const float CellSizeM = 64.0f;
        public const float WorldSizeM = 16_000.0f;
        public const float WorldHalfM = WorldSizeM / 2.0f;
        public const ushort CellsPerSide = 250;

        /// <summary>How far past its centre cell a player may go before a window follows.</summary>
        public const float HysteresisM = 8.0f;

        public const uint OpenWorld = 0;

        /// <summary>Grid index along one axis, clamped to the map edge.</summary>
        public static ushort AxisIndex(float coordM)
        {
            float i = MathF.Floor((coordM + WorldHalfM) / CellSizeM);
            if (float.IsNaN(i) || i < 0.0f)
                return 0;
            return i >= CellsPerSide - 1 ? (ushort)(CellsPerSide - 1) : (ushort)i;
        }

        public static ulong Pack(uint instance, ushort cy, ushort cx) =>
            ((ulong)instance << 32) | ((ulong)cy << 16) | cx;

        public static (uint Instance, ushort Cy, ushort Cx) Unpack(ulong key) =>
            ((uint)(key >> 32), (ushort)(key >> 16), (ushort)key);

        /// <summary>The cell containing world position (x, z). y is up and plays no part.</summary>
        public static ulong CellOf(uint instance, float x, float z) =>
            Pack(instance, AxisIndex(z), AxisIndex(x));

        /// <summary>Every cell key of the window around <paramref name="center"/>, clipped to the map, row by row.</summary>
        public static List<ulong> WindowCells(ulong center, ushort radius)
        {
            var (instance, cy, cx) = Unpack(center);
            int last = CellsPerSide - 1;
            int yLo = Math.Max(cy - radius, 0), yHi = Math.Min(cy + radius, last);
            int xLo = Math.Max(cx - radius, 0), xHi = Math.Min(cx + radius, last);
            var cells = new List<ulong>((yHi - yLo + 1) * (xHi - xLo + 1));
            for (int y = yLo; y <= yHi; y++)
                for (int x = xLo; x <= xHi; x++)
                    cells.Add(Pack(instance, (ushort)y, (ushort)x));
            return cells;
        }

        /// <summary>
        /// Where a window now centred on <paramref name="center"/> should be centred for a player at
        /// (x, z): the same cell until the player is more than <paramref name="marginM"/> outside it,
        /// then the cell the player is in. Non-finite positions leave the window where it is.
        /// </summary>
        public static ulong Recenter(ulong center, float x, float z, float marginM)
        {
            var (instance, cy, cx) = Unpack(center);
            static bool Near(float coord, ushort i, float margin)
            {
                float lo = i * CellSizeM - WorldHalfM;
                return coord >= lo - margin && coord < lo + CellSizeM + margin;
            }
            if (!float.IsFinite(x) || !float.IsFinite(z) || Near(x, cx, marginM) && Near(z, cy, marginM))
                return center;
            return CellOf(instance, x, z);
        }
    }
}
