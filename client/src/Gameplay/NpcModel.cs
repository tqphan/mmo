using System;
using System.Collections.Generic;
using System.Linq;
using Mmo.Content;

namespace Mmo.Gameplay
{
    /// <summary>One stack on a corpse, as the client needs it (<c>loot</c>, docs/design/npcs.md).</summary>
    public readonly record struct LootInfo(ushort Def, uint Count, ulong Owner, long FreeAt);

    /// <summary>What the client shows about NPCs and their loot (docs/design/npcs.md), kept apart from Godot so it can be tested.</summary>
    public static class NpcModel
    {
        /// <summary>The kind an NPC entity's <c>ref_id</c> names, if any.</summary>
        public static NpcKindData? Kind(ulong refId) => refId < (ulong)GameContent.NpcKinds.Count ? GameContent.NpcKinds[(int)refId] : null;

        /// <summary>The placeholder body's colour, 0 to 1 per channel.</summary>
        public static (float R, float G, float B) Color(NpcKindData k) =>
            (((k.Color >> 16) & 0xff) / 255f, ((k.Color >> 8) & 0xff) / 255f, (k.Color & 0xff) / 255f);

        /// <summary>It attacks on sight: its name shows red.</summary>
        public static bool Aggressive(NpcKindData k) => k.AggroM > 0;

        /// <summary>We may take <paramref name="l"/> now: it's our character's, or free.</summary>
        public static bool MayTake(LootInfo l, ulong me, long now) => l.Owner == me || now >= l.FreeAt;

        /// <summary>
        /// The prompt for a corpse with <paramref name="loot"/> on it: what F takes, or how long until
        /// it's ours to take. Empty with nothing on it.
        /// </summary>
        public static string Prompt(string corpse, IReadOnlyList<LootInfo> loot, ulong me, long now)
        {
            if (loot.Count == 0)
                return "";
            if (loot.All(l => MayTake(l, me, now)))
            {
                var what = loot.Select(l => l.Count > 1 ? $"{ItemName(l.Def)} ×{l.Count}" : ItemName(l.Def));
                return $"F: loot the {corpse} ({string.Join(", ", what)})";
            }
            long wait = loot.Where(l => !MayTake(l, me, now)).Max(l => l.FreeAt) - now;
            return $"{corpse}: not yours to loot for {(wait + 999_999) / 1_000_000} s";
        }

        private static string ItemName(ushort def) => GameContent.Item(def)?.Name ?? $"item {def}";
    }
}
