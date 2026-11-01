using System;
using System.Collections.Generic;
using Mmo.Content;

namespace Mmo.World
{
    /// <summary>The kinds of cosmetic flora and stone (docs/design/art.md); each has a few mesh variants.</summary>
    public enum Flora : byte
    {
        Pine,
        Oak,
        Birch,
        Autumn,
        DeadTree,
        Bush,
        Boulder,
        Rock,
        Driftwood,
        Grass,
        Flowers,
        Reeds,
        Pebbles,
    }

    /// <summary>One placed piece: kind, mesh variant, ground position, turn, size and a tint from 0 to 1.</summary>
    public readonly record struct FloraItem(Flora Kind, int Variant, float X, float Y, float Z, float Yaw, float Scale, float Tint);

    /// <summary>
    /// Where the world's trees, bushes, rocks and ground cover stand (docs/design/art.md, ADR 0041).
    /// Cosmetic and client-side only: nothing collides with them and the server never hears of them.
    /// Placement is a pure function of the world's files, so every client sees the same forest.
    /// Square cells of <see cref="Layer.CellM"/> are filled from a jittered grid; each point gets at
    /// most one piece, chosen by the ground under it with the same rules as the terrain shader
    /// (beaches by the open sea, marsh on the flats, rock on steep ground) and by the named area.
    /// </summary>
    public sealed class FloraPlan
    {
        /// <summary>A layer of flora: cell size, spacing of the candidate grid, and how far from the camera it's drawn.</summary>
        public sealed record Layer(string Name, float CellM, float SpacingM, float RadiusM, bool Ground);

        /// <summary>Trees, bushes and stones, drawn far.</summary>
        public static readonly Layer Big = new("big", 256.0f, 5.0f, 900.0f, false);
        /// <summary>Grass tufts, flowers, reeds and pebbles, drawn near.</summary>
        public static readonly Layer Small = new("small", 32.0f, 0.9f, 80.0f, true);

        public const int Variants = 3;

        private readonly HeightMap map;
        private readonly LandRaster? land;
        private readonly byte[]? masks;
        private readonly IReadOnlyList<Area> areas;

        public FloraPlan(HeightMap map, LandRaster? land, byte[]? masks, IReadOnlyList<Area> areas)
        {
            this.map = map;
            this.land = land;
            this.masks = masks;
            this.areas = areas;
        }

        /// <summary>Everything in cell (<paramref name="cx"/>, <paramref name="cz"/>) of a layer, counted from the map's (−x, −z) corner.</summary>
        public List<FloraItem> Cell(Layer layer, int cx, int cz)
        {
            var items = new List<FloraItem>();
            int n = (int)MathF.Round(layer.CellM / layer.SpacingM);
            float spacing = layer.CellM / n;
            float x0 = cx * layer.CellM - HeightMap.WorldHalf, z0 = cz * layer.CellM - HeightMap.WorldHalf;
            for (int b = 0; b < n; b++)
            {
                for (int a = 0; a < n; a++)
                {
                    uint seed = Hash((uint)(cx * n + a), (uint)(cz * n + b), layer.Ground ? 7u : 3u);
                    float x = x0 + (a + Rand(ref seed)) * spacing;
                    float z = z0 + (b + Rand(ref seed)) * spacing;
                    var ground = Ground(x, z);
                    var kind = layer.Ground ? Cover(ground, ref seed) : Standing(ground, x, z, ref seed);
                    if (kind is not { } k)
                        continue;
                    float scale = k switch
                    {
                        Flora.Pine or Flora.Oak or Flora.Birch or Flora.Autumn => 0.75f + 0.6f * Rand(ref seed),
                        Flora.Boulder => 0.7f + 1.1f * Rand(ref seed),
                        _ => 0.7f + 0.6f * Rand(ref seed),
                    };
                    int variant = (int)(Rand(ref seed) * Variants) % Variants;
                    float yaw = Rand(ref seed) * MathF.Tau;
                    // Sink a little, so trunks and stones don't float where the drawn triangles dip.
                    float sink = k is Flora.Boulder or Flora.Rock ? 0.35f * scale : 0.12f;
                    items.Add(new FloraItem(k, variant, x, ground.Height - sink, z, yaw, scale, Rand(ref seed)));
                }
            }
            return items;
        }

