using System;
using System.Collections.Generic;
using System.Linq;
using Mmo.Content;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>Where the world's cosmetic flora goes (docs/design/art.md, ADR 0041).</summary>
    public class FloraPlanTests
    {
        /// <summary>
        /// The map's middle 4 × 4 cells (x and z from 0 to 256 m): sea 5 m deep west of x = 40, a
        /// beach rising to 3 m by x = 80, then meadow at 3 m, with a steep 30 m ridge from x = 200.
        /// A road runs along z = 120 and a town covers z 200 to 256.
        /// </summary>
        private static FloraPlan Plan(params Area[] areas)
        {
            const int cells = 4, side = cells * HeightMap.QuadsPerCell + 1;
            var samples = new ushort[side * side];
            for (int j = 0; j < side; j++)
            {
                for (int i = 0; i < side; i++)
                {
                    float x = i * HeightMap.Spacing;
                    float h = x < 40 ? -5 : x < 80 ? -1 + 4 * (x - 40) / 40 : x < 200 ? 3 : 3 + (x - 200) * 0.6f;
                    samples[j * side + i] = (ushort)MathF.Round((h + 512) * 32);
                }
            }
            var map = HeightMap.FromSamples(125, 125, cells, cells, samples);
            var texels = new byte[LandRaster.Side * LandRaster.Side];
            int t0 = 125 * LandRaster.TexelsPerCell;
            for (int j = t0; j < t0 + cells * LandRaster.TexelsPerCell; j++)
                for (int i = t0; i < t0 + cells * LandRaster.TexelsPerCell; i++)
                    texels[j * LandRaster.Side + i] = (byte)((j - t0) == 30 ? LandUse.Road : (j - t0) >= 50 ? LandUse.Town : LandUse.Wild);
            var land = new LandRaster(texels);
            return new FloraPlan(map, land, GroundMasks.Build(map, land), areas);
        }

        private static readonly Lazy<FloraPlan> Open = new(() => Plan());
        private static readonly Lazy<FloraPlan> Wooded = new(() => Plan(new Area("Greymane Wood", 128, 128, 400)));

        /// <summary>Everything on the test ground (x and z from 0 to 256 m), both layers.</summary>
        private static List<FloraItem> All(FloraPlan plan)
        {
            var items = new List<FloraItem>();
            foreach (var layer in new[] { FloraPlan.Big, FloraPlan.Small })
            {
                int first = (int)MathF.Floor(HeightMap.WorldHalf / layer.CellM), last = (int)MathF.Floor((HeightMap.WorldHalf + 255) / layer.CellM);
                for (int cz = first; cz <= last; cz++)
                    for (int cx = first; cx <= last; cx++)
                        items.AddRange(plan.Cell(layer, cx, cz));
            }
            return items.Where(it => it.X >= 0 && it.X < 256 && it.Z >= 0 && it.Z < 256).ToList();
        }

        private static bool IsTree(Flora k) => k is Flora.Pine or Flora.Oak or Flora.Birch or Flora.Autumn;

        [Fact]
        public void TheSamePlaceGetsTheSameFloraEveryTime()
        {
            var a = Wooded.Value.Cell(FloraPlan.Big, 32, 32);
            var b = Plan(new Area("Greymane Wood", 128, 128, 400)).Cell(FloraPlan.Big, 32, 32);
            Assert.NotEmpty(a);
            Assert.Equal(a, b);
        }

        [Fact]
        public void NothingGrowsInTheSeaOnRoadsOrInTowns()
        {
            foreach (var it in All(Wooded.Value))
            {
                Assert.True(it.X >= 30, $"{it.Kind} at x {it.X} is in the sea");
                if (IsTree(it.Kind) || it.Kind is Flora.Bush or Flora.Boulder)
                {
                    Assert.False(it.Z >= 200 && it.Z < 256, $"{it.Kind} in the town at {it.X}, {it.Z}");
                    Assert.False(it.Z >= 120 && it.Z < 124, $"{it.Kind} on the road at {it.X}, {it.Z}");
                }
            }
        }

        [Fact]
        public void TreesStandInlandAndTheBeachGetsDriftwoodAndPebbles()
        {
            var items = All(Wooded.Value);
            // The beach is sand below about 1.8 m (x 70 here) or within 20 m of the waterline (x 50).
            Assert.DoesNotContain(items, it => IsTree(it.Kind) && it.X < 66);
            Assert.Contains(items, it => it.Kind is Flora.Driftwood or Flora.Pebbles && it.X < 70);
            Assert.DoesNotContain(items, it => it.Kind is Flora.Driftwood && it.X > 140);
            Assert.DoesNotContain(items, it => it.Kind == Flora.Reeds);
        }

        [Fact]
        public void ANamedWoodHasManyMoreTreesThanOpenGround()
        {
            int Trees(FloraPlan p) => All(p).Count(it => IsTree(it.Kind));
            Assert.True(Trees(Wooded.Value) > 3 * Math.Max(Trees(Open.Value), 5), $"{Trees(Wooded.Value)} against {Trees(Open.Value)}");
        }

        [Fact]
        public void SteepGroundGetsStones()
        {
            var ridge = All(Open.Value).Where(it => it.X > 205 && it.Z < 190).ToList();
            Assert.Contains(ridge, it => it.Kind is Flora.Boulder or Flora.Rock);
        }
    }
}
