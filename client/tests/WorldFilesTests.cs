using System;
using System.Buffers.Binary;
using System.Globalization;
using System.IO;
using System.Linq;
using Mmo.Content;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>
    /// The client reads the world's files as <c>tools/worldtool</c> writes them, and works out the same
    /// terrain hash as <c>sim</c> and the server (docs/design/world.md).
    /// </summary>
    public class WorldFilesTests
    {
        private static string RepoWorld()
        {
            for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir != null; dir = dir.Parent)
                if (File.Exists(Path.Combine(dir.FullName, "world", WorldFiles.Manifest)))
                    return Path.Combine(dir.FullName, "world");
            throw new DirectoryNotFoundException("no world/ above the test's directory");
        }

        private static string[][] Lines(string kind) =>
            File.ReadLines(Path.Combine(AppContext.BaseDirectory, "vectors", "sim.txt"))
                .Where(l => l.StartsWith(kind + " "))
                .Select(l => l.Split(' '))
                .ToArray();

        /// <summary>A copy of the world to damage.</summary>
        private static string CopyOfWorld()
        {
            string from = RepoWorld(), to = Path.Combine(Path.GetTempPath(), $"mmo-world-{Guid.NewGuid():N}");
            Directory.CreateDirectory(Path.Combine(to, "regions"));
            File.Copy(Path.Combine(from, WorldFiles.Manifest), Path.Combine(to, WorldFiles.Manifest));
            foreach (var f in Directory.GetFiles(Path.Combine(from, "regions")))
                File.Copy(f, Path.Combine(to, "regions", Path.GetFileName(f)));
            return to;
        }

        [Fact]
        public void TheCommittedWorldLoadsWithTheManifestsHash()
        {
            string dir = RepoWorld();
            var world = WorldFiles.Load(dir);
            var manifest = WorldFiles.ParseManifest(File.ReadAllText(Path.Combine(dir, WorldFiles.Manifest)));
            Assert.Equal(manifest.TerrainHash, world.TerrainHash);
            Assert.Equal((8001, 8001), (world.Heights.Width, world.Heights.Depth));
            Assert.True(world.Heights.Height(0, 0) > 5, "spawn is on the trade island");
            Assert.Equal(LandUse.Town, LandRaster.Use(world.Land!.At(0, 0)));
            Assert.Equal(Climate.Subarctic, LandRaster.ClimateOf(world.Land.At(0, -6000)));
            Assert.Equal(LandUse.Wild, LandRaster.Use(world.Land.At(0, 7900)));
        }

        [Fact]
        public void ChunkAndTerrainHashesMatchSim()
        {
            var bytes = Convert.FromHexString(Lines("mmoh").Single()[1]);
            var map = HeightMap.Parse(bytes);
            var chunks = Lines("chunk_hash");
            Assert.Equal(2, chunks.Length);
            for (int c = 0; c < chunks.Length; c++)
            {
                // The file's cell c along x: rows of 33 samples from sample (32c, j).
                var samples = new ushort[33 * 33];
                for (int j = 0; j < 33; j++)
                    for (int i = 0; i < 33; i++)
                        samples[j * 33 + i] = BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(14 + 2 * (j * map.Width + 32 * c + i)));
                ulong cell = ulong.Parse(chunks[c][1], CultureInfo.InvariantCulture);
                Assert.Equal(Sim.Cell.Pack(0, (ushort)map.CellZ, (ushort)(map.CellX + c)), cell);
                Assert.Equal(ulong.Parse(chunks[c][2], CultureInfo.InvariantCulture), HeightMap.ChunkHash(cell, samples));
            }
            Assert.Equal(ulong.Parse(Lines("terrain_hash").Single()[1], CultureInfo.InvariantCulture), map.TerrainHash());
        }

        [Fact]
        public void ABareHeightFileLoadsWithoutLandUse()
        {
            string path = Path.GetTempFileName();
            try
            {
                File.WriteAllBytes(path, Convert.FromHexString(Lines("mmoh").Single()[1]));
                var world = WorldFiles.Load(path);
                Assert.Null(world.Land);
                Assert.Equal(ulong.Parse(Lines("terrain_hash").Single()[1], CultureInfo.InvariantCulture), world.TerrainHash);
            }
            finally
            {
                File.Delete(path);
            }
        }

        [Fact]
        public void DamagedWorldsAreRefused()
        {
            string dir = CopyOfWorld();
            try
            {
                string manifest = Path.Combine(dir, WorldFiles.Manifest), region = Path.Combine(dir, "regions", "x12z12.mmor");
                string text = File.ReadAllText(manifest);
                var good = File.ReadAllBytes(region);

                File.WriteAllBytes(region, good[..^5]);
                var e = Assert.Throws<InvalidDataException>(() => WorldFiles.Load(dir));
                Assert.Contains("x12z12", e.Message);

                // The cells of another region.
                var moved = (byte[])good.Clone();
                moved[6]++;
                File.WriteAllBytes(region, moved);
                Assert.Throws<InvalidDataException>(() => WorldFiles.Load(dir));
                File.WriteAllBytes(region, good);

                var hash = WorldFiles.ParseManifest(text).TerrainHash;
                File.WriteAllText(manifest, text.Replace(hash.ToString("x16"), (hash ^ 1).ToString("x16")));
                Assert.Contains("terrain hash", Assert.Throws<InvalidDataException>(() => WorldFiles.Load(dir)).Message);

                File.WriteAllText(manifest, text.Replace("format = 1", "format = 2"));
                Assert.Throws<InvalidDataException>(() => WorldFiles.Load(dir));
                File.WriteAllText(manifest, "format = 1\n");
                Assert.Throws<InvalidDataException>(() => WorldFiles.Load(dir));
                File.Delete(manifest);
                Assert.ThrowsAny<IOException>(() => WorldFiles.Load(dir));
            }
            finally
            {
                Directory.Delete(dir, recursive: true);
            }
        }

        [Fact]
        public void TexelsPackUseAndClimate()
        {
            var texels = new byte[LandRaster.Side * LandRaster.Side];
            texels[0] = (byte)LandUse.Road | (byte)Climate.Arid << 4;
            texels[^1] = (byte)LandUse.Housing | (byte)Climate.Tropical << 4;
            var land = new LandRaster(texels);
            Assert.Equal((LandUse.Road, Climate.Arid), (LandRaster.Use(land.At(-8000, -8000)), LandRaster.ClimateOf(land.At(-7996.1f, -7996.1f))));
            // Off the map clamps to the edge.
            Assert.Equal(texels[^1], land.At(9000, 9000));
            Assert.Equal(LandUse.Wild, LandRaster.Use(land.At(-7996, -8000)));
            Assert.Throws<ArgumentException>(() => new LandRaster(new byte[10]));
        }
    }
}
