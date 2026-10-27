using System;
using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>Why a skill can't be used. The value is the code in the vectors.</summary>
    public enum CombatRefusal : byte
    {
        Dead = 1,
        Casting = 2,
        NotReady = 3,
        NoMana = 4,
        Yourself = 5,
        TargetDead = 6,
        OutOfRange = 7,
        NotInFront = 8,
        NoSight = 9,
        NoTarget = 10,
    }

    /// <summary>The caster as a skill check sees it: its mana now, and when the later of the global cooldown and the skill's cooldown ends.</summary>
    public readonly record struct Caster(Vector3 At, float Yaw, bool Alive, bool Casting, int Mp, long ReadyAt);

    /// <summary>The target, already resolved: for an ally skill on anything but a living character, or a caster skill, the caster.</summary>
    public readonly record struct CombatTarget(Vector3 At, bool Alive, bool IsCaster);

    /// <summary>
    /// Combat's rules (docs/design/combat.md): HP and mana from timestamps, the checks before a skill
    /// is used, interrupting casts, projectile flight and the wait to respawn. A port of
    /// <c>sim::combat</c>, checked against generated vectors; the client uses it to refuse what it can
    /// see is wrong and to draw bars and cooldowns, and the server decides.
    /// </summary>
    public static class Combat
    {
        private const long MicrosPerS = 1_000_000;
        private const long MicrosPerMs = 1_000;
        /// <summary>Closer than this, a target always counts as in front.</summary>
        public const float FrontExemptM = 1.0f;

        private static CombatRules R => GameContent.Combat;

        /// <summary>
        /// HP or mana at <paramref name="now"/>: <paramref name="value"/> as it was at <paramref name="at"/>, plus
        /// regeneration at <paramref name="rateIn"/> per second until <paramref name="combatUntil"/> and
        /// <paramref name="rateOut"/> after, never above <paramref name="max"/>. A value already at or above the maximum stays.
        /// </summary>
        public static int Settle(int value, int max, long at, long combatUntil, long now, int rateIn, int rateOut)
        {
            if (value >= max)
                return value;
            now = Math.Max(now, at);
            long split = Math.Clamp(combatUntil, at, now);
            static Int128 Part(int rate, long micros) => (Int128)Math.Max(rate, 0) * micros / MicrosPerS;
            Int128 total = value + Part(rateIn, split - at) + Part(rateOut, now - split);
            return total >= max ? max : (int)total;
        }

        public static int HpNow(int hp, int max, long at, long combatUntil, long now, bool dead) =>
            dead ? hp : Settle(hp, max, at, combatUntil, now, R.HpRegenIn, R.HpRegenOut);

        public static int MpNow(int mp, int max, long at, long combatUntil, long now, bool dead) =>
            dead ? mp : Settle(mp, max, at, combatUntil, now, R.MpRegenIn, R.MpRegenOut);

        public static long GcdMicros => R.GcdMs * MicrosPerMs;

        public static long CastMicros(SkillData s) => s.CastMs * MicrosPerMs;

        public static long CooldownMicros(SkillData s) => s.CooldownMs * MicrosPerMs;

        /// <summary>How long a projectile at <paramref name="speedMps"/> takes to cover <paramref name="distanceM"/>; 0 for one that lands at once.</summary>
        public static long ImpactMicros(float distanceM, float speedMps)
        {
            if (speedMps <= 0.0f)
                return 0;
            return (long)Math.Round((double)Math.Max(distanceM, 0.0f) / speedMps * 1e6, MidpointRounding.AwayFromZero);
        }

        /// <summary>The deaths that count toward the wait after a death at <paramref name="now"/>.</summary>
        public static uint DeathsAfter(uint deaths, long? lastDeathAt, long now)
        {
            long stacking = R.DeathStackS * MicrosPerS;
            return lastDeathAt is long last && now - last < stacking ? (deaths == uint.MaxValue ? deaths : deaths + 1) : 1;
        }

        /// <summary>How long after dying a character with <paramref name="deaths"/> counted deaths must wait to respawn.</summary>
        public static long RespawnWaitMicros(uint deaths)
        {
            long extra = (long)(deaths == 0 ? 0 : deaths - 1) * R.RespawnStepS;
            return Math.Min(R.RespawnFirstS + extra, R.RespawnMaxS) * MicrosPerS;
        }

        public static int RespawnShare(int max) => (int)((long)max * R.RespawnPct / 100);

        public static float Distance(Vector3 a, Vector3 b) => MathF.Sqrt(Distance2(a, b));

        private static float Distance2(Vector3 a, Vector3 b)
        {
            float dx = b.X - a.X, dy = b.Y - a.Y, dz = b.Z - a.Z;
            return dx * dx + dy * dy + dz * dz;
        }

        public static bool InRange(Vector3 from, Vector3 to, float rangeM) => Distance2(from, to) <= rangeM * rangeM;

        /// <summary>Someone at <paramref name="from"/> facing <paramref name="yaw"/> has <paramref name="to"/> within 90° either side, horizontally; close by always counts.</summary>
        public static bool InFront(float yaw, Vector3 from, Vector3 to)
        {
            float dx = to.X - from.X, dz = to.Z - from.Z;
            if (dx * dx + dz * dz < FrontExemptM * FrontExemptM)
                return true;
            var (fx, fz) = Yaw.Facing(yaw);
            return fx * dx + fz * dz >= 0.0f;
        }

        /// <summary>A caster that began at <paramref name="start"/> has moved far enough to interrupt its cast.</summary>
        public static bool MovedOff(Vector3 start, Vector3 at) => Distance2(start, at) > R.CastMoveM * R.CastMoveM;

        public static string Message(CombatRefusal r) => r switch
        {
            CombatRefusal.Dead => "you are dead",
            CombatRefusal.Casting => "already casting",
            CombatRefusal.NotReady => "not ready yet",
            CombatRefusal.NoMana => "not enough mana",
            CombatRefusal.Yourself => "can't target yourself",
            CombatRefusal.TargetDead => "the target is dead",
            CombatRefusal.OutOfRange => "out of range",
            CombatRefusal.NotInFront => "the target isn't in front of you",
            CombatRefusal.NoSight => "no line of sight",
            _ => "no target",
        };

        /// <summary>Everything about using <paramref name="skill"/> but line of sight, in the order players are told; null if it may be used.</summary>
        public static CombatRefusal? Check(SkillData skill, Caster c, CombatTarget t, long now)
        {
            if (!c.Alive)
                return CombatRefusal.Dead;
            if (c.Casting)
                return CombatRefusal.Casting;
            if (now < c.ReadyAt)
                return CombatRefusal.NotReady;
            if (c.Mp < skill.CostMp)
                return CombatRefusal.NoMana;
            if (skill.Target == SkillTarget.Enemy && t.IsCaster)
                return CombatRefusal.Yourself;
            if (!t.Alive)
                return CombatRefusal.TargetDead;
            if (t.IsCaster)
                return null;
            if (!InRange(c.At, t.At, skill.RangeM))
                return CombatRefusal.OutOfRange;
            if (skill.Facing && !InFront(c.Yaw, c.At, t.At))
                return CombatRefusal.NotInFront;
            return null;
        }

        /// <summary>A cast finishing: the target alive and within range plus the leeway.</summary>
        public static CombatRefusal? CheckFinish(SkillData skill, Vector3 from, CombatTarget t)
        {
            if (!t.Alive)
                return CombatRefusal.TargetDead;
            if (!t.IsCaster && !InRange(from, t.At, skill.RangeM + R.CastLeewayM))
                return CombatRefusal.OutOfRange;
            return null;
        }
    }
}
