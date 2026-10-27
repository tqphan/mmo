using System;
using Mmo.Content;
using Mmo.Sim;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The placement preview's inputs (docs/design/land.md): the client's whole-map readers feed <see cref="Land.CheckSite"/> as the server's chunks do.</summary>
    public class LandTests
    {
        private static readonly PlotData Garden = GameContent.Plots[0];

        /// <summary>The map's middle 2 × 2 cells, x and z from 0 to 128 m, flat at 10 m except a hump at (20, 20); housing round the origin.</summary>
        private static (HeightMap Map, LandRaster Land) World()
        {
            const int side = 2 * HeightMap.QuadsPerCell + 1;
            var samples = new ushort[side * side];
            for (int j = 0; j < side; j++)
                for (int i = 0; i < side; i++)
                    samples[j * side + i] = (ushort)MathF.Round((10.0f + (i == 10 && j == 10 ? 5.0f : 0.0f) + 512) * 32);
            var map = HeightMap.FromSamples(125, 125, 2, 2, samples);
            var texels = new byte[LandRaster.Side * LandRaster.Side];
            int t0 = 125 * LandRaster.TexelsPerCell;
            for (int j = t0; j < t0 + 32; j++)
                for (int i = t0; i < t0 + 32; i++)
                    texels[j * LandRaster.Side + i] = (byte)LandUse.Housing;
            return (map, new LandRaster(texels));
        }

        private static SiteError? Check(float x, float z, params Rect[] others)
        {
            var (map, land) = World();
            return Land.CheckSite(land.Texel, map.GridSample, Garden, Land.Snap(x), Land.Snap(z), others).Error;
        }

        [Fact]
        public void APreviewOnTheClientsMapsPassesAndFails()
        {
            Assert.Null(Check(60.9f, 60.2f));
            Assert.Equal(SiteError.TooSteep, Check(20, 20));
            Assert.Equal(SiteError.NotHousing, Check(-4, 60)); // west of the housing
            Assert.Equal(SiteError.Taken, Check(60, 60, Rect.Plot(Garden, 64, 64)));
            Assert.Null(Check(60, 60, Rect.Plot(Garden, 68, 60)));
            // Housing on land the file doesn't cover is no ground.
            var (map, _) = World();
            var all = new byte[LandRaster.Side * LandRaster.Side];
            Array.Fill(all, (byte)LandUse.Housing);
            Assert.Equal(SiteError.NoGround, Land.CheckSite(new LandRaster(all).Texel, map.GridSample, Garden, 126, 60, Array.Empty<Rect>()).Error);
        }

        [Fact]
        public void GridSamplesFollowTheServersChunks()
        {
            var (map, _) = World();
            int i0 = 125 * HeightMap.QuadsPerCell;
            Assert.Equal(15.0f, map.GridSample(i0 + 10, i0 + 10));
            Assert.Equal(10.0f, map.GridSample(i0 + 63, i0));
            Assert.Null(map.GridSample(i0 + 64, i0)); // the next cell's, which the file lacks
            Assert.Null(map.GridSample(i0 - 1, i0));
            Assert.Null(map.GridSample(-1, 0));
        }

        [Fact]
        public void SpansReadAsDaysHoursMinutesOrSeconds()
        {
            const long s = 1_000_000;
            Assert.Equal("3d 4h", Land.Span((3 * 86_400 + 4 * 3_600 + 59) * s));
            Assert.Equal("2h 5m", Land.Span((2 * 3_600 + 5 * 60) * s));
            Assert.Equal("1m 30s", Land.Span(90 * s));
            Assert.Equal("45s", Land.Span(45 * s));
            Assert.Equal("0s", Land.Span(-5 * s));
        }
    }
}