        /// <summary>What the ground is like at a point, read the way the terrain shader reads it.</summary>
        public readonly record struct GroundAt(float X, float Z, float Height, float Slope, LandUse Use, float Sea, float Pools, string Area);

        public GroundAt Ground(float x, float z)
        {
            float h = map.Height(x, z);
            const float d = 2.0f;
            float gx = (map.Height(x + d, z) - map.Height(x - d, z)) / (2 * d);
            float gz = (map.Height(x, z + d) - map.Height(x, z - d)) / (2 * d);
            // 1 − the normal's y, as in the shader.
            float slope = 1.0f - 1.0f / MathF.Sqrt(1 + gx * gx + gz * gz);
            var use = land is null ? LandUse.Wild : LandRaster.Use(land.At(x, z));
            float sea = 0, pools = 0;
            if (masks != null)
            {
                int i = Math.Clamp((int)MathF.Floor((x + HeightMap.WorldHalf) / LandRaster.TexelSize), 0, GroundMasks.Side - 1);
                int j = Math.Clamp((int)MathF.Floor((z + HeightMap.WorldHalf) / LandRaster.TexelSize), 0, GroundMasks.Side - 1);
                int t = (j * GroundMasks.Side + i) * 4;
                pools = masks[t + 2] / 255.0f;
                sea = masks[t + 3] / 255.0f;
            }
            return new GroundAt(x, z, h, slope, use, sea, pools, Smallest(x, z));
        }

        private string Smallest(float x, float z)
        {
            string name = "";
            float best = float.MaxValue;
            foreach (var a in areas)
            {
                float dx = a.X - x, dz = a.Z - z;
                if (dx * dx + dz * dz <= a.Radius * a.Radius && a.Radius < best)
                {
                    best = a.Radius;
                    name = a.Name;
                }
            }
            return name;
        }

        private static bool Sandy(GroundAt g) => g.Sea > 0.29f || (g.Sea > 0.04f && g.Height < 1.8f);
        /// <summary>Marsh as the terrain shader draws it: none within reach of the open sea's mask.</summary>
        private static bool Marshy(GroundAt g) => g.Sea < 0.02f && (g.Pools > 0.05f && g.Height < 1.8f || g.Height < 1.1f);

        /// <summary>Trees, bushes and stones: forests where the wide noise is high, more in the named woods; rocks on steep ground.</summary>
        private Flora? Standing(GroundAt g, float x, float z, ref uint seed)
        {
            if (g.Height < 0.3f || g.Use is LandUse.Road or LandUse.Town)
                return null;
            float r = Rand(ref seed);
            // Housing land stays open for building: only the odd bush.
            if (g.Use == LandUse.Housing)
                return r < 0.01f ? Flora.Bush : null;
            if (Sandy(g))
                return r < 0.012f ? Flora.Driftwood : r < 0.02f ? Flora.Rock : null;
            if (Marshy(g))
                return r < 0.012f ? Flora.DeadTree : r < 0.05f ? Flora.Bush : null;
            if (g.Slope > 0.12f)
            {
                // Steep: stones, and pines that cling on.
                return r < 0.10f ? Flora.Boulder : r < 0.22f ? Flora.Rock : r < 0.28f ? Flora.Pine : null;
            }

            float wild = Fbm(x * 0.004f, z * 0.004f);
            float forest = g.Area switch
            {
                "Greymane Wood" => 0.32f,
                "Wrackline Point" => 0.22f,
                "Cairn Crags" or "Ashwing Scarp" => 0.2f,
                "Bristle Downs" or "Mossback Meadow" => 0.06f,
                "Croaker Fen" or "Wreckers' Cove" => 0.04f,
                _ => Math.Clamp((wild - 0.5f) * 1.2f, 0.0f, 0.25f),
            };
            // Clearings and thickets within any forest.
            forest *= Math.Clamp((Fbm(x * 0.02f + 40, z * 0.02f) - 0.3f) * 2.5f, 0.0f, 1.4f);
            // High ground and the point are pine country; low ground broadleaf.
            float pineShare = Math.Clamp((g.Height - 20.0f) / 60.0f, 0.15f, 0.9f);
            if (g.Area == "Wrackline Point")
                pineShare = 0.9f;
            if (g.Area == "Greymane Wood")
                pineShare = 0.2f;
            if (r < forest)
            {
                float s = Rand(ref seed);
                if (s < pineShare)
                    return Flora.Pine;
                s = (s - pineShare) / (1 - pineShare);
                return s < 0.55f ? Flora.Oak : s < 0.85f ? Flora.Birch : s < 0.97f ? Flora.Autumn : Flora.DeadTree;
            }
            // Open ground: lone trees, bushes and boulders.
            r = Rand(ref seed);
            if (r < 0.006f)
                return Rand(ref seed) < 0.5f ? Flora.Oak : Flora.Birch;
            if (r < 0.03f + forest * 0.08f)
                return Flora.Bush;
            if (r < 0.038f + forest * 0.08f)
                return Flora.Boulder;
            if (r < 0.045f + forest * 0.08f)
                return Flora.Rock;
            return null;
        }

