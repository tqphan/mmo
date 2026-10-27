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
        public void GatherReach()
        {
            int n = 0, hits = 0;
            foreach (var v in Lines("gather_reach"))
            {
                bool want = v[7] == "1";
                Assert.Equal(want, Gather.InReach(new System.Numerics.Vector3(F(v[1]), F(v[2]), F(v[3])), new System.Numerics.Vector3(F(v[4]), F(v[5]), F(v[6]))));
                hits += want ? 1 : 0;
                n++;
            }
            Assert.True(n >= 80 && hits >= 10 && hits < n, $"{n} vectors, {hits} in reach");
        }

        [Fact]
        public void GatherProgress()
        {
            int n = 0;
            foreach (var v in Lines("gather_progress"))
            {
                var kind = GameContent.Node(byte.Parse(v[1]))!;
                Assert.Equal(F(v[4]), Gather.Progress(kind, long.Parse(v[2]), long.Parse(v[3])));
                n++;
            }
            Assert.True(n >= 50, $"{n} vectors");
        }

        [Fact]
        public void CraftReach()
        {
            int n = 0, hits = 0;
            foreach (var v in Lines("craft_reach"))
            {
                bool want = v[7] == "1";
                Assert.Equal(want, Craft.InReach(new System.Numerics.Vector3(F(v[1]), F(v[2]), F(v[3])), new System.Numerics.Vector3(F(v[4]), F(v[5]), F(v[6]))));
                hits += want ? 1 : 0;
                n++;
            }
            Assert.True(n >= 80 && hits >= 10 && hits < n, $"{n} vectors, {hits} in reach");
        }

        [Fact]
        public void CraftProgress()
        {
            int n = 0;
            foreach (var v in Lines("craft_progress"))
            {
                var r = GameContent.Recipe(ushort.Parse(v[1]))!;
                Assert.Equal(F(v[4]), Craft.Progress(r, long.Parse(v[2]), long.Parse(v[3])));
                n++;
            }
            Assert.True(n >= 50, $"{n} vectors");
        }

        [Fact]
        public void CraftAffordable()
        {
            int n = 0, some = 0;
            foreach (var v in Lines("craft_affordable"))
            {
                var r = GameContent.Recipe(ushort.Parse(v[1]))!;
                var held = r.Inputs.Select((i, k) => (i.Item, Count: uint.Parse(v[3 + k]))).ToDictionary(p => p.Item, p => p.Count);
                uint want = uint.Parse(v[3 + r.Inputs.Count]);
                Assert.Equal(want, Craft.Affordable(r, w => held.TryGetValue(w, out var c) ? c : 0, int.Parse(v[2])));
                some += want > 0 ? 1 : 0;
                n++;
            }
            Assert.True(n >= 50 && some > 10 && some < n, $"{n} vectors, {some} affordable");
        }

        [Fact]
        public void TradeReach()
        {
            int n = 0, hits = 0;
            foreach (var v in Lines("trade_reach"))
            {
                bool want = v[7] == "1";
                Assert.Equal(want, Trade.InReach(new System.Numerics.Vector3(F(v[1]), F(v[2]), F(v[3])), new System.Numerics.Vector3(F(v[4]), F(v[5]), F(v[6]))));
                hits += want ? 1 : 0;
                n++;
            }
            Assert.True(n >= 40 && hits >= 5 && hits < n, $"{n} vectors, {hits} in reach");
        }

        [Fact]
        public void TradeDemand()
        {
            int n = 0;
            foreach (var v in Lines("trade_demand"))
            {
                uint now = Trade.DemandNow(uint.Parse(v[1]), long.Parse(v[2]), long.Parse(v[3]));
                Assert.Equal(uint.Parse(v[4]), now);
                Assert.Equal(uint.Parse(v[5]), Trade.AfterSale(now));
                n++;
            }
            Assert.True(n >= 40, $"{n} vectors");
        }

        [Fact]
        public void TradeFresh()
        {
            int n = 0, spoiled = 0;
            foreach (var v in Lines("trade_fresh"))
            {
                var got = Trade.FreshPermille(long.Parse(v[1]), long.Parse(v[2]));
                Assert.Equal(v[3] == "spoiled" ? null : uint.Parse(v[3]), got);
                spoiled += got is null ? 1 : 0;
                n++;
            }
            Assert.True(n >= 40 && spoiled > 0 && spoiled < n, $"{n} vectors, {spoiled} spoiled");
        }

        [Fact]
        public void TradeQuote()
        {
            int n = 0;
            foreach (var v in Lines("trade_quote"))
            {
                var (q, refusal) = Trade.Quote(ushort.Parse(v[1]), new System.Numerics.Vector2(F(v[2]), F(v[3])), long.Parse(v[4]),
                    new System.Numerics.Vector2(F(v[5]), F(v[6])), uint.Parse(v[7]), uint.Parse(v[8]), long.Parse(v[9]));
                string got = q is { } x ? $"{x.RoutePct}:{x.Demand}:{x.Fresh}:{x.Copper}" : ((byte)refusal!).ToString();
                Assert.Equal(v[10], got);
                n++;
            }
            Assert.True(n >= 100, $"{n} vectors");
        }

        private static byte[] Hex(string hex) => Convert.FromHexString(hex);

        [Fact]
        public void LandSnap()
        {
            int n = 0;
            foreach (var v in Lines("land_snap"))
            {
                Assert.Equal(F(v[2]), Land.Snap(F(v[1])));
                n++;
            }
            Assert.True(n >= 5, $"{n} vectors");
        }

        [Fact]
        public void LandSite()
        {
            var map = Mmo.World.HeightMap.Parse(Hex(Lines("land_mmoh").Single()[1]));
            var chunks = Lines("land_chunk").ToDictionary(v => ulong.Parse(v[1]), v => Hex(v[2]));
            byte Texel(int i, int j)
            {
                ulong cell = ((ulong)(uint)(j / 16) << 16) | (uint)(i / 16);
                return chunks.TryGetValue(cell, out var t) ? t[j % 16 * 16 + i % 16] : (byte)0;
            }
            int n = 0;
            var seen = new HashSet<string>();
            foreach (var v in Lines("land_site"))
            {
                var kind = GameContent.Plot(byte.Parse(v[1]))!;
                var others = v[4] == "-"
                    ? new List<Rect>()
                    : v[4].Split(',').Select(o => o.Split(':')).Select(o => Rect.Plot(GameContent.Plot(byte.Parse(o[0]))!, F(o[1]), F(o[2]))).ToList();
                var (lowest, error) = Land.CheckSite(Texel, map.GridSample, kind, F(v[2]), F(v[3]), others);
                string got = error is SiteError e ? ((byte)e).ToString(CultureInfo.InvariantCulture) : $"ok:{lowest.ToString("R", CultureInfo.InvariantCulture)}";
                if (v[5].StartsWith("ok:"))
                {
                    Assert.Null(error);
                    Assert.Equal(F(v[5][3..]), lowest);
                }
                else
                    Assert.Equal(v[5], got);
                seen.Add(v[5].StartsWith("ok:") ? "ok" : v[5]);
                n++;
            }
            Assert.True(n >= 300 && seen.Count == 7, $"{n} vectors, results {string.Join(",", seen)}");
        }

        [Fact]
        public void LandTimes()
        {
            int n = 0;
            foreach (var v in Lines("land_demolish_at"))
            {
                Assert.Equal(long.Parse(v[2]), Land.DemolishAt(long.Parse(v[1])));
                n++;
            }
            foreach (var v in Lines("land_standing"))
            {
                long paid = long.Parse(v[1]), now = long.Parse(v[2]);
                Assert.Equal((Standing)byte.Parse(v[3]), Land.StandingOf(paid, now));
                Assert.Equal(v[4] == "1", Land.CanPay(paid, now));
                n++;
            }
            Assert.True(n >= 100, $"{n} vectors");
        }

        [Fact]
        public void LandTax()
        {
            int n = 0;
            foreach (var v in Lines("land_tax"))
            {
                var got = Land.WeeklyTax(GameContent.Plot(byte.Parse(v[1]))!, int.Parse(v[2]));
                Assert.Equal(v[3] == "none" ? null : uint.Parse(v[3]), got);
                n++;
            }
            Assert.True(n >= 10, $"{n} vectors");
        }

        [Fact]
        public void FarmGrowthAndStages()
        {
            long? Opt(string s) => s == "none" ? null : long.Parse(s);
            int n = 0;
            foreach (var v in Lines("farm_growth"))
            {
                var kind = GameContent.Crop(byte.Parse(v[1]))!;
                bool ideal = v[2] == "1";
                long from = long.Parse(v[3]), ripe = long.Parse(v[4]);
                Assert.Equal(ripe, Farm.RipeAt(kind, from, ideal));
                Assert.Equal(Opt(v[5]), Farm.RegrownAt(kind, from, ideal));
                Assert.Equal(Opt(v[6]), Farm.WithersAt(kind, ripe));
                n++;
            }
            foreach (var v in Lines("farm_stage"))
            {
                var kind = GameContent.Crop(byte.Parse(v[1]))!;
                long from = long.Parse(v[2]), ripe = long.Parse(v[3]), now = long.Parse(v[4]);
                Assert.Equal((Stage)byte.Parse(v[5]), Farm.StageOf(kind, ripe, now));
                Assert.Equal(F(v[6]), Farm.Progress(from, ripe, now));
                n++;
            }
            Assert.True(n >= 80, $"{n} vectors");
        }

        [Fact]
        public void FarmSpot()
        {
            var map = Mmo.World.HeightMap.Parse(Hex(Lines("land_mmoh").Single()[1]));
            var chunks = Lines("land_chunk").ToDictionary(v => ulong.Parse(v[1]), v => Hex(v[2]));
            byte Texel(int i, int j)
            {
                ulong cell = ((ulong)(uint)(j / 16) << 16) | (uint)(i / 16);
                return chunks.TryGetValue(cell, out var t) ? t[j % 16 * 16 + i % 16] : (byte)0;
            }
            int n = 0;
            var seen = new HashSet<string>();
            foreach (var v in Lines("farm_spot"))
            {
                var kind = GameContent.Crop(byte.Parse(v[1]))!;
                var plots = v[4] == "-"
                    ? new List<PlotSpot>()
                    : v[4].Split(',').Select(o => o.Split(':')).Select(o => new PlotSpot(new Rect(F(o[0]), F(o[1]), F(o[2]), F(o[3])), o[4] == "1")).ToList();
                var others = v[5] == "-"
                    ? new List<Planting>()
                    : v[5].Split(',').Select(o => o.Split(':')).Select(o => new Planting(F(o[0]), F(o[1]), F(o[2]))).ToList();
                var (spot, error) = Farm.CheckSpot(Texel, map.GridSample, kind, F(v[2]), F(v[3]), plots, others);
                if (v[6].StartsWith("ok:"))
                {
                    var ok = v[6].Split(':');
                    Assert.Null(error);
                    Assert.Equal(F(ok[1]), spot.Y);
                    Assert.Equal(ok[2] == "-" ? null : int.Parse(ok[2]), spot.Plot);
                    Assert.Equal(ok[3] == "1", spot.Ideal);
                }
                else
                    Assert.Equal(v[6], error is SpotError e ? ((byte)e).ToString(CultureInfo.InvariantCulture) : "ok");
                seen.Add(v[6].StartsWith("ok:") ? "ok" : v[6]);
                n++;
            }
            Assert.True(n >= 300 && seen.Count == 8, $"{n} vectors, results {string.Join(",", seen)}");
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

        [Fact]
        public void CombatSettle()
        {
            int n = 0;
            foreach (var v in Lines("combat_settle"))
            {
                int got = Combat.Settle(int.Parse(v[1]), int.Parse(v[2]), long.Parse(v[3]), long.Parse(v[4]), long.Parse(v[5]), int.Parse(v[6]), int.Parse(v[7]));
                Assert.True(int.Parse(v[8]) == got, $"combat_settle {string.Join(' ', v[1..8])}: want {v[8]}, got {got}");
                n++;
            }
            Assert.True(n >= 60, $"only {n} vectors");
        }

        [Fact]
        public void CombatDeaths()
        {
            int n = 0;
            foreach (var v in Lines("combat_deaths"))
            {
                long? last = v[2] == "none" ? null : long.Parse(v[2]);
                uint after = Combat.DeathsAfter(uint.Parse(v[1]), last, long.Parse(v[3]));
                Assert.Equal(uint.Parse(v[4]), after);
                Assert.Equal(long.Parse(v[5]), Combat.RespawnWaitMicros(after));
                n++;
            }
            Assert.True(n >= 30, $"only {n} vectors");
        }

        [Fact]
        public void CombatFront()
        {
            int n = 0;
            foreach (var v in Lines("combat_front"))
            {
                var from = new System.Numerics.Vector3(F(v[2]), F(v[3]), F(v[4]));
                var to = new System.Numerics.Vector3(F(v[5]), F(v[6]), F(v[7]));
                Assert.True((v[8] == "1") == Combat.InFront(F(v[1]), from, to), $"combat_front {string.Join(' ', v[1..8])}");
                Assert.Equal(v[9] == "1", Combat.MovedOff(from, to));
                Assert.Equal(long.Parse(v[10]), Combat.ImpactMicros(Combat.Distance(from, to), 30.0f));
                n++;
            }
            Assert.True(n >= 60, $"only {n} vectors");
        }

        [Fact]
        public void CombatCheck()
        {
            int n = 0, refused = 0;
            foreach (var v in Lines("combat_check"))
            {
                var skill = GameContent.Skill(ushort.Parse(v[1]))!;
                var at = new System.Numerics.Vector3(F(v[2]), F(v[3]), F(v[4]));
                var c = new Caster(at, F(v[5]), v[6] == "1", v[7] == "1", int.Parse(v[8]), long.Parse(v[9]));
                var t = new CombatTarget(new System.Numerics.Vector3(F(v[10]), F(v[11]), F(v[12])), v[13] == "1", v[14] == "1");
                byte code = (byte?)Combat.Check(skill, c, t, long.Parse(v[15])) ?? 0;
                Assert.True(byte.Parse(v[16]) == code, $"combat_check {string.Join(' ', v[1..16])}: want {v[16]}, got {code}");
                Assert.Equal(byte.Parse(v[17]), (byte?)Combat.CheckFinish(skill, at, t) ?? 0);
                if (code != 0)
                    refused++;
                n++;
            }
            Assert.True(n >= 120 && refused > 20 && refused < n, $"{n} vectors, {refused} refused");
        }
    }
}
