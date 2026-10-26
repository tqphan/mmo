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
    public class ShipPredictorTests
    {
        private static readonly HullData Clipper = Hull.Clipper.Data();

        /// <summary>The server's course: 10 Hz steps from rest at the origin.</summary>
        private static List<ShipMotion> Course(ShipInput input, int steps)
        {
            var m = new ShipMotion(0, 0, 0, 0);
            var course = new List<ShipMotion> { m };
            for (int i = 0; i < steps; i++)
                course.Add(m = Ship.Step(m, Clipper, input, 0.1f));
            return course;
        }

        [Fact]
        public void BetweenRowsItFollowsTheServersCourse()
        {
            var input = new ShipInput(100, 100);
            var course = Course(input, 300);
            var p = new ShipPredictor(Clipper);
            p.SetInput(0, input);
            // Rows at 5 Hz, as the server publishes them, rounded as transform rounds them.
            double worst = 0;
            for (int i = 0; i <= 290; i++)
            {
                double now = i * 0.1;
                if (i % 2 == 0)
                {
                    var m = course[i];
                    p.Row(now, m.X, m.Z, Yaw.FromU16(Yaw.ToU16(m.Yaw)), MathF.Round(m.Speed * 100) / 100);
                }
                // Halfway to the next row, and one tick on.
                foreach (double ahead in new[] { 0.05, 0.1 })
                {
                    var want = course[i + 1];
                    var got = p.Pose(now + ahead);
                    var wantAt = ahead == 0.1 ? want : Ship.Step(course[i], Clipper, input, 0.05f);
                    worst = Math.Max(worst, new Vector2(got.X - wantAt.X, got.Z - wantAt.Z).Length());
                }
            }
            Assert.True(worst < 0.05, $"worst {worst} m off the server's course");
        }

        [Fact]
        public void WithoutRowsItSailsOnUnderTheHelm()
        {
            var p = new ShipPredictor(Clipper);
            p.Row(0, 0, 0, 0, 0);
            p.SetInput(0, new ShipInput(100, 0));
            var m = p.Advance(20);
            Assert.Equal(Clipper.MaxSpeedMps, m.Speed, 3);
            var want = Course(new ShipInput(100, 0), 200)[^1];
            Assert.Equal(want.Z, m.Z, 2);
            // Asking for an earlier time doesn't step backwards.
            Assert.True(p.Advance(19).Z <= m.Z + Clipper.MaxSpeedMps * ShipPredictor.StepS, $"{p.Advance(19)}");
        }

        [Fact]
        public void ACorrectionFadesInsteadOfJumping()
        {
            var p = new ShipPredictor(Clipper);
            p.Row(0, 0, 0, 0, 0);
            Assert.Equal(0, p.Pose(1).X, 4);
            // The server says it's 2 m east: drawn where it was, then eased over.
            p.Row(1, 2, 0, 0, 0);
            Assert.Equal(0, p.Pose(1).X, 3);
            Assert.InRange(p.Pose(1.25).X, 1.0f, 1.5f);
            Assert.Equal(2, p.Pose(3).X, 2);
            // Too far to be a correction: snap.
            p.Row(4, 50, 0, 0, 0);
            Assert.Equal(50, p.Pose(4).X, 3);
        }

        [Fact]
        public void AnInputChangeTakesEffectFromThen()
        {
            var p = new ShipPredictor(Clipper);
            p.Row(0, 0, 0, 0, 11);
            p.SetInput(0, new ShipInput(100, 0));
            var before = p.Advance(2);
            p.SetInput(2, new ShipInput(100, 100));
            // No jump at the change, and turning to starboard (yaw goes down) after it.
            Assert.Equal(before.Z, p.Pose(2).Z, 4);
            Assert.True(Yaw.Delta(0, p.Pose(4).Yaw) < -0.3f, $"{p.Pose(4).Yaw}");
            Assert.Equal(new ShipInput(100, 100), p.Input);
        }
    }

    public class BoardingTests
    {
        private static readonly HullData Clipper = Hull.Clipper.Data();
        /// <summary>A clipper facing west (yaw 90°) at (100, 50), sailing at 11 m/s.</summary>
        private static readonly Deck A = new(7, Clipper, new ShipPose(100, 2, 50, MathF.PI / 2), 11);
        private static readonly Deck B = new(8, Clipper, new ShipPose(100, 2, 70, 0), 0);
        private static readonly Deck[] Both = { A, B };

        private static Vector3 OnA(float x, float y, float z) => A.Pose.ToWorld(new Vector3(x, y, z));

        [Fact]
        public void StandingOnADeckPutsYouInItsFrame()
        {
            Assert.Equal(7ul, Boarding.FrameAfter(0, OnA(1, 0, -3), true, Both));
            Assert.Equal(8ul, Boarding.FrameAfter(7, B.Pose.ToWorld(new Vector3(0, 0.1f, 0)), true, Both));
            // In the air over a deck you haven't landed on, you stay where you were.
            Assert.Equal(0ul, Boarding.FrameAfter(0, OnA(1, 0.8f, -3), false, Both));
            Assert.Equal(0ul, Boarding.FrameAfter(0, new Vector3(0, 0, 0), true, Both));
        }

        [Fact]
        public void AboardYouStayOverTheRailAndInTheAir()
        {
            Assert.Equal(7ul, Boarding.FrameAfter(7, OnA(0, 1.5f, 0), false, Both));
            Assert.Equal(7ul, Boarding.FrameAfter(7, OnA(Clipper.DeckHalfWidthM + 1.2f, -0.5f, 0), false, Both));
            // Past the rail, short of the server's limit; or dropped below the deck.
            Assert.Equal(0ul, Boarding.FrameAfter(7, OnA(Clipper.DeckHalfWidthM + Ship.RailM - 0.2f, 0, 0), false, Both));
            Assert.Equal(0ul, Boarding.FrameAfter(7, OnA(Clipper.DeckHalfWidthM + 0.5f, -1.2f, 0), false, Both));
            // A ship that's gone takes its frame with it.
            Assert.Equal(0ul, Boarding.FrameAfter(9, OnA(0, 0, 0), true, Array.Empty<Deck>()));
        }

        [Fact]
        public void ClimbingFromTheWaterLandsJustInsideTheDeck()
        {
            // Alongside to starboard, on the sea floor.
            var t = Boarding.ClimbTarget(A, OnA(Clipper.DeckHalfWidthM + 0.4f, -4, 3));
            Assert.NotNull(t);
            Assert.True(Vector3.Distance(new Vector3(Clipper.DeckHalfWidthM - Boarding.ClimbInsetM, 0, 3), t!.Value) < 1e-4f, $"{t}");
            // Off the bow's corner.
            var bow = Boarding.ClimbTarget(A, OnA(-3, -1, -Clipper.DeckHalfLengthM - 1));
            var corner = new Vector3(-(Clipper.DeckHalfWidthM - Boarding.ClimbInsetM), 0, -(Clipper.DeckHalfLengthM - Boarding.ClimbInsetM));
            Assert.True(Vector3.Distance(corner, bow!.Value) < 1e-4f, $"{bow}");
            Assert.Null(Boarding.ClimbTarget(A, OnA(Clipper.DeckHalfWidthM + 2, -1, 0)));
            Assert.Null(Boarding.ClimbTarget(A, OnA(Clipper.DeckHalfWidthM + 0.5f, -10, 0)));
            Assert.Null(Boarding.ClimbTarget(A, OnA(Clipper.DeckHalfWidthM + 0.5f, 3, 0)));
        }

        [Fact]
        public void AClimbWaitsForTheServersAllowance()
        {
            var from = OnA(Clipper.DeckHalfWidthM + 0.4f, -3, 0);
            var to = OnA(Clipper.DeckHalfWidthM - 0.5f, 0, 0);
            // 0.9 m across: not at once (the server's slack is 0.5 m), but a fifth of a second later.
            Assert.False(Boarding.WithinAllowance(from, to, 0.0f, 5.4f, 0));
            Assert.True(Boarding.WithinAllowance(from, to, 0.2f, 5.4f, 0));
            // 5.4 m right across the deck: a second's running, or less on a ship under way, for its lag.
            var far = OnA(-Clipper.DeckHalfWidthM, 0, 0);
            Assert.False(Boarding.WithinAllowance(from, far, 0.5f, 5.4f, 0));
            Assert.True(Boarding.WithinAllowance(from, far, 1.0f, 5.4f, 0));
            Assert.True(Boarding.WithinAllowance(from, far, 0.1f, 5.4f, 11));
        }

        [Fact]
        public void StationsAreFoundWithinReach()
        {
            Assert.Equal(new Station(StationKind.Helm, 0), Boarding.StationAt(Clipper, new Vector3(0.5f, 0, 6.5f)));
            Assert.Equal(new Station(StationKind.Cannon, 1), Boarding.StationAt(Clipper, new Vector3(1.8f, 0, 1)));
            Assert.Equal(new Station(StationKind.Cannon, 0), Boarding.StationAt(Clipper, new Vector3(-1, 0, 0)));
            Assert.Null(Boarding.StationAt(Clipper, new Vector3(0, 0, -6)));
        }
    }

    public class ShipAutopilotTests
    {
        private static readonly HullData Clipper = Hull.Clipper.Data();
        private static readonly Deck Ship7 = new(7, Clipper, new ShipPose(-430, 2, 0, MathF.PI / 2), 0);

        private static PilotView View(double now, Vector2 world, ulong parent = 0, Vector3 local = default, Station? manning = null, bool loaded = false, params Deck[] decks) =>
            new(now, world, parent, local, decks, manning, loaded);

        [Fact]
        public void ShipRoutesParse()
        {
            Assert.True(Autopilot.Parse("deck:-430,0,60")!.Sails);
            Assert.True(Autopilot.Parse("helm:-430,0,100")!.Sails);
            Assert.True(Autopilot.Parse("gun:-430,0,1")!.Sails);
            Assert.False(Autopilot.Parse("circle:0,0,5")!.Sails);
            Assert.Throws<ArgumentException>(() => Autopilot.Parse("helm:0,0,101"));
            Assert.Throws<ArgumentException>(() => Autopilot.Parse("gun:0,0,1.5"));
            Assert.Throws<ArgumentException>(() => Autopilot.Parse("deck:0,0,-1"));
        }

        [Fact]
        public void ItWaitsForAShipThenRunsToItAndClimbs()
        {
            var a = Autopilot.Parse("deck:-430,0,0")!;
            Assert.Equal(new Vector2(-1, 0), a.Drive(View(0, new Vector2(0, 0))).Dir);
            Assert.Equal(Vector2.Zero, a.Drive(View(1, new Vector2(-410, 0))).Dir);
            var o = a.Drive(View(2, new Vector2(-410, 0), decks: Ship7));
            Assert.Equal(PilotAction.Climb, o.Action);
            Assert.Equal(new Vector2(-1, 0), o.Dir);
        }

        [Fact]
        public void TheDeckRouteLapsJumpsAndGoesOverTheSide()
        {
            var a = Autopilot.Parse("deck:-430,0,10")!;
            var local = new Vector3(0, 0, 0);
            int jumps = 0;
            var seen = new HashSet<(int, int)>();
            for (double t = 0; t < 9.9; t += 1.0 / 60)
            {
                var o = a.Drive(View(t, default, 7, local, decks: Ship7));
                Assert.True(o.OnDeck);
                jumps += o.Jump ? 1 : 0;
                local += new Vector3(o.Dir.X, 0, o.Dir.Y) * 5.4f / 60;
                Assert.True(Ship.OnDeck(Clipper, local.X, local.Z, 0), $"off the deck at {local}");
                seen.Add((Math.Sign(local.X), Math.Sign(local.Z)));
            }
            Assert.Equal(4, seen.Count);
            Assert.InRange(jumps, 3, 4);
            Assert.Equal(Vector2.UnitX, a.Drive(View(10, default, 7, local, decks: Ship7)).Dir);
            // Once over, it stays in the water.
            Assert.Equal(Vector2.Zero, a.Drive(View(11, default, decks: Ship7)).Dir);
        }

        [Fact]
        public void TheHelmRouteTakesTheHelmAndSails()
        {
            var a = Autopilot.Parse("helm:-430,0,60")!;
            var far = a.Drive(View(0, default, 7, new Vector3(0, 0, 0), decks: Ship7));
            Assert.Equal(new Vector2(0, 1), far.Dir);
            var at = new Vector3(Clipper.HelmX, 0, Clipper.HelmZ - 0.5f);
            Assert.Equal(PilotAction.Use, a.Drive(View(1, default, 7, at, decks: Ship7)).Action);
            Assert.Equal(PilotAction.None, a.Drive(View(1.5, default, 7, at, decks: Ship7)).Action);
            var helm = new Station(StationKind.Helm, 0);
            Assert.Equal(new ShipInput(0, 0), a.Drive(View(3, default, 7, at, helm, decks: Ship7)).Helm);
            Assert.Equal(new ShipInput(100, 0), a.Drive(View(3 + Autopilot.CrewWaitS, default, 7, at, helm, decks: Ship7)).Helm);
            Assert.Equal(new ShipInput(100, 60), a.Drive(View(3 + Autopilot.CrewWaitS + Autopilot.RudderAfterS, default, 7, at, helm, decks: Ship7)).Helm);
        }

        [Fact]
        public void TheGunRouteFiresWhenLoaded()
        {
            var a = Autopilot.Parse("gun:-430,0,1")!;
            var at = new Vector3(Clipper.Mounts[1].X, 0, Clipper.Mounts[1].Z);
            Assert.Equal(PilotAction.Use, a.Drive(View(0, default, 7, at, decks: Ship7)).Action);
            var gun = new Station(StationKind.Cannon, 1);
            Assert.Equal(PilotAction.Fire, a.Drive(View(1, default, 7, at, gun, loaded: true, decks: Ship7)).Action);
            Assert.Equal(PilotAction.None, a.Drive(View(2, default, 7, at, gun, loaded: false, decks: Ship7)).Action);
        }
    }

    public class SwimmingTests
    {
        [Fact]
        public void YouSwimInDeepWaterAndWalkOutOfShallows()
        {
            // Wading in along the sea floor.
            Assert.False(Swimming.Swims(false, -1.4f, -1.4f));
            Assert.True(Swimming.Swims(false, -1.6f, -1.6f));
            // Afloat, you keep swimming until the floor is shallower than 1.3 m.
            Assert.True(Swimming.Swims(true, -1.4f, Swimming.FloatY));
            Assert.False(Swimming.Swims(true, -1.2f, Swimming.FloatY));
            // Falling in from a deck, you swim once you're down to the float height.
            Assert.False(Swimming.Swims(false, -10, 1.0f));
            Assert.True(Swimming.Swims(false, -10, Swimming.FloatY));
        }

        [Fact]
        public void ASwimmerStaysWhereTheServerAllowsIt()
        {
            // Within the server's swim limits: not above sea level + 1 m, and above the sea floor
            // where swimming starts, less its 0.5 m slack.
            Assert.InRange(Swimming.FloatY, -Swimming.LeaveDepthM - 0.5f, 1.0f);
            Assert.True(Swimming.FloatY > -Swimming.LeaveDepthM, "feet clear of the floor where swimming ends");
            // Rising from the sea floor and settling from a fall both end at the float height.
            float feet = -6;
            for (int i = 0; i < 600; i++)
                feet += Swimming.VerticalSpeed(feet) / 60;
            Assert.Equal(Swimming.FloatY, feet, 3);
            Assert.Equal(-Swimming.SinkMps, Swimming.VerticalSpeed(2));
        }
    }
}
