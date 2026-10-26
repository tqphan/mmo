using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The C# ports of <c>sim</c> agree with it on every generated vector (vectors/sim.txt).</summary>
    public class SimVectorTests
    {
        private static IEnumerable<string[]> Lines(string kind) =>
            File.ReadLines(Path.Combine(AppContext.BaseDirectory, "vectors", "sim.txt"))
                .Where(l => l.StartsWith(kind + " "))
                .Select(l => l.Split(' '));

        private static float F(string s) => float.Parse(s, CultureInfo.InvariantCulture);

        [Fact]
        public void CellOf()
        {
            int n = 0;
            foreach (var v in Lines("cell_of"))
            {
                Assert.Equal(ulong.Parse(v[4]), Cell.CellOf(uint.Parse(v[1]), F(v[2]), F(v[3])));
                n++;
            }
            Assert.True(n > 200, $"only {n} vectors");
        }

        [Fact]
        public void Recenter()
        {
            int n = 0;
            foreach (var v in Lines("recenter"))
            {
                var got = Cell.Recenter(ulong.Parse(v[1]), F(v[2]), F(v[3]), F(v[4]));
                Assert.True(ulong.Parse(v[5]) == got, $"recenter {string.Join(' ', v[1..5])}: want {v[5]}, got {got}");
                n++;
            }
            Assert.True(n > 200, $"only {n} vectors");
        }

        [Fact]
        public void WindowCells()
        {
            int n = 0;
            foreach (var v in Lines("window"))
            {
                var want = v[3].Split(',').Select(ulong.Parse).ToList();
                Assert.Equal(want, Cell.WindowCells(ulong.Parse(v[1]), ushort.Parse(v[2])));
                n++;
            }
            Assert.True(n > 15, $"only {n} vectors");
        }

        [Fact]
        public void YawToWire()
        {
            foreach (var v in Lines("yaw_to_u16"))
                Assert.True(ushort.Parse(v[2]) == Yaw.ToU16(F(v[1])), $"yaw {v[1]}: want {v[2]}, got {Yaw.ToU16(F(v[1]))}");
            foreach (var v in Lines("yaw_from_u16"))
                Assert.Equal(F(v[2]), Yaw.FromU16(ushort.Parse(v[1])));
        }

        [Fact]
        public void YawFacingRoundTrips()
        {
            for (float yaw = -3.1f; yaw < 3.1f; yaw += 0.1f)
            {
                var (x, z) = Yaw.Facing(yaw);
                Assert.Equal(yaw, Yaw.Of(x, z), 4);
            }
            // 0 faces north (-z); a quarter turn left faces west (-x).
            Assert.Equal((0f, -1f), Yaw.Facing(0));
            Assert.Equal(-1f, Yaw.Facing(MathF.PI / 2).X, 5);
            // Turns take the short way round.
            Assert.Equal(0.1f + MathF.Tau - 6.2f, Yaw.Delta(6.2f, 0.1f), 4);
            Assert.Equal(-(0.1f + MathF.Tau - 6.2f), Yaw.Delta(0.1f, 6.2f), 4);
            Assert.Equal(MathF.PI / 2, Yaw.Delta(0, MathF.PI / 2), 5);
        }
    }
}
