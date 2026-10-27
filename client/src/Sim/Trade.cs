using System;
using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>Why a post won't buy a pack. The value is the code in the vectors.</summary>
    public enum TradeRefusal : byte
    {
        NotAPack = 1,
        TooClose = 2,
        Spoiled = 3,
    }

    /// <summary>What a post would pay for a pack, and why: route in percent, demand and freshness in tenths of a percent, and the copper.</summary>
    public readonly record struct TradeQuote(uint RoutePct, uint Demand, uint Fresh, ulong Copper);

    /// <summary>
    /// Trade packs' rules (docs/design/trade.md): reach, the carrier's speed, and what a post pays
    /// from the route, its demand and the pack's freshness, all from timestamps. A port of
    /// <c>sim::trade</c>, checked against generated vectors; the client uses it for the sell prompt
    /// and its own speed, and the server decides.
    /// </summary>
    public static class Trade
    {
        private const long MicrosPerS = 1_000_000;
        private const long MicrosPerH = 3_600 * MicrosPerS;
        /// <summary>Demand and freshness are in tenths of a percent.</summary>
        public const uint Full = 1000;

        private static TradeRules R => GameContent.Trade;

        /// <summary>Every movement mode's speed limit for a carrier.</summary>
        public static float CarryLimit(float maxSpeedMps) => maxSpeedMps * R.CarrySpeedPct / 100.0f;

        /// <summary>A character at <paramref name="at"/> (its feet) can sell at a post, or pick up a pack, at <paramref name="thing"/>.</summary>
        public static bool InReach(Vector3 thing, Vector3 at)
        {
            float dx = at.X - thing.X, dz = at.Z - thing.Z;
            float r = R.ReachM;
            return dx * dx + dz * dz <= r * r && MathF.Abs(at.Y - thing.Y) <= r;
        }

        /// <summary>The route factor in percent from <paramref name="origin"/> to a post at <paramref name="post"/> (both x, z), or null if the post is too close.</summary>
        public static uint? RoutePct(Vector2 origin, Vector2 post, uint bonusPct)
        {
            double dx = (double)post.X - origin.X, dz = (double)post.Y - origin.Y;
            double km = Math.Sqrt(dx * dx + dz * dz) / 1000.0;
            if (km < R.MinKm)
                return null;
            double route = Math.Min(100.0 + R.PerKmPct * km, R.MaxRoutePct);
            return (uint)Math.Floor(route) + bonusPct;
        }

        /// <summary>A post's demand for a pack kind at <paramref name="now"/>, from <paramref name="permille"/> at <paramref name="at"/>.</summary>
        public static uint DemandNow(uint permille, long at, long now)
        {
            long gained = Math.Max(now - at, 0) * (R.RecoverPctPerH * 10L) / MicrosPerH;
            return (uint)Math.Min(permille + gained, Full);
        }

        /// <summary>Demand right after a sale at <paramref name="demand"/>.</summary>
        public static uint AfterSale(uint demand)
        {
            uint drop = (uint)R.SaleDropPct * 10;
            return Math.Max(demand > drop ? demand - drop : 0, (uint)R.DemandMinPct * 10);
        }

        /// <summary>How fresh a pack made at <paramref name="madeAt"/> is, in tenths of a percent, or null once it has spoiled.</summary>
        public static uint? FreshPermille(long madeAt, long now)
        {
            long age = Math.Max(now - madeAt, 0);
            long fresh = R.FreshS * MicrosPerS, expire = R.ExpireS * MicrosPerS;
            if (age >= expire)
                return null;
            if (age <= fresh)
                return Full;
            long fall = Full - R.StalePct * 10L;
            return Full - (uint)(fall * (age - fresh) / (expire - fresh));
        }

        public static long SpoilsAt(long madeAt) => madeAt + R.ExpireS * MicrosPerS;

        /// <summary><paramref name="value"/> copper times route (percent), demand and freshness (permille), rounded down.</summary>
        public static ulong Price(uint value, uint routePct, uint demand, uint fresh) =>
            (ulong)value * routePct * demand * fresh / (100UL * Full * Full);

        /// <summary>The <c>trade_demand</c> key for a post kind and a pack item.</summary>
        public static uint DemandKey(byte post, ushort item) => (uint)post << 16 | item;

        /// <summary>What a post at <paramref name="at"/> with <paramref name="bonusPct"/> and <paramref name="demand"/> now would pay for a pack, or why it won't.</summary>
        public static (TradeQuote? Quote, TradeRefusal? Refusal) Quote(ushort def, Vector2 origin, long madeAt, Vector2 at, uint bonusPct, uint demand, long now)
        {
            if (GameContent.Pack(def) is not { } pack)
                return (null, TradeRefusal.NotAPack);
            if (FreshPermille(madeAt, now) is not uint fresh)
                return (null, TradeRefusal.Spoiled);
            if (RoutePct(origin, at, bonusPct) is not uint route)
                return (null, TradeRefusal.TooClose);
            return (new TradeQuote(route, demand, fresh, Price(pack.Value, route, demand, fresh)), null);
        }

        public static string Message(TradeRefusal r) => r switch
        {
            TradeRefusal.NotAPack => "that isn't a pack",
            TradeRefusal.TooClose => "this post is too close to where the pack was made",
            _ => "this pack has spoiled",
        };

        /// <summary>Copper as players read it: <c>3g 22s 50c</c>, leaving out leading zero parts.</summary>
        public static string Coins(ulong copper)
        {
            ulong g = copper / 10_000, s = copper / 100 % 100, c = copper % 100;
            return g > 0 ? $"{g}g {s}s {c}c" : s > 0 ? $"{s}s {c}c" : $"{c}c";
        }
    }
}
