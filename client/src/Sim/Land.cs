using System;
using System.Collections.Generic;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>Why a plot can't stand somewhere. The value is the code in the vectors.</summary>
    public enum SiteError : byte
    {
        OffMap = 1,
        NotHousing = 2,
        NoGround = 3,
        UnderWater = 4,
        TooSteep = 5,
        Taken = 6,
    }

    /// <summary>Where a plot stands, from its tax.</summary>
    public enum Standing : byte
    {
        Paid = 0,
        Overdue = 1,
        Demolished = 2,
    }

    /// <summary>An axis-aligned rectangle on the ground.</summary>
    public readonly record struct Rect(float X0, float Z0, float X1, float Z1)
    {
        /// <summary>The square a plot of <paramref name="kind"/> covers around centre (<paramref name="x"/>, <paramref name="z"/>).</summary>
        public static Rect Plot(PlotData kind, float x, float z)
        {
            float h = kind.SizeM / 2.0f;
            return new Rect(x - h, z - h, x + h, z + h);
        }

        /// <summary>The two share some area; sharing only an edge or a corner doesn't count.</summary>
        public bool Overlaps(Rect o) => X0 < o.X1 && o.X0 < X1 && Z0 < o.Z1 && o.Z0 < Z1;

        /// <summary>Horizontal distance from (<paramref name="x"/>, <paramref name="z"/>) to the rectangle, 0 inside it.</summary>
        public float Distance(float x, float z)
        {
            float dx = MathF.Max(MathF.Max(X0 - x, x - X1), 0), dz = MathF.Max(MathF.Max(Z0 - z, z - Z1), 0);
            return MathF.Sqrt(dx * dx + dz * dz);
        }
    }

    /// <summary>
    /// Land's rules (docs/design/land.md): where a plot may stand, its tax, and whether it's paid,
    /// overdue or demolished. A port of <c>sim::land</c>, checked against generated vectors; the
    /// client uses it for the placement preview and the plot sign, and the server decides.
    /// </summary>
    public static class Land
    {
        public const float TexelM = 4.0f;
        public const float SpacingM = 2.0f;
        public const float WorldHalfM = 8000.0f;
        public const float MinGroundM = 0.5f;
        private const long MicrosPerS = 1_000_000;
        private const long DayMicros = 86_400 * MicrosPerS;

        public static string Message(SiteError e) => e switch
        {
            SiteError.OffMap => "off the map",
            SiteError.NotHousing => "plots can only go on housing land",
            SiteError.NoGround => "no ground here",
            SiteError.UnderWater => "too close to the water",
            SiteError.TooSteep => "the ground is too steep",
            _ => "the land is taken",
        };

        /// <summary>A plot centre on the 2 m height grid. Halves round away from zero, as Rust's <c>round</c> does.</summary>
        public static float Snap(float v) => MathF.Round(v / SpacingM, MidpointRounding.AwayFromZero) * SpacingM;

        public static bool InReach(Rect r, float x, float z) => r.Distance(x, z) <= GameContent.Property.ReachM;

        /// <summary>
        /// Whether a plot of <paramref name="kind"/> may stand centred on (<paramref name="x"/>,
        /// <paramref name="z"/>), already snapped: the lowest ground under it, or why not.
        /// <paramref name="texel"/> reads the land texel at texel (i, j) from the map's corner (0 where
        /// unknown); <paramref name="sample"/> reads the height at grid point (i, j), null where unknown.
        /// </summary>
        public static (float Lowest, SiteError? Error) CheckSite(
            Func<int, int, byte> texel, Func<int, int, float?> sample, PlotData kind, float x, float z, IEnumerable<Rect> others)
        {
            var r = Rect.Plot(kind, x, z);
            const float lo = -WorldHalfM;
            if (!(r.X0 >= lo && r.Z0 >= lo && r.X1 <= WorldHalfM && r.Z1 <= WorldHalfM))
                return (0, SiteError.OffMap);
            int ti0 = (int)MathF.Floor((r.X0 - lo) / TexelM), ti1 = (int)MathF.Ceiling((r.X1 - lo) / TexelM);
            int tj0 = (int)MathF.Floor((r.Z0 - lo) / TexelM), tj1 = (int)MathF.Ceiling((r.Z1 - lo) / TexelM);
            for (int j = tj0; j < tj1; j++)
                for (int i = ti0; i < ti1; i++)
                {
                    int use = texel(i, j) & 0x0f;
                    if (use >= GameContent.LandUses.Count || !GameContent.LandUses[use].Buildable)
                        return (0, SiteError.NotHousing);
                }
            int si0 = Grid(r.X0 - lo), si1 = Grid(r.X1 - lo), sj0 = Grid(r.Z0 - lo), sj1 = Grid(r.Z1 - lo);
            float min = float.PositiveInfinity, max = float.NegativeInfinity;
            for (int j = sj0; j <= sj1; j++)
                for (int i = si0; i <= si1; i++)
                {
                    if (sample(i, j) is not float h)
                        return (0, SiteError.NoGround);
                    min = MathF.Min(min, h);
                    max = MathF.Max(max, h);
                }
            if (min < MinGroundM)
                return (0, SiteError.UnderWater);
            if (max - min > kind.MaxRiseM)
                return (0, SiteError.TooSteep);
            foreach (var o in others)
                if (o.Overlaps(r))
                    return (0, SiteError.Taken);
            return (min, null);
        }

        private static int Grid(float fromCorner) => (int)MathF.Round(fromCorner / SpacingM, MidpointRounding.AwayFromZero);

        /// <summary>Levy Seals a week for one plot of <paramref name="kind"/> when the account owns <paramref name="owned"/> plots (this one included); null past the limit.</summary>
        public static uint? WeeklyTax(PlotData kind, int owned)
        {
            var steps = GameContent.Property.HeavyTaxPct;
            if (owned < 1 || owned > steps.Count)
                return null;
            return (kind.Tax * steps[owned - 1] + 99) / 100;
        }

        public static int MaxPlots => GameContent.Property.HeavyTaxPct.Count;

        public static long WeekMicros => GameContent.Property.WeekS * MicrosPerS;

        /// <summary>When a plot whose tax ran out at <paramref name="paidUntil"/> is demolished: the first demolition hour at least the grace later.</summary>
        public static long DemolishAt(long paidUntil)
        {
            long t = paidUntil + GameContent.Property.GraceS * MicrosPerS;
            long hour = GameContent.Property.DemolishHourUtc * 3_600 * MicrosPerS;
            long past = ((t - hour) % DayMicros + DayMicros) % DayMicros;
            return past == 0 ? t : t + DayMicros - past;
        }

        public static Standing StandingOf(long paidUntil, long now) =>
            now < paidUntil ? Standing.Paid : now < DemolishAt(paidUntil) ? Standing.Overdue : Standing.Demolished;

        /// <summary>Whether one more week may be paid at <paramref name="now"/>.</summary>
        public static bool CanPay(long paidUntil, long now) =>
            StandingOf(paidUntil, now) != Standing.Demolished && paidUntil + WeekMicros - now <= GameContent.Property.PrepayWeeks * WeekMicros;

        /// <summary>A span of time as "3d 4h", "2h 5m" or "45s".</summary>
        public static string Span(long micros)
        {
            long s = Math.Max(micros, 0) / MicrosPerS;
            if (s >= 86_400)
                return $"{s / 86_400}d {s % 86_400 / 3_600}h";
            if (s >= 3_600)
                return $"{s / 3_600}h {s % 3_600 / 60}m";
            return s >= 60 ? $"{s / 60}m {s % 60}s" : $"{s}s";
        }
    }
}
