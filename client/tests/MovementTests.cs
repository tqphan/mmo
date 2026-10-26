using System;
using System.Collections.Generic;
using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Mmo.World;
using Xunit;

namespace Mmo.Tests
{
    public class MoveSenderTests
    {
        private const byte Run = (byte)MoveMode.Run;
        private const byte Walk = (byte)MoveMode.Walk;

        private static MoveState At(float x, float z, float yaw = 0, float speed = 0, byte mode = Run, bool grounded = true) =>
            new(new Vector3(x, 0, z), yaw, speed, mode, grounded);

        private static MoveSender Started(uint lastSeq = 0)
        {
            var s = new MoveSender();
            s.Reset(lastSeq, At(0, 0), 0);
            return s;
        }

        [Fact]
        public void NothingIsSentBeforeEntering()
        {
            Assert.Null(new MoveSender().Update(1, At(1, 0, speed: 5.4f), false));
        }

        [Fact]
        public void StandingStillSendsNothing()
        {
            var s = Started();
            for (double t = 0; t < 5; t += 1.0 / 60)
                Assert.Null(s.Update(t, At(0, 0), false));
        }

        [Fact]
        public void RunningSendsStartHeartbeatsAndStop()
        {
            var s = Started(lastSeq: 41);
            var sent = new List<MoveMsg>();
            double t = 0;
            float z = 0;
            for (; t < 2.0; t += 1.0 / 60)
            {
                z -= 5.4f / 60;
                if (s.Update(t, At(0, z, speed: 5.4f), false) is MoveMsg m)
                    sent.Add(m);
            }
            if (s.Update(t, At(0, z), false) is MoveMsg stop)
                sent.Add(stop);
            Assert.Equal(MoveReason.Start, sent[0].Reason);
            Assert.Equal(42u, sent[0].Seq);
            Assert.Equal(MoveReason.Stop, sent[^1].Reason);
            Assert.Equal(0, sent[^1].SpeedCmS);
            Assert.Equal(540, sent[0].SpeedCmS);
            // Just under 2 s of running: heartbeats at 0.5, 1 and 1.5 s.
            Assert.Equal(3, sent.FindAll(m => m.Reason == MoveReason.Heartbeat).Count);
            for (int i = 1; i < sent.Count; i++)
                Assert.Equal(sent[i - 1].Seq + 1, sent[i].Seq);
        }

        [Fact]
        public void EveryEventIsWithinTheServersSpeedCheck()
        {
            // Run in a circle for 10 s and check each event against sim::movement's allowance.
            var s = Started();
            var last = (Pos: Vector3.Zero, T: 0.0);
            float maxSpeed = MoveMode.Run.Data().MaxSpeedMps;
            for (double t = 1.0 / 60; t < 10; t += 1.0 / 60)
            {
                float a = (float)t * 0.5f;
                var pos = new Vector3(10 * MathF.Sin(a), 0, 10 * MathF.Cos(a) - 10);
                if (s.Update(t, new MoveState(pos, a + MathF.PI / 2, maxSpeed, Run, true), false) is MoveMsg m)
                {
                    double dt = Math.Min(t - last.T, 2.0);
                    float allowed = maxSpeed * 1.15f * (float)dt + 0.5f;
                    var d = new Vector2(m.Pos.X - last.Pos.X, m.Pos.Z - last.Pos.Z).Length();
                    Assert.True(d <= allowed, $"moved {d} m in {dt} s");
                    last = (m.Pos, t);
                }
            }
        }

        [Fact]
        public void TurnsAreSentButRateLimited()
        {
            var s = Started();
            s.Update(0.0, At(0, 0, speed: 5.4f), false);
            // Spinning 90°/s for a second.
            int turns = 0;
            for (double t = 1.0 / 60; t < 1.0; t += 1.0 / 60)
                if (s.Update(t, At(0, 0, yaw: (float)t * MathF.PI / 2, speed: 5.4f), false) is { Reason: MoveReason.Turn })
                    turns++;
            Assert.InRange(turns, 5, 10);
        }

        [Fact]
        public void JumpLandAndModeChangesAreSent()
        {
            var s = Started();
            // The controller reports the jump frame as airborne.
            Assert.Equal(MoveReason.Jump, s.Update(0.1, At(0, 0, grounded: false), true)?.Reason);
            Assert.Null(s.Update(0.2, At(0, 0, grounded: false), false));
            Assert.Equal(MoveReason.Heartbeat, s.Update(0.65, At(0, 0, grounded: false), false)?.Reason);
            Assert.Equal(MoveReason.Stop, s.Update(0.8, At(0, 0), false)?.Reason);
            Assert.Equal(MoveReason.Mode, s.Update(0.9, At(0, 0, mode: Walk), false)?.Reason);
        }

        [Fact]
        public void ResetContinuesTheSequenceFromTheServer()
        {
            var s = Started(lastSeq: 10);
            Assert.Equal(11u, s.Update(0.1, At(0, 0), true)?.Seq);
            s.Reset(5, At(0, 0), 0.2); // a correction never moves the numbering back
            Assert.Equal(12u, s.Update(0.3, At(0, 0), true)?.Seq);
            s.Reset(20, At(0, 0), 0.4);
            Assert.Equal(21u, s.Update(0.5, At(0, 0), true)?.Seq);
        }
    }

