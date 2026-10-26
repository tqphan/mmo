using System;
using System.Numerics;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>
    /// The predicted player as the sender sees it. Speed is horizontal, m/s; yaw is the heading.
    /// <see cref="Parent"/> is the frame: 0 for the world, or the ship whose deck the position and
    /// yaw are relative to (docs/design/ships.md).
    /// </summary>
    public readonly record struct MoveState(Vector3 Pos, float Yaw, float Speed, byte Mode, bool Grounded, ulong Parent = 0)
    {
        public bool Moving => Speed > MoveSender.StillMps || !Grounded;
    }

    public enum MoveReason { Start, Stop, Turn, Jump, Land, Mode, Speed, Heartbeat, Frame }

    /// <summary>One <c>move_event</c> to send.</summary>
    public readonly record struct MoveMsg(uint Seq, ulong Parent, Vector3 Pos, ushort Yaw, short SpeedCmS, byte Mode, MoveReason Reason);

    /// <summary>
    /// Decides when the player's movement goes to the server (PLAN §6.2, docs/design/movement.md):
    /// on start, stop, turn, jump, landing, boarding or leaving a ship, and mode or speed change,
    /// plus a heartbeat every 0.5 s while moving or in the air. Each event is numbered so the server can drop stale ones.
    /// </summary>
    public sealed class MoveSender
    {
        public const double HeartbeatS = 0.5;

        /// <summary>A heading change this large while moving is a turn.</summary>
        public static readonly float TurnRad = 10.0f * MathF.PI / 180.0f;

        /// <summary>Turns are sent at most this often, so steering with the mouse doesn't flood the server.</summary>
        public const double MinTurnGapS = 0.1;

        /// <summary>A change in speed this large while moving is sent.</summary>
        public const float SpeedStepMps = 0.5f;

        /// <summary>Slower than this counts as standing still.</summary>
        public const float StillMps = 0.05f;

        private uint seq;
        private MoveState last;
        private double lastAt;
        private bool started;
        /// <summary>The old mode's last position has been sent; the mode change goes next.</summary>
        private bool modeFlushed;

        /// <summary>Number of events sent since the last reset.</summary>
        public int Sent { get; private set; }

        /// <summary>The last state sent (or reset to): where the server has us, and since when.</summary>
        public MoveState Last => last;
        public double LastAt => lastAt;

        /// <summary>
        /// Start again from where the server has us: after entering the world or a correction. The next
        /// event is numbered after <paramref name="lastSeq"/>, the server's last accepted one.
        /// </summary>
        public void Reset(uint lastSeq, MoveState at, double now)
        {
            seq = Math.Max(seq, lastSeq);
            last = at;
            lastAt = now;
            started = true;
            modeFlushed = false;
        }

        /// <summary>Call every physics frame. Returns the event to send, if one is due.</summary>
        public MoveMsg? Update(double now, MoveState s, bool jumped)
        {
            if (!started)
                return null;
            MoveReason? reason = null;
            double since = now - lastAt;
            if (s.Parent != last.Parent)
                reason = MoveReason.Frame;
            else if (jumped)
                reason = MoveReason.Jump;
            else if (s.Mode != last.Mode)
            {
                // The server checks the distance since the last event at the new mode's speed, and
                // swimming is slower than running. So first report where the old mode got us, and
                // change mode on the next update.
                reason = MoveReason.Mode;
                if (!modeFlushed)
                    s = s with { Mode = last.Mode };
                modeFlushed = !modeFlushed;
            }
            else if (s.Moving != last.Moving)
                reason = s.Moving ? MoveReason.Start : MoveReason.Stop;
            else if (s.Grounded && !last.Grounded)
                reason = MoveReason.Land;
            else if (s.Moving)
            {
                if (MathF.Abs(s.Speed - last.Speed) > SpeedStepMps)
                    reason = MoveReason.Speed;
                else if (MathF.Abs(Yaw.Delta(last.Yaw, s.Yaw)) > TurnRad && since >= MinTurnGapS)
                    reason = MoveReason.Turn;
                else if (since >= HeartbeatS)
                    reason = MoveReason.Heartbeat;
            }
            if (reason is not MoveReason r)
                return null;
            seq++;
            Sent++;
            last = s;
            lastAt = now;
            short cms = (short)Math.Clamp(MathF.Round(s.Speed * 100.0f), 0, short.MaxValue);
            return new MoveMsg(seq, s.Parent, s.Pos, Yaw.ToU16(s.Yaw), cms, s.Mode, r);
        }
    }
}
