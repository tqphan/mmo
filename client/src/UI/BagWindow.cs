using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;

namespace Mmo.UI
{
    /// <summary>
    /// The bag (docs/design/items.md): I or B opens it. Click a stack to pick it up and a slot to put
    /// it there, shift-click to split off half, ctrl + right-click to destroy (after a confirmation),
    /// Esc to put down what's held. It draws the server's rows every frame; the click rules are in
    /// <see cref="BagModel"/>.
    /// </summary>
    public partial class BagWindow : CanvasLayer
    {
        public const int Columns = 10;
        private const float SlotPx = 72;

        private readonly Session session;
        private readonly Slot[] slots = new Slot[GameContent.BagSlots];
        private readonly Label title = new();
        private readonly ConfirmationDialog confirm = new();
        private PanelContainer panel = null!;
        private DestroyIntent? pending;

        public BagWindow(Session session)
        {
            this.session = session;
            Name = "BagWindow";
            Layer = 2;
        }

        public BagModel Model { get; } = new();

        public BagStack?[] Grid { get; private set; } = new BagStack?[GameContent.BagSlots];

        public bool Open
        {
            get => panel.Visible;
            set
            {
                panel.Visible = value;
                if (!value)
                    Model.Cancel();
            }
        }

        public override void _Ready()
        {
            panel = new PanelContainer { Visible = false, MouseFilter = Control.MouseFilterEnum.Stop };
            panel.AddThemeStyleboxOverride("panel", new StyleBoxFlat
            {
                BgColor = new Color(0.08f, 0.09f, 0.11f, 0.92f),
                ContentMarginLeft = 10, ContentMarginRight = 10, ContentMarginTop = 8, ContentMarginBottom = 10,
                CornerRadiusTopLeft = 6, CornerRadiusTopRight = 6, CornerRadiusBottomLeft = 6, CornerRadiusBottomRight = 6,
            });
            // Pinned 16 px from the right edge, centred vertically, growing left and both ways to fit.
            panel.AnchorLeft = panel.AnchorRight = 1;
            panel.AnchorTop = panel.AnchorBottom = 0.5f;
            panel.OffsetLeft = panel.OffsetRight = -16;
            panel.GrowHorizontal = Control.GrowDirection.Begin;
            panel.GrowVertical = Control.GrowDirection.Both;
            var box = new VBoxContainer();
            box.AddThemeConstantOverride("separation", 6);
            title.AddThemeFontSizeOverride("font_size", 16);
            box.AddChild(title);
            var grid = new GridContainer { Columns = Columns };
            grid.AddThemeConstantOverride("h_separation", 4);
            grid.AddThemeConstantOverride("v_separation", 4);
            for (int i = 0; i < slots.Length; i++)
            {
                var s = new Slot((ushort)i);
                s.GuiInput += e => OnSlotInput(s.Index, e);
                slots[i] = s;
                grid.AddChild(s);
            }
            box.AddChild(grid);
            var help = new Label { Text = "click: move · shift-click: split half · ctrl+right-click: destroy · Esc: put down" };
            help.AddThemeFontSizeOverride("font_size", 11);
            help.AddThemeColorOverride("font_color", new Color(0.7f, 0.7f, 0.7f));
            box.AddChild(help);
            panel.AddChild(box);
            AddChild(panel);
            confirm.Title = "Destroy";
            confirm.Confirmed += () =>
            {
                if (pending is { } d)
                    Send(d);
                pending = null;
            };
            confirm.Canceled += () => pending = null;
            AddChild(confirm);
        }

        public override void _Process(double delta)
        {
            if (session.Me is null)
                return;
            if (Input.IsActionJustPressed("bag"))
                Open = !Open;
            if (!Open)
                return;
            if (Input.IsActionJustPressed("ui_cancel"))
                Model.Cancel();
            Grid = BagModel.Grid(session.Bag, GameContent.BagSlots);
            Model.Refresh(Grid);
            var held = Model.Holding;
            for (int i = 0; i < slots.Length; i++)
                slots[i].Show(Grid[i], held is { } h && h.Slot == i ? h : null);
            int used = Grid.Count(s => s is not null);
            title.Text = held is { } hh
                ? $"Bag  {used} / {Grid.Length}   holding {(hh.Split is uint n ? $"{n} (split)" : "a stack")}"
                : $"Bag  {used} / {Grid.Length}";
        }

        /// <summary>A left click on <paramref name="slot"/>, as the mouse would make it.</summary>
        public void ClickSlot(ushort slot, bool shift)
        {
            if (Model.Click(Grid, slot, shift) is { } intent)
                Send(intent);
        }

