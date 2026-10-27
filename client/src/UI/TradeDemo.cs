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
    /// <c>--trade-demo</c> (dev builds, docs/design/trade.md): once in the world, finds the
    /// Packwright's Bench and Gullhaven Post, teleports to the bench, grants the inputs and crafts a
    /// Grain Bale onto our back through the craft window, walks a few seconds with it at the
    /// carrier's speed, drops it and picks it up again, then teleports to the post and sells it,
    /// logging the purse before and after. It pauses at each step for captures.
    /// </summary>
    public partial class TradeDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly StationRegistry stations;
        private readonly Crafting crafting;
        private readonly CraftWindow window;
        private readonly TradeRegistry registry;
        private readonly Gameplay.Trade trade;
        private bool started;

        public TradeDemo(Session session, PlayerController player, StationRegistry stations, Crafting crafting, CraftWindow window, TradeRegistry registry, Gameplay.Trade trade)
        {
            this.session = session;
            this.player = player;
            this.stations = stations;
            this.crafting = crafting;
            this.window = window;
            this.registry = registry;
            this.trade = trade;
            Name = "TradeDemo";
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
            // The bench and posts are far apart: ask for all of them once, beside the Static tier.
            bool found = false;
            session.Conn!.SubscriptionBuilder().OnApplied(_ => found = true).Subscribe(new[] { "SELECT * FROM station", "SELECT * FROM trade_post" });
            if (!await Until(() => found, 10))
                return;
            var benchKind = GameContent.Stations.First(s => s.Id == "packwright_bench");
            var postKind = GameContent.Posts.First(p => p.Id == "gullhaven_post");
            if (stations.Rows.FirstOrDefault(s => s.Kind == benchKind.Wire) is not { } bench || registry.Posts.FirstOrDefault(p => p.Kind == postKind.Wire) is not { } post)
            {
                session.Log("trade demo: no Packwright's Bench or Gullhaven Post on the server; run `cargo xtask world`");
                return;
            }
            if (trade.Carried is not null)
                session.Conn.Reducers.DevDiscardPack();

            // To the bench, and craft a Grain Bale onto our back.
            var (fx, fz) = Sim.Yaw.Facing(Sim.Yaw.FromU16(bench.Yaw));
            player.DevTeleport(bench.X - 2.0f * fx, bench.Z - 2.0f * fz);
            session.Conn.Reducers.DevGrantItem("hearthgrain", 20);
            session.Conn.Reducers.DevGrantItem("reedcloth", 2);
            await Wait(2.0);
            session.Log($"trade demo: at the {benchKind.Name}; {Purse()}");
            if (!crafting.Interact())
            {
                session.Log("trade demo: the bench isn't in reach");
                return;
            }
            window.Select(GameContent.Recipes.First(r => r.Id == "grain_bale").Wire);
            await Wait(1.5);
            window.PressCraft();
            if (!await Until(() => trade.Carried is not null, 15))
                return;
            window.Close();
            session.Log($"trade demo: carrying a {GameContent.Item(trade.Carried!.Def)?.Name}, speed ×{trade.SpeedScale:F2}");

            // Walk away from the bench for 4 s at the carrier's speed.
            await Wait(1.0);
            var start = player.GlobalPosition;
            player.DemoWish = new Vector2(fx, fz);
            await Wait(4.0);
            player.DemoWish = null;
            var walked = new Vector2(player.GlobalPosition.X - start.X, player.GlobalPosition.Z - start.Z).Length();
            session.Log($"trade demo: walked {walked:F1} m in 4 s with the pack");
            await Wait(2.0);

            // Drop it, look at it, pick it up.
            trade.Drop();
            if (!await Until(() => trade.NearestDropped() is not null, 5))
                return;
            var dropped = trade.NearestDropped()!;
            // Step back from it to look at it, then go back for it.
            player.DemoWish = new Vector2(-fx, -fz);
            await Wait(0.5);
            player.DemoWish = null;
            await Wait(3.0);
            player.DemoWish = new Vector2(fx, fz);
            await Wait(0.4);
            player.DemoWish = null;
            await Wait(0.5);
            trade.PickUp(dropped);
            if (!await Until(() => trade.Carried is not null, 5))
                return;
            session.Log("trade demo: picked the pack up again");
            await Wait(1.0);

            // To Gullhaven Post, across the sea, and sell.
            (fx, fz) = Sim.Yaw.Facing(Sim.Yaw.FromU16(post.Yaw));
            player.DevTeleport(post.X - 2.5f * fx, post.Z - 2.5f * fz);
            await Wait(3.0);
            session.Log($"trade demo: at {postKind.Name}: {trade.Prompt}");
            int sold = trade.Sold;
            if (trade.NearestPost() is not { } here)
            {
                session.Log("trade demo: the post isn't in reach");
                return;
            }
            trade.Sell(here);
            if (!await Until(() => trade.Sold > sold, 5))
                return;
            await Wait(1.0);
            session.Log($"trade demo: done; {Purse()}");
        }

        private string Purse() => TradeModel.PurseLine(trade.Copper);

        private async Task<bool> Until(Func<bool> cond, double secs)
        {
            var limit = DateTime.UtcNow.AddSeconds(secs);
            while (!cond())
            {
                if (DateTime.UtcNow > limit)
                {
                    session.Log("trade demo: timed out; " + trade.LastResult);
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
