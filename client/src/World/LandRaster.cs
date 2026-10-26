using System;
using Mmo.Content;

namespace Mmo.World
{
    /// <summary>
    /// The world's land-use raster (docs/design/world.md): a byte per 4 m texel over the whole map,
    /// rows of increasing z, with the use class in the low 4 bits and the climate in bits 4–6.
    /// </summary>
    public sealed class LandRaster
    {
        public const float TexelSize = 4.0f;
        public const int TexelsPerCell = HeightMap.QuadsPerCell / 2;
        public const int Side = HeightMap.CellsPerSide * TexelsPerCell;

        private readonly byte[] texels;

        public LandRaster(byte[] texels)
        {
            if (texels.Length != Side * Side)
                throw new ArgumentException($"a land raster has {Side * Side} texels, not {texels.Length}");
            this.texels = texels;
        }

        /// <summary>The texel under a world position, clamped to the map's edges.</summary>
        public byte At(float x, float z)
        {
            int i = Math.Clamp((int)MathF.Floor((x + HeightMap.WorldHalf) / TexelSize), 0, Side - 1);
            int j = Math.Clamp((int)MathF.Floor((z + HeightMap.WorldHalf) / TexelSize), 0, Side - 1);
            return texels[j * Side + i];
        }

        public static LandUse Use(byte texel) => (LandUse)(texel & 0x0f);

        public static Climate ClimateOf(byte texel) => (Climate)((texel >> 4) & 0x07);
    }
}
