using System;
using System.Collections.Generic;
using System.Linq;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>How a quest stands, for the log and the tracker (docs/design/quests.md).</summary>
    public enum QuestStanding : byte
    {
        /// <summary>Not far enough to turn in.</summary>
        Going,
        /// <summary>May be turned in for less than full.</summary>
        Early,
        /// <summary>Every objective met.</summary>
        Done,
        /// <summary>Past done, still counting.</summary>
        Extra,
        /// <summary>As far as it counts.</summary>
        Over,
    }

    /// <summary>
    /// Quests' rules (docs/design/quests.md): how far objectives count, how far along a quest is,
    /// when it may be turned in, what it pays and who may take it. A port of <c>sim::quest</c>,
    /// checked against generated vectors; the client uses it for the quest window, the log and the
    /// givers' markers, and the server decides.
    /// </summary>
    public static class Quest
    {
        /// <summary>How far an objective of <paramref name="need"/> counts: half again for 2 or more, just <paramref name="need"/> for a single act.</summary>
        public static uint Cap(uint need) => need < 2 ? need : (uint)Math.Max((ulong)need * (ulong)GameContent.Quest.OverPct / 100, need);

        /// <summary>An objective's count after <paramref name="add"/> more, up to its cap.</summary>
        public static uint Tally(uint done, uint need, uint add) => (uint)Math.Min((ulong)done + add, Cap(need));

        private static uint At(IReadOnlyList<uint> done, int k) => k < done.Count ? done[k] : 0;

        /// <summary>How far along a quest is, %: its least advanced objective's share, or 100 for an errand.</summary>
        public static uint Score(IReadOnlyList<ObjectiveData> objectives, IReadOnlyList<uint> done)
        {
            if (objectives.Count == 0)
                return 100;
            return objectives.Select((o, k) => (uint)((ulong)Math.Min(At(done, k), Cap(o.Count)) * 100 / Math.Max(o.Count, 1u))).Min();
        }

        public static bool Ready(uint score) => score >= GameContent.Quest.EarlyPct;

        /// <summary>The experience a quest worth <paramref name="xp"/> pays at <paramref name="score"/>.</summary>
        public static uint XpFor(uint xp, uint score) => (uint)Math.Min((ulong)xp * score / 100, uint.MaxValue);

        public static QuestStanding Standing(IReadOnlyList<ObjectiveData> objectives, IReadOnlyList<uint> done)
        {
            uint s = Score(objectives, done);
            bool capped = objectives.Select((o, k) => At(done, k) >= Cap(o.Count)).All(c => c);
            if (!Ready(s))
                return QuestStanding.Going;
            if (s < 100)
                return QuestStanding.Early;
            if (capped && objectives.Any(o => Cap(o.Count) > o.Count))
                return QuestStanding.Over;
            if (objectives.Select((o, k) => At(done, k) > o.Count).Any(x => x))
                return QuestStanding.Extra;
            return QuestStanding.Done;
        }

        /// <summary>Why <paramref name="q"/> can't be taken by a character at <paramref name="level"/> who has turned in <paramref name="turnedIn"/>, or null if it can: "level:N" or "after:W".</summary>
        public static string? Refusal(QuestData q, byte level, IReadOnlyCollection<ushort> turnedIn)
        {
            if (level < q.Level)
                return $"level:{q.Level}";
            foreach (var a in q.After)
                if (!turnedIn.Contains(a))
                    return $"after:{a}";
            return null;
        }
    }
}
