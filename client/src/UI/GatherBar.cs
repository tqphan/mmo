using Godot;
using Mmo.Gameplay;

namespace Mmo.UI
{
    /// <summary>A progress bar low in the middle of the screen while we gather (docs/design/gathering.md).</summary>
    public partial class GatherBar : CanvasLayer
    {
        private readonly Gathering gathering;
        private readonly Control root = new();
        private readonly ProgressBar bar = new();
        private readonly Label label = new();

        public GatherBar(Gathering gathering)
        {
            this.gathering = gathering;
            Name = "GatherBar";
        }

        public override void _Ready()
        {
            root.SetAnchorsPreset(Control.LayoutPreset.CenterBottom);
            root.Position = new Vector2(-160, -140);
            root.Size = new Vector2(320, 48);
            AddChild(root);
            label.Size = new Vector2(320, 22);
            label.HorizontalAlignment = HorizontalAlignment.Center;
            label.AddThemeColorOverride("font_outline_color", Colors.Black);
            label.AddThemeConstantOverride("outline_size", 4);
            root.AddChild(label);
            bar.Position = new Vector2(0, 24);
            bar.Size = new Vector2(320, 18);
            bar.MaxValue = 1.0;
            bar.ShowPercentage = false;
            bar.AddThemeStyleboxOverride("fill", new StyleBoxFlat { BgColor = new Color(0.95f, 0.75f, 0.25f) });
            bar.AddThemeStyleboxOverride("background", new StyleBoxFlat { BgColor = new Color(0, 0, 0, 0.6f) });
            root.AddChild(bar);
            root.Visible = false;
        }

        public override void _Process(double delta)
        {
            root.Visible = gathering.Bar is not null;
            if (gathering.Bar is { } b)
            {
                label.Text = $"Gathering {b.Kind.Name}";
                bar.Value = b.Progress;
            }
        }
    }
}
