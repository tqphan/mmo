using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Runtime.ExceptionServices;
using System.Threading.Tasks;

namespace Mmo.World
{
    /// <summary>The world as the client holds it: heights, land use, and the terrain hash it sends on entering.</summary>
    public sealed record WorldData(HeightMap Heights, LandRaster? Land, ulong TerrainHash);

    /// <summary>
    /// Reads the world's files (<c>world/</c>, docs/design/world.md) as <c>tools/worldtool</c> writes
    /// them: a manifest and 25 × 25 region files of 10 × 10 cells, each with zlib-compressed heights
    /// (differences from a planar prediction, low bytes then high bytes) and a land-use raster.
    /// Regions are read in parallel straight into one sample array for the whole map.
    /// </summary>
    public static class WorldFiles
    {
        public const string Manifest = "world.toml";
        public const int Format = 1;
        public const int RegionCells = 10;
        public const int RegionsPerSide = HeightMap.CellsPerSide / RegionCells;
        private const ushort RegionVersion = 1;

        /// <summary>
        /// A world directory, or a bare <c>.mmoh</c> height file (no land use). Throws
        /// <see cref="InvalidDataException"/> or <see cref="IOException"/> if it can't be read, and
        /// if the heights don't match the manifest's terrain hash.
        /// </summary>
        public static WorldData Load(string path)
        {
            if (File.Exists(path))
            {
                var map = HeightMap.Load(path);
                return new WorldData(map, null, map.TerrainHash());
            }
            var (format, regionCells, hash) = ParseManifest(File.ReadAllText(Path.Combine(path, Manifest)));
            if (format != Format || regionCells != RegionCells)
                throw new InvalidDataException($"world format {format} with {regionCells}-cell regions; this client reads format {Format} with {RegionCells}");
            int width = HeightMap.CellsPerSide * HeightMap.QuadsPerCell + 1;
            var samples = new ushort[width * width];
            var land = new byte[LandRaster.Side * LandRaster.Side];
            // Each region writes its samples except a leading edge it shares with its west or north
            // neighbour, so every sample is written once; those edges are checked afterwards.
            var leading = new (ushort[] Row, ushort[] Column)[RegionsPerSide * RegionsPerSide];
            try
            {
                Parallel.For(0, RegionsPerSide * RegionsPerSide, r =>
                {
                    int rx = r % RegionsPerSide, rz = r / RegionsPerSide;
                    string file = Path.Combine(path, "regions", $"x{rx:00}z{rz:00}.mmor");
                    try
                    {
                        leading[r] = ReadRegion(File.ReadAllBytes(file), rx, rz, samples, width, land);
                    }
                    catch (InvalidDataException e)
                    {
                        throw new InvalidDataException($"{file}: {e.Message}", e);
                    }
                });
            }
            catch (AggregateException e)
            {
                // Report the first region that failed as itself, not wrapped.
                ExceptionDispatchInfo.Capture(e.InnerExceptions[0]).Throw();
            }
            const int side = RegionCells * HeightMap.QuadsPerCell + 1;
            for (int r = 0; r < leading.Length; r++)
            {
                int i0 = r % RegionsPerSide * (side - 1), j0 = r / RegionsPerSide * (side - 1);
                for (int k = 0; k < side; k++)
                {
                    if ((j0 > 0 && samples[j0 * width + i0 + k] != leading[r].Row[k]) || (i0 > 0 && samples[(j0 + k) * width + i0] != leading[r].Column[k]))
                        throw new InvalidDataException($"region x{r % RegionsPerSide:00}z{r / RegionsPerSide:00}: an edge differs from its neighbour's");
                }
            }
            var heights = HeightMap.FromSamples(0, 0, HeightMap.CellsPerSide, HeightMap.CellsPerSide, samples);
            ulong actual = heights.TerrainHash();
            if (actual != hash)
                throw new InvalidDataException($"the regions' terrain hash is {actual:x16}, but the manifest says {hash:x16}");
            return new WorldData(heights, new LandRaster(land), actual);
        }

