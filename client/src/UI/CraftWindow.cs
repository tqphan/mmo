using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using Station = SpacetimeDB.Types.Station;

namespace Mmo.UI
{
    /// <summary>
    /// The craft window (docs/design/crafting.md): F at a station opens it with that station's
    /// recipes. Each shows its inputs as have/need, its labor and time, and how many units the bag
    /// and labor cover; − + and Max set the count, Craft starts it and Cancel stops it. F or Esc
    /// closes it, and so does walking away. It draws what the server says every frame; the rules
    /// are in <see cref="CraftModel"/>.
    /// </summary>
    public partial class CraftWindow : CanvasLayer
    {
        private static readonly Color Short = new(1.0f, 0.45f, 0.4f);
        private static readonly Color Enough = new(0.75f, 0.95f, 0.7f);
        private static readonly Color Dim = new(0.7f, 0.7f, 0.7f);

        private readonly Session session;
        private readonly Crafting crafting;
        private readonly Label title = new();
        private readonly VBoxContainer list = new();
        private readonly Label output = new();
        private readonly VBoxContainer inputs = new();
        private readonly Label cost = new();
        private readonly Label afford = new();
        private readonly Label count = new();
        private readonly Label status = new();
        private readonly Button craft = Plain("Craft");
        private readonly Button cancel = Plain("Cancel");
        private readonly List<(ushort Wire, Button Button)> rows = new();
        private PanelContainer panel = null!;
        private IReadOnlyList<RecipeLine> lines = new List<RecipeLine>();

        public CraftWindow(Session session, Crafting crafting)
        {
            this.session = session;
            this.crafting = crafting;
            Name = "CraftWindow";
            Layer = 2;
        }

        public CraftModel Model { get; private set; } = new();

        /// <summary>The station the window is open at.</summary>
        public Station? Station { get; private set; }

        public bool Open => panel.Visible;

        public void Show(Station station)
        {
            if (Station?.Id != station.Id)
                Model = new CraftModel();
            Station = station;
            var kind = GameContent.Station(station.Kind);
            title.Text = kind?.Name ?? $"station {station.Kind}";
            foreach (var (_, b) in rows)
                b.QueueFree();
            rows.Clear();
            foreach (var r in GameContent.Recipes.Where(r => r.Station == station.Kind))
            {
                var b = Plain("");
                b.Alignment = HorizontalAlignment.Left;
                b.ToggleMode = true;
                b.CustomMinimumSize = new Vector2(230, 0);
                ushort wire = r.Wire;
                b.Pressed += () => Select(wire);
                list.AddChild(b);
                rows.Add((wire, b));
            }
            if (Model.Selected is null && rows.Count > 0)
                Model.Select(rows[0].Wire);
            panel.Visible = true;
            Refresh();
        }

        public void Close()
        {
            panel.Visible = false;
            Station = null;
        }

        /// <summary>Pick a recipe by wire, as clicking its row would.</summary>
        public void Select(ushort recipe)
        {
            Model.Select(recipe);
            Refresh();
        }

        /// <summary>Press Craft.</summary>
        public void PressCraft()
        {
            if (Station is { } st && Model.CraftIntent(lines, crafting.Job is not null) is { } i && GameContent.Recipe(i.Recipe) is { } r)
                crafting.Start(st, r, i.Count);
        }

