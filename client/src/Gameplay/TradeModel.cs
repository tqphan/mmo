using System;
using System.Numerics;
using Mmo.Content;


namespace Mmo.Gameplay
{
    /// <summary>A pack as the client sees it on a back or the ground (docs/design/trade.md).</summary>
    public readonly record struct PackInfo(ushort Def, ulong Owner, float OriginX, float OriginZ, long MadeAt);

    /// <summary>
    /// What the HUD says about trade packs: the sell prompt with its price, the pick-up prompt, the
    /// pack carried and the purse. No Godot dependency, so xUnit covers it; the prices come from
    /// <see cref="Trade"/>, the port of <c>sim::trade</c>.
    /// </summary>
    public static class TradeModel
    {
        private static string NameOf(ushort def) => GameContent.Item(def)?.Name ?? "pack";

        private static string Pct(uint permille) => FormattableString.Invariant($"{permille / 10.0:0.#}%");

        /// <summary>What a post would pay for <paramref name="pack"/>, or null with the reason it won't.</summary>
        public static (Sim.TradeQuote? Quote, Sim.TradeRefusal? Refusal) Offer(PackInfo pack, PostData post, Vector2 postAt, uint demand, long now) =>
            Sim.Trade.Quote(pack.Def, new Vector2(pack.OriginX, pack.OriginZ), pack.MadeAt, postAt, post.BonusPct, demand, now);

        /// <summary><c>F: sell Grain Bale at Gullhaven Post for 3g 22s 50c (route 215%, demand 100%, fresh 100%)</c>, or why not.</summary>
        public static string SellPrompt(PackInfo pack, PostData post, Vector2 postAt, uint demand, long now)
        {
            var (q, refusal) = Offer(pack, post, postAt, demand, now);
            if (q is not { } x)
                return $"{post.Name}: {Sim.Trade.Message(refusal!.Value)}";
            return $"F: sell {NameOf(pack.Def)} at {post.Name} for {Sim.Trade.Coins(x.Copper)} (route {x.RoutePct}%, demand {Pct(x.Demand)}, fresh {Pct(x.Fresh)})";
        }

        /// <summary><c>F: pick up Grain Bale</c>, marked as theft when it isn't one of ours, or why not.</summary>
        public static string PickUpPrompt(PackInfo pack, bool ours, bool carrying, long now)
        {
            if (carrying)
                return $"{NameOf(pack.Def)}: already carrying a pack";
            if (Sim.Trade.FreshPermille(pack.MadeAt, now) is null)
                return $"{NameOf(pack.Def)}: this pack has spoiled";
            return $"F: pick up {NameOf(pack.Def)}{(ours ? "" : " (theft)")}";
        }

        /// <summary><c>carrying Grain Bale, fresh 100%, spoils in 5d 23h (X: drop)</c>.</summary>
        public static string PackLine(PackInfo pack, long now)
        {
            if (Sim.Trade.FreshPermille(pack.MadeAt, now) is not uint fresh)
                return $"carrying {NameOf(pack.Def)}, spoiled (X: drop it)";
            long secs = Math.Max(0, (Sim.Trade.SpoilsAt(pack.MadeAt) - now) / 1_000_000);
            string left = secs >= 86_400 ? $"{secs / 86_400}d {secs % 86_400 / 3600}h" : $"{secs / 3600}h {secs % 3600 / 60}m";
            return $"carrying {NameOf(pack.Def)}, fresh {Pct(fresh)}, spoils in {left} (X: drop)";
        }

        public static string PurseLine(ulong copper) => $"purse {Sim.Trade.Coins(copper)}";
    }
}