        /// <summary><c>key = value</c> lines and <c># comments</c>, as <c>worldtool::Manifest</c> writes them.</summary>
        public static (int Format, int RegionCells, ulong TerrainHash) ParseManifest(string text)
        {
            var values = new Dictionary<string, string>();
            foreach (var raw in text.Split('\n'))
            {
                var line = raw.Trim();
                if (line.Length == 0 || line[0] == '#')
                    continue;
                int eq = line.IndexOf('=');
                if (eq < 0)
                    throw new InvalidDataException($"manifest: expected `key = value`, got `{line}`");
                values[line[..eq].Trim()] = line[(eq + 1)..].Trim();
            }
            try
            {
                return (int.Parse(values["format"], CultureInfo.InvariantCulture),
                        int.Parse(values["region_cells"], CultureInfo.InvariantCulture),
                        ulong.Parse(values["terrain_hash"].Trim('"'), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture));
            }
            catch (Exception e) when (e is KeyNotFoundException or FormatException or OverflowException)
            {
                throw new InvalidDataException("manifest needs format, region_cells and a hex terrain_hash", e);
            }
        }

        /// <summary>
        /// Decode region (<paramref name="rx"/>, <paramref name="rz"/>) into the whole map's arrays,
        /// returning its leading row and column for the edge check.
        /// </summary>
        private static (ushort[] Row, ushort[] Column) ReadRegion(byte[] bytes, int rx, int rz, ushort[] samples, int width, byte[] land)
        {
            if (bytes.Length < 12 || bytes[0] != 'M' || bytes[1] != 'M' || bytes[2] != 'O' || bytes[3] != 'R')
                throw new InvalidDataException("not a .mmor region file");
            ushort Word(int at) => BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(at));
            if (Word(4) != RegionVersion)
                throw new InvalidDataException($"region version {Word(4)}, expected {RegionVersion}");
            if (Word(6) != rx * RegionCells || Word(8) != rz * RegionCells || Word(10) != RegionCells)
                throw new InvalidDataException("the region covers the wrong cells");
            const int side = RegionCells * HeightMap.QuadsPerCell + 1, texels = RegionCells * LandRaster.TexelsPerCell;
            int at = 12;
            var planes = Inflate(bytes, ref at, 2 * side * side, "heights");
            var raster = Inflate(bytes, ref at, texels * texels, "land use");
            if (at != bytes.Length)
                throw new InvalidDataException($"{bytes.Length - at} bytes after the land use");

            // Undo the prediction: left + above − above-left, modulo 2^16 (worldtool::region).
            var s = new ushort[side * side];
            int n = side * side;
            for (int j = 0; j < side; j++)
            {
                for (int i = 0; i < side; i++)
                {
                    int k = j * side + i;
                    ushort z = (ushort)(planes[k] | planes[n + k] << 8);
                    short d = (short)((z >> 1) ^ -(z & 1));
                    ushort p = (i, j) switch
                    {
                        (0, 0) => 0,
                        (_, 0) => s[k - 1],
                        (0, _) => s[k - side],
                        _ => (ushort)(s[k - 1] + s[k - side] - s[k - side - 1]),
                    };
                    s[k] = (ushort)(p + d);
                }
            }
            int i0 = rx * (side - 1), j0 = rz * (side - 1);
            for (int j = rz > 0 ? 1 : 0; j < side; j++)
                s.AsSpan(j * side + (rx > 0 ? 1 : 0), side - (rx > 0 ? 1 : 0)).CopyTo(samples.AsSpan((j0 + j) * width + i0 + (rx > 0 ? 1 : 0)));
            int t0 = rx * texels, u0 = rz * texels;
            for (int j = 0; j < texels; j++)
            {
                var row = raster.AsSpan(j * texels, texels);
                foreach (byte t in row)
                    if ((t & 0x80) != 0)
                        throw new InvalidDataException("land use has the reserved bit set");
                row.CopyTo(land.AsSpan((u0 + j) * LandRaster.Side + t0));
            }
            var column = new ushort[side];
            for (int j = 0; j < side; j++)
                column[j] = s[j * side];
            return (s[..side], column);
        }

        /// <summary>A length-prefixed zlib stream at <paramref name="at"/>, which must inflate to exactly <paramref name="want"/> bytes.</summary>
        private static byte[] Inflate(byte[] bytes, ref int at, int want, string what)
        {
            if (at + 4 > bytes.Length)
                throw new InvalidDataException($"{what}: truncated");
            int len = (int)BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(at));
            if (len < 0 || at + 4 + len > bytes.Length)
                throw new InvalidDataException($"{what}: truncated");
            var output = new byte[want];
            using (var z = new ZLibStream(new MemoryStream(bytes, at + 4, len), CompressionMode.Decompress))
            {
                int got = 0, read;
                while (got < want && (read = z.Read(output, got, want - got)) > 0)
                    got += read;
                if (got != want || z.ReadByte() != -1)
                    throw new InvalidDataException($"{what}: the wrong size, expected {want} bytes");
            }
            at += 4 + len;
            return output;
        }
    }
}
