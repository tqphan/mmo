using System;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.World;

namespace Mmo.UI
{
    /// <summary>A debug readout: connection, position, labor, land use, cell windows, entities in view, corrections, the ship we're on or next to, and resource nodes and stations in reach.</summary>
    public partial class Hud : CanvasLayer
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly LandRaster? land;
        private readonly ShipControls controls;
        private readonly Gathering gathering;
        private readonly Crafting crafting;
        private readonly Label label = new();

        /// <summary>More prompts, each shown on its own line when not empty (land: docs/design/land.md).</summary>
        public System.Collections.Generic.List<Func<string>> Prompts { get; } = new();

        public Hud(Session session, PlayerController player, EntityRegistry entities, LandRaster? land, ShipControls controls, Gathering gathering, Crafting crafting)
        {
            this.gathering = gathering;
            this.crafting = crafting;
            this.controls = controls;
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.land = land;
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
                text += $"\nentity {me.EntityId}  pos ({p.X:F1}, {p.Y:F1}, {p.Z:F1})  {player.Mode.Data().Id}";
                text += $"\ncorrections {me.Corrections}  moves sent {player.MovesSent}";
                if (session.Labor is { } pool)
                    text += "\n" + LaborLine(pool, session.GameNowMicros);
                if (land?.At(p.X, p.Z) is byte t)
                {
                    var use = LandRaster.Use(t).Data();
                    text += $"\nland {use.Id}{(use.Buildable ? " (buildable)" : "")}  climate {LandRaster.ClimateOf(t).Id()}";
                }
            }
            if (session.Near is { } near && near.Center is ulong c)
            {
                var (_, cy, cx) = Sim.Cell.Unpack(c);
                text += $"\nnear window ({cx}, {cy})  cells {near.HeldCount}/{near.WantedCount}{(near.Ready ? "" : " loading")}  moved {near.Recenters}x";
            }
            if (session.Static is { } st && st.Center is not null)
                text += $"\nstatic cells {st.HeldCount}/{st.WantedCount}{(st.Ready ? "" : " loading")}";
            text += $"\nin view {entities.Count}  {Engine.GetFramesPerSecond():F0} fps";
            if (controls.Prompt.Length > 0)
                text += "\n" + controls.Prompt;
            if (gathering.Prompt.Length > 0)
                text += "\n" + gathering.Prompt;
            if (crafting.Prompt.Length > 0)
                text += "\n" + crafting.Prompt;
            foreach (var prompt in Prompts)
                if (prompt() is { Length: > 0 } line)
                    text += "\n" + line;
            label.Text = text;
        }

        /// <summary><c>labor 4,980 / 5,000 (+10 in 2:31)</c>, or <c>(full)</c> (docs/design/labor.md).</summary>
        public static string LaborLine(Sim.LaborPool pool, long now)
        {
            var rules = GameContent.Labor;
            var line = FormattableString.Invariant($"labor {pool.Current(rules, now):N0} / {rules.Cap:N0}");
            if (pool.NextRegenAt(rules, now) is not long next)
                return line + " (full)";
            long secs = Math.Max(0, (next - now + 999_999) / 1_000_000);
            return line + $" (+{rules.RegenPoints} in {secs / 60}:{secs % 60:D2})";
        }
    }
}
