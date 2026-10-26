using System;
using System.Collections.Generic;

namespace Mmo.World
{
    /// <summary>
    /// Which terrain tiles to draw, and at what level of detail (docs/design/worldstream.md, ADR 0017).
    /// The map is cut into tiles of <see cref="TileQuads"/> quads (256 m); level k puts a vertex
    /// every 2·2^k m. The level grows by one each time the distance from the camera to the tile
    /// doubles past <see cref="Level0M"/>, and tiles past <see cref="ViewM"/> or wholly in deep
    /// water aren't drawn. A tile changes level only once the distance is 10% past a threshold.
    /// </summary>
    public sealed class TerrainPlan
    {
        public const int TileQuads = 128;
        public const int Levels = 6;
        public const float Level0M = 200.0f;
        public const float ViewM = 10000.0f;
        /// <summary>A tile whose highest sample is below this is under deep water, and isn't drawn.</summary>
        public const float DeepM = -25.0f;
        private const float Hysteresis = 1.1f;
        /// <summary>The level of a tile that isn't drawn.</summary>
        public const int None = Levels;

        public readonly HeightMap Map;
        public readonly int TilesX, TilesZ;
        private readonly bool[] deep;
        private readonly int[] level;

        public TerrainPlan(HeightMap map)
        {
            Map = map;
            TilesX = (map.Width - 1 + TileQuads - 1) / TileQuads;
            TilesZ = (map.Depth - 1 + TileQuads - 1) / TileQuads;
            deep = new bool[TilesX * TilesZ];
            level = new int[TilesX * TilesZ];
            Array.Fill(level, None);
            for (int t = 0; t < deep.Length; t++)
            {
                var (i0, j0) = Origin(t);
                deep[t] = map.Max(i0, j0, i0 + TileQuads, j0 + TileQuads) < DeepM;
            }
        }

        public int Count => level.Length;

        public int DeepCount
        {
            get
            {
                int n = 0;
                foreach (bool d in deep)
                    n += d ? 1 : 0;
                return n;
            }
        }

        /// <summary>The tile's first sample along x and z.</summary>
        public (int I, int J) Origin(int tile) => (tile % TilesX * TileQuads, tile / TilesX * TileQuads);

        /// <summary>The level the plan currently wants for a tile, or <see cref="None"/>.</summary>
        public int Level(int tile) => level[tile];

        /// <summary>Level by distance alone: 0 under 200 m, one more per doubling, None past the view distance.</summary>
        public static int LevelAt(float distance)
        {
            if (distance >= ViewM)
                return None;
            int k = 0;
            for (float edge = Level0M; distance >= edge && k < Levels - 1; edge *= 2)
                k++;
            return k;
        }

        /// <summary>Horizontal distance from (x, z) to the nearest point of the tile.</summary>
        public float Distance(int tile, float x, float z)
        {
            var (i0, j0) = Origin(tile);
            float x0 = Map.X0 + i0 * HeightMap.Spacing, z0 = Map.Z0 + j0 * HeightMap.Spacing;
            float size = TileQuads * HeightMap.Spacing;
            float dx = Math.Max(0, Math.Max(x0 - x, x - (x0 + size)));
            float dz = Math.Max(0, Math.Max(z0 - z, z - (z0 + size)));
            return MathF.Sqrt(dx * dx + dz * dz);
        }

        /// <summary>
        /// Move the camera to (x, z): returns the tiles whose level changed and their new level
        /// (<see cref="None"/> to drop), nearest first.
        /// </summary>
        public List<(int Tile, int Level)> Update(float x, float z)
        {
            var changes = new List<(int Tile, int Level, float Distance)>();
            for (int t = 0; t < level.Length; t++)
            {
                if (deep[t])
                    continue;
                float d = Distance(t, x, z);
                int current = level[t];
                int coarser = LevelAt(d / Hysteresis), finer = LevelAt(d * Hysteresis);
                int next = coarser > current ? coarser : finer < current ? finer : current;
                if (next != current)
                {
                    level[t] = next;
                    changes.Add((t, next, d));
                }
            }
            changes.Sort((a, b) => a.Distance.CompareTo(b.Distance));
            return changes.ConvertAll(c => (c.Tile, c.Level));
        }
    }
}
