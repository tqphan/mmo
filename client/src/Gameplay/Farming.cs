using System.Collections.Generic;
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
    /// Planting and tending crops, trees and livestock (docs/design/farming.md). Right-clicking a
    /// seed in the bag starts planting: a disc of the kind's radius follows a point in front of the
    /// player, green where <see cref="Farm.CheckSpot"/> passes and red with the reason in the HUD; F
    /// or a left click plants, and planting goes on with the next seed until Esc or a right click.
    /// Otherwise F harvests the nearest ripe planting we may take, and G (twice) uproots one. The
    /// server decides each.
    /// </summary>
    public partial class Farming : Node3D
    {
        private static readonly Color Good = new(0.4f, 1.0f, 0.45f);
        private static readonly Color Bad = new(1.0f, 0.3f, 0.25f);
        /// <summary>How long a first G waits for the second.</summary>
        private const double ConfirmS = 3.0;

        private readonly Session session;
        private readonly PlayerController player;
        private readonly HeightMap map;
        private readonly LandRaster? land;
        private readonly PlotRegistry plots;
        private readonly CropRegistry crops;
        private readonly StandardMaterial3D material = new()
        {
            ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded,
            AlbedoColor = Good,
            Transparency = BaseMaterial3D.TransparencyEnum.Alpha,
        };
        private readonly MeshInstance3D preview = new() { Name = "Preview", Visible = false };
        private (ulong Crop, System.DateTime Until)? uprootArmed;

        public Farming(Session session, PlayerController player, HeightMap map, LandRaster? land, PlotRegistry plots, CropRegistry crops)
        {
            this.session = session;
            this.player = player;
            this.map = map;
            this.land = land;
            this.plots = plots;
            this.crops = crops;
            Name = "Farming";
            preview.MaterialOverride = material;
            AddChild(preview);
        }

        public CropRegistry Crops => crops;

        /// <summary>The seed being planted, and what grows from it.</summary>
        public (ushort Def, CropData Kind)? Active { get; private set; }

        /// <summary>Where the planting would go, and why it can't (null if it can).</summary>
        public (float X, float Z, SpotError? Error)? Spot { get; private set; }

        public string Prompt { get; private set; } = "";

        /// <summary>Aim at this point instead of ahead of the player; for the demo.</summary>
        public (float X, float Z)? Aim { get; set; }

        /// <summary>The server's answer to our last request, for the HUD and the demo.</summary>
        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        private string lastResult = "";
        private System.DateTime resultUntil;

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnPlant += (ctx, _, x, z) => Answer(ctx, $"planted at ({x:F1}, {z:F1})");
            r.OnHarvest += (ctx, id) => Answer(ctx, $"harvested crop {id}");
            r.OnUproot += (ctx, id) => Answer(ctx, $"uprooted crop {id}");
        }

        private void Answer(ReducerEventContext ctx, string ok)
        {
            lastResult = ctx.Event.Status is Status.Failed(var reason) ? $"refused: {reason}" : ok;
            resultUntil = System.DateTime.UtcNow.AddSeconds(6);
            session.Log($"farm: {lastResult}");
        }

        /// <summary>Start planting the seed in <paramref name="stack"/>; false if it isn't one.</summary>
        public bool Begin(BagStack stack)
        {
            if (GameContent.CropBySeed(stack.Def) is not CropData kind)
                return false;
            Active = (stack.Def, kind);
            session.Log($"planting {kind.Name}: F or click to plant, Esc to stop");
            return true;
        }

        public void Cancel()
        {
            Active = null;
            Spot = null;
            preview.Visible = false;
        }

        /// <summary>Plant where the preview stands, if the preview says it can.</summary>
        public bool Confirm()
        {
            if (Active is not { } a || Spot is not { Error: null } s || session.Conn is null)
                return false;
            if (session.Bag.Where(b => b.Def == a.Def).Select(b => (BagStack?)b).FirstOrDefault() is not { } stack)
                return false;
            session.Conn.Reducers.Plant(stack.Id, s.X, s.Z);
            return true;
        }

        /// <summary>F on land: plant while planting, else harvest the nearest ripe planting we may take. False if neither, so F does its other jobs.</summary>
        public bool Interact()
        {
            if (Active is not null)
            {
                Confirm();
                return true;
            }
            if (session.Conn is null || player.Mode == MoveMode.Swim || Nearest(c => crops.StageOf(c) == Stage.Ripe && RightsOf(c) != Rights.None) is not { } crop)
                return false;
            session.Conn.Reducers.Harvest(crop.Id);
            return true;
        }

        /// <summary>G: the first press picks the nearest planting we may uproot, the second within a few seconds uproots it.</summary>
        public void Uproot()
        {
            if (session.Conn is null || Nearest(c => RightsOf(c) != Rights.None) is not { } crop)
                return;
            if (uprootArmed is { } armed && armed.Crop == crop.Id && System.DateTime.UtcNow < armed.Until)
            {
                session.Conn.Reducers.Uproot(crop.Id);
                uprootArmed = null;
                return;
            }
            uprootArmed = (crop.Id, System.DateTime.UtcNow.AddSeconds(ConfirmS));
        }

        public enum Rights
        {
            Tend,
            Steal,
            None,
        }

        /// <summary>What we may do with a planting, as the server decides it (farming.rs <c>rights</c>).</summary>
        public Rights RightsOf(Crop c)
        {
            if (c.Plot != 0)
                return crops.PlotOf(c) is not { } p ? Rights.Tend : p.Access == 1 || plots.Mine(p) ? Rights.Tend : Rights.None;
            if (crops.Mine(c))
                return Rights.Tend;
            return crops.StageOf(c) switch { Stage.Ripe => Rights.Steal, Stage.Growing => Rights.None, _ => Rights.Tend };
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
            if (Active is { } a && !session.Bag.Any(s => s.Def == a.Def))
                Cancel();
            if (Input.IsActionJustPressed("uproot") && Active is null && player.Parent == 0)
                Uproot();
            crops.Focus = Active is null && player.Parent == 0 && Nearest(_ => true) is { } near ? near.Id : 0;
            if (Active is not { } act)
            {
                Prompt = TendPrompt();
                return;
            }
            if (Input.IsActionJustPressed("ui_cancel"))
            {
                Cancel();
                return;
            }
            var kind = act.Kind;
            var at = player.GlobalPosition;
            var (fx, fz) = Sim.Yaw.Facing(player.Facing);
            var (x, z) = Aim ?? (at.X + fx * 1.5f, at.Z + fz * 1.5f);
            var (_, error) = Farm.CheckSpot(Texel, map.GridSample, kind, x, z, PlotSpots(), Others());
            Spot = (x, z, error);
            preview.Mesh ??= new CylinderMesh { TopRadius = 1, BottomRadius = 1, Height = 0.06f, RadialSegments = 24, Rings = 1 };
            preview.Scale = new Vector3(kind.SpaceM, 1, kind.SpaceM);
            preview.Position = new Vector3(x, map.Height(x, z) + 0.05f, z);
            preview.Visible = true;
            material.AlbedoColor = (error is null ? Good : Bad) with { A = 0.7f };
            int seeds = (int)session.Bag.Where(s => s.Def == act.Def).Sum(s => (long)s.Count);
            Prompt = error is SpotError e
                ? $"planting {kind.Name} ({seeds} left): can't, {Farm.Message(e)} (Esc to stop)"
                : $"planting {kind.Name} ({seeds} left): F or click to plant (Esc to stop){Result()}";
        }

        private string Result() => LastResult.Length > 0 ? $"\nfarm: {LastResult}" : "";

        private byte Texel(int i, int j) => land?.Texel(i, j) ?? 0;

        /// <summary>The standing plots, with whether we may tend plantings on them.</summary>
        private List<PlotSpot> PlotSpots() =>
            plots.Standing.Where(p => GameContent.Plot(p.Kind) is not null)
                .Select(p => new PlotSpot(Sim.Rect.Plot(GameContent.Plot(p.Kind)!, p.X, p.Z), p.Access == 1 || plots.Mine(p)))
                .ToList();

        private IEnumerable<Planting> Others() =>
            crops.Rows.Where(c => !crops.Gone(c) && GameContent.Crop(c.Kind) is not null).Select(c => new Planting(c.X, c.Z, GameContent.Crop(c.Kind)!.SpaceM));

        private string TendPrompt()
        {
            if (player.Parent != 0 || session.Me is null || Nearest(_ => true) is not { } c || GameContent.Crop(c.Kind) is not { } k)
                return LastResult.Length > 0 ? $"farm: {LastResult}" : "";
            var rights = RightsOf(c);
            string uproot = uprootArmed is { } armed && armed.Crop == c.Id && System.DateTime.UtcNow < armed.Until
                ? $"\nG again to uproot the {k.Name}"
                : rights != Rights.None ? "  (G: uproot)" : "";
            string what = crops.StageOf(c) == Stage.Ripe && rights != Rights.None
                ? $"F: harvest {k.Name} ({k.Labor} labor){(rights == Rights.Steal ? " — not yours: theft" : "")}"
                : $"{k.Name}: {Farm.StageText(k, c.GrownFrom.MicrosecondsSinceUnixEpoch, c.RipeAt.MicrosecondsSinceUnixEpoch, session.GameNowMicros)}";
            return what + uproot + Result();
        }

        /// <summary>The nearest planting in reach that <paramref name="ok"/> accepts.</summary>
        private Crop? Nearest(System.Func<Crop, bool> ok)
        {
            var at = player.GlobalPosition;
            return crops.Rows
                .Where(c => Farm.InReach(c.X, c.Z, at.X, at.Z) && ok(c))
                .OrderBy(c => new Vector2(c.X - at.X, c.Z - at.Z).LengthSquared())
                .FirstOrDefault();
        }
    }
}
