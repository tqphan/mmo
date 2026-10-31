using System;
using System.Collections.Generic;
using System.Numerics;
using Mmo.Content;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>What F would do at a shipyard, and whether it can: the prompt says why not.</summary>
    public readonly record struct YardAction(ulong Yard, string Prompt, bool Can);

    /// <summary>
    /// The rules for shipyards and wrecks that the client applies before asking the server
    /// (docs/design/vehicles.md §2B), kept free of Godot so xUnit can check them.
    /// </summary>
    public static class ShipyardModel
    {
        /// <summary>A shipyard as these rules see it.</summary>
        public readonly record struct Seen(ulong Id, ShipyardData Kind, ShipMotion Footprint, byte Stage, bool Ours);

        /// <summary>
        /// The nearest yard in reach of <paramref name="at"/> (world) and what its next stage needs,
        /// given the pack we carry (0 for none). Null if none is in reach.
        /// </summary>
        public static YardAction? Pick(IEnumerable<Seen> yards, Vector3 at, ushort carrying)
        {
            Seen? best = null;
            float bestD = float.PositiveInfinity;
            foreach (var y in yards)
            {
                if (!Ship.YardReach(y.Kind.Hull.Data(), y.Footprint, at))
                    continue;
                float d = Vector2.DistanceSquared(new Vector2(y.Footprint.X, y.Footprint.Z), new Vector2(at.X, at.Z));
                if (d < bestD)
                    (best, bestD) = (y, d);
            }
            return best is { } b ? Action(b, carrying) : null;
        }

        /// <summary>What F would do at <paramref name="y"/>.</summary>
        public static YardAction Action(Seen y, ushort carrying)
        {
            var stages = y.Kind.Stages;
            if (y.Stage >= stages.Count)
                return new(y.Id, $"{y.Kind.Name}: finished", false);
            var st = stages[y.Stage];
            string labor = st.Labor > 0 ? $", {st.Labor} labor" : "";
            if (y.Stage + 1 == stages.Count)
            {
                string ship = y.Kind.Hull.Data().Name;
                return y.Ours ? new(y.Id, $"F: launch the {ship} ({st.Name}{labor})", true) : new(y.Id, $"only the owner can launch this {ship}", false);
            }
            if (st.Pack != 0)
            {
                string pack = GameContent.Item(st.Pack)?.Name ?? "pack";
                return carrying == st.Pack
                    ? new(y.Id, $"F: deliver the {pack} ({st.Name}{labor})", true)
                    : new(y.Id, $"{st.Name} needs {Article(pack)} {pack}", false);
            }
            return new(y.Id, $"F: {st.Name} ({labor.TrimStart(',', ' ')})", true);
        }

        /// <summary>A yard's label: its name, stages done, and the next one.</summary>
        public static string StageLine(ShipyardData kind, byte stage) =>
            stage < kind.Stages.Count ? $"{kind.Name}  {stage}/{kind.Stages.Count}: {kind.Stages[stage].Name} next" : kind.Name;

        /// <summary>A wrecked vehicle item's note in the bag: "wrecked", "mending, N min", or nothing once it's ready.</summary>
        public static string WreckLabel(bool mending, long readyAtMicros, long nowMicros)
        {
            if (!mending)
                return "wrecked";
            if (nowMicros >= readyAtMicros)
                return "";
            long mins = (readyAtMicros - nowMicros + 59_999_999) / 60_000_000;
            return $"mending, {mins} min";
        }

        private static string Article(string name) => "AEIOU".IndexOf(char.ToUpperInvariant(name[0])) >= 0 ? "an" : "a";
    }
}
