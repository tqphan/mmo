using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.Sim;
using Mmo.World;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Placing a plot (docs/design/land.md). Right-clicking a deed in the bag starts it: a square
    /// follows a point in front of the player, snapped like the server's, green where
    /// <see cref="Land.CheckSite"/> passes and red with the reason in the HUD. R turns the marker, F
    /// or a left click places, Esc or a right click cancels. The server decides.
    /// </summary>
    public partial class Placement : Node3D
    {
        private static readonly Color Good = new(0.4f, 1.0f, 0.45f);
        private static readonly Color Bad = new(1.0f, 0.3f, 0.25f);

        private readonly Session session;
        private readonly PlayerController player;
        private readonly HeightMap map;
        private readonly LandRaster? land;
        private readonly PlotRegistry plots;
        private readonly StandardMaterial3D material = new() { ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded, AlbedoColor = Good };
        private readonly MeshInstance3D preview = new() { Name = "Preview", Visible = false };
        private (float X, float Z)? drawn;

        public Placement(Session session, PlayerController player, HeightMap map, LandRaster? land, PlotRegistry plots)
        {
            this.session = session;
            this.player = player;
            this.map = map;
            this.land = land;
            this.plots = plots;
            Name = "Placement";
            preview.MaterialOverride = material;
            AddChild(preview);
        }

        /// <summary>The deed being placed, and what it claims.</summary>
        public (ulong ItemId, PlotData Kind)? Active { get; private set; }

        /// <summary>Quarter turns of the marker.</summary>
        public byte Yaw { get; private set; }

        /// <summary>Where the plot would go, and why it can't (null if it can).</summary>
        public (float X, float Z, SiteError? Error)? Site { get; private set; }

        public string Prompt { get; private set; } = "";

        /// <summary>Aim at this point instead of ahead of the player; for the demo.</summary>
        public (float X, float Z)? Aim { get; set; }

        /// <summary>The server's answer to our last placement, for the HUD and the demo.</summary>
        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        private string lastResult = "";
        private System.DateTime resultUntil;

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnPlacePlot += (ctx, _, x, z, _) => Answer(ctx, $"placed a plot at ({x:F0}, {z:F0})");
        }

        private void Answer(ReducerEventContext ctx, string ok)
        {
            lastResult = ctx.Event.Status is Status.Failed(var reason) ? $"refused: {reason}" : ok;
            // Shown for a few seconds.
            resultUntil = System.DateTime.UtcNow.AddSeconds(6);
            session.Log($"land: {lastResult}");
        }

        /// <summary>Start placing the deed in <paramref name="stack"/>; false if it isn't one.</summary>
        public bool Begin(BagStack stack)
        {
            if (GameContent.PlotByDeed(stack.Def) is not PlotData kind)
                return false;
            Active = (stack.Id, kind);
            session.Log($"placing a {kind.Name}: F or click to place, R to turn, Esc to cancel");
            return true;
        }

        public void Cancel()
        {
            Active = null;
            Site = null;
            preview.Visible = false;
        }

        public void Turn() => Yaw = (byte)((Yaw + 1) % 4);

        /// <summary>Place it where the preview stands, if the preview says it can.</summary>
        public bool Confirm()
        {
            if (Active is not { } a || Site is not { Error: null } s || session.Conn is null)
                return false;
            session.Conn.Reducers.PlacePlot(a.ItemId, s.X, s.Z, Yaw);
            Cancel();
            return true;
        }

        /// <summary>F while placing places (or does nothing on a bad site). False when not placing, so F does its other jobs.</summary>
        public bool Interact()
        {
            if (Active is null)
                return false;
            Confirm();
            return true;
        }

        public override void _UnhandledInput(InputEvent e)
        {
            if (Active is null)
                return;
            if (e is InputEventMouseButton { Pressed: true } b)
            {
                if (b.ButtonIndex == MouseButton.Left)
                    Confirm();
                else if (b.ButtonIndex == MouseButton.Right)
                    Cancel();
                else
                    return;
                GetViewport().SetInputAsHandled();
            }
        }

        public override void _Process(double delta)
        {
            if (Active is { } a && !session.Bag.Any(s => s.Id == a.ItemId))
                Cancel();
            if (Active is not { } act)
            {
                Prompt = LastResult.Length > 0 ? $"land: {LastResult}" : "";
                return;
            }
            if (Input.IsActionJustPressed("ui_cancel"))
            {
                Cancel();
                return;
            }
            if (Input.IsActionJustPressed("rotate"))
                Turn();
            var kind = act.Kind;
            // Just far enough ahead that the player stands outside the square.
            var at = player.GlobalPosition;
            var (fx, fz) = Sim.Yaw.Facing(player.Facing);
            float ahead = kind.SizeM / 2.0f + 1.5f;
            var (ax, az) = Aim ?? (at.X + fx * ahead, at.Z + fz * ahead);
            float x = Land.Snap(ax), z = Land.Snap(az);
            var others = plots.Standing.Where(p => GameContent.Plot(p.Kind) is not null).Select(p => Sim.Rect.Plot(GameContent.Plot(p.Kind)!, p.X, p.Z));
            byte Texel(int i, int j) => land?.Texel(i, j) ?? 0;
            var (_, error) = Land.CheckSite(Texel, map.GridSample, kind, x, z, others);
            Site = (x, z, error);
            if (drawn != (x, z))
            {
                preview.Mesh = PlotView.Edge(map, Sim.Rect.Plot(kind, x, z), 0.4f);
                drawn = (x, z);
            }
            preview.Visible = true;
            material.AlbedoColor = error is null ? Good : Bad;
            var tax = Land.WeeklyTax(kind, plots.Standing.Count(plots.Mine) + 1);
            Prompt = error is SiteError e
                ? $"placing {kind.Name}: can't, {Land.Message(e)} (R turn, Esc cancel)"
                : $"placing {kind.Name}: F or click to place, {tax?.ToString() ?? "?"} Levy Seals for the first week (R turn, Esc cancel)";
        }
    }
}
