using System;
using System.Buffers.Binary;
using System.IO;
using System.Runtime.InteropServices;

namespace Mmo.World
{
    /// <summary>
    /// The terrain heightmap, read from the same <c>.mmoh</c> file the server checks movement against
    /// (docs/design/terrain.md, ADR 0017): <c>u16</c> samples every 2 m, height = raw / 32 − 512 m.
    /// The file covers whole 64 m cells from (<see cref="CellX"/>, <see cref="CellZ"/>); rows run
    /// along x, in order of increasing z. Kept in step with <c>sim::terrain::HeightFile</c>.
    /// </summary>
    public sealed class HeightMap
    {
        public const float Spacing = 2.0f;
        public const float CellSize = 64.0f;
        public const int QuadsPerCell = 32;
        public const float WorldHalf = 8000.0f;
        public const int CellsPerSide = 250;
        private const ushort Version = 1;

        public readonly int CellX, CellZ, CellsX, CellsZ;
        /// <summary>Samples across (x) and down (z).</summary>
        public readonly int Width, Depth;
        /// <summary>World position of the first sample.</summary>
        public readonly float X0, Z0;
        private readonly ushort[] samples;

        private HeightMap(int cellX, int cellZ, int cellsX, int cellsZ, ushort[] samples)
        {
            CellX = cellX;
            CellZ = cellZ;
            CellsX = cellsX;
            CellsZ = cellsZ;
            Width = cellsX * QuadsPerCell + 1;
            Depth = cellsZ * QuadsPerCell + 1;
            X0 = cellX * CellSize - WorldHalf;
            Z0 = cellZ * CellSize - WorldHalf;
            this.samples = samples;
        }

        public static float Decode(ushort raw) => raw / 32.0f - 512.0f;

        /// <summary>Read a file straight into the sample array, without a second copy of it in memory.</summary>
        public static HeightMap Load(string path)
        {
            using var file = File.OpenRead(path);
            Span<byte> header = stackalloc byte[14];
            if (file.Read(header) < header.Length)
                throw new InvalidDataException("not a .mmoh height file");
            var (cellX, cellZ, cellsX, cellsZ, samples) = Header(header, file.Length);
            file.ReadExactly(MemoryMarshal.AsBytes(samples.AsSpan()));
            return Finish(cellX, cellZ, cellsX, cellsZ, samples);
        }

        public static HeightMap Parse(ReadOnlySpan<byte> bytes)
        {
            if (bytes.Length < 14)
                throw new InvalidDataException("not a .mmoh height file");
            var (cellX, cellZ, cellsX, cellsZ, samples) = Header(bytes[..14], bytes.Length);
            MemoryMarshal.Cast<byte, ushort>(bytes[14..]).CopyTo(samples);
            return Finish(cellX, cellZ, cellsX, cellsZ, samples);
        }

        /// <summary>Check the 14-byte header against the file's length, and allocate the samples.</summary>
        private static (int, int, int, int, ushort[]) Header(ReadOnlySpan<byte> header, long length)
        {
            if (header[0] != 'M' || header[1] != 'M' || header[2] != 'O' || header[3] != 'H')
                throw new InvalidDataException("not a .mmoh height file");
            Span<ushort> h = stackalloc ushort[5];
            for (int k = 0; k < 5; k++)
                h[k] = BinaryPrimitives.ReadUInt16LittleEndian(header[(4 + 2 * k)..]);
            if (h[0] != Version)
                throw new InvalidDataException($"height file version {h[0]}, expected {Version}");
            int cellX = h[1], cellZ = h[2], cellsX = h[3], cellsZ = h[4];
            if (cellsX == 0 || cellsZ == 0 || cellX + cellsX > CellsPerSide || cellZ + cellsZ > CellsPerSide)
                throw new InvalidDataException("height file region is empty or off the map");
            long w = cellsX * QuadsPerCell + 1, d = cellsZ * QuadsPerCell + 1;
            if (length != 14 + 2 * w * d)
                throw new InvalidDataException($"height file has {length} bytes, expected {14 + 2 * w * d}");
            return (cellX, cellZ, cellsX, cellsZ, new ushort[w * d]);
        }

        private static HeightMap Finish(int cellX, int cellZ, int cellsX, int cellsZ, ushort[] samples)
        {
            if (!BitConverter.IsLittleEndian)
                BinaryPrimitives.ReverseEndianness(samples, samples);
            return new HeightMap(cellX, cellZ, cellsX, cellsZ, samples);
        }

        /// <summary>Height of sample (<paramref name="i"/> along x, <paramref name="j"/> along z), clamped to the file's edges.</summary>
        public float Sample(int i, int j)
        {
            i = Math.Clamp(i, 0, Width - 1);
            j = Math.Clamp(j, 0, Depth - 1);
            return Decode(samples[j * Width + i]);
        }

        /// <summary>Bilinear height at a world position, clamped to the file's edges.</summary>
        public float Height(float x, float z)
        {
            float fx = Math.Clamp((x - X0) / Spacing, 0, Width - 1), fz = Math.Clamp((z - Z0) / Spacing, 0, Depth - 1);
            int i = Math.Min((int)fx, Width - 2), j = Math.Min((int)fz, Depth - 2);
            float tx = fx - i, tz = fz - j;
            float a = Sample(i, j) + (Sample(i + 1, j) - Sample(i, j)) * tx;
            float b = Sample(i, j + 1) + (Sample(i + 1, j + 1) - Sample(i, j + 1)) * tx;
            return a + (b - a) * tz;
        }

        /// <summary>The highest sample in [i0, i1] × [j0, j1], clamped to the file.</summary>
        public float Max(int i0, int j0, int i1, int j1)
        {
            ushort max = 0;
            for (int j = Math.Max(j0, 0); j <= Math.Min(j1, Depth - 1); j++)
                for (int i = Math.Max(i0, 0); i <= Math.Min(i1, Width - 1); i++)
                    max = Math.Max(max, samples[j * Width + i]);
            return Decode(max);
        }
    }
}
