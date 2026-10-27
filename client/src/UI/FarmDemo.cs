using System;
using System.Linq;
using System.Threading.Tasks;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.Sim;
using Mmo.World;

namespace Mmo.UI
{
    /// <summary>
    /// <c>--farm-demo</c> (dev builds, docs/design/farming.md): once in the world, finds the nearest
    /// housing site where a farm fits, places one through the placement mode and plants on it from
    /// the bag: a row of Hearthgrain, a Woolback and an Amberpear Tree, with one more Hearthgrain
    /// just off the plot. It moves the dev clock to half grown and then ripe, harvests everything on
    /// the farm, and then moves the clock on until the crop off the plot withers.
    /// </summary>
    public partial class FarmDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly HeightMap map;
        private readonly LandRaster? land;
        private readonly BagWindow bag;
        private readonly Placement placement;
        private readonly PlotRegistry plots;
        private readonly Farming farming;
        private bool started;

        public FarmDemo(Session session, PlayerController player, HeightMap map, LandRaster? land, BagWindow bag, Placement placement, PlotRegistry plots, Farming farming)
        {
            this.session = session;
            this.player = player;
            this.map = map;
            this.land = land;
            this.bag = bag;
            this.placement = placement;
            this.plots = plots;
            this.farming = farming;
            Name = "FarmDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null || session.Static is not { Ready: true })
                return;
            started = true;
            _ = Run();
        }

        private async Task Run()
        {
            var farm = GameContent.Plots[1];
            var here = player.GlobalPosition;
            if (Site(farm, here.X, here.Z) is not { } site)
            {
                session.Log("farm demo: no housing site within 8 km");
                return;
            }
            var (sx, sz) = site;
            session.Log($"farm demo: a farm fits at ({sx}, {sz}), climate {LandRaster.ClimateOf(land!.At(sx, sz)).Id()}");
            // The camera starts looking north (−z), so stand south of the site.
            player.DevTeleport(sx, sz + 14.0f);
            await Wait(2.0);
            var r = session.Conn!.Reducers;
            r.DevGrantItem("farm_deed", 1);
            r.DevGrantItem("levy_seal", 30);
            r.DevGrantItem("hearthgrain_seed", 8);
            r.DevGrantItem("woolback_lamb", 1);
            r.DevGrantItem("amberpear_sapling", 1);
            await Wait(1.0);
            bag.Open = true;
            await Wait(1.0);
            if (!Use(farm.Deed))
            {
                session.Log("farm demo: no deed in the bag");
                return;
            }
            // Only what this run makes counts, should the character have farmed before.
            var oldPlots = plots.Standing.Select(p => p.Id).ToHashSet();
            var oldCrops = farming.Crops.Rows.Select(c => c.Id).ToHashSet();
            placement.Aim = (sx, sz);
            await Wait(1.5);
            placement.Confirm();
            placement.Aim = null;
            var limit = DateTime.UtcNow.AddSeconds(5);
            while (!plots.Standing.Any(p => plots.Mine(p) && !oldPlots.Contains(p.Id)) && DateTime.UtcNow < limit)
                await Wait(0.1);
            if (plots.Standing.FirstOrDefault(p => plots.Mine(p) && !oldPlots.Contains(p.Id)) is not { } plot)
            {
                session.Log($"farm demo: no farm ({placement.LastResult})");
                return;
            }
            session.Log($"farm demo: placed farm {plot.Id}");
            bag.Open = false;

            // A row of Hearthgrain, a Woolback and a tree on the farm, and one crop just off it.
            var grain = GameContent.Crops.First(c => c.Id == "hearthgrain");
            await Plant(grain, (sx - 3.5f, sz + 5.5f), Enumerable.Range(0, 6).Select(i => (sx - 6.0f + i, sz + 4.0f)).ToArray());
            await Plant(GameContent.Crops.First(c => c.Id == "woolback"), (sx + 3.0f, sz + 5.5f), (sx + 3.0f, sz + 3.0f));
            await Plant(GameContent.Crops.First(c => c.Id == "amberpear_tree"), (sx + 3.0f, sz - 0.5f), (sx + 3.0f, sz - 3.0f));
            await Plant(grain, (sx - 3.0f, sz + 12.0f), (sx - 3.0f, sz + 10.0f));
            farming.Cancel();
            await Wait(1.0);
            var mine = farming.Crops.Rows.Where(c => farming.Crops.Mine(c) && !oldCrops.Contains(c.Id)).ToList();
            session.Log($"farm demo: {mine.Count} plantings, {mine.Count(c => c.Plot == plot.Id)} on the farm; ideal climate: {string.Join(", ", mine.Select(c => c.Ideal ? "yes" : "no"))}");
            player.DevTeleport(sx, sz + 16.0f);
            await Wait(4.0);
            session.Log("farm demo: just planted");

            // Half grown, then everything ripe, by the dev clock.
            long grainRipe = mine.Where(c => c.Kind == grain.Wire).Min(c => c.RipeAt.MicrosecondsSinceUnixEpoch);
            long planted = mine.Min(c => c.PlantedAt.MicrosecondsSinceUnixEpoch);
            Advance(planted + (grainRipe - planted) / 2);
            await Wait(4.0);
            session.Log("farm demo: the grain is half grown");
            long allRipe = mine.Max(c => c.RipeAt.MicrosecondsSinceUnixEpoch);
            Advance(allRipe + 60_000_000);
            await Wait(4.0);
            session.Log($"farm demo: all ripe; {farming.Crops.Rows.Count(c => farming.Crops.StageOf(c) == Stage.Ripe)} ripe plantings");

            // Harvest the farm with F.
            player.DevTeleport(sx - 3.5f, sz + 5.5f);
            await Wait(1.5);
            for (int i = 0; i < 6; i++)
            {
                farming.Interact();
                await Wait(0.6);
            }
            player.DevTeleport(sx + 3.0f, sz + 5.5f);
            await Wait(1.5);
            session.Log($"farm demo: {farming.Prompt}");
            farming.Interact();
            await Wait(1.0);
            player.DevTeleport(sx + 3.0f, sz - 0.5f);
            await Wait(1.5);
            farming.Interact();
            await Wait(1.0);
            session.Log($"farm demo: harvested; bag {Bag()}");
            player.DevTeleport(sx, sz + 16.0f);
            await Wait(4.0);
            session.Log("farm demo: after the harvest");

            // The crop off the plot is left ripe until it withers.
            var off = farming.Crops.Rows.FirstOrDefault(c => c.Plot == 0 && farming.Crops.Mine(c) && !oldCrops.Contains(c.Id));
            if (off is null || Farm.WithersAt(grain, off.RipeAt.MicrosecondsSinceUnixEpoch) is not long withers)
            {
                session.Log("farm demo: no crop off the plot");
                return;
            }
            Advance(withers + 60_000_000);
            player.DevTeleport(sx - 1.0f, sz + 12.0f);
            await Wait(4.0);
            session.Log($"farm demo: off the plot: {farming.Prompt}");
            session.Log("farm demo: done");
        }

