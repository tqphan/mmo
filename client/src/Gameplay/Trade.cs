using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Trade packs (docs/design/trade.md). With a pack on our back, F at a post in reach sells it;
    /// without one, F picks up the nearest dropped pack in reach, and X drops ours. The server
    /// decides; the prompt shows its price by the same rules. While a pick-up is on its way the
    /// player stands still (<see cref="Halted"/>), so its next move is already at the carrier's speed.
    /// </summary>
    public partial class Trade : Node
    {
        /// <summary>How long a pick-up may hold the player still waiting for its answer.</summary>
        private const double HaltS = 2.0;

        private readonly Session session;
        private readonly PlayerController player;
        private readonly TradeRegistry registry;
        private double haltUntil = double.NegativeInfinity;
        private string lastResult = "";
        private System.DateTime resultUntil;

        public Trade(Session session, PlayerController player, TradeRegistry registry)
        {
            this.session = session;
            this.player = player;
            this.registry = registry;
            Name = "Trade";
        }

        /// <summary>What the HUD says about posts and packs nearby.</summary>
        public string Prompt { get; private set; } = "";

        /// <summary>The purse and the pack carried, for the HUD.</summary>
        public string Status { get; private set; } = "";

        /// <summary>The pack on our back, as the server has it.</summary>
        public Pack? Carried =>
            session.Conn is { } c && session.Me is { } me ? c.Db.MyPack.Iter().FirstOrDefault(p => p.Carrier == me.CharId) : null;

        /// <summary>Our purse, in copper.</summary>
        public ulong Copper =>
            session.Conn is { } c && session.Me is { } me ? c.Db.MyPurse.Iter().FirstOrDefault(p => p.CharId == me.CharId)?.Copper ?? 0 : 0;

        /// <summary>The factor on our speed: the carrier's while we carry a pack.</summary>
        public float SpeedScale => Carried is null ? 1.0f : Sim.Trade.CarryLimit(1.0f);

        /// <summary>A pick-up is on its way: stand still until the server answers.</summary>
        public bool Halted => Clock.Now < haltUntil;

        /// <summary>Sales made, for the demo.</summary>
        public int Sold { get; private set; }

        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnPickUpPack += (ctx, id) =>
            {
                haltUntil = double.NegativeInfinity;
                Answer(ctx, $"picked up pack {id}");
            };
            r.OnDropPack += ctx => Answer(ctx, "dropped the pack");
            r.OnSellPack += (ctx, _) =>
            {
                if (ctx.Event.Status is Status.Committed)
                    Sold++;
                Answer(ctx, $"sold the pack; purse now {Sim.Trade.Coins(Copper)}");
            };
        }

        private void Answer(ReducerEventContext ctx, string ok)
        {
            lastResult = ctx.Event.Status is Status.Failed(var reason) ? $"refused: {reason}" : ok;
            resultUntil = System.DateTime.UtcNow.AddSeconds(6);
            session.Log($"trade: {lastResult}");
        }

        /// <summary>F was pressed on land: sell at a post, or pick up a pack. False if neither is in reach.</summary>
        public bool Interact()
        {
            if (session.Conn is not { } c || player.Parent != 0 || player.Mode == MoveMode.Swim)
                return false;
            if (Carried is not null)
            {
                if (NearestPost() is not { } post)
                    return false;
                Sell(post);
                return true;
            }
            if (NearestDropped() is not { } d)
                return false;
            PickUp(d);
            return true;
        }

        public void Sell(TradePost post)
        {
            session.Conn?.Reducers.SellPack(post.Id);
            session.Log($"selling at post {post.Id}");
        }

        public void PickUp(DroppedPack d)
        {
            haltUntil = Clock.Now + HaltS;
            session.Conn?.Reducers.PickUpPack(d.Id);
            session.Log($"picking up pack {d.Id}");
        }

        public void Drop()
        {
            session.Conn?.Reducers.DropPack();
            session.Log("dropping the pack");
        }

        public override void _Process(double delta)
        {
            if (Input.IsActionJustPressed("drop_pack") && Carried is not null && player.Parent == 0)
                Drop();
            long now = session.GameNowMicros;
            var carried = Carried is { } pk ? Info(pk) : (PackInfo?)null;
            Status = TradeModel.PurseLine(Copper) + (carried is { } c ? "  " + TradeModel.PackLine(c, now) : "");
            Prompt = MakePrompt(carried, now);
            if (LastResult.Length > 0)
                Prompt = Prompt.Length > 0 ? Prompt + "\n" + LastResult : LastResult;
        }

        private string MakePrompt(PackInfo? carried, long now)
        {
            if (player.Parent != 0 || session.Me is null)
                return "";
            if (carried is { } pack)
            {
                if (NearestPost() is not { } post || GameContent.Post(post.Kind) is not { } kind)
                    return "";
                return TradeModel.SellPrompt(pack, kind, new System.Numerics.Vector2(post.X, post.Z), Demand(post.Kind, pack.Def, now), now);
            }
            if (NearestDropped() is not { } d)
                return "";
            return TradeModel.PickUpPrompt(Info(d), Ours(d.Owner), false, now);
        }

        /// <summary>A post kind's demand for a pack kind now.</summary>
        public uint Demand(byte post, ushort def, long now) =>
            session.Conn?.Db.TradeDemand.Key.Find(Sim.Trade.DemandKey(post, def)) is { } d
                ? Sim.Trade.DemandNow(d.Permille, d.At.MicrosecondsSinceUnixEpoch, now)
                : Sim.Trade.Full;

        public static PackInfo Info(Pack p) => new(p.Def, p.Owner, p.OriginX, p.OriginZ, p.MadeAt.MicrosecondsSinceUnixEpoch);

        public static PackInfo Info(DroppedPack d) => new(d.Def, d.Owner, d.OriginX, d.OriginZ, d.MadeAt.MicrosecondsSinceUnixEpoch);

        /// <summary>One of our account's characters made it.</summary>
        private bool Ours(ulong owner) => session.Conn?.Db.MyCharacters.Iter().Any(c => c.CharId == owner) ?? false;

        private System.Numerics.Vector3 At => PlayerController.ToNumerics(player.GlobalPosition);

        public TradePost? NearestPost() =>
            registry.Posts
                .Where(p => Sim.Trade.InReach(new System.Numerics.Vector3(p.X, p.Y, p.Z), At))
                .OrderBy(p => System.Numerics.Vector3.DistanceSquared(new System.Numerics.Vector3(p.X, p.Y, p.Z), At))
                .FirstOrDefault();

        public DroppedPack? NearestDropped() =>
            registry.Dropped
                .Where(d => Sim.Trade.InReach(new System.Numerics.Vector3(d.X, d.Y, d.Z), At))
                .OrderBy(d => System.Numerics.Vector3.DistanceSquared(new System.Numerics.Vector3(d.X, d.Y, d.Z), At))
                .FirstOrDefault();
    }
}
