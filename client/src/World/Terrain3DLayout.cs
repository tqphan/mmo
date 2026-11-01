using System;
using System.IO;

namespace Mmo.World
{
    /// <summary>
    /// How the world's files map onto Terrain3D regions (ADR 0032, docs/spikes/terrain3d.md), without
    /// Godot, so both directions are tested in xUnit. Terrain3D puts pixel (px, pz) of the whole
    /// terrain at (px, pz) × the 2 m vertex spacing, and region (rx, rz) holds pixels
    /// rx·1024 … rx·1024 + 1023. The world's samples run from −8000 m, so sample i is pixel i − 4000,
    /// and regions −4 … 3 along each side (16,384 m) cover the map's 16,000 m with 192 m to spare.
    /// Pixels off the map repeat the nearest edge sample.
    /// <para>
    /// The control map's bits 3–6, which Terrain3D v1.1.0 leaves unused, hold the land use class of
    /// the 4 m texel each pixel falls in. The climate isn't stored in Terrain3D (it doesn't fit), so
    /// the way back keeps the climate already in <c>world/</c>.
    /// </para>
    /// </summary>
    public static class Terrain3DLayout
    {
        public const int RegionPx = 1024;
        public const int RegionsPerSide = 8;
        /// <summary>The first region's location along each side.</summary>
        public const int FirstRegion = -RegionsPerSide / 2;
        /// <summary>Pixel of the world's first sample along each side.</summary>
        public const int SampleOffsetPx = -4000;
        public const int Samples = HeightMap.CellsPerSide * HeightMap.QuadsPerCell + 1;
        public const int UseShift = 3;
        public const uint UseMask = 0xFu << UseShift;

        /// <summary>Every region location, west to east then north to south.</summary>
        public static (int Rx, int Rz)[] Regions()
        {
            var all = new (int, int)[RegionsPerSide * RegionsPerSide];
            for (int k = 0; k < all.Length; k++)
                all[k] = (FirstRegion + k % RegionsPerSide, FirstRegion + k / RegionsPerSide);
            return all;
        }

        /// <summary>Region (<paramref name="rx"/>, <paramref name="rz"/>)'s heights in metres, rows of increasing z.</summary>
        public static void FillHeights(HeightMap map, int rx, int rz, Span<float> dst)
        {
            Check(map, dst.Length);
            for (int v = 0; v < RegionPx; v++)
            {
                int j = rz * RegionPx + v - SampleOffsetPx;
                for (int u = 0; u < RegionPx; u++)
                    dst[v * RegionPx + u] = map.Sample(rx * RegionPx + u - SampleOffsetPx, j);
            }
        }

        /// <summary>Region (<paramref name="rx"/>, <paramref name="rz"/>)'s control bits: the land use class in bits 3–6, the rest 0.</summary>
        public static void FillControl(LandRaster? land, int rx, int rz, Span<uint> dst)
        {
            if (dst.Length != RegionPx * RegionPx)
                throw new ArgumentException($"a region has {RegionPx * RegionPx} pixels");
            for (int v = 0; v < RegionPx; v++)
            {
                int j = rz * RegionPx + v - SampleOffsetPx;
                for (int u = 0; u < RegionPx; u++)
                {
                    int i = rx * RegionPx + u - SampleOffsetPx;
                    // Sample i lies on texel i / 2; off the map, Texel is 0 (wild).
                    byte texel = land is null || i < 0 || j < 0 ? (byte)0 : land.Texel(i >> 1, j >> 1);
                    dst[v * RegionPx + u] = (uint)(texel & 0x0f) << UseShift;
                }
            }
        }

        /// <summary>
        /// Write region (<paramref name="rx"/>, <paramref name="rz"/>)'s heights back into the whole map's
        /// <c>.mmoh</c> samples (<see cref="Samples"/>² of them), quantised to the nearest 1/32 m.
        /// </summary>
        public static void ReadHeights(ReadOnlySpan<float> src, int rx, int rz, ushort[] samples)
        {
            if (src.Length != RegionPx * RegionPx || samples.Length != Samples * Samples)
                throw new ArgumentException("wrong sizes for a region or the map");
            for (int v = 0; v < RegionPx; v++)
            {
                int j = rz * RegionPx + v - SampleOffsetPx;
                if (j < 0 || j >= Samples)
                    continue;
                for (int u = 0; u < RegionPx; u++)
                {
                    int i = rx * RegionPx + u - SampleOffsetPx;
                    if (i >= 0 && i < Samples)
                        samples[j * Samples + i] = Encode(src[v * RegionPx + u]);
                }
            }
        }

        /// <summary>
        /// Write region (<paramref name="rx"/>, <paramref name="rz"/>)'s land use classes back into the
        /// whole map's <see cref="LandRaster.Side"/>² texels, each read from the pixel at its north-west corner.
        /// </summary>
        public static void ReadUses(ReadOnlySpan<uint> src, int rx, int rz, byte[] uses)
        {
            if (src.Length != RegionPx * RegionPx || uses.Length != LandRaster.Side * LandRaster.Side)
                throw new ArgumentException("wrong sizes for a region or the map's texels");
            for (int v = 0; v < RegionPx; v++)
            {
                int j = rz * RegionPx + v - SampleOffsetPx;
                if (j < 0 || (j & 1) != 0 || j >> 1 >= LandRaster.Side)
                    continue;
                for (int u = 0; u < RegionPx; u++)
                {
                    int i = rx * RegionPx + u - SampleOffsetPx;
                    if (i >= 0 && (i & 1) == 0 && i >> 1 < LandRaster.Side)
                        uses[(j >> 1) * LandRaster.Side + (i >> 1)] = (byte)((src[v * RegionPx + u] & UseMask) >> UseShift);
                }
            }
        }

        /// <summary>A height in metres as a <c>.mmoh</c> sample: <c>(h + 512) · 32</c>, rounded and clamped.</summary>
        public static ushort Encode(float heightM) =>
            (ushort)Math.Clamp(MathF.Round((heightM + 512.0f) * 32.0f), 0, ushort.MaxValue);

        /// <summary>The whole map's samples as a <c>.mmoh</c> file covering every cell (docs/design/terrain.md).</summary>
        public static void WriteHeightFile(string path, ushort[] samples)
        {
            if (samples.Length != Samples * Samples)
                throw new ArgumentException("not the whole map's samples");
            using var file = new BinaryWriter(File.Create(path));
            file.Write("MMOH"u8);
            foreach (ushort h in new ushort[] { 1, 0, 0, HeightMap.CellsPerSide, HeightMap.CellsPerSide })
                file.Write(h);
            if (!BitConverter.IsLittleEndian)
                throw new PlatformNotSupportedException(".mmoh files are little-endian");
            file.Write(System.Runtime.InteropServices.MemoryMarshal.AsBytes(samples.AsSpan()));
        }

        private static void Check(HeightMap map, int length)
        {
            if (length != RegionPx * RegionPx)
                throw new ArgumentException($"a region has {RegionPx * RegionPx} pixels");
            if (map.CellX != 0 || map.CellZ != 0 || map.Width != Samples || map.Depth != Samples)
                throw new ArgumentException("Terrain3D regions are cut from the whole map");
        }
    }
}
