using Godot;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.World;

namespace Mmo.UI
{
    /// <summary>A debug readout: connection, position, cell window, entities in view, corrections.</summary>
    public partial class Hud : CanvasLayer
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Label label = new();

        public Hud(Session session, PlayerController player, EntityRegistry entities)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            Name = "Hud";
        }

        public override void _Ready()
        {
            label.Position = new Vector2(12, 8);
            label.AddThemeFontSizeOverride("font_size", 16);
            label.AddThemeColorOverride("font_outline_color", Colors.Black);
            label.AddThemeConstantOverride("outline_size", 4);
            AddChild(label);
        }

        public override void _Process(double delta)
        {
            var text = session.Status;
            if (session.Me is { } me)
            {
                var p = player.GlobalPosition;
                text += $"\nentity {me.EntityId}  pos ({p.X:F1}, {p.Y:F1}, {p.Z:F1})";
                text += $"\ncorrections {me.Corrections}  moves sent {player.MovesSent}";
            }
            if (session.Near is { } near && near.Center is ulong c)
            {
                var (_, cy, cx) = Sim.Cell.Unpack(c);
                text += $"\nnear window ({cx}, {cy})  cells {near.HeldCount}/{near.WantedCount}{(near.Ready ? "" : " loading")}  moved {near.Recenters}x";
            }
            text += $"\nin view {entities.Count}  {Engine.GetFramesPerSecond():F0} fps";
            label.Text = text;
        }
    }
}