        /// <summary>Stand at <paramref name="from"/> and plant <paramref name="kind"/> at each spot through the planting mode.</summary>
        private async Task Plant(CropData kind, (float X, float Z) from, params (float X, float Z)[] spots)
        {
            player.DevTeleport(from.X, from.Z);
            await Wait(1.0);
            if (!Use(kind.Seed))
            {
                session.Log($"farm demo: no {kind.Name} seed in the bag");
                return;
            }
            foreach (var s in spots)
            {
                farming.Aim = s;
                await Wait(0.5);
                if (farming.Spot is { Error: SpotError e })
                    session.Log($"farm demo: {kind.Name} at ({s.X}, {s.Z}): {Farm.Message(e)}");
                farming.Confirm();
                await Wait(0.5);
            }
            farming.Aim = null;
        }

        /// <summary>Move the dev clock on to game time <paramref name="to"/>, if it isn't there already.</summary>
        private void Advance(long to)
        {
            long by = to - session.GameNowMicros;
            if (by > 0)
                session.Conn!.Reducers.DevAdvanceClock(by);
        }

        /// <summary>Right-click the first stack of <paramref name="def"/> in the bag.</summary>
        private bool Use(ushort def)
        {
            if (session.Bag.Where(s => s.Def == def).Select(s => (BagStack?)s).FirstOrDefault() is not { } stack)
                return false;
            bag.UseSlot(stack.Slot);
            return true;
        }

        /// <summary>The nearest snapped centre within 8 km where a plot of <paramref name="kind"/> fits now.</summary>
        private (float, float)? Site(PlotData kind, float x0, float z0)
        {
            if (land is null)
                return null;
            var others = plots.Standing.Select(p => Sim.Rect.Plot(GameContent.Plot(p.Kind)!, p.X, p.Z)).ToList();
            for (float r = 0; r < 8000; r += 8)
            {
                int steps = Math.Max(1, (int)(Mathf.Tau * r / 8));
                for (int k = 0; k < steps; k++)
                {
                    float a = k * Mathf.Tau / steps;
                    float x = Land.Snap(x0 + r * MathF.Cos(a)), z = Land.Snap(z0 + r * MathF.Sin(a));
                    if (!LandRaster.Use(land.At(x, z)).Data().Buildable)
                        continue;
                    // Room south of it for the crop off the plot.
                    if (Land.CheckSite(land.Texel, map.GridSample, kind, x, z, others).Error is null
                        && Land.CheckSite(land.Texel, map.GridSample, GameContent.Plots[0], x, z + 12.0f, others).Error is null)
                        return (x, z);
                }
            }
            return null;
        }

        private string Bag() => string.Join(", ", session.Bag.OrderBy(s => s.Slot).Select(s => $"{GameContent.Item(s.Def)?.Id ?? s.Def.ToString()} {s.Count}"));

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
