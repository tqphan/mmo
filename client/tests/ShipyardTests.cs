using System.Linq;
using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    public class ShipyardTests
    {
        private static readonly ShipyardData Yard = GameContent.Shipyards[0];
        private static readonly HullData Clipper = Hull.Clipper.Data();
        private static ushort Wire(string id) => GameContent.Items.First(i => i.Id == id).Wire;

        /// <summary>A yard at the origin facing -z, with <paramref name="stage"/> stages done.</summary>
        private static ShipyardModel.Seen At0(byte stage, bool ours = true) => new(7, Yard, new ShipMotion(0, 0, 0, 0), stage, ours);

        [Fact]
        public void TheYardPromptSaysWhatTheNextStageNeeds()
        {
            var by = new Vector3(Clipper.DeckHalfWidthM + 2, 0, 0);
            var none = ShipyardModel.Pick(new[] { At0(0) }, by, 0);
            Assert.Equal(new YardAction(7, "Keel and Frames needs a Timber Bundle", false), none);
            var wrong = ShipyardModel.Pick(new[] { At0(1) }, by, Wire("timber_bundle"));
            Assert.Equal("Ironwork needs an Ironwork Crate", wrong?.Prompt);
            Assert.Equal(new YardAction(7, "F: deliver the Timber Bundle (Keel and Frames, 25 labor)", true), ShipyardModel.Pick(new[] { At0(0) }, by, Wire("timber_bundle")));
            Assert.Equal(new YardAction(7, "F: Fitting Out I (25 labor)", true), ShipyardModel.Pick(new[] { At0(3) }, by, 0));
            Assert.Equal(new YardAction(7, "F: launch the Swiftgull (Fitting Out III, 25 labor)", true), ShipyardModel.Pick(new[] { At0(5) }, by, 0));
            Assert.Equal(new YardAction(7, "only the owner can launch this Swiftgull", false), ShipyardModel.Pick(new[] { At0(5, ours: false) }, by, 0));
        }

        [Fact]
        public void YardsAreOnlyReachedNearTheirFootprint()
        {
            Assert.NotNull(ShipyardModel.Pick(new[] { At0(0) }, new Vector3(0, 0, Clipper.DeckHalfLengthM + 3.9f), 0));
            Assert.Null(ShipyardModel.Pick(new[] { At0(0) }, new Vector3(0, 0, Clipper.DeckHalfLengthM + 4.1f), 0));
            Assert.Null(ShipyardModel.Pick(new ShipyardModel.Seen[0], Vector3.Zero, 0));
        }

        [Fact]
        public void LabelsShowStagesAndWrecks()
        {
            Assert.Equal("Swiftgull Shipyard  2/6: Rigging next", ShipyardModel.StageLine(Yard, 2));
            Assert.Equal("wrecked", ShipyardModel.WreckLabel(false, 0, 100));
            const long min = 60_000_000;
            Assert.Equal("mending, 30 min", ShipyardModel.WreckLabel(true, 30 * min, 0));
            Assert.Equal("mending, 1 min", ShipyardModel.WreckLabel(true, 30 * min, 30 * min - 1));
            Assert.Equal("", ShipyardModel.WreckLabel(true, 30 * min, 30 * min));
        }

        [Fact]
        public void VFallsBackToOurVehicleOutOfSightAndHRepairsWhatsInReach()
        {
            var at = new Vector3(0, 0, 0);
            var theirs = new VehicleModel.Seen(3, Clipper, new ShipPose(5, 2, 0, 0), new ushort[] { 0, 0 }, false, 5000);
            Assert.Equal(42UL, VehicleModel.StowTarget(new[] { theirs }, 0, at, new ulong[] { 42 }));
            Assert.Null(VehicleModel.StowTarget(new[] { theirs }, 0, at, new ulong[0]));
            // H: anyone's damaged vehicle in reach, or the one we're aboard.
            Assert.Equal(3UL, VehicleModel.RepairTarget(new[] { theirs }, 0, at));
            Assert.Null(VehicleModel.RepairTarget(new[] { theirs with { Hp = Clipper.Hp } }, 0, at));
            Assert.Null(VehicleModel.RepairTarget(new[] { theirs with { Pose = new ShipPose(50, 2, 0, 0) } }, 0, at));
            Assert.Equal(3UL, VehicleModel.RepairTarget(new[] { theirs with { Hp = Clipper.Hp } }, 3, at));
            Assert.Equal("it's sinking!", VehicleModel.StateLine(theirs with { Hp = 0 }, 0));
            Assert.Equal("under fire", VehicleModel.StateLine(theirs with { HitAt = 1_000_000 }, 5_000_000));
            Assert.StartsWith("H: repair (Hullmender's Kit, 10 labor)", VehicleModel.StateLine(theirs with { HitAt = 1_000_000 }, 20_000_000));
            Assert.Equal("", VehicleModel.StateLine(theirs with { Hp = Clipper.Hp }, 0));
        }
    }
}