        /// <summary>Ctrl + right-click on <paramref name="slot"/>: ask first, unless <paramref name="confirmed"/>.</summary>
        public void DestroySlot(ushort slot, bool confirmed = false)
        {
            if (Model.DestroyClick(Grid, slot) is not DestroyIntent d)
                return;
            if (confirmed)
            {
                Send(d);
                return;
            }
            pending = d;
            var name = Grid[slot] is { } s ? GameContent.Item(s.Def)?.Name ?? $"item {s.Def}" : "?";
            confirm.DialogText = $"Destroy {d.Count} {name}?";
            confirm.PopupCentered();
        }

        /// <summary>Press OK on the open destroy confirmation.</summary>
        public void ConfirmDestroy()
        {
            var d = pending;
            pending = null;
            confirm.Hide();
            if (d is not null)
                Send(d);
        }

        private void OnSlotInput(ushort slot, InputEvent e)
        {
            if (e is not InputEventMouseButton { Pressed: true } b)
                return;
            if (b.ButtonIndex == MouseButton.Left)
                ClickSlot(slot, b.ShiftPressed);
            else if (b.ButtonIndex == MouseButton.Right && b.CtrlPressed)
                DestroySlot(slot);
            else
                return;
            slots[slot].AcceptEvent();
        }

        private void Send(BagIntent intent)
        {
            if (session.Conn is not { } c)
                return;
            switch (intent)
            {
                case MoveIntent m: c.Reducers.MoveItem(m.Id, m.To); break;
                case SplitIntent s: c.Reducers.SplitItem(s.Id, s.Count, s.To); break;
                case DestroyIntent d: c.Reducers.DestroyItem(d.Id, d.Count); break;
            }
        }

        /// <summary>One slot: a coloured square with the item's name and count.</summary>
        private partial class Slot : Panel
        {
            private readonly StyleBoxFlat style = new()
            {
                BorderWidthLeft = 2, BorderWidthRight = 2, BorderWidthTop = 2, BorderWidthBottom = 2,
                CornerRadiusTopLeft = 3, CornerRadiusTopRight = 3, CornerRadiusBottomLeft = 3, CornerRadiusBottomRight = 3,
            };
            private readonly Label name = new();
            private readonly Label count = new();

            public Slot(ushort index)
            {
                Index = index;
                CustomMinimumSize = new Vector2(SlotPx, SlotPx);
                MouseFilter = MouseFilterEnum.Stop;
                AddThemeStyleboxOverride("panel", style);
                name.AutowrapMode = TextServer.AutowrapMode.WordSmart;
                name.ClipText = true;
                name.Position = new Vector2(3, 2);
                name.Size = new Vector2(SlotPx - 6, SlotPx - 18);
                name.AddThemeFontSizeOverride("font_size", 11);
                name.MouseFilter = MouseFilterEnum.Ignore;
                count.HorizontalAlignment = HorizontalAlignment.Right;
                count.Position = new Vector2(2, SlotPx - 19);
                count.Size = new Vector2(SlotPx - 6, 16);
                count.AddThemeFontSizeOverride("font_size", 12);
                count.AddThemeColorOverride("font_outline_color", Colors.Black);
                count.AddThemeConstantOverride("outline_size", 3);
                count.MouseFilter = MouseFilterEnum.Ignore;
                AddChild(name);
                AddChild(count);
            }

            public ushort Index { get; }

            public void Show(BagStack? stack, BagModel.Held? held)
            {
                var def = stack is { } s ? GameContent.Item(s.Def) : null;
                style.BgColor = stack is null ? new Color(0.16f, 0.17f, 0.19f) : KindColor(def?.Kind);
                style.BorderColor = held is not null ? new Color(1.0f, 0.85f, 0.3f) : new Color(0.28f, 0.29f, 0.32f);
                name.Text = stack is { } st ? def?.Name ?? $"item {st.Def}" : "";
                count.Text = stack is { Count: > 1 } c ? (held?.Split is uint n ? $"{n}/{c.Count}" : c.Count.ToString()) : "";
                TooltipText = stack is { } t ? $"{name.Text} × {t.Count}" : "";
            }

            private static Color KindColor(ItemKind? kind) => kind switch
            {
                ItemKind.Material => new Color(0.30f, 0.36f, 0.24f),
                ItemKind.Consumable => new Color(0.48f, 0.30f, 0.20f),
                ItemKind.Gear => new Color(0.24f, 0.32f, 0.46f),
                _ => new Color(0.4f, 0.2f, 0.4f),
            };
        }
    }
}
