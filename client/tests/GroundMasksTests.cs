using System;
using Mmo.Content;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The ground masks the terrain and sea shaders read (docs/design/art.md).</summary>
    public class GroundMasksTests
    {
        /// <summary>
        /// The map's middle 2 × 2 cells (x and z from 0 to 128 m) in sea 5 m deep, which the map's
        /// clamped edges carry out to the edge of the world. An island 5 m high from 20 to 108 m holds
        /// a pool 1 m deep from 56 to 72 m. A road crosses the island at z 40 m; the rest of it is town
        /// north of 90 m and housing south of it.
        /// </summary>
        private static byte[] Build()
        {
            const int side = 2 * HeightMap.QuadsPerCell + 1;
            var samples = new ushort[side * side];
            for (int j = 0; j < side; j++)
            {
                for (int i = 0; i < side; i++)
                {
                    float x = i * HeightMap.Spacing, z = j * HeightMap.Spacing;
                    bool island = x >= 20 && x <= 108 && z >= 20 && z <= 108;
                    bool pool = x >= 56 && x <= 72 && z >= 56 && z <= 72;
                    float h = pool ? -1 : island ? 5 : -5;
                    samples[j * side + i] = (ushort)MathF.Round((h + 512) * 32);
                }
            }
            var map = HeightMap.FromSamples(125, 125, 2, 2, samples);
            var texels = new byte[LandRaster.Side * LandRaster.Side];
            int t0 = 125 * LandRaster.TexelsPerCell;
            for (int j = t0; j < t0 + 32; j++)
                for (int i = t0; i < t0 + 32; i++)
                    texels[j * LandRaster.Side + i] = (byte)(j - t0 == 10 ? LandUse.Road : j - t0 > 22 ? LandUse.Town : LandUse.Housing);
            return GroundMasks.Build(map, new LandRaster(texels));
        }

        private static readonly Lazy<byte[]> Masks = new(Build);

        /// <summary>The mask bytes (R, G, B, A) at world position (x, z).</summary>
        private static (byte Road, byte Settled, byte Pools, byte Sea) At(float x, float z)
        {
            int i = (int)MathF.Floor((x + HeightMap.WorldHalf) / LandRaster.TexelSize);
            int j = (int)MathF.Floor((z + HeightMap.WorldHalf) / LandRaster.TexelSize);
            int t = (j * GroundMasks.Side + i) * 4;
            var m = Masks.Value;
            return (m[t], m[t + 1], m[t + 2], m[t + 3]);
        }

        [Fact]
        public void TheOpenSeaIsFullAndFadesInlandToNothing()
        {
            Assert.Equal(255, At(-200, -200).Sea);
            Assert.Equal(255, At(2000, 3000).Sea);
            // At the coast it's about half; it reaches some way in, but not across the island.
            Assert.InRange(At(22, 64).Sea, 90, 170);
            Assert.True(At(30, 64).Sea > 0);
            Assert.True(At(30, 64).Sea < At(22, 64).Sea);
        }

        [Fact]
        public void AnInlandPoolIsNotOpenSea()
        {
            // 44 m from the coast it gets only the open sea's fading edge.
            var pool = At(64, 64);
            Assert.InRange(pool.Sea, 1, 40);
            // A 16 m pool, spread over about 100 m: clear where it is, less beside it.
            Assert.True(pool.Pools > 0.02 * 255);
            Assert.InRange(At(44, 64).Pools, 1, pool.Pools - 1);
            Assert.Equal(0, At(-200, -200).Pools);
        }

        [Fact]
        public void LandUseGoesInRedAndGreen()
        {
            Assert.Equal((255, 0), (At(30, 42).Road, At(30, 42).Settled));
            Assert.Equal((0, GroundMasks.HousingByte), (At(30, 60).Road, At(30, 60).Settled));
            Assert.Equal((0, GroundMasks.TownByte), (At(30, 100).Road, At(30, 100).Settled));
            Assert.Equal((0, 0), (At(-200, -200).Road, At(-200, -200).Settled));
        }
    }
}
