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
    /// <c>--vehicle-demo</c> (dev builds, docs/design/vehicles.md): on the trade island west of
    /// spawn, grants a Furrow Cart and summons it from the bag, loads a dev pack into it, climbs
    /// onto the bench and drives it a little way and round a bend, unloads the pack and stows the
    /// cart. Then on the west beach it summons a Driftpine Skiff, climbs in from the water, rows it
    /// and stows it, leaving us swimming. It pauses at each step for captures.
    /// </summary>
    public partial class VehicleDemo : Node
    {
        /// <summary>Firm, gentle ground on the placeholder world's trade island (the `vehicle` scenario's spot).</summary>
        private static readonly Vector2 CartAt = new(-150, 30);
        /// <summary>Where to start looking west for water deep enough for a skiff.</summary>
        private static readonly Vector2 ShoreFrom = new(-300, 30);

        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly ShipControls controls;
        private readonly Vehicles vehicles;
        private readonly HeightMap map;
        private bool started;

        public VehicleDemo(Session session, PlayerController player, EntityRegistry entities, ShipControls controls, Vehicles vehicles, HeightMap map)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.controls = controls;
            this.vehicles = vehicles;
            this.map = map;
            Name = "VehicleDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null || session.Near is not { Ready: true })
                return;
            started = true;
            _ = Run();
        }

        private ShipView? Ours(Hull hull) =>
            entities.Ships.FirstOrDefault(s => s.Hull.Hull == hull && s.Vehicle is { } v && v.Owner == session.Me?.CharId);

        private bool Carrying => session.Conn!.Db.MyPack.Iter().Any(p => p.Carrier == session.Me?.CharId);

        private async Task Run()
        {
            var r = session.Conn!.Reducers;
            if (Carrying)
                r.DevDiscardPack();
            var cartHull = Hull.FurrowCart.Data();
            var cart = await Summon(Hull.FurrowCart, "furrow_cart", CartAt, 0);
            if (cart is null)
                return;

            // A pack into the first slot, from the ground beside it.
            r.DevGivePack("grain_bale");
            if (!await Until(() => Carrying, 5))
                return;
            var beside = cart.Pose.ToWorld(new System.Numerics.Vector3(cartHull.Slots[0].X - 1.6f, 0, cartHull.Slots[0].Z));
            player.DevTeleport(beside.X, beside.Z);
            await Wait(1.5);
            if (!vehicles.Interact() || !await Until(() => cart.Vehicle?.Cargo[0] != 0, 5))
                return;
            session.Log("vehicle demo: loaded a Grain Bale into the cart");
            await Wait(2.0);

            // Up onto the bench, and drive: ahead, then round to the right.
            if (!await Board(cart))
                return;
            r.TakeHelm();
            if (!await Until(() => controls.HoldsHelm, 5))
                return;
            var from = cart.Pose;
            controls.DemoInput = new ShipInput(100, 0);
            await Wait(5.0);
            controls.DemoInput = new ShipInput(100, 60);
            await Wait(4.0);
            controls.DemoInput = new ShipInput(0, 0);
            await Wait(2.0);
            session.Log($"vehicle demo: drove the cart {new Vector2(cart.Pose.X - from.X, cart.Pose.Z - from.Z).Length():F1} m, deck {from.Y:F2} → {cart.Pose.Y:F2} m; {session.Me?.Corrections ?? 0} corrections");

            // Unload from the bench, get down and stow it.
            if (!vehicles.Interact() || !await Until(() => Carrying, 5))
                return;
            session.Log("vehicle demo: unloaded the pack");
            await Wait(1.5);
            r.DevDiscardPack();
            controls.DemoInput = null;
            r.LeaveHelm();
            await Wait(0.5);
            vehicles.Stow(cart.Id);
            if (!await Until(() => Ours(Hull.FurrowCart) is null, 5))
                return;
            session.Log("vehicle demo: stowed the cart");
            await Wait(1.5);

            // A skiff off the west beach.
            if (FindWater() is not Vector2 shore)
            {
                session.Log("vehicle demo: no water deep enough for a skiff west of the island");
                return;
            }
            var skiff = await Summon(Hull.Skiff, "driftpine_skiff", shore, Mathf.Pi / 2);
            if (skiff is null || !await Board(skiff))
                return;
            r.TakeHelm();
            if (!await Until(() => controls.HoldsHelm, 5))
                return;
            controls.DemoInput = new ShipInput(100, 0);
            await Wait(8.0);
            controls.DemoInput = new ShipInput(0, 0);
            await Wait(2.0);
            controls.DemoInput = null;
            vehicles.Stow(skiff.Id);
            if (!await Until(() => Ours(Hull.Skiff) is null && player.Parent == 0, 5))
                return;
            await Wait(3.0);
            session.Log($"vehicle demo: done; {session.Me?.Corrections ?? 0} corrections");
        }

        /// <summary>Grant item <paramref name="item"/>, stand at <paramref name="at"/> facing <paramref name="yaw"/>, and summon it from the bag.</summary>
        private async Task<ShipView?> Summon(Hull hull, string item, Vector2 at, float yaw)
        {
            var conn = session.Conn!;
            ushort wire = GameContent.Items.First(i => i.Id == item).Wire;
            if (!conn.Db.MyInventory.Iter().Any(i => i.Def == wire))
                conn.Reducers.DevGrantItem(item, 1);
            if (!await Until(() => conn.Db.MyInventory.Iter().Any(i => i.Def == wire), 5))
                return null;
            player.DevTeleport(at.X, at.Y);
            await Wait(1.5);
            player.Face(yaw);
            await Wait(0.5);
            var stack = conn.Db.MyInventory.Iter().First(i => i.Def == wire);
            vehicles.Begin(new BagStack(stack.Id, stack.Slot, stack.Def, stack.Count));
            if (!await Until(() => Ours(hull) is { Predictor.HasRow: true }, 5))
                return null;
            await Wait(2.0);
            session.Log($"vehicle demo: summoned a {hull.Data().Name}");
            return Ours(hull);
        }

        /// <summary>Stand beside the helm and climb aboard.</summary>
        private async Task<bool> Board(ShipView v)
        {
            var h = v.Hull;
            var beside = v.Pose.ToWorld(new System.Numerics.Vector3(h.DeckHalfWidthM + 0.9f, 0, h.HelmZ));
            player.DevTeleport(beside.X, beside.Z);
            await Wait(1.5);
            player.DemoClimb();
            return await Until(() => player.Parent == v.Id, 5);
        }

        /// <summary>A spot west of the island where a skiff summoned facing west would float, and go on floating for 40 m.</summary>
        private Vector2? FindWater()
        {
            var h = Hull.Skiff.Data();
            Ground? Ground(float x, float z) => Terrain.Ground(map.QuadCorner, x, z);
            for (float x = ShoreFrom.X; x > ShoreFrom.X - 400; x -= 2)
            {
                // Facing west (yaw π/2), the skiff's centre lies half its length and the gap further on.
                var m = new ShipMotion(x - h.DeckHalfLengthM - 1.5f, ShoreFrom.Y, Mathf.Pi / 2, 0);
                bool clear = true;
                for (float d = 0; d <= 40 && clear; d += 4)
                    clear = Ship.Stand(h, m with { X = m.X - d }, Ground) is not null;
                if (map.Height(x, ShoreFrom.Y) > -1.0f && clear)
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
                    session.Log("vehicle demo: timed out; " + vehicles.LastResult);
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
