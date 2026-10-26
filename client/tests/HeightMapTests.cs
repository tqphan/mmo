using System;
using System.Globalization;
using System.IO;
using System.Linq;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The client reads <c>.mmoh</c> files as <c>sim::terrain::HeightFile</c> writes them (vectors/sim.txt).</summary>
    public class HeightMapTests
    {
        private static string[][] Lines(string kind) =>
            File.ReadLines(Path.Combine(AppContext.BaseDirectory, "vectors", "sim.txt"))
                .Where(l => l.StartsWith(kind + " "))
                .Select(l => l.Split(' '))
                .ToArray();

        private static HeightMap FromVectors() => HeightMap.Parse(Convert.FromHexString(Lines("mmoh").Single()[1]));

        /// <summary>A file of 1 × 1 cell at (0, 0) with every sample from <paramref name="height"/>.</summary>
        private static byte[] File1x1(Func<int, int, float> height)
        {
            var bytes = new byte[14 + 2 * 33 * 33];
            "MMOH"u8.CopyTo(bytes);
            bytes[4] = 1;
            bytes[10] = 1;
            bytes[12] = 1;
            for (int j = 0; j < 33; j++)
                for (int i = 0; i < 33; i++)
                {
                    ushort raw = (ushort)MathF.Round((height(i, j) + 512) * 32);
                    bytes[14 + 2 * (j * 33 + i)] = (byte)raw;
                    bytes[15 + 2 * (j * 33 + i)] = (byte)(raw >> 8);
                }
            return bytes;
        }

        [Fact]
        public void ReadsTheHeaderAndSamplesSimWrites()
        {
            var map = FromVectors();
            Assert.Equal((3, 5, 2, 1), (map.CellX, map.CellZ, map.CellsX, map.CellsZ));
            Assert.Equal((65, 33), (map.Width, map.Depth));
            Assert.Equal((3 * 64 - 8000f, 5 * 64 - 8000f), (map.X0, map.Z0));
            var samples = Lines("mmoh_sample");
            Assert.True(samples.Length >= 40);
            foreach (var v in samples)
                Assert.Equal(float.Parse(v[3], CultureInfo.InvariantCulture), map.Sample(int.Parse(v[1]), int.Parse(v[2])));
        }

        [Fact]
        public void HeightIsBilinearBetweenSamplesAndClampedOutside()
        {
            var map = HeightMap.Parse(File1x1((i, j) => i * 0.5f + j * 0.25f));
            Assert.Equal(0f, map.Height(-8000, -8000));
            // A plane: bilinear reproduces it exactly anywhere inside.
            Assert.Equal(3.1f * 0.25f + 5.3f * 0.125f, map.Height(-8000 + 3.1f, -8000 + 5.3f), 3);
            Assert.Equal(map.Sample(32, 32), map.Height(0, 0));
            Assert.Equal(map.Sample(0, 32), map.Height(-9000, 100));
            Assert.Equal(16f + 8f, map.Max(0, 0, 100, 100));
        }

        [Fact]
        public void LoadsFromAFileLikeItParses()
        {
            var bytes = Convert.FromHexString(Lines("mmoh").Single()[1]);
            string path = Path.GetTempFileName();
            try
            {
                File.WriteAllBytes(path, bytes);
                var loaded = HeightMap.Load(path);
                var parsed = HeightMap.Parse(bytes);
                Assert.Equal((parsed.Width, parsed.Depth, parsed.X0, parsed.Z0), (loaded.Width, loaded.Depth, loaded.X0, loaded.Z0));
                for (int j = 0; j < parsed.Depth; j++)
                    for (int i = 0; i < parsed.Width; i++)
                        Assert.Equal(parsed.Sample(i, j), loaded.Sample(i, j));
                File.WriteAllBytes(path, bytes[..^2]);
                Assert.Throws<InvalidDataException>(() => HeightMap.Load(path));
            }
            finally
            {
                File.Delete(path);
            }
        }

        [Fact]
        public void RefusesBadFiles()
        {
            var good = File1x1((_, _) => 0);
            Assert.Throws<InvalidDataException>(() => HeightMap.Parse(good.AsSpan(0, 13)));
            var magic = (byte[])good.Clone();
            magic[0] = (byte)'X';
            Assert.Throws<InvalidDataException>(() => HeightMap.Parse(magic));
            var version = (byte[])good.Clone();
            version[4] = 2;
            Assert.Throws<InvalidDataException>(() => HeightMap.Parse(version));
            Assert.Throws<InvalidDataException>(() => HeightMap.Parse(good.AsSpan(0, good.Length - 2)));
            var offMap = (byte[])good.Clone();
            offMap[6] = 250;
            Assert.Throws<InvalidDataException>(() => HeightMap.Parse(offMap));
        }
    }
}
