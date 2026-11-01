using System;
using System.Collections.Generic;
using Mmo.Content;

namespace Mmo.World
{
    /// <summary>
    /// What the terrain and water shaders need to know about the ground, a texel per 4 m over the
    /// whole map, on the land raster's grid (docs/design/art.md): RGBA bytes with road land in R,
    /// town (<see cref="TownByte"/>) or housing land (<see cref="HousingByte"/>) in G, and in B and A
    /// how much inland water and open sea lie within about <see cref="ShoreRadius"/> texels. Open sea
    /// is water joined to the map's edge, so beaches form along the coast and mud round inland pools
    /// such as Croaker Fen's.
    /// </summary>
    public static class GroundMasks
    {
        public const int Side = LandRaster.Side;
        /// <summary>Half-width of each of the two box blurs that spread the sea onto the land, in texels (6 × 4 m).</summary>
        public const int ShoreRadius = 6;
        public const byte TownByte = 255, HousingByte = 100;

        public static byte[] Build(HeightMap map, LandRaster? land)
        {
            var (sea, wet) = Water(map);
            var shore = Blur(Blur(sea));
            var pools = new float[sea.Length];
            for (int t = 0; t < pools.Length; t++)
                pools[t] = wet[t] && sea[t] == 0 ? 1 : 0;
            pools = Blur(Blur(pools));
            var rgba = new byte[Side * Side * 4];
            for (int t = 0; t < Side * Side; t++)
            {
                if (land != null)
                {
                    var use = LandRaster.Use(land.Texel(t % Side, t / Side));
                    rgba[t * 4] = use == LandUse.Road ? (byte)255 : (byte)0;
                    rgba[t * 4 + 1] = use switch { LandUse.Town => TownByte, LandUse.Housing => HousingByte, _ => 0 };
                }
                rgba[t * 4 + 2] = (byte)MathF.Round(Math.Clamp(pools[t], 0, 1) * 255);
                rgba[t * 4 + 3] = (byte)MathF.Round(Math.Clamp(shore[t], 0, 1) * 255);
            }
            return rgba;
        }

        /// <summary>The ground's height at the centre of texel (<paramref name="i"/>, <paramref name="j"/>).</summary>
        public static float Height(HeightMap map, int i, int j)
        {
            float x = (i + 0.5f) * LandRaster.TexelSize - HeightMap.WorldHalf, z = (j + 0.5f) * LandRaster.TexelSize - HeightMap.WorldHalf;
            return map.Height(x, z);
        }

        /// <summary>
        /// Texels under sea level (<c>Wet</c>), and of those (1 in <c>Sea</c>) the ones joined, side to
        /// side, to water at the map's edge.
        /// </summary>
        public static (float[] Sea, bool[] Wet) Water(HeightMap map)
        {
            var wet = new bool[Side * Side];
            System.Threading.Tasks.Parallel.For(0, Side, j =>
            {
                for (int i = 0; i < Side; i++)
                    wet[j * Side + i] = Height(map, i, j) < 0;
            });
            var sea = new float[Side * Side];
            var queue = new Queue<int>();
            void Visit(int t)
            {
                if (wet[t] && sea[t] == 0)
                {
                    sea[t] = 1;
                    queue.Enqueue(t);
                }
            }
            for (int k = 0; k < Side; k++)
            {
                Visit(k);
                Visit((Side - 1) * Side + k);
                Visit(k * Side);
                Visit(k * Side + Side - 1);
            }
            while (queue.TryDequeue(out int t))
            {
                int i = t % Side, j = t / Side;
                if (i > 0) Visit(t - 1);
                if (i < Side - 1) Visit(t + 1);
                if (j > 0) Visit(t - Side);
                if (j < Side - 1) Visit(t + Side);
            }
            return (sea, wet);
        }

        /// <summary>A box blur of <see cref="ShoreRadius"/> along x then z, clamped at the edges.</summary>
        private static float[] Blur(float[] src)
        {
            var across = new float[src.Length];
            var both = new float[src.Length];
            const float width = 2 * ShoreRadius + 1;
            for (int j = 0; j < Side; j++)
            {
                int row = j * Side;
                float sum = 0;
                for (int k = -ShoreRadius; k <= ShoreRadius; k++)
                    sum += src[row + Math.Clamp(k, 0, Side - 1)];
                for (int i = 0; i < Side; i++)
                {
                    across[row + i] = sum / width;
                    sum += src[row + Math.Min(i + ShoreRadius + 1, Side - 1)] - src[row + Math.Max(i - ShoreRadius, 0)];
                }
            }
            for (int i = 0; i < Side; i++)
            {
                float sum = 0;
                for (int k = -ShoreRadius; k <= ShoreRadius; k++)
                    sum += across[Math.Clamp(k, 0, Side - 1) * Side + i];
                for (int j = 0; j < Side; j++)
                {
                    both[j * Side + i] = sum / width;
                    sum += across[Math.Min(j + ShoreRadius + 1, Side - 1) * Side + i] - across[Math.Max(j - ShoreRadius, 0) * Side + i];
                }
            }
            return both;
        }
    }
}
