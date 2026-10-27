using System;
using System.Linq;
using Mmo.Content;
using Mmo.Sim;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The planting preview's inputs and the crop labels (docs/design/farming.md).</summary>
    public class FarmTests
    {
        private static readonly CropData Grain = GameContent.Crops.First(c => c.Id == "hearthgrain");
        private static readonly CropData Woolback = GameContent.Crops.First(c => c.Id == "woolback");

        /// <summary>The map's middle 2 × 2 cells, x and z from 0 to 128 m, flat at 10 m; wild and temperate except a town texel at (40, 40) and an arid one at (60, 60).</summary>
        private static (HeightMap Map, LandRaster Land) World()
        {
            const int side = 2 * HeightMap.QuadsPerCell + 1;
            var samples = new ushort[side * side];
            Array.Fill(samples, (ushort)MathF.Round((10.0f + 512) * 32));
            var map = HeightMap.FromSamples(125, 125, 2, 2, samples);
            var texels = new byte[LandRaster.Side * LandRaster.Side];
            int t0 = 125 * LandRaster.TexelsPerCell;
            texels[(t0 + 10) * LandRaster.Side + t0 + 10] = (byte)LandUse.Town;
            texels[(t0 + 15) * LandRaster.Side + t0 + 15] = (byte)((byte)Climate.Arid << 4);
            return (map, new LandRaster(texels));
        }

        private static (Spot Spot, SpotError? Error) Check(CropData kind, float x, float z, PlotSpot[]? plots = null, params Planting[] others)
        {
            var (map, land) = World();
            return Farm.CheckSpot(land.Texel, map.GridSample, kind, x, z, plots ?? Array.Empty<PlotSpot>(), others);
        }

        [Fact]
        public void APreviewOnTheClientsMapsPassesAndFails()
        {
            Assert.Equal((new Spot(10, null, true), (SpotError?)null), Check(Grain, 20.5f, 20.5f));
            Assert.False(Check(Grain, 61, 61).Spot.Ideal, "arid isn't Hearthgrain's climate");
            Assert.Equal(SpotError.NotPlantable, Check(Grain, 41, 41).Error);
            // The next cell's samples are missing.
            Assert.Equal(SpotError.NoGround, Check(Grain, 127.5f, 20).Error);
            Assert.Equal(SpotError.PlotOnly, Check(Woolback, 20, 20).Error);
            var farm = new PlotSpot(new Rect(16, 16, 32, 32), true);
            Assert.Equal(0, Check(Woolback, 20, 20, new[] { farm }).Spot.Plot);
            Assert.Equal(SpotError.NoAccess, Check(Grain, 20, 20, new[] { farm with { MayTend = false } }).Error);
            Assert.Equal(SpotError.TooClose, Check(Grain, 20, 20, null, new Planting(20.9f, 20, 0.5f)).Error);
            Assert.Null(Check(Grain, 20, 20, null, new Planting(21, 20, 0.5f)).Error);
        }

        [Fact]
        public void LabelsSayWhenItsRipe()
        {
            const long s = 1_000_000, t = 1_800_000_000 * s;
            long ripe = Farm.RipeAt(Grain, t, ideal: true);
            Assert.Equal(30, (ripe - t) / (60 * s));
            Assert.Equal("50% · ripe in 15m 3s", Farm.StageText(Grain, t, ripe, t + (ripe - t) / 2));
            Assert.Equal("ripe · withers in 1d 0h", Farm.StageText(Grain, t, ripe, ripe));
            Assert.Equal("withered", Farm.StageText(Grain, t, ripe, Farm.WithersAt(Grain, ripe)!.Value));
            Assert.Equal("ripe", Farm.StageText(Woolback, t, t, t + 365 * 86_400 * s));
        }
    }
}
