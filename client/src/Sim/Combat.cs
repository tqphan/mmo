using System;
using System.Collections.Generic;
using System.Linq;
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
        Controlled = 11,
        Unlearned = 12,
    }

    /// <summary>The caster as a skill check sees it: its mana now, and when the later of the global cooldown and the skill's cooldown ends.</summary>
    public readonly record struct Caster(Vector3 At, float Yaw, bool Alive, bool Controlled, bool Knows, bool Casting, int Mp, long ReadyAt);

    /// <summary>A status on a fighter, from <c>Start</c> until <c>Until</c>, µs.</summary>
    public readonly record struct Slot(byte Status, long Start, long Until)
    {
        public bool Active(long now) => Start <= now && now < Until;

        public StatusData? Data => GameContent.Status(Status);
    }

    /// <summary>How many controls of a diminishing-returns group landed in a row, and when its window runs out, µs.</summary>
    public readonly record struct ResistWindow(byte Group, byte Count, long Until);

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
            CombatRefusal.Controlled => "you can't act right now",
            CombatRefusal.Unlearned => "you haven't learned that skill",
            _ => "no target",
        };

        /// <summary>Everything about using <paramref name="skill"/> but line of sight, in the order players are told; null if it may be used.</summary>
        public static CombatRefusal? Check(SkillData skill, Caster c, CombatTarget t, long now)
        {
            if (!c.Alive)
                return CombatRefusal.Dead;
            if (c.Controlled)
                return CombatRefusal.Controlled;
            if (!c.Knows)
                return CombatRefusal.Unlearned;
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

        private static ProgressRules P => GameContent.Progress;

        public static byte LevelMax => (byte)(P.XpToNext.Count + 1);

        private static int Grown(byte level) => Math.Clamp((int)level, 1, LevelMax) - 1;

        public static int HpMaxAt(byte level) => R.HpMax + P.HpPerLevel * Grown(level);

        public static int MpMaxAt(byte level) => R.MpMax + P.MpPerLevel * Grown(level);

        /// <summary>The experience from <paramref name="level"/> to the next; 0 at the cap.</summary>
        public static uint XpToNext(byte level) => Math.Max((int)level, 1) - 1 < P.XpToNext.Count ? P.XpToNext[Math.Max((int)level, 1) - 1] : 0;

        /// <summary>How many skillsets a character at <paramref name="level"/> may have picked.</summary>
        public static int SkillsetSlots(byte level) => P.SkillsetAt.Count(l => l <= level);

        public static bool Knows(SkillData s, byte level, IEnumerable<byte> picks) => level >= s.Level && picks.Contains(s.Skillset);

        /// <summary>Experience for a character at <paramref name="killer"/> killing a foe at <paramref name="victim"/> that isn't a character.</summary>
        public static uint KillXp(byte victim, byte killer)
        {
            ulong b = P.KillXp + (ulong)P.KillXpPerLevel * (ulong)(Math.Max((int)victim, 1) - 1);
            ulong gap = (ulong)Math.Max(killer - victim, 0);
            ulong pct = 100 - Math.Min(100, gap * P.KillXpGapPct);
            return (uint)Math.Min(b * pct / 100, uint.MaxValue);
        }

        /// <summary>Level and experience into it after gaining <paramref name="gained"/>; experience stops at the cap.</summary>
        public static (byte Level, uint Xp) GainXp(byte level, uint xp, uint gained)
        {
            byte l = (byte)Math.Clamp((int)level, 1, LevelMax);
            ulong x = (ulong)xp + gained;
            while (l < LevelMax && x >= XpToNext(l))
            {
                x -= XpToNext(l);
                l++;
            }
            if (l == LevelMax)
                x = 0;
            return (l, (uint)Math.Min(x, uint.MaxValue));
        }

        /// <summary>Experience into the level after a death to anything but another character.</summary>
        public static uint XpAfterDeath(byte level, uint xp)
        {
            ulong loss = (ulong)XpToNext(level) * P.DeathXpPct / 100;
            return loss >= xp ? 0 : (uint)(xp - loss);
        }

        public static bool Has(IEnumerable<Slot> slots, byte status, long now) => slots.Any(s => s.Status == status && s.Active(now));

        /// <summary>Stunned or knocked down at <paramref name="now"/>.</summary>
        public static bool Controlled(IEnumerable<Slot> slots, long now) =>
            slots.Any(s => s.Active(now) && s.Data is { Control: CrowdControl.Stun or CrowdControl.Knockdown });

        /// <summary>
        /// The speed limit's factor for a move over <paramref name="from"/> to <paramref name="to"/>: 0 stunned or knocked down,
        /// a snare's share snared, counting only controls that held before the move began and still hold at its end.
        /// </summary>
        public static float MoveScale(IEnumerable<Slot> slots, long from, long to)
        {
            float scale = 1.0f;
            foreach (var s in slots)
                if (s.Start < from && s.Until >= to && s.Data is { } d)
                    scale = Math.Min(scale, SpeedFactor(d));
            return scale;
        }

        /// <summary>A status's factor on the speed limit: 0 for a stun or knockdown, a snare's share, 1 otherwise.</summary>
        public static float SpeedFactor(StatusData d) => d.Control switch
        {
            CrowdControl.Stun or CrowdControl.Knockdown => 0.0f,
            CrowdControl.Snare => d.SpeedPct / 100.0f,
            _ => 1.0f,
        };

        /// <summary>The sum of the active statuses' dealt (or, with <paramref name="taken"/>, taken) percentages.</summary>
        public static int Modifier(IEnumerable<Slot> slots, long now, bool taken) =>
            slots.Where(s => s.Active(now)).Select(s => s.Data).Where(d => d is not null).Sum(d => taken ? d!.TakenPct : d!.DealtPct);

        public static int ScaledDamage(int amount, int dealtPct, int takenPct)
        {
            static long F(int pct) => Math.Max(100 + pct, 0);
            return (int)Math.Min((long)Math.Max(amount, 0) * F(dealtPct) * F(takenPct) / 10_000, int.MaxValue);
        }

        public static byte? ControlGroup(CrowdControl c) => c switch
        {
            CrowdControl.Stun or CrowdControl.Knockdown => 0,
            CrowdControl.Snare => 1,
            _ => null,
        };

        /// <summary>The share of its duration, in percent, a control in <paramref name="group"/> lands for; null when immune.</summary>
        public static uint? Diminished(IEnumerable<ResistWindow> resists, byte group, long now)
        {
            int landed = resists.Where(r => r.Group == group && now < r.Until).Select(r => (int)r.Count).DefaultIfEmpty(0).First();
            var steps = GameContent.Diminishing.StepsPct;
            return landed < steps.Count ? steps[landed] : null;
        }

        /// <summary>The resists after a control in <paramref name="group"/> landed at <paramref name="now"/> for <paramref name="lasts"/> µs.</summary>
        public static List<ResistWindow> Resisted(IEnumerable<ResistWindow> resists, byte group, long now, long lasts)
        {
            var all = resists.ToList();
            byte count = all.Where(r => r.Group == group && now < r.Until).Select(r => r.Count).DefaultIfEmpty((byte)0).First();
            var o = all.Where(r => r.Group != group && now < r.Until).ToList();
            o.Add(new ResistWindow(group, count == byte.MaxValue ? count : (byte)(count + 1), now + lasts + GameContent.Diminishing.WindowS * MicrosPerS));
            return o;
        }

        public static long ControlMicros(uint ms, uint pct) => (long)ms * MicrosPerMs * pct / 100;

        /// <summary>When a status that began at <paramref name="start"/> next ticks after <paramref name="last"/>, if by <paramref name="until"/>.</summary>
        public static long? NextTick(long start, long until, uint tickMs, long last)
        {
            if (tickMs == 0)
                return null;
            long every = tickMs * MicrosPerMs;
            long n = Math.Max(last - start, 0) / every + 1;
            long due = start + n * every;
            return due <= until ? due : null;
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
