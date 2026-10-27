using System;
using System.Linq;
using System.Threading.Tasks;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.World;

namespace Mmo.UI
{
    /// <summary>
    /// <c>--craft-demo</c> (dev builds, docs/design/crafting.md): once in the world and the Static
    /// tier has loaded, grants 6 Driftpine Logs and 6 Rustvein Ore, walks up (by dev teleport) to
    /// the Cookhearth and crafts 3 Charcoal through the window, then does the same at the Bloomery
    /// for 2 Rustiron Bars, logging the bag and labor before and after.
    /// </summary>
    public partial class CraftDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly StationRegistry stations;
        private readonly Crafting crafting;
        private readonly CraftWindow window;
        private bool started;

        public CraftDemo(Session session, PlayerController player, StationRegistry stations, Crafting crafting, CraftWindow window)
        {
            this.session = session;
            this.player = player;
            this.stations = stations;
            this.crafting = crafting;
            this.window = window;
            Name = "CraftDemo";
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
            await Wait(1.0);
            session.Conn!.Reducers.DevGrantItem("driftpine_log", 6);
            session.Conn.Reducers.DevGrantItem("rustvein_ore", 6);
            await Wait(0.5);
            foreach (var (station, recipe, count) in new[] { ("cookhearth", "charcoal", 3u), ("bloomery", "rustiron_bar", 2u) })
            {
                var kind = GameContent.Stations.First(s => s.Id == station);
                var r = GameContent.Recipes.First(x => x.Id == recipe);
                var here = player.GlobalPosition;
                var st = stations.Rows.Where(s => s.Kind == kind.Wire).OrderBy(s => new Vector2(s.X - here.X, s.Z - here.Z).Length()).FirstOrDefault();
                if (st is null)
                {
                    session.Log($"craft demo: no {kind.Name} in view");
                    continue;
                }
                // Stand 2 m in front of it, facing it.
                float yaw = Sim.Yaw.FromU16(st.Yaw);
                var (fx, fz) = Sim.Yaw.Facing(yaw);
                player.DevTeleport(st.X - 2.0f * fx, st.Z - 2.0f * fz);
                await Wait(1.5);
                session.Log($"craft demo: at the {kind.Name}; bag {Bag()}, labor {Labor()}");
                if (!crafting.Interact())
                {
                    session.Log("craft demo: no station in reach");
                    continue;
                }
                window.Select(r.Wire);
                await Wait(1.0);
                for (int i = 1; i < count; i++)
                {
                    window.Model.Step(CraftModel.Lines(st.Kind, session.Bag, Labor()), +1);
                    await Wait(0.3);
                }
                int before = crafting.Finished;
                window.PressCraft();
                var limit = DateTime.UtcNow.AddSeconds(count * r.CraftS + 10);
                while (crafting.Finished == before && DateTime.UtcNow < limit)
                    await Wait(0.1);
                session.Log($"craft demo: bag {Bag()}, labor {Labor()}");
                await Wait(2.0);
                window.Close();
            }
            session.Log("craft demo: done");
        }

        private string Bag() => string.Join(", ", session.Bag.OrderBy(s => s.Slot).Select(s => $"{GameContent.Item(s.Def)?.Id ?? s.Def.ToString()} {s.Count}"));

        private int Labor() => session.Labor?.Current(GameContent.Labor, session.GameNowMicros) ?? -1;

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
