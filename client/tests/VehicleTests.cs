using System.Collections.Generic;
using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    public class VehicleTests
    {
        private static readonly HullData Cart = Hull.FurrowCart.Data();
        private static readonly HullData Clipper = Hull.Clipper.Data();
        private static readonly ushort Bale = GameContent.Items[0].Wire;

        /// <summary>Flat ground 3 m up, rising at 1.5 (too steep) past z = -30, known within 100 m.</summary>
        private static Ground? Ground(float x, float z)
        {
            if (System.MathF.Abs(x) > 100 || System.MathF.Abs(z) > 100)
                return null;
            return z < -30 ? new Ground(3 + 1.5f * (-30 - z), 3 + 1.5f * (-30 - z), 1.5f) : new Ground(3, 3, 0);
        }

        private static VehicleModel.Seen CartAt(ulong id, float x, float z, bool ours, params ushort[] cargo) =>
            new(id, Cart, new ShipPose(x, 3.9f, z, 0), cargo, ours);

        [Fact]
        public void TheCartStopsAtTheBankLikeTheServer()
        {
            var p = new ShipPredictor(Cart, Ground);
            p.Row(0, 0, 0, 0, 0, 3.9f);
            p.SetInput(0, new ShipInput(100, 0));
            // Heading -z at 4 m/s, the bow reaches z = -30 with the centre at -28.4.
            var stopped = p.Pose(30);
            Assert.InRange(stopped.Z, -28.9f, -28.0f);
            Assert.Equal(0, p.Speed(30));
            Assert.Equal(3 + Cart.DeckHeightM, stopped.Y, 4);
        }

        [Fact]
        public void TheDeckRidesTheGround()
        {
            var p = new ShipPredictor(Cart, (x, z) => new Ground(2 + z * 0.1f, 2 + z * 0.1f, 0.1f));
            p.Row(0, 0, 0, 0, 0, 2.9f);
            p.SetInput(0, new ShipInput(-100, 0));
            var back = p.Pose(10);
            Assert.True(back.Z > 5, $"{back}");
            Assert.Equal(2 + back.Z * 0.1f + Cart.DeckHeightM, back.Y, 2);
        }

        [Fact]
        public void FLoadsIntoTheNearestEmptySlotAndUnloadsOnlyOurOwn()
        {
            var at = new Vector3(1.5f, 3, 0.6f);
            var mine = CartAt(1, 0, 0, ours: true, Bale, 0);
            var theirs = CartAt(2, 0, 0, ours: false, Bale, 0);
            // Carrying: slot 1 is the empty one.
            Assert.Equal(new SlotAction(1, 1, true, 0), VehicleModel.Pick(new[] { mine }, at, carrying: true));
            // Free back: unload slot 0 of ours, but nothing of theirs.
            Assert.Equal(new SlotAction(1, 0, false, Bale), VehicleModel.Pick(new[] { mine }, at, carrying: false));
            Assert.Null(VehicleModel.Pick(new[] { theirs }, at, carrying: false));
            Assert.Equal(new SlotAction(2, 1, true, 0), VehicleModel.Pick(new[] { theirs }, at, carrying: true));
            // Out of reach, and a full cart, offer nothing.
            Assert.Null(VehicleModel.Pick(new[] { mine }, new Vector3(5, 3, 0), carrying: true));
            Assert.Null(VehicleModel.Pick(new[] { CartAt(3, 0, 0, true, Bale, Bale) }, at, carrying: true));
            Assert.Null(VehicleModel.Pick(new[] { new VehicleModel.Seen(4, Hull.Skiff.Data(), new ShipPose(0, 0.5f, 0, 0), new ushort[0], true) }, at, true));
        }

        [Fact]
        public void VStowsTheOneWereAboardElseOurNearest()
        {
            var at = new Vector3(0, 3, 0);
            var near = CartAt(1, 5, 0, ours: true);
            var far = new VehicleModel.Seen(2, Clipper, new ShipPose(50, 2, 0, 0), new ushort[] { 0, 0 }, true);
            var other = CartAt(3, 1, 0, ours: false);
            Assert.Equal(1UL, VehicleModel.StowTarget(new[] { far, near, other }, 0, at));
            Assert.Equal(2UL, VehicleModel.StowTarget(new[] { far, near, other }, 2, at));
            Assert.Null(VehicleModel.StowTarget(new[] { other }, 3, at));
        }

        [Fact]
        public void TheCargoLineNamesEachSlot()
        {
            Assert.Equal($"cargo: {GameContent.Item(Bale)!.Name}, empty", VehicleModel.CargoLine(new[] { Bale, (ushort)0 }));
            Assert.Equal("", VehicleModel.CargoLine(new ushort[0]));
        }
    }
}
