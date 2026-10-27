using System;
using System.Collections.Generic;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>A planting's stage. The value is the code in the vectors.</summary>
    public enum Stage : byte
    {
        Growing = 0,
        Ripe = 1,
        Withered = 2,
    }

    /// <summary>Why a planting can't go somewhere. The value is the code in the vectors.</summary>
    public enum SpotError : byte
    {
        OffMap = 1,
        NotPlantable = 2,
        NoGround = 3,
        UnderWater = 4,
        NoAccess = 5,
        PlotOnly = 6,
        TooClose = 7,
    }

    /// <summary>A standing plot near a spot, and whether we may tend plantings on it.</summary>
    public readonly record struct PlotSpot(Rect Rect, bool MayTend);

    /// <summary>Another planting that isn't gone: its centre and radius.</summary>
    public readonly record struct Planting(float X, float Z, float SpaceM);

    /// <summary>Where a planting may go: its ground, the index of its plot in the list given, and whether it's in its ideal climate.</summary>
    public readonly record struct Spot(float Y, int? Plot, bool Ideal);

    /// <summary>
    /// Farming's rules (docs/design/farming.md): when a planting is ripe or withered and where one
    /// may go. A port of <c>sim::farm</c>, checked against generated vectors; the client uses it for
    /// the planting preview and the crop labels, and the server decides.
    /// </summary>
    public static class Farm
    {
        private const long MicrosPerS = 1_000_000;

        public static string Message(SpotError e) => e switch
        {
            SpotError.OffMap => "off the map",
            SpotError.NotPlantable => "nothing can be planted here",
            SpotError.NoGround => "no ground here",
            SpotError.UnderWater => "too close to the water",
            SpotError.NoAccess => "this plot isn't yours to plant",
            SpotError.PlotOnly => "this can only be kept on a plot",
            _ => "too close to another planting",
        };

        public static long GrowthMicros(uint secs, bool ideal)
        {
            long us = secs * MicrosPerS;
            return ideal ? us * GameContent.FarmIdealPct / 100 : us;
        }

        public static long RipeAt(CropData kind, long from, bool ideal) => from + GrowthMicros(kind.GrowS, ideal);

        public static long? RegrownAt(CropData kind, long from, bool ideal) => kind.RegrowS > 0 ? from + GrowthMicros(kind.RegrowS, ideal) : null;

        public static long? WithersAt(CropData kind, long ripeAt) => kind.WitherS > 0 ? ripeAt + kind.WitherS * MicrosPerS : null;

        public static Stage StageOf(CropData kind, long ripeAt, long now)
        {
            if (now < ripeAt)
                return Stage.Growing;
            return WithersAt(kind, ripeAt) is long w && now >= w ? Stage.Withered : Stage.Ripe;
        }

        public static float Progress(long grownFrom, long ripeAt, long now)
        {
            long total = ripeAt - grownFrom;
            if (total <= 0)
                return 1.0f;
            return (float)Math.Clamp((now - grownFrom) / (double)total, 0.0, 1.0);
        }

        public static bool InReach(float px, float pz, float x, float z)
        {
            float dx = x - px, dz = z - pz;
            return dx * dx + dz * dz <= GameContent.FarmReachM * GameContent.FarmReachM;
        }

        /// <summary>On the plot covering <paramref name="r"/> by the centre: west and north edges in, east and south out.</summary>
        public static bool OnPlot(Rect r, float x, float z) => x >= r.X0 && x < r.X1 && z >= r.Z0 && z < r.Z1;

        /// <summary>
        /// Whether a planting of <paramref name="kind"/> may go at (<paramref name="x"/>,
        /// <paramref name="z"/>). <paramref name="texel"/> and <paramref name="sample"/> read the land
        /// texels and height samples as in <see cref="Land.CheckSite"/>.
        /// </summary>
        public static (Spot Spot, SpotError? Error) CheckSpot(
            Func<int, int, byte> texel, Func<int, int, float?> sample, CropData kind, float x, float z, IReadOnlyList<PlotSpot> plots, IEnumerable<Planting> others)
        {
            const float lo = -Land.WorldHalfM;
            if (!(float.IsFinite(x) && float.IsFinite(z) && x >= lo && x < Land.WorldHalfM && z >= lo && z < Land.WorldHalfM))
                return (default, SpotError.OffMap);
            byte t = texel((int)((x - lo) / Land.TexelM), (int)((z - lo) / Land.TexelM));
            int use = t & 0x0f;
            if (use >= GameContent.LandUses.Count || !GameContent.LandUses[use].Plantable)
                return (default, SpotError.NotPlantable);
            int i = (int)((x - lo) / Land.SpacingM), j = (int)((z - lo) / Land.SpacingM);
            float y = float.PositiveInfinity;
            foreach (var (di, dj) in new[] { (0, 0), (1, 0), (0, 1), (1, 1) })
            {
                if (sample(i + di, j + dj) is not float h)
                    return (default, SpotError.NoGround);
                y = MathF.Min(y, h);
            }
            if (y < Land.MinGroundM)
                return (default, SpotError.UnderWater);
            int? plot = null;
            for (int k = 0; k < plots.Count; k++)
                if (OnPlot(plots[k].Rect, x, z))
                {
                    plot = k;
                    break;
                }
            if (plot is int p && !plots[p].MayTend)
                return (default, SpotError.NoAccess);
            if (plot is null && kind.PlotOnly)
                return (default, SpotError.PlotOnly);
            foreach (var o in others)
            {
                float dx = o.X - x, dz = o.Z - z, r = o.SpaceM + kind.SpaceM;
                if (dx * dx + dz * dz < r * r)
                    return (default, SpotError.TooClose);
            }
            return (new Spot(y, plot, (byte)((t >> 4) & 0x07) == (byte)kind.Climate), null);
        }

        /// <summary>What a label says about a planting: the time left, ripe, or withered.</summary>
        public static string StageText(CropData kind, long grownFrom, long ripeAt, long now) => StageOf(kind, ripeAt, now) switch
        {
            Stage.Growing => $"{Progress(grownFrom, ripeAt, now) * 100:F0}% · ripe in {Land.Span(ripeAt - now)}",
            Stage.Ripe => WithersAt(kind, ripeAt) is long w ? $"ripe · withers in {Land.Span(w - now)}" : "ripe",
            _ => "withered",
        };
    }
}
