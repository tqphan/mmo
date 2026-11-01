using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using QuestGiver = SpacetimeDB.Types.QuestGiver;

namespace Mmo.UI
{
    /// <summary>
    /// Quests on screen (docs/design/quests.md). F by a giver opens its panel: quests to turn in to
    /// it, with what they pay now, then quests it gives, each with its text, objectives and reward,
    /// and why it waits if it can't be taken yet. L opens the log, where each open quest can be
    /// abandoned (pressed twice). The tracker on the right lists open quests and their objectives
    /// while the log is shut. Panels are rebuilt only when what they show changes, so their buttons
    /// can be clicked; the rules are in <see cref="QuestModel"/>.
    /// </summary>
    public partial class QuestWindow : CanvasLayer
    {
        private static readonly Color Dim = new(0.7f, 0.7f, 0.7f);
        private static readonly Color Gold = new(1.0f, 0.85f, 0.35f);
        private static readonly Color Green = new(0.75f, 0.95f, 0.7f);

        private readonly Session session;
        private readonly Quests quests;
        private readonly Label tracker = new();
        private PanelContainer giverPanel = null!;
        private PanelContainer logPanel = null!;
        private readonly VBoxContainer giverBox = new();
        private readonly VBoxContainer logBox = new();
        private string giverKey = "", logKey = "";
        /// <summary>The quest whose Abandon was pressed once, waiting for the second press.</summary>
        private ushort? abandoning;

        public QuestWindow(Session session, Quests quests)
        {
            this.session = session;
            this.quests = quests;
            Name = "QuestWindow";
            Layer = 2;
        }

        /// <summary>The giver whose panel is open.</summary>
        public QuestGiver? Giver { get; private set; }

        public bool LogOpen => logPanel.Visible;

        public void ShowGiver(QuestGiver g)
        {
            Giver = g;
            giverKey = "";
            giverPanel.Visible = true;
        }

        public void CloseGiver()
        {
            Giver = null;
            giverPanel.Visible = false;
        }

        public void ToggleLog()
        {
            logPanel.Visible = !logPanel.Visible;
            abandoning = null;
            logKey = "";
        }

        /// <summary>Take <paramref name="q"/> from the open giver, as its button would.</summary>
        public void PressTake(QuestData q)
        {
            if (Giver is { } g)
                quests.Accept(g, q);
        }

        /// <summary>Turn <paramref name="q"/> in to the open giver, as its button would.</summary>
        public void PressTurnIn(QuestData q)
        {
            if (Giver is { } g)
                quests.TurnIn(g, q);
        }

        public override void _Ready()
        {
            giverPanel = Panel(giverBox);
            logPanel = Panel(logBox);
            tracker.AddThemeFontSizeOverride("font_size", 14);
            tracker.AddThemeColorOverride("font_color", new Color(0.95f, 0.92f, 0.8f));
            tracker.AddThemeColorOverride("font_outline_color", Colors.Black);
            tracker.AddThemeConstantOverride("outline_size", 4);
            AddChild(tracker);
        }

        /// <summary>A panel at the top centre of the screen, clear of the HUD's readout on the left, the chat box below it and the tracker on the right.</summary>
        private PanelContainer Panel(VBoxContainer box)
        {
            var panel = new PanelContainer { Visible = false, MouseFilter = Control.MouseFilterEnum.Stop };
            panel.AddThemeStyleboxOverride("panel", new StyleBoxFlat
            {
                BgColor = new Color(0.08f, 0.09f, 0.11f, 0.92f),
                ContentMarginLeft = 12, ContentMarginRight = 12, ContentMarginTop = 8, ContentMarginBottom = 10,
                CornerRadiusTopLeft = 6, CornerRadiusTopRight = 6, CornerRadiusBottomLeft = 6, CornerRadiusBottomRight = 6,
            });
            panel.AnchorLeft = panel.AnchorRight = 0.5f;
            panel.AnchorTop = panel.AnchorBottom = 0;
            panel.OffsetLeft = panel.OffsetRight = 0;
            panel.OffsetTop = panel.OffsetBottom = 24;
            panel.GrowHorizontal = Control.GrowDirection.Both;
            panel.GrowVertical = Control.GrowDirection.End;
            box.AddThemeConstantOverride("separation", 6);
            box.CustomMinimumSize = new Vector2(380, 0);
            // Scrolls when it would run into the bars at the bottom of the screen.
            var scroll = new ScrollContainer { HorizontalScrollMode = ScrollContainer.ScrollMode.Disabled };
            scroll.AddChild(box);
            panel.AddChild(scroll);
            AddChild(panel);
            return panel;
        }

        public override void _Process(double delta)
        {
            if (session.Me is null)
                return;
            if (GameInput.JustPressed("quest_log"))
                ToggleLog();
            if (GameInput.JustPressed("ui_cancel"))
            {
                CloseGiver();
                if (LogOpen)
                    ToggleLog();
            }
            var log = quests.Log;
            string state = string.Join(";", log.Select(e => $"{e.Quest}:{e.TurnedIn}:{string.Join(",", e.Done)}")) + $"|{quests.Level}";
            if (Giver is { } g && $"{g.Id}|{state}" != giverKey)
            {
                giverKey = $"{g.Id}|{state}";
                BuildGiver(g, log);
            }
            if (LogOpen && $"{state}|{abandoning}" != logKey)
            {
                logKey = $"{state}|{abandoning}";
                BuildLog(log);
            }
            foreach (var (panel, box) in new[] { (giverPanel, giverBox), (logPanel, logBox) })
                if (panel.Visible && panel.GetChild<ScrollContainer>(0) is { } scroll)
                    scroll.CustomMinimumSize = new Vector2(box.GetCombinedMinimumSize().X + 8, Mathf.Min(box.GetCombinedMinimumSize().Y, GetViewport().GetVisibleRect().Size.Y - 200));
            // The tracker sits at the right, below the party frame, while the log is shut.
            var lines = QuestModel.Tracker(log);
            tracker.Visible = !LogOpen && lines.Count > 0;
            tracker.Text = lines.Count > 0 ? "Quests (L)\n" + string.Join("\n", lines) : "";
            var size = GetViewport().GetVisibleRect().Size;
            tracker.Size = tracker.GetMinimumSize();
            tracker.Position = new Vector2(size.X - tracker.Size.X - 16, 240);
        }

