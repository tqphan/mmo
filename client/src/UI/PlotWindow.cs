using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.Sim;
using Plot = SpacetimeDB.Types.Plot;

namespace Mmo.UI
{
    /// <summary>
    /// A plot's window (docs/design/land.md): F at a plot opens it. It shows the kind, the owner,
    /// where the tax stands and the times that follow from it, and this week's tax at the account's
    /// rate against the seals held. Owners get Pay tax, an access toggle and Abandon (confirmed).
    /// F closes it, and so does walking away. It draws the server's row every frame.
    /// </summary>
    public partial class PlotWindow : CanvasLayer
    {
        private static readonly Color Short = new(1.0f, 0.45f, 0.4f);
        private static readonly Color Dim = new(0.7f, 0.7f, 0.7f);

        private readonly Session session;
        private readonly Plots plots;
        private readonly Label title = new();
        private readonly Label owner = new();
        private readonly Label standing = new();
        private readonly Label tax = new();
        private readonly Label status = new();
        private readonly Button pay = Plain("Pay a week");
        private readonly Button access = Plain("");
        private readonly Button abandon = Plain("Abandon");
        private readonly ConfirmationDialog confirm = new();
        private PanelContainer panel = null!;

        public PlotWindow(Session session, Plots plots)
        {
            this.session = session;
            this.plots = plots;
            Name = "PlotWindow";
            Layer = 2;
        }

        /// <summary>The plot the window is open at.</summary>
        public Plot? Plot { get; private set; }

        public bool Open => panel.Visible;

        public void Show(Plot p)
        {
            Plot = p;
            panel.Visible = true;
            Refresh();
        }

        public void Close()
        {
            panel.Visible = false;
            Plot = null;
        }

        /// <summary>Press Pay a week.</summary>
        public void PressPay()
        {
            if (Current() is { } p)
                plots.PayTax(p);
        }

        /// <summary>Press the access toggle.</summary>
        public void PressAccess()
        {
            if (Current() is { } p)
                plots.SetAccess(p, p.Access == 1 ? (byte)0 : (byte)1);
        }

        /// <summary>Press Abandon: ask first, unless <paramref name="confirmed"/>.</summary>
        public void PressAbandon(bool confirmed = false)
        {
            if (Current() is not { } p)
                return;
            if (confirmed)
            {
                plots.Abandon(p);
                return;
            }
            confirm.DialogText = $"Abandon this {GameContent.Plot(p.Kind)?.Name}? The deed comes back; paid tax doesn't.";
            confirm.PopupCentered();
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
            // Pinned 16 px from the left edge, below the HUD's readout, like the craft window.
            panel.OffsetLeft = panel.OffsetRight = 16;
            panel.OffsetTop = panel.OffsetBottom = 270;
            panel.GrowHorizontal = Control.GrowDirection.End;
            panel.GrowVertical = Control.GrowDirection.End;
            var box = new VBoxContainer { CustomMinimumSize = new Vector2(320, 0) };
            box.AddThemeConstantOverride("separation", 6);
            title.AddThemeFontSizeOverride("font_size", 18);
            box.AddChild(title);
            foreach (var l in new[] { owner, standing, tax })
            {
                l.AddThemeFontSizeOverride("font_size", 14);
                box.AddChild(l);
            }
            var buttons = new HBoxContainer();
            buttons.AddThemeConstantOverride("separation", 8);
            pay.Pressed += PressPay;
            access.Pressed += PressAccess;
            abandon.Pressed += () => PressAbandon();
            foreach (var b in new[] { pay, access, abandon })
                buttons.AddChild(b);
            box.AddChild(buttons);
            status.AddThemeFontSizeOverride("font_size", 12);
            status.AddThemeColorOverride("font_color", Dim);
            box.AddChild(status);
            panel.AddChild(box);
            AddChild(panel);
            confirm.Title = "Abandon";
            confirm.Confirmed += () => PressAbandon(confirmed: true);
            AddChild(confirm);
        }

        public override void _Process(double delta)
        {
            if (Open)
                Refresh();
        }

        /// <summary>The row as the server has it now.</summary>
        private Plot? Current() => Plot is { } p ? plots.Registry.Find(p.Id) : null;

        private void Refresh()
        {
            if (Current() is not { } p || GameContent.Plot(p.Kind) is not { } kind)
                return;
            Plot = p;
            long now = session.GameNowMicros, paid = p.TaxPaidUntil.MicrosecondsSinceUnixEpoch;
            bool mine = plots.Registry.Mine(p);
            title.Text = $"{kind.Name}  {kind.SizeM} × {kind.SizeM} m";
            owner.Text = $"owner {plots.Registry.OwnerName(p)} · {(p.Access == 1 ? "public" : "private")}";
            var st = Land.StandingOf(paid, now);
            standing.Text = st == Standing.Paid
                ? $"tax paid for {Land.Span(paid - now)}; demolished {Land.Span(Land.DemolishAt(paid) - now)} from now if unpaid"
                : $"tax overdue {Land.Span(now - paid)}; demolished in {Land.Span(Land.DemolishAt(paid) - now)}";
            standing.AddThemeColorOverride("font_color", st == Standing.Paid ? Colors.White : Short);
            int owned = mine ? plots.Registry.Standing.Count(plots.Registry.Mine) : 0;
            uint? week = Land.WeeklyTax(kind, System.Math.Max(owned, 1));
            uint held = (uint)session.Bag.Where(s => s.Def == GameContent.Property.TaxItem).Sum(s => (long)s.Count);
            string seal = GameContent.Item(GameContent.Property.TaxItem)?.Name ?? "seals";
            tax.Text = $"tax {week?.ToString() ?? "?"} {seal}s a week{(mine ? $" ({owned} plots owned); you hold {held}" : "")}";
            tax.AddThemeColorOverride("font_color", mine && week is uint w && held < w ? Short : Colors.White);
            pay.Disabled = !mine || !Land.CanPay(paid, now) || week is not uint need || held < need;
            pay.TooltipText = Land.CanPay(paid, now) ? "" : $"at most {GameContent.Property.PrepayWeeks} weeks ahead";
            access.Text = p.Access == 1 ? "Make private" : "Make public";
            access.Disabled = !mine;
            abandon.Disabled = !mine;
            status.Text = plots.LastResult.Length > 0 ? plots.LastResult : mine ? "F: close" : "not yours · F: close";
        }

        private static Button Plain(string text)
        {
            var b = new Button { Text = text, FocusMode = Control.FocusModeEnum.None };
            b.AddThemeFontSizeOverride("font_size", 14);
            return b;
        }
    }
}
