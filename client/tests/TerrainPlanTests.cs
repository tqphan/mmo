using System;
using System.Linq;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    public class TerrainPlanTests
    {
        /// <summary>A file of <paramref name="cells"/>² cells from the map's corner, at one height.</summary>
        private static HeightMap Flat(int cells, float height)
        {
            int side = cells * 32 + 1;
            var bytes = new byte[14 + 2 * side * side];
            "MMOH"u8.CopyTo(bytes);
            bytes[4] = 1;
            bytes[10] = (byte)cells;
            bytes[12] = (byte)cells;
            ushort raw = (ushort)((height + 512) * 32);
            for (int k = 0; k < side * side; k++)
            {
                bytes[14 + 2 * k] = (byte)raw;
                bytes[15 + 2 * k] = (byte)(raw >> 8);
            }
            return HeightMap.Parse(bytes);
        }

        [Theory]
        [InlineData(0, 0)]
        [InlineData(199, 0)]
        [InlineData(200, 1)]
        [InlineData(399, 1)]
        [InlineData(400, 2)]
        [InlineData(800, 3)]
        [InlineData(1600, 4)]
        [InlineData(3200, 5)]
        [InlineData(9999, 5)]
        [InlineData(10000, TerrainPlan.None)]
        public void LevelDoublesWithDistance(float distance, int level) => Assert.Equal(level, TerrainPlan.LevelAt(distance));

        [Fact]
        public void TilesCoverTheFileAndStartNearestFirst()
        {
            // 20 cells = 640 quads = 5 tiles of 128 across.
            var plan = new TerrainPlan(Flat(20, 10));
            Assert.Equal((5, 5), (plan.TilesX, plan.TilesZ));
            var first = plan.Update(-8000 + 10, -8000 + 10);
            Assert.Equal(25, first.Count);
            Assert.Equal((0, 0), first[0]);
            var distances = first.Select(c => plan.Distance(c.Tile, -7990, -7990)).ToArray();
            Assert.Equal(distances.OrderBy(d => d), distances);
            // Nothing moved, nothing changes.
            Assert.Empty(plan.Update(-7990, -7990));
        }

        [Fact]
        public void LevelsChangeOnlyPastTheHysteresis()
        {
            var plan = new TerrainPlan(Flat(16, 10));
            float x0 = -8000 + 256, z = -8000 + 100;
            // Tile 1 starts at x0. Stand 100 m west of it: level 0.
            plan.Update(x0 - 100, z);
            Assert.Equal(0, plan.Level(1));
            // 210 m is past 200 m but not by 10%: no change. 225 m is.
            Assert.DoesNotContain(plan.Update(x0 - 210, z), c => c.Tile == 1);
            Assert.Contains((1, 1), plan.Update(x0 - 225, z));
            // Back to 190 m: still within 10% of 200, so it stays at level 1. 175 m goes back.
            Assert.DoesNotContain(plan.Update(x0 - 190, z), c => c.Tile == 1);
            Assert.Contains((1, 0), plan.Update(x0 - 175, z));
        }

        [Fact]
        public void DeepWaterAndFarTilesAreNotDrawn()
        {
            var deep = new TerrainPlan(Flat(16, -30));
            Assert.Equal(deep.Count, deep.DeepCount);
            Assert.Empty(deep.Update(-7900, -7900));
            var shallow = new TerrainPlan(Flat(16, -20));
            Assert.Equal(0, shallow.DeepCount);
            Assert.Equal(shallow.Count, shallow.Update(-7900, -7900).Count);
            // From 12 km away, everything is past the view distance.
            Assert.Empty(new TerrainPlan(Flat(16, 0)).Update(4000, 4000));
        }
    }
}
