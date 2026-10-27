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
    /// <c>--gather-demo</c> (dev builds, docs/design/gathering.md): once in the world and the Static
    /// tier has loaded, walks up (by dev teleport) to the nearest ready plant and gathers it, then
    /// does the same with the nearest tree, logging the bag and labor before and after.
    /// </summary>
    public partial class GatherDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly NodeRegistry nodes;
        private readonly Gathering gathering;
        private bool started;

        public GatherDemo(Session session, PlayerController player, NodeRegistry nodes, Gathering gathering)
        {
            this.session = session;
            this.player = player;
            this.nodes = nodes;
            this.gathering = gathering;
            Name = "GatherDemo";
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
            foreach (var kind in new[] { NodeKind.Plant, NodeKind.Tree })
            {
                var here = player.GlobalPosition;
                long now = session.GameNowMicros;
                var n = nodes.Rows
                    .Where(r => GameContent.Node(r.Kind)?.Kind == kind && r.Gatherer == 0 && Sim.Gather.Ready(r.ReadyAt.MicrosecondsSinceUnixEpoch, now))
                    .OrderBy(r => new Vector2(r.X - here.X, r.Z - here.Z).Length())
                    .FirstOrDefault();
                if (n is null)
                {
                    session.Log($"gather demo: no ready {kind} in view");
                    continue;
                }
                // Stand 1.5 m off it, on the side we came from, facing it.
                var away = new Vector2(here.X - n.X, here.Z - n.Z);
                away = away.Length() > 0.01f ? away.Normalized() : Vector2.Right;
                player.DevTeleport(n.X + 1.5f * away.X, n.Z + 1.5f * away.Y);
                await Wait(1.5);
                int before = gathering.Gathered;
                session.Log($"gather demo: by {GameContent.Node(n.Kind)!.Name} at ({n.X:F1}, {n.Z:F1}); bag {Bag()}, labor {Labor()}");
                if (!gathering.Interact())
                {
                    session.Log("gather demo: nothing to gather in reach");
                    continue;
                }
                var limit = DateTime.UtcNow.AddSeconds(15);
                while (gathering.Gathered == before && DateTime.UtcNow < limit)
                    await Wait(0.1);
                session.Log($"gather demo: bag {Bag()}, labor {Labor()}");
                await Wait(2.0);
            }
            session.Log("gather demo: done");
        }

        private string Bag() => string.Join(", ", session.Bag.OrderBy(s => s.Slot).Select(s => $"{GameContent.Item(s.Def)?.Id ?? s.Def.ToString()} {s.Count}"));

        private int Labor() => session.Labor?.Current(GameContent.Labor, session.GameNowMicros) ?? -1;

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
