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
    /// <c>--shipyard-demo</c> (dev builds, docs/design/vehicles.md §2B): off the trade island's west
    /// beach, places a Swiftgull Shipyard from a dev design and builds it through every stage with
    /// dev packs, launches it, summons the Swiftgull and loads a pack. A dev hit puts it under fire
    /// and a kit repairs it; a second sinks it, and we swim to the floating pack and take it. The
    /// wreck is mended with kits, the dev clock runs out the wait, and it's summoned again. It
    /// pauses at each step for captures.
    /// </summary>
    public partial class ShipyardDemo : Node
    {
        /// <summary>Where to start looking west for water deep enough for a Swiftgull.</summary>
        private static readonly Vector2 ShoreFrom = new(-330, 30);
        private const float West = Mathf.Pi / 2;

        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Vehicles vehicles;
        private readonly Shipyards shipyards;
        private readonly ShipyardRegistry yards;
        private readonly Gameplay.Trade trade;
        private readonly TradeRegistry packs;
        private readonly HeightMap map;
        private bool started;

        public ShipyardDemo(Session session, PlayerController player, EntityRegistry entities, Vehicles vehicles, Shipyards shipyards, ShipyardRegistry yards, Gameplay.Trade trade, TradeRegistry packs, HeightMap map)
        {
            this.packs = packs;
            this.map = map;
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.vehicles = vehicles;
            this.shipyards = shipyards;
            this.yards = yards;
            this.trade = trade;
            Name = "ShipyardDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null || session.Near is not { Ready: true } || session.Static is not { Ready: true })
                return;
            started = true;
            _ = Run();
        }

        private ShipView? Swiftgull =>
            entities.Ships.FirstOrDefault(s => s.Hull.Hull == Hull.Clipper && s.Vehicle is { } v && v.Owner == session.Me?.CharId);

        private SpacetimeDB.Types.Shipyard? Yard => yards.Rows.FirstOrDefault(y => y.Owner == session.Me?.CharId);

        private ushort Carrying => session.Conn!.Db.MyPack.Iter().FirstOrDefault(p => p.Carrier == session.Me?.CharId)?.Def ?? 0;

        private SpacetimeDB.Types.Item? Stack(string id)
        {
            ushort wire = GameContent.Items.First(i => i.Id == id).Wire;
            return session.Conn!.Db.MyInventory.Iter().FirstOrDefault(i => i.Holder == session.Me?.CharId && i.Def == wire && i.Place == Session.BagPlace);
        }

        private bool Use(string id) => Stack(id) is { } s && (vehicles.Begin(new BagStack(s.Id, s.Slot, s.Def, s.Count)) || shipyards.Begin(new BagStack(s.Id, s.Slot, s.Def, s.Count)));

        private async Task Run()
        {
            var r = session.Conn!.Reducers;
            if (Carrying != 0)
                r.DevDiscardPack();
            if (FindWater() is not Vector2 shore)
            {
                session.Log("shipyard demo: no water deep enough west of the island");
                return;
            }

            // The yard, from a design.
            r.DevSetLabor(5000);
            r.DevGrantItem("swiftgull_design", 1);
            if (!await Until(() => Stack("swiftgull_design") is not null, 5))
                return;
            player.DevTeleport(shore.X, shore.Y);
            await Wait(1.5);
            player.Face(West);
            await Wait(0.5);
            if (!Use("swiftgull_design") || !await Until(() => Yard is not null, 5))
                return;
            session.Log($"shipyard demo: placed a shipyard at ({Yard!.X:F0}, {Yard.Z:F0})");
            await Wait(2.0);

            // Every stage, bringing each pack it needs.
            var kind = GameContent.Shipyard(Yard.Kind)!;
            for (int i = 0; i < kind.Stages.Count; i++)
            {
                var st = kind.Stages[i];
                if (st.Pack != 0)
                {
                    r.DevGivePack(GameContent.Item(st.Pack)!.Id);
                    if (!await Until(() => Carrying == st.Pack, 5))
                        return;
                    await Wait(1.0);
                }
                byte before = Yard?.Stage ?? 0;
                if (!shipyards.Interact() || !await Until(() => Yard is null || Yard.Stage > before, 5))
                    return;
                session.Log($"shipyard demo: {st.Name} done");
                await Wait(2.0);
            }
            if (!await Until(() => Stack("swiftgull") is not null, 5))
                return;
            session.Log("shipyard demo: launched a Swiftgull into the bag");

            // Out on the water, with a pack aboard.
            player.DevTeleport(shore.X, shore.Y);
            await Wait(1.5);
            player.Face(West);
            await Wait(0.5);
            if (!Use("swiftgull") || !await Until(() => Swiftgull is { Predictor.HasRow: true }, 5))
                return;
            var ship = Swiftgull!;
            await Wait(2.0);
            r.DevGivePack("grain_bale");
            if (!await Until(() => Carrying != 0, 5))
                return;
            var h = ship.Hull;
            var beside = ship.Pose.ToWorld(new System.Numerics.Vector3(h.DeckHalfWidthM + 0.9f, 0, h.Slots[1].Z));
            player.DevTeleport(beside.X, beside.Z);
            await Wait(1.5);
            player.DemoClimb();
            if (!await Until(() => player.Parent == ship.Id, 5))
                return;
            await Wait(1.0);
            if (!vehicles.Interact() || !await Until(() => ship.Vehicle?.Cargo.Any(c => c != 0) == true, 5))
                return;
            session.Log("shipyard demo: loaded a Grain Bale");
            await Wait(2.0);

            // A hit, a wait under fire, and a repair.
            r.DevGrantItem("hullmenders_kit", 1);
            r.DevDamageVehicle(ship.Id, 3000);
            if (!await Until(() => ship.Vehicle?.Hp < h.Hp, 5))
                return;
            await Wait(GameContent.Vehicles.UnderFireS + 1);
            vehicles.Repair(ship.Id);
            if (!await Until(() => ship.Vehicle?.Hp > h.Hp - 3000, 5))
                return;
            session.Log($"shipyard demo: repaired to {ship.Vehicle?.Hp} HP");
            await Wait(2.0);

            // Sunk: we're put in the water, and swim to the floating pack.
            r.DevDamageVehicle(ship.Id, 99_999);
            if (!await Until(() => Swiftgull is null && player.Parent == 0, GameContent.Vehicles.SinkS + 5))
                return;
            session.Log("shipyard demo: the Swiftgull went down");
            if (!await Until(() => packs.Dropped.Any(d => d.Owner == session.Me?.CharId), 5))
                return;
            var floating = packs.Dropped.First(d => d.Owner == session.Me?.CharId);
            await Wait(1.5);
            player.DevTeleport(floating.X, floating.Z);
            await Wait(1.5);
            if (!trade.Interact() || !await Until(() => Carrying != 0, 5))
                return;
            session.Log("shipyard demo: took the floating Grain Bale from the water");
            await Wait(2.0);

            // The wreck: mended, the wait run out by the dev clock, and summoned again.
            r.DevDiscardPack();
            r.DevGrantItem("hullmenders_kit", (uint)h.MendKits);
            if (!await Until(() => Stack("hullmenders_kit") is { } k && k.Count >= h.MendKits, 5))
                return;
            if (!Use("swiftgull") || !await Until(() => session.Conn.Db.MyWrecks.Iter().Any(w => w.Mending), 5))
                return;
            session.Log("shipyard demo: mending the wreck");
            await Wait(2.0);
            r.DevAdvanceClock(((long)h.MendS + 1) * 1_000_000);
            await Wait(1.0);
            player.DevTeleport(shore.X, shore.Y);
            await Wait(1.5);
            player.Face(West);
            await Wait(0.5);
            if (!Use("swiftgull") || !await Until(() => Swiftgull is { Predictor.HasRow: true }, 5))
                return;
            await Wait(3.0);
            session.Log($"shipyard demo: done; {session.Me?.Corrections ?? 0} corrections");
        }

        /// <summary>A spot west of the island where a Swiftgull summoned facing west would float.</summary>
        private Vector2? FindWater()
        {
            var h = Hull.Clipper.Data();
            Ground? Ground(float x, float z) => Terrain.Ground(map.QuadCorner, x, z);
            for (float x = ShoreFrom.X; x > ShoreFrom.X - 400; x -= 2)
            {
                var m = new ShipMotion(x - h.DeckHalfLengthM - 1.5f, ShoreFrom.Y, West, 0);
                if (Ship.Stand(h, m, Ground) is not null)
                    return new Vector2(x, ShoreFrom.Y);
            }
            return null;
        }

        private async Task<bool> Until(Func<bool> cond, double secs)
        {
            var limit = DateTime.UtcNow.AddSeconds(secs);
            while (!cond())
            {
                if (DateTime.UtcNow > limit)
                {
                    session.Log($"shipyard demo: timed out; {shipyards.LastResult} {vehicles.LastResult}");
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
