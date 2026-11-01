using System.Collections.Generic;
using System.Linq;
using Mmo.Content;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>One quest in our log, as <c>my_quests</c> has it.</summary>
    public sealed record QuestEntry(ushort Quest, IReadOnlyList<uint> Done, bool TurnedIn);

    /// <summary>What a giver offers us: a quest we may take now, or one we can't yet and why.</summary>
    public sealed record QuestOffer(QuestData Quest, string? Locked);

    /// <summary>
    /// The quest window's, the log's and the givers' markers' rules (docs/design/quests.md), with
    /// no Godot in them so xUnit can check them. Everything comes from content and our log; the
    /// server decides.
    /// </summary>
    public static class QuestModel
    {
        private static QuestEntry? Entry(IEnumerable<QuestEntry> log, ushort quest) => log.FirstOrDefault(e => e.Quest == quest);

        private static HashSet<ushort> TurnedIn(IEnumerable<QuestEntry> log) => log.Where(e => e.TurnedIn).Select(e => e.Quest).ToHashSet();

        /// <summary>The quests giver <paramref name="giver"/> gives that we've never taken, those we may take first.</summary>
        public static List<QuestOffer> Offers(byte giver, byte level, IReadOnlyCollection<QuestEntry> log)
        {
            var done = TurnedIn(log);
            return GameContent.Quests
                .Where(q => q.Giver == giver && Entry(log, q.Wire) is null)
                .Select(q => new QuestOffer(q, Locked(Quest.Refusal(q, level, done))))
                .OrderBy(o => o.Locked is not null)
                .ThenBy(o => o.Quest.Wire)
                .ToList();
        }

        /// <summary>Our open quests that are turned in to <paramref name="giver"/>.</summary>
        public static List<(QuestData Quest, QuestEntry Entry)> TurnIns(byte giver, IEnumerable<QuestEntry> log) =>
            log.Where(e => !e.TurnedIn)
                .Select(e => (Quest: GameContent.QuestOf(e.Quest), Entry: e))
                .Where(p => p.Quest is not null && p.Quest.To == giver)
                .Select(p => (p.Quest!, p.Entry))
                .ToList();

        /// <summary>What floats over a giver: ? when something is ready to turn in, ! when a quest may be taken, … when something of theirs is under way, else nothing.</summary>
        public static string Marker(byte giver, byte level, IReadOnlyCollection<QuestEntry> log)
        {
            var turnIns = TurnIns(giver, log);
            if (turnIns.Any(t => Quest.Ready(Quest.Score(t.Quest.Objectives, t.Entry.Done))))
                return "?";
            if (Offers(giver, level, log).Any(o => o.Locked is null))
                return "!";
            return turnIns.Count > 0 ? "…" : "";
        }

        /// <summary>Why a quest can't be taken, in words, from <see cref="Quest.Refusal"/>.</summary>
        public static string? Locked(string? refusal)
        {
            if (refusal is null)
                return null;
            var (kind, value) = (refusal.Split(':')[0], refusal.Split(':')[1]);
            return kind == "level" ? $"needs level {value}" : $"after {GameContent.QuestOf(ushort.Parse(value))?.Name ?? "another quest"}";
        }

        /// <summary>What an objective asks, without the count: "Shore Skitter slain", "Grain Bale sold at Gullhaven Post".</summary>
        public static string What(ObjectiveData o)
        {
            string thing = o.Act == QuestAct.Kill ? GameContent.NpcKind((byte)o.Target)?.Name ?? "?" : GameContent.Item(o.Target)?.Name ?? "?";
            string verb = o.Act switch
            {
                QuestAct.Kill => "slain",
                QuestAct.Gather => "gathered",
                QuestAct.Craft => "crafted",
                QuestAct.Harvest => "harvested",
                _ => "sold",
            };
            string at = o.Post is byte p && GameContent.Post(p) is { } post ? $" at {post.Name}" : "";
            return $"{thing} {verb}{at}";
        }

        /// <summary>One line per objective, "Shore Skitter slain 3/6", or the errand for one with none.</summary>
        public static List<string> Objectives(QuestData q, IReadOnlyList<uint> done)
        {
            if (q.Objectives.Count == 0)
                return new List<string> { $"Speak to {GameContent.Giver(q.To)?.Name}" };
            return q.Objectives.Select((o, k) => $"{What(o)} {(k < done.Count ? done[k] : 0)}/{o.Count}").ToList();
        }

        public static string StandingLabel(QuestStanding s) => s switch
        {
            QuestStanding.Early => "can turn in early",
            QuestStanding.Done => "done",
            QuestStanding.Extra => "done, extra",
            QuestStanding.Over => "overachieved",
            _ => "",
        };

        /// <summary>What turning in pays now: "180 experience, 2 Emberleaf Salve".</summary>
        public static string Pays(QuestData q, IReadOnlyList<uint> done)
        {
            var parts = new List<string> { $"{Quest.XpFor(q.Xp, Quest.Score(q.Objectives, done))} experience" };
            parts.AddRange(q.Rewards.Select(r => $"{r.Count} {GameContent.Item(r.Item)?.Name}"));
            return string.Join(", ", parts);
        }

        /// <summary>The tracker: each open quest's name with its standing, then its objectives, indented.</summary>
        public static List<string> Tracker(IEnumerable<QuestEntry> log)
        {
            var lines = new List<string>();
            foreach (var e in log.Where(e => !e.TurnedIn).OrderBy(e => e.Quest))
            {
                if (GameContent.QuestOf(e.Quest) is not { } q)
                    continue;
                string label = StandingLabel(Quest.Standing(q.Objectives, e.Done));
                lines.Add(label == "" ? q.Name : $"{q.Name} ({label})");
                lines.AddRange(Objectives(q, e.Done).Select(l => "  " + l));
            }
            return lines;
        }
    }
}