        public override void _Ready()
        {
            panel = new PanelContainer { Visible = false, MouseFilter = Control.MouseFilterEnum.Stop };
            panel.AddThemeStyleboxOverride("panel", new StyleBoxFlat
            {
                BgColor = new Color(0.08f, 0.09f, 0.11f, 0.92f),
                ContentMarginLeft = 12, ContentMarginRight = 12, ContentMarginTop = 8, ContentMarginBottom = 10,
                CornerRadiusTopLeft = 6, CornerRadiusTopRight = 6, CornerRadiusBottomLeft = 6, CornerRadiusBottomRight = 6,
            });
            // Pinned 16 px from the left edge, below the HUD's readout, growing right and down to fit.
            panel.AnchorLeft = panel.AnchorRight = 0;
            panel.AnchorTop = panel.AnchorBottom = 0;
            panel.OffsetLeft = panel.OffsetRight = 16;
            panel.OffsetTop = panel.OffsetBottom = 270;
            panel.GrowHorizontal = Control.GrowDirection.End;
            panel.GrowVertical = Control.GrowDirection.End;

            var box = new VBoxContainer();
            box.AddThemeConstantOverride("separation", 8);
            title.AddThemeFontSizeOverride("font_size", 18);
            box.AddChild(title);
            var columns = new HBoxContainer();
            columns.AddThemeConstantOverride("separation", 14);
            list.AddThemeConstantOverride("separation", 2);
            columns.AddChild(list);

            var detail = new VBoxContainer { CustomMinimumSize = new Vector2(280, 0) };
            detail.AddThemeConstantOverride("separation", 6);
            output.AddThemeFontSizeOverride("font_size", 16);
            detail.AddChild(output);
            detail.AddChild(Caption("Needs, for each:"));
            detail.AddChild(inputs);
            cost.AddThemeColorOverride("font_color", Dim);
            detail.AddChild(cost);
            detail.AddChild(afford);
            var counter = new HBoxContainer();
            counter.AddThemeConstantOverride("separation", 4);
            var less = Plain("−");
            less.Pressed += () => { Model.Step(lines, -1); Refresh(); };
            var more = Plain("+");
            more.Pressed += () => { Model.Step(lines, +1); Refresh(); };
            var max = Plain("Max");
            max.Pressed += () => { Model.Max(lines); Refresh(); };
            count.CustomMinimumSize = new Vector2(44, 0);
            count.HorizontalAlignment = HorizontalAlignment.Center;
            counter.AddChild(less);
            counter.AddChild(count);
            counter.AddChild(more);
            counter.AddChild(max);
            detail.AddChild(counter);
            var actions = new HBoxContainer();
            actions.AddThemeConstantOverride("separation", 8);
            craft.CustomMinimumSize = new Vector2(110, 0);
            craft.Pressed += PressCraft;
            cancel.Pressed += crafting.Cancel;
            actions.AddChild(craft);
            actions.AddChild(cancel);
            detail.AddChild(actions);
            detail.AddChild(status);
            columns.AddChild(detail);
            box.AddChild(columns);
            box.AddChild(Caption("F or Esc: close · moving cancels a craft"));
            panel.AddChild(box);
            AddChild(panel);
        }

        public override void _Process(double delta)
        {
            if (!Open)
                return;
            if (session.Me is null || Input.IsActionJustPressed("ui_cancel"))
            {
                Close();
                return;
            }
            Refresh();
        }

        private void Refresh()
        {
            if (Station is not { } st)
                return;
            int labor = session.Labor?.Current(GameContent.Labor, session.GameNowMicros) ?? 0;
            lines = CraftModel.Lines(st.Kind, session.Bag, labor);
            Model.Refresh(lines);
            foreach (var (wire, b) in rows)
            {
                var l = lines.FirstOrDefault(x => x.Recipe.Wire == wire);
                if (l is null)
                    continue;
                string made = l.Recipe.OutputCount > 1 ? $" ×{l.Recipe.OutputCount}" : "";
                b.Text = $"{l.Output?.Name ?? l.Recipe.Id}{made}" + (l.Affordable > 0 ? $"   ({l.Affordable})" : "");
                b.SetPressedNoSignal(Model.Selected == wire);
                b.AddThemeColorOverride("font_color", l.Affordable > 0 ? Colors.White : Dim);
            }
            var job = crafting.Job;
            if (Model.Current(lines) is { } cur)
            {
                var r = cur.Recipe;
                output.Text = $"{cur.Output?.Name ?? r.Id}" + (r.OutputCount > 1 ? $" ×{r.OutputCount}" : "");
                SyncInputs(cur.Inputs);
                cost.Text = $"{r.Labor} labor · {r.CraftS:0.#} s each";
                afford.Text = cur.Affordable > 0 ? $"Enough for {cur.Affordable}" : "Not enough to make one";
                afford.AddThemeColorOverride("font_color", cur.Affordable > 0 ? Enough : Short);
            }
            count.Text = Model.Count.ToString();
            craft.Disabled = Model.CraftIntent(lines, job is not null) is null;
            cancel.Disabled = job is null;
            status.Text = crafting.Bar is { } bar
                ? $"Crafting {GameContent.Item(bar.Recipe.Output)?.Name} {bar.Made + 1} / {bar.Count}  {bar.Progress * 100:F0}%"
                : "";
        }

        /// <summary>One line per input: name, have / need, red when short.</summary>
        private void SyncInputs(IReadOnlyList<InputLine> want)
        {
            while (inputs.GetChildCount() < want.Count)
                inputs.AddChild(new Label());
            for (int i = 0; i < inputs.GetChildCount(); i++)
            {
                var label = inputs.GetChild<Label>(i);
                label.Visible = i < want.Count;
                if (i >= want.Count)
                    continue;
                var line = want[i];
                label.Text = $"  {line.Item?.Name ?? $"item {line.Wire}"}  {line.Have} / {line.Need}";
                label.AddThemeColorOverride("font_color", line.Short ? Short : Enough);
            }
        }

        private static Label Caption(string text)
        {
            var l = new Label { Text = text };
            l.AddThemeFontSizeOverride("font_size", 11);
            l.AddThemeColorOverride("font_color", Dim);
            return l;
        }

        /// <summary>A button that never takes keyboard focus, so F, Space and the movement keys keep working.</summary>
        private static Button Plain(string text) => new() { Text = text, FocusMode = Control.FocusModeEnum.None };
    }
}
