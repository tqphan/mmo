using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Quests (docs/design/quests.md). F by a giver opens the quest window for it; the window takes
    /// and turns in through here, and the log abandons. Every frame the givers' markers follow our
    /// log from <c>my_quests</c>, and the server's answers to what we asked are kept as
    /// <see cref="LastResult"/> for a few seconds. The rules are in <see cref="QuestModel"/>; the
    /// server decides.
    /// </summary>
    public partial class Quests : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly GiverRegistry givers;
        private readonly Combat combat;
        private string lastResult = "";
        private System.DateTime resultUntil;

        public Quests(Session session, PlayerController player, GiverRegistry givers, Combat combat)
        {
            this.session = session;
            this.player = player;
            this.givers = givers;
            this.combat = combat;
            Name = "Quests";
        }

        /// <summary>The quest window; set by <see cref="Main"/>.</summary>
        public UI.QuestWindow? Window { get; set; }

        /// <summary>What the HUD says about givers nearby.</summary>
        public string Prompt { get; private set; } = "";

        /// <summary>The server's answer to our last take, turn-in or abandon, while it's fresh.</summary>
        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        /// <summary>Quests turned in this session, for the demo.</summary>
        public int TurnedIn { get; private set; }

        public byte Level => combat.Level;

        /// <summary>Our character's log, open quests and turned-in ones.</summary>
        public List<QuestEntry> Log =>
            session.Conn is { } c && session.Me is { } me
                ? c.Db.MyQuests.Iter().Where(e => e.CharId == me.CharId).Select(e => new QuestEntry(e.Quest, e.Done, e.TurnedInAt is not null)).ToList()
                : new List<QuestEntry>();

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnAcceptQuest += (ctx, _, quest) => Answer(ctx.Event.Status, $"took {QuestName(quest)}");
            r.OnTurnInQuest += (ctx, _, quest) =>
            {
                if (ctx.Event.Status is Status.Committed)
                    TurnedIn++;
                Answer(ctx.Event.Status, $"turned in {QuestName(quest)}");
            };
            r.OnAbandonQuest += (ctx, quest) => Answer(ctx.Event.Status, $"abandoned {QuestName(quest)}");
        }

        private static string QuestName(ushort quest) => GameContent.QuestOf(quest)?.Name ?? $"quest {quest}";

        private void Answer(Status status, string ok)
        {
            lastResult = status is Status.Failed(var reason) ? $"refused: {reason}" : ok;
            resultUntil = System.DateTime.UtcNow.AddSeconds(5);
            session.Log($"quests: {lastResult}");
        }

        public void Accept(QuestGiver giver, QuestData q) => session.Conn?.Reducers.AcceptQuest(giver.Id, q.Wire);

        public void TurnIn(QuestGiver giver, QuestData q) => session.Conn?.Reducers.TurnInQuest(giver.Id, q.Wire);

        public void Abandon(QuestData q) => session.Conn?.Reducers.AbandonQuest(q.Wire);

        /// <summary>F was pressed on land: open (or close) the quest window for the nearest giver in reach. False if there's none.</summary>
        public bool Interact()
        {
            if (Window is not { } w || player.Parent != 0)
                return false;
            if (w.Giver is not null)
            {
                w.CloseGiver();
                return true;
            }
            if (NearestInReach() is not { } g)
                return false;
            w.ShowGiver(g);
            return true;
        }

        public override void _Process(double delta)
        {
            var log = Log;
            foreach (var g in givers.Rows)
                givers.SetMarker(g.Id, QuestModel.Marker(g.Kind, Level, log));
            if (Window is { Giver: { } open } w && (givers.Find(open.Id) is null || !InReach(open)))
                w.CloseGiver();
            Prompt = player.Parent == 0 && NearestInReach() is { } n && GameContent.Giver(n.Kind) is { } k
                ? Window?.Giver is not null ? $"F: leave {k.Name}" : $"F: talk to {k.Name}"
                : "";
            if (LastResult is { Length: > 0 } said)
                Prompt = Prompt.Length > 0 ? $"{Prompt}\n{said}" : said;
        }

        public bool InReach(QuestGiver g)
        {
            var at = player.GlobalPosition;
            float r = GameContent.Quest.ReachM;
            float dx = at.X - g.X, dz = at.Z - g.Z;
            return dx * dx + dz * dz <= r * r && Mathf.Abs(at.Y - g.Y) <= r;
        }

        /// <summary>The nearest giver in reach.</summary>
        public QuestGiver? NearestInReach()
        {
            var at = player.GlobalPosition;
            return givers.Rows.Where(InReach).OrderBy(g => new Vector2(g.X - at.X, g.Z - at.Z).LengthSquared()).FirstOrDefault();
        }

        /// <summary>The nearest giver of any reach, for the demo.</summary>
        public QuestGiver? Nearest()
        {
            var at = player.GlobalPosition;
            return givers.Rows.OrderBy(g => new Vector2(g.X - at.X, g.Z - at.Z).LengthSquared()).FirstOrDefault();
        }
    }
}