    public class SnapshotBufferTests
    {
        private static Snapshot S(double t, float x, float z = 0, float yaw = 0, float speed = 0) =>
            new(t, new Vector3(x, 0, z), yaw, speed);

        [Fact]
        public void DrawsTheDelayInThePast()
        {
            var b = new SnapshotBuffer();
            b.Add(S(1.0, 0, speed: 20));
            b.Add(S(1.5, 10, speed: 20));
            Assert.Equal(0f, b.Sample(1.2).Pos.X, 4);
            Assert.Equal(5f, b.Sample(1.25 + SnapshotBuffer.Delay).Pos.X, 4);
        }

        [Fact]
        public void ExtrapolatesAlongYawPastTheNewestRowThenStops()
        {
            var b = new SnapshotBuffer();
            float west = MathF.PI / 2; // yaw facing -x
            b.Add(S(1.0, 0, yaw: west, speed: 4));
            var p = b.Sample(1.0 + SnapshotBuffer.Delay + 0.25).Pos;
            Assert.Equal(-1f, p.X, 4);
            Assert.Equal(0f, p.Z, 4);
            var far = b.Sample(10).Pos;
            Assert.Equal(-4f * (float)SnapshotBuffer.MaxExtrapolateS, far.X, 4);
        }

        [Fact]
        public void ARunningPlayerMovesSmoothlyBetweenHeartbeats()
        {
            // Heartbeats every 0.5 s at 5 m/s east, arriving with up to 100 ms of tick jitter. Switching
            // from carrying on past a row to interpolating towards the next can step back a little
            // when that row is late (RemoteView smooths it), but never far.
            var b = new SnapshotBuffer();
            float east = -MathF.PI / 2;
            var rng = new Random(1);
            double nextRow = 0;
            float x = 0, lastDrawn = float.NaN;
            for (double now = 0; now < 5; now += 1.0 / 60)
            {
                if (now >= nextRow)
                {
                    b.Add(S(now + rng.NextDouble() * 0.1, x, yaw: east, speed: 5));
                    x += 2.5f;
                    nextRow += 0.5;
                }
                float drawn = b.Sample(now).Pos.X;
                if (!float.IsNaN(lastDrawn))
                {
                    float step = drawn - lastDrawn;
                    Assert.True(step >= -0.35f, $"went backwards {step} m at {now:F2}");
                    Assert.True(step <= 0.5f, $"jumped {step} m at {now:F2}");
                }
                lastDrawn = drawn;
            }
            Assert.InRange(lastDrawn, x - 2.5f - 3.0f, x);
        }

        [Fact]
        public void ATeleportSnapsInsteadOfSliding()
        {
            var b = new SnapshotBuffer();
            b.Add(S(1.0, 0));
            b.Add(S(1.1, 500));
            Assert.Equal(500f, b.Sample(1.15).Pos.X);
        }

        [Fact]
        public void YawTurnsTheShortWay()
        {
            var b = new SnapshotBuffer();
            b.Add(S(1.0, 0, yaw: 6.2f));
            b.Add(S(2.0, 0, yaw: 0.2f));
            float mid = b.Sample(1.5 + SnapshotBuffer.Delay).Yaw;
            Assert.Equal(0.0f, Yaw.Delta(6.2f + (0.2f + MathF.Tau - 6.2f) / 2, mid), 3);
        }
    }

    public class AutopilotTests
    {
        [Fact]
        public void CircleRunsToTheRingThenRoundIt()
        {
            var a = Autopilot.Parse("circle:0,-8,5")!;
            var pos = new Vector2(0, 0);
            double angle = 0;
            float? lastA = null;
            for (int i = 0; i < 60 * 30; i++)
            {
                pos += a.Steer(pos) * 5.4f / 60;
                float d = Vector2.Distance(pos, new Vector2(0, -8));
                if (i > 60 * 3)
                {
                    Assert.InRange(d, 4.0f, 6.0f);
                    float ang = MathF.Atan2(pos.Y + 8, pos.X);
                    if (lastA is float l)
                        angle += Yaw.Delta(l, ang);
                    lastA = ang;
                }
            }
            // About 4.5 laps in 27 s; atan2 with z south turns negative going anticlockwise from above.
            Assert.True(Math.Abs(angle) > 4 * MathF.Tau, $"turned {angle} rad");
        }

        [Fact]
        public void PaceTurnsAtEachEnd()
        {
            var a = Autopilot.Parse("pace:0,0,10")!;
            var pos = new Vector2(0, 0);
            float minX = 0, maxX = 0;
            for (int i = 0; i < 60 * 10; i++)
            {
                pos += a.Steer(pos) * 5.4f / 60;
                minX = MathF.Min(minX, pos.X);
                maxX = MathF.Max(maxX, pos.X);
            }
            Assert.InRange(maxX, 4.4f, 5.5f);
            Assert.InRange(minX, -5.5f, -4.4f);
        }

        [Fact]
        public void BadSpecsAreRefused()
        {
            Assert.Null(Autopilot.Parse("none"));
            Assert.Throws<ArgumentException>(() => Autopilot.Parse("circle:1,2"));
            Assert.Throws<ArgumentException>(() => Autopilot.Parse("spiral:1,2,3"));
            Assert.Throws<ArgumentException>(() => Autopilot.Parse("circle:0,0,0"));
        }
    }
}