        private static void Clear(VBoxContainer box)
        {
            foreach (var c in box.GetChildren())
                c.QueueFree();
        }

        private void BuildGiver(QuestGiver g, List<QuestEntry> log)
        {
            Clear(giverBox);
            var kind = GameContent.Giver(g.Kind);
            giverBox.AddChild(Text($"{kind?.Name}, {kind?.Title}", 18, Colors.White));
            var turnIns = QuestModel.TurnIns(g.Kind, log);
            var offers = QuestModel.Offers(g.Kind, quests.Level, log);
            foreach (var (q, e) in turnIns)
            {
                var standing = Sim.Quest.Standing(q.Objectives, e.Done);
                bool ready = Sim.Quest.Ready(Sim.Quest.Score(q.Objectives, e.Done));
                giverBox.AddChild(Text((ready ? "? " : "… ") + q.Name + (QuestModel.StandingLabel(standing) is { Length: > 0 } s ? $"  ({s})" : ""), 16, ready ? Green : Dim));
                foreach (var line in QuestModel.Objectives(q, e.Done))
                    giverBox.AddChild(Text("    " + line, 13, Dim));
                if (ready)
                {
                    giverBox.AddChild(Text($"    Pays {QuestModel.Pays(q, e.Done)}", 13, Gold));
                    giverBox.AddChild(Action("Turn in", () => PressTurnIn(q)));
                }
            }
            foreach (var o in offers)
            {
                var q = o.Quest;
                giverBox.AddChild(Text((o.Locked is null ? "! " : "  ") + q.Name + (o.Locked is { } why ? $"  ({why})" : ""), 16, o.Locked is null ? Gold : Dim));
                if (o.Locked is not null)
                    continue;
                giverBox.AddChild(Wrapped(q.Text));
                foreach (var line in QuestModel.Objectives(q, new uint[0]))
                    giverBox.AddChild(Text("    " + line, 13, Dim));
                giverBox.AddChild(Text($"    Reward: {QuestModel.Pays(q, q.Objectives.Select(x => x.Count).ToArray())}", 13, Gold));
                giverBox.AddChild(Action("Take", () => PressTake(q)));
            }
            if (turnIns.Count == 0 && offers.Count == 0)
                giverBox.AddChild(Text("Nothing for you just now.", 14, Dim));
            giverBox.AddChild(Text("F or Esc: close", 11, Dim));
        }

        private void BuildLog(List<QuestEntry> log)
        {
            Clear(logBox);
            var open = log.Where(e => !e.TurnedIn).ToList();
            logBox.AddChild(Text($"Quests ({open.Count}/{GameContent.Quest.Log}) · {log.Count(e => e.TurnedIn)} turned in", 18, Colors.White));
            foreach (var e in open)
            {
                if (GameContent.QuestOf(e.Quest) is not { } q)
                    continue;
                var standing = QuestModel.StandingLabel(Sim.Quest.Standing(q.Objectives, e.Done));
                logBox.AddChild(Text(q.Name + (standing.Length > 0 ? $"  ({standing})" : ""), 16, Gold));
                logBox.AddChild(Wrapped(q.Text));
                foreach (var line in QuestModel.Objectives(q, e.Done))
                    logBox.AddChild(Text("    " + line, 13, Dim));
                logBox.AddChild(Text($"    Turn in to {GameContent.Giver(q.To)?.Name}", 13, Dim));
                ushort wire = q.Wire;
                logBox.AddChild(Action(abandoning == wire ? "Abandon? Press again" : "Abandon", () =>
                {
                    if (abandoning == wire)
                    {
                        quests.Abandon(q);
                        abandoning = null;
                    }
                    else
                        abandoning = wire;
                    logKey = "";
                }));
            }
            if (open.Count == 0)
                logBox.AddChild(Text("No quests under way. Look for a ! over someone in town.", 14, Dim));
            logBox.AddChild(Text("L or Esc: close", 11, Dim));
        }

        private static Label Text(string text, int size, Color color)
        {
            var l = new Label { Text = text };
            l.AddThemeFontSizeOverride("font_size", size);
            l.AddThemeColorOverride("font_color", color);
            return l;
        }

        private static Label Wrapped(string text)
        {
            var l = Text(text, 13, new Color(0.88f, 0.88f, 0.85f));
            l.AutowrapMode = TextServer.AutowrapMode.WordSmart;
            l.CustomMinimumSize = new Vector2(380, 0);
            return l;
        }

        /// <summary>A button that never takes keyboard focus, so F and the movement keys keep working.</summary>
        private static Button Action(string text, System.Action pressed)
        {
            var b = new Button { Text = text, FocusMode = Control.FocusModeEnum.None, SizeFlagsHorizontal = Control.SizeFlags.ShrinkBegin };
            b.Pressed += pressed;
            return b;
        }
    }
}