        /// <summary>Ground cover: grass tufts in clumps, flowers in patches, reeds in the marsh and pebbles on the beach.</summary>
        private static Flora? Cover(GroundAt g, ref uint seed)
        {
            if (g.Height < 0.05f || g.Use is LandUse.Road)
                return null;
            float r = Rand(ref seed);
            if (Sandy(g))
                return g.Height > 1.0f && r < 0.04f ? Flora.Grass : r < 0.02f ? Flora.Pebbles : null;
            if (Marshy(g))
                return r < 0.45f ? Flora.Reeds : r < 0.6f ? Flora.Grass : null;
            if (g.Slope > 0.14f)
                return r < 0.05f ? Flora.Pebbles : r < 0.15f ? Flora.Grass : null;
            float town = g.Use == LandUse.Town ? 0.3f : 1.0f;
            float meadow = g.Area is "Mossback Meadow" or "Bristle Downs" ? 2.0f : 1.0f;
            // Tufts in clumps, flowers in patches.
            float clump = Noise(g.X * 0.15f, g.Z * 0.15f);
            float patch = Noise(g.X * 0.05f + 70, g.Z * 0.05f);
            if (r < 0.06f * meadow * town * Math.Clamp((patch - 0.55f) * 4, 0, 1))
                return Flora.Flowers;
            return r < (0.15f + 0.7f * clump * clump) * town ? Flora.Grass : null;
        }

        // Integer hashing and value noise, as in shaders/noise.gdshaderinc, so placement is the same on
        // every machine.
        private static uint Hash(uint x, uint z, uint salt)
        {
            uint h = x * 1597334677u ^ z * 3812015801u ^ salt * 2654435761u;
            h = (h ^ (h >> 16)) * 2246822519u;
            h ^= h >> 13;
            h *= 3266489917u;
            return h ^ (h >> 16);
        }

        /// <summary>A float in [0, 1), stepping the seed (xorshift).</summary>
        private static float Rand(ref uint seed)
        {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            return (seed & 0xffffff) / 16777216.0f;
        }

        private static float Lattice(int x, int z) => (Hash((uint)x, (uint)z, 11u) & 0xffffff) / 16777215.0f;

        private static float Noise(float x, float z)
        {
            int ix = (int)MathF.Floor(x), iz = (int)MathF.Floor(z);
            float fx = x - ix, fz = z - iz;
            float ux = fx * fx * (3 - 2 * fx), uz = fz * fz * (3 - 2 * fz);
            float a = Lattice(ix, iz), b = Lattice(ix + 1, iz), c = Lattice(ix, iz + 1), d = Lattice(ix + 1, iz + 1);
            return (a + (b - a) * ux) + ((c + (d - c) * ux) - (a + (b - a) * ux)) * uz;
        }

        private static float Fbm(float x, float z) => 0.55f * Noise(x, z) + 0.3f * Noise(x * 2.03f + 17, z * 2.03f) + 0.15f * Noise(x * 4.1f, z * 4.1f + 31);
    }
}
