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
    /// <c>--land-demo</c> (dev builds, docs/design/land.md): once in the world, finds the nearest
    /// housing site where a garden fits, teleports beside it and grants a Garden Deed and Levy Seals.
    /// It starts placing from the bag, shows a refused spot and then the good one, places the garden,
    /// opens its window, pays a week and makes it public. Then it moves the dev clock until the tax
    /// is overdue, and on past the demolition, and checks the deed comes back.
    /// </summary>
    public partial class LandDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly HeightMap map;
        private readonly LandRaster? land;
        private readonly BagWindow bag;
        private readonly Placement placement;
        private readonly Plots plots;
        private readonly PlotWindow window;
        private bool started;

        public LandDemo(Session session, PlayerController player, HeightMap map, LandRaster? land, BagWindow bag, Placement placement, Plots plots, PlotWindow window)
        {
            this.session = session;
            this.player = player;
            this.map = map;
            this.land = land;
            this.bag = bag;
            this.placement = placement;
            this.plots = plots;
            this.window = window;
            Name = "LandDemo";
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
            var garden = GameContent.Plots[0];
            var here = player.GlobalPosition;
            if (Site(garden, here.X, here.Z) is not { } site)
            {
                session.Log("land demo: no housing site within 8 km");
                return;
            }
            var (sx, sz) = site;
            session.Log($"land demo: a garden fits at ({sx}, {sz})");
            // The camera starts looking north (−z), so stand south of the site.
            player.DevTeleport(sx, sz + garden.SizeM / 2.0f + 6.0f);
            await Wait(2.0);
            session.Conn!.Reducers.DevGrantItem("garden_deed", 1);
            session.Conn.Reducers.DevGrantItem("levy_seal", 30);
            await Wait(1.0);
            session.Log($"land demo: bag {Bag()}");
            bag.Open = true;
            await Wait(1.0);
            if (!session.Bag.Any(s => s.Def == garden.Deed))
            {
                session.Log("land demo: no deed in the bag");
                return;
            }
            bag.UseSlot(session.Bag.First(s => s.Def == garden.Deed).Slot);
            // A refused spot nearby first, if there is one, then the site.
            if (Refused(garden, sx, sz) is { } bad)
            {
                placement.Aim = bad;
                await Wait(2.0);
                session.Log($"land demo: preview at ({placement.Site?.X}, {placement.Site?.Z}): {placement.Prompt}");
            }
            placement.Aim = (sx, sz);
            await Wait(2.5);
            session.Log($"land demo: preview: {placement.Prompt}");
            placement.Confirm();
            placement.Aim = null;
            var limit = DateTime.UtcNow.AddSeconds(5);
            while (!plots.Registry.Standing.Any(plots.Registry.Mine) && DateTime.UtcNow < limit)
                await Wait(0.1);
            if (plots.Registry.Standing.FirstOrDefault(plots.Registry.Mine) is not { } plot)
            {
                session.Log($"land demo: no plot ({placement.LastResult})");
                return;
            }
            session.Log($"land demo: placed garden {plot.Id}; bag {Bag()}");
            await Wait(2.0);
            if (!plots.Interact())
                session.Log("land demo: the plot isn't in reach");
            await Wait(1.0);
            window.PressPay();
            await Wait(1.0);
            window.PressAccess();
            await Wait(2.5);
            session.Log($"land demo: {plots.LastResult}; bag {Bag()}");
            window.Close();

            // Overdue by the dev clock.
            plot = plots.Registry.Find(plot.Id)!;
            long paid = plot.TaxPaidUntil.MicrosecondsSinceUnixEpoch;
            session.Conn.Reducers.DevAdvanceClock(paid - session.GameNowMicros + 3_600_000_000);
            await Wait(4.0);
            session.Log($"land demo: overdue; {plots.Prompt}");
            session.Conn.Reducers.DevAdvanceClock(Land.DemolishAt(paid) - session.GameNowMicros + 60_000_000);
            limit = DateTime.UtcNow.AddSeconds(5);
            while (plots.Registry.Find(plot.Id) is not null && DateTime.UtcNow < limit)
                await Wait(0.1);
            await Wait(1.0);
            session.Log(plots.Registry.Find(plot.Id) is null
                ? $"land demo: demolished; bag {Bag()}"
                : "land demo: the plot is still there");
            session.Log("land demo: done");
        }

        /// <summary>The nearest snapped centre within 8 km where a plot of <paramref name="kind"/> fits now.</summary>
        private (float, float)? Site(PlotData kind, float x0, float z0)
        {
            if (land is null)
                return null;
            var others = plots.Registry.Standing.Select(p => Sim.Rect.Plot(GameContent.Plot(p.Kind)!, p.X, p.Z)).ToList();
            for (float r = 0; r < 8000; r += 8)
            {
                int steps = Math.Max(1, (int)(Mathf.Tau * r / 8));
                for (int k = 0; k < steps; k++)
                {
                    float a = k * Mathf.Tau / steps;
                    float x = Land.Snap(x0 + r * MathF.Cos(a)), z = Land.Snap(z0 + r * MathF.Sin(a));
                    if (!LandRaster.Use(land.At(x, z)).Data().Buildable)
                        continue;
                    if (Land.CheckSite(land.Texel, map.GridSample, kind, x, z, others).Error is null)
                        return (x, z);
                }
            }
            return null;
        }

        /// <summary>A spot beside the site the preview refuses, to show it red.</summary>
        private (float, float)? Refused(PlotData kind, float sx, float sz)
        {
            if (land is null)
                return null;
            foreach (var (dx, dz) in new[] { (0f, -12f), (0f, -20f), (6f, -12f), (-6f, -12f), (0f, -30f), (12f, 0f), (-12f, 0f) })
            {
                float x = Land.Snap(sx + dx), z = Land.Snap(sz + dz);
                if (Land.CheckSite(land.Texel, map.GridSample, kind, x, z, Array.Empty<Sim.Rect>()).Error is not null)
                    return (x, z);
            }
            return null;
        }

        private string Bag() => string.Join(", ", session.Bag.OrderBy(s => s.Slot).Select(s => $"{GameContent.Item(s.Def)?.Id ?? s.Def.ToString()} {s.Count}"));

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
