using System;
using System.Collections.Generic;
using System.Linq;
using Mmo.Content;

namespace Mmo.Gameplay
{
    /// <summary>One input as the craft window shows it: what a unit needs and what the bag holds.</summary>
    public sealed record InputLine(ItemData? Item, ushort Wire, uint Need, uint Have)
    {
        public bool Short => Have < Need;
    }

    /// <summary>One recipe as the craft window shows it.</summary>
    public sealed record RecipeLine(RecipeData Recipe, ItemData? Output, IReadOnlyList<InputLine> Inputs, uint Affordable);

    /// <summary>
    /// The craft window's rules (docs/design/crafting.md), with no Godot in them: the station's
    /// recipes with have/need per input, how many units the bag and labor cover, and the count to
    /// ask for. The server decides; this only stops the window asking for what it would refuse.
    /// </summary>
    public sealed class CraftModel
    {
        /// <summary>Every recipe made at station kind <paramref name="station"/>, given the bag and the labor now.</summary>
        public static IReadOnlyList<RecipeLine> Lines(byte station, IEnumerable<BagStack> bag, int labor)
        {
            var held = new Dictionary<ushort, uint>();
            foreach (var s in bag)
                held[s.Def] = (held.TryGetValue(s.Def, out var n) ? n : 0) + s.Count;
            uint Have(ushort wire) => held.TryGetValue(wire, out var n) ? n : 0;
            return GameContent.Recipes
                .Where(r => r.Station == station)
                .Select(r => new RecipeLine(
                    r,
                    GameContent.Item(r.Output),
                    r.Inputs.Select(i => new InputLine(GameContent.Item(i.Item), i.Item, i.Count, Have(i.Item))).ToList(),
                    // Packs are made one at a time, onto the back (docs/design/trade.md).
                    GameContent.Pack(r.Output) is null ? Sim.Craft.Affordable(r, Have, labor) : Math.Min(Sim.Craft.Affordable(r, Have, labor), 1u)))
                .ToList();
        }

        /// <summary>The recipe picked, by wire, if any.</summary>
        public ushort? Selected { get; private set; }

        /// <summary>How many units to ask for; at least 1.</summary>
        public uint Count { get; private set; } = 1;

        public void Select(ushort recipe)
        {
            if (Selected != recipe)
                Count = 1;
            Selected = recipe;
        }

        /// <summary>The selected recipe's line in <paramref name="lines"/>, if it's there.</summary>
        public RecipeLine? Current(IReadOnlyList<RecipeLine> lines) => lines.FirstOrDefault(l => l.Recipe.Wire == Selected);

        /// <summary>Change the count by <paramref name="delta"/>, kept from 1 to what's affordable (1 when nothing is).</summary>
        public void Step(IReadOnlyList<RecipeLine> lines, int delta)
        {
            uint most = Math.Max(1, Current(lines)?.Affordable ?? 1);
            Count = (uint)Math.Clamp((long)Count + delta, 1, most);
        }

        /// <summary>Ask for as many as are affordable.</summary>
        public void Max(IReadOnlyList<RecipeLine> lines) => Count = Math.Max(1, Current(lines)?.Affordable ?? 1);

        /// <summary>Keep the count within what's affordable as the bag and labor change.</summary>
        public void Refresh(IReadOnlyList<RecipeLine> lines)
        {
            if (Current(lines) is { } l && l.Affordable > 0 && Count > l.Affordable)
                Count = l.Affordable;
        }

        /// <summary>What Craft would send, or null when it shouldn't be pressed: nothing picked, nothing affordable, or a craft already running.</summary>
        public (ushort Recipe, uint Count)? CraftIntent(IReadOnlyList<RecipeLine> lines, bool busy)
        {
            if (busy || Current(lines) is not { } l || l.Affordable == 0)
                return null;
            return (l.Recipe.Wire, Math.Min(Count, l.Affordable));
        }
    }
}
