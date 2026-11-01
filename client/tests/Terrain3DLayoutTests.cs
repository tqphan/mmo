using System;
using System.IO;
using System.Linq;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>
    /// The world's files go into Terrain3D regions and come back unchanged (ADR 0032,
    /// docs/spikes/terrain3d.md): heights to the sample, land use classes to the texel.
    /// </summary>
    public class Terrain3DLayoutTests
    {
        private static WorldData CommittedWorld()
        {
            for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir != null; dir = dir.Parent)
                if (File.Exists(Path.Combine(dir.FullName, "world", WorldFiles.Manifest)))
                    return WorldFiles.Load(Path.Combine(dir.FullName, "world"));
            throw new DirectoryNotFoundException("no world/ above the test's directory");
        }

        [Fact]
        public void SixtyFourRegionsCoverTheMap()
        {
            var regions = Terrain3DLayout.Regions();
            Assert.Equal(64, regions.Distinct().Count());
            int first = Terrain3DLayout.FirstRegion * Terrain3DLayout.RegionPx;
            int last = (Terrain3DLayout.FirstRegion + Terrain3DLayout.RegionsPerSide) * Terrain3DLayout.RegionPx - 1;
            // The first and last samples, at ±8000 m, are pixels inside the regions.
            Assert.True(first <= Terrain3DLayout.SampleOffsetPx);
            Assert.True(last >= Terrain3DLayout.SampleOffsetPx + Terrain3DLayout.Samples - 1);
            Assert.Equal(-8000.0f, Terrain3DLayout.SampleOffsetPx * HeightMap.Spacing);
        }

        [Fact]
        public void EveryQuantisedHeightSurvivesAFloat()
        {
            for (int raw = 0; raw <= ushort.MaxValue; raw++)
                Assert.Equal((ushort)raw, Terrain3DLayout.Encode(HeightMap.Decode((ushort)raw)));
            Assert.Equal((ushort)0, Terrain3DLayout.Encode(-9999));
            Assert.Equal(ushort.MaxValue, Terrain3DLayout.Encode(9999));
        }

        [Fact]
        public void TheCommittedWorldRoundTrips()
        {
            var world = CommittedWorld();
            int n = Terrain3DLayout.Samples;
            var samples = new ushort[n * n];
            var uses = new byte[LandRaster.Side * LandRaster.Side];
            Array.Fill(samples, (ushort)1);
            Array.Fill(uses, (byte)0xff);
            var heights = new float[Terrain3DLayout.RegionPx * Terrain3DLayout.RegionPx];
            var control = new uint[heights.Length];
            foreach (var (rx, rz) in Terrain3DLayout.Regions())
            {
                Terrain3DLayout.FillHeights(world.Heights, rx, rz, heights);
                Terrain3DLayout.FillControl(world.Land, rx, rz, control);
                Assert.All(control, c => Assert.Equal(0u, c & ~Terrain3DLayout.UseMask));
                Terrain3DLayout.ReadHeights(heights, rx, rz, samples);
                Terrain3DLayout.ReadUses(control, rx, rz, uses);
            }
            for (int j = 0; j < n; j++)
                for (int i = 0; i < n; i++)
                    if (Terrain3DLayout.Encode(world.Heights.Sample(i, j)) != samples[j * n + i])
                        Assert.Fail($"sample ({i}, {j}) came back as {samples[j * n + i]}");
            for (int j = 0; j < LandRaster.Side; j++)
                for (int i = 0; i < LandRaster.Side; i++)
                    if ((world.Land!.Texel(i, j) & 0x0f) != uses[j * LandRaster.Side + i])
                        Assert.Fail($"texel ({i}, {j}) came back as {uses[j * LandRaster.Side + i]}");
        }

        [Fact]
        public void PixelsOffTheMapRepeatItsEdge()
        {
            var world = CommittedWorld();
            var heights = new float[Terrain3DLayout.RegionPx * Terrain3DLayout.RegionPx];
            int rx = Terrain3DLayout.FirstRegion, rz = Terrain3DLayout.FirstRegion;
            Terrain3DLayout.FillHeights(world.Heights, rx, rz, heights);
            // Pixel (0, 0) of the north-west region is 96 pixels west and north of the first sample.
            Assert.Equal(world.Heights.Sample(0, 0), heights[0]);
            int inside = 96 * Terrain3DLayout.RegionPx + 96;
            Assert.Equal(world.Heights.Sample(0, 0), heights[inside]);
            Assert.Equal(world.Heights.Sample(1, 0), heights[inside + 1]);
        }
    }
}
