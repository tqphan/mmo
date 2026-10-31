using System;
using Mmo.Content;
using Mmo.Sim;

namespace Mmo.World
{
    /// <summary>
    /// Where a ship is now (ADR 0014, docs/design/ships.md). Ships are published at 5 Hz with the
    /// helm's input in <c>vehicle</c>, and stepped between rows with <see cref="Ship.Step"/> at the
    /// server's 10 Hz, so the prediction follows the server's own course. Unlike characters, ships
    /// are drawn at the present, not 200 ms back: players stand on their decks and must agree with
    /// the server's pose when they board or step off.
    ///
    /// A new row or input restarts the prediction from there. The difference between the old and
    /// new answers is kept as an offset that fades at <see cref="SmoothRate"/>, so the ship never
    /// jumps unless it was more than <see cref="SnapM"/> off.
    ///
    /// Given the ground, it also checks each step's footing with <see cref="Ship.Stand"/>, as the
    /// server's tick does: a cart stops at the same bank or ford, a ship at the same shoal, and a
    /// cart's deck rides the ground (docs/design/vehicles.md).
    /// </summary>
    public sealed class ShipPredictor
    {
        /// <summary>The server's tick.</summary>
        public const float StepS = 0.1f;
        /// <summary>How fast a correction fades, per second.</summary>
        public const float SmoothRate = 4.0f;
        /// <summary>Further off than this is a teleport or a respawn: snap.</summary>
        public const float SnapM = 8.0f;

        private readonly HullData hull;
        private readonly Func<float, float, Ground?>? ground;
        /// <summary>The state <see cref="steps"/> whole steps after the last restart, at <see cref="start"/>, and its deck's height.</summary>
        private ShipMotion state;
        private float stateY;
        private double start = double.NaN;
        private int steps;
        private float offX, offZ, offYaw;
        private double offAt;

        public ShipPredictor(HullData hull, Func<float, float, Ground?>? ground = null)
        {
            this.hull = hull;
            this.ground = ground;
        }

        public HullData Hull => hull;
        public ShipInput Input { get; private set; }
        public bool HasRow => !double.IsNaN(start);

        private double Stepped => start + steps * (double)StepS;

        /// <summary>A <c>transform</c> row: position, yaw and speed, and the deck's height if known, received at <paramref name="now"/>.</summary>
        public void Row(double now, float x, float z, float yaw, float speed, float? y = null) =>
            Restart(now, new ShipMotion(x, z, yaw, speed), y: y);

        /// <summary>The helm's input changed at <paramref name="now"/>, from <c>vehicle</c> or our own helm.</summary>
        public void SetInput(double now, ShipInput input)
        {
            if (input == Input)
                return;
            if (HasRow)
            {
                var (m, y) = AdvanceWithDeck(now);
                Restart(now, m, keepOffset: true, y: y);
            }
            Input = input;
        }

        private void Restart(double now, ShipMotion from, bool keepOffset = false, float? y = null)
        {
            float deck = y ?? (HasRow ? stateY : from.Pose(hull).Y);
            if (HasRow && !keepOffset)
            {
                // Carry what we were drawing over as an offset, so the switch doesn't show.
                var was = Pose(now);
                var fresh = from.Pose(hull);
                (offX, offZ, offYaw) = (was.X - fresh.X, was.Z - fresh.Z, Yaw.Delta(fresh.Yaw, was.Yaw));
                offAt = now;
                if (MathF.Sqrt(offX * offX + offZ * offZ) > SnapM)
                    (offX, offZ, offYaw) = (0, 0, 0);
            }
            state = from;
            stateY = deck;
            start = now;
            steps = 0;
        }

        /// <summary>One step from <paramref name="m"/>, stopped where it can't stand.</summary>
        private (ShipMotion, float) StepOnce(ShipMotion m, float y, float dt)
        {
            var next = Ship.Step(m, hull, Input, dt);
            if (ground is null)
                return (next, y);
            return Ship.Stand(hull, next, ground) is float stood ? (next, stood) : (m with { Speed = 0 }, y);
        }

        /// <summary>The exact predicted state at <paramref name="now"/>, without the fading offset.</summary>
        public ShipMotion Advance(double now) => AdvanceWithDeck(now).Motion;

        private (ShipMotion Motion, float Y) AdvanceWithDeck(double now)
        {
            if (!HasRow)
                throw new InvalidOperationException("no row yet");
            // Whole steps are kept; the remainder is a partial step from there. An earlier time
            // than the last whole step gets that step: the prediction never runs backwards.
            while (now - Stepped >= StepS)
            {
                (state, stateY) = StepOnce(state, stateY, StepS);
                steps++;
            }
            float rest = (float)(now - Stepped);
            return rest > 0 ? StepOnce(state, stateY, rest) : (state, stateY);
        }

        /// <summary>Where to draw the ship, and where its deck is, at <paramref name="now"/>.</summary>
        public ShipPose Pose(double now)
        {
            var (m, y) = AdvanceWithDeck(now);
            var pose = m.PoseAt(y);
            float k = MathF.Exp(-SmoothRate * (float)Math.Max(0, now - offAt));
            return pose with { X = pose.X + offX * k, Z = pose.Z + offZ * k, Yaw = pose.Yaw + offYaw * k };
        }

        /// <summary>The predicted speed at <paramref name="now"/>, m/s, negative astern.</summary>
        public float Speed(double now) => Advance(now).Speed;
    }
}
