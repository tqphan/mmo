using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using Mmo.Content;
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

        private static void Near(float want, float got, float tol, string what) =>
            Assert.True(MathF.Abs(want - got) <= tol, $"{what}: want {want}, got {got}");

        // Ships are compared within a tolerance, not bit for bit: sin and cos may round differently
        // in another C runtime, and a step's error stays far below anything the server would notice.

        [Fact]
        public void ShipFrames()
        {
            int n = 0;
            foreach (var v in Lines("ship_pose"))
            {
                var pose = new ShipPose(F(v[1]), F(v[2]), F(v[3]), F(v[4]));
                var local = new System.Numerics.Vector3(F(v[5]), F(v[6]), F(v[7]));
                var w = pose.ToWorld(local);
                Near(F(v[8]), w.X, 1e-3f, "x");
                Near(F(v[9]), w.Y, 1e-5f, "y");
                Near(F(v[10]), w.Z, 1e-3f, "z");
                var back = pose.ToLocal(w);
                Assert.True(System.Numerics.Vector3.Distance(local, back) < 2e-3f, $"round trip {local} → {back}");
                var dir = new System.Numerics.Vector3(0.3f, 0, -0.8f);
                Assert.True(System.Numerics.Vector3.Distance(pose.Turn(dir, true), w - pose.ToWorld(local - dir)) < 2e-3f, "turning a direction");
                Assert.True(System.Numerics.Vector3.Distance(pose.Turn(pose.Turn(dir, true), false), dir) < 1e-5f, "turning back");
                n++;
            }
            Assert.True(n >= 30, $"only {n} vectors");
        }

        [Fact]
        public void ShipStep()
        {
            int n = 0;
            foreach (var v in Lines("ship_step"))
            {
                var hull = GameContent.Hulls[int.Parse(v[1])];
                var m = new ShipMotion(F(v[2]), F(v[3]), F(v[4]), F(v[5]));
                var got = Ship.Step(m, hull, new ShipInput(sbyte.Parse(v[6]), sbyte.Parse(v[7])), F(v[8]));
                string what = $"step {string.Join(' ', v[1..9])}";
                Near(F(v[9]), got.X, 1e-3f, what + " x");
                Near(F(v[10]), got.Z, 1e-3f, what + " z");
                // A yaw just under 2π and one just over 0 are the same heading.
                Near(0, Yaw.Delta(F(v[11]), got.Yaw), 1e-5f, what + " yaw");
                Near(F(v[12]), got.Speed, 1e-5f, what + " speed");
                n++;
            }
            Assert.True(n >= 200, $"only {n} vectors");
        }

        [Fact]
        public void ShipSailsTheServersCourse()
        {
            int n = 0;
            foreach (var v in Lines("ship_sail"))
            {
                var hull = GameContent.Hulls[int.Parse(v[1])];
                var input = new ShipInput(sbyte.Parse(v[2]), sbyte.Parse(v[3]));
                var m = new ShipMotion(0, 0, 0, 0);
                for (int i = 0; i < int.Parse(v[4]); i++)
                    m = Ship.Step(m, hull, input, 0.1f);
                Near(F(v[5]), m.X, 0.01f, $"sail {v[2]} {v[3]} x");
                Near(F(v[6]), m.Z, 0.01f, $"sail {v[2]} {v[3]} z");
                Near(0, Yaw.Delta(F(v[7]), m.Yaw), 1e-4f, $"sail {v[2]} {v[3]} yaw");
                Near(F(v[8]), m.Speed, 1e-4f, $"sail {v[2]} {v[3]} speed");
                n++;
            }
            Assert.True(n >= 6, $"only {n} vectors");
        }

        [Fact]
        public void CannonFlight()
        {
            int n = 0, none = 0;
            foreach (var v in Lines("cannon_flight"))
            {
                var from = new System.Numerics.Vector3(F(v[1]), F(v[2]), F(v[3]));
                var got = Ship.CannonFlight(from, F(v[4]), F(v[5]), F(v[6]));
                if (v[7] == "none")
                {
                    Assert.Null(got);
                    none++;
                    continue;
                }
                Assert.NotNull(got);
                Near(F(v[7]), got!.Value.Secs, 1e-5f, "secs");
                Near(F(v[8]), got.Value.Landing.X, 1e-3f, "landing x");
                Near(F(v[9]), got.Value.Landing.Z, 1e-3f, "landing z");
                n++;
            }
            Assert.True(n >= 30 && none >= 1, $"{n} flights, {none} from the water");
        }

        // Labor is integer arithmetic, so it must match exactly.

        [Fact]
        public void LaborSettles()
        {
            int n = 0, full = 0;
            foreach (var v in Lines("labor"))
            {
                var pool = new LaborPool(int.Parse(v[1]), long.Parse(v[2]));
                long now = long.Parse(v[3]);
                Assert.Equal(new LaborPool(int.Parse(v[4]), long.Parse(v[5])), pool.Settle(GameContent.Labor, now));
                var next = pool.NextRegenAt(GameContent.Labor, now);
                Assert.Equal(v[6] == "none" ? null : long.Parse(v[6]), next);
                full += next is null ? 1 : 0;
                n++;
            }
            Assert.True(n >= 60 && full >= 3, $"{n} vectors, {full} full");
        }

        [Fact]
        public void LaborSpends()
        {
            int n = 0, short_ = 0;
            foreach (var v in Lines("labor_spend"))
            {
                var pool = new LaborPool(int.Parse(v[1]), long.Parse(v[2]));
                var got = pool.Spend(GameContent.Labor, long.Parse(v[3]), int.Parse(v[4]), out int have);
                if (v[5] == "short")
                {
                    Assert.Null(got);
                    Assert.Equal(int.Parse(v[6]), have);
                    short_++;
                }
                else
                {
                    Assert.Equal(new LaborPool(int.Parse(v[5]), long.Parse(v[6])), got);
                }
                n++;
            }
            Assert.True(n >= 60 && short_ >= 10, $"{n} vectors, {short_} short");
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
