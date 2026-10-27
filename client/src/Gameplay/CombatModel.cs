using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Mmo.Content;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>A fighter's <c>vitals</c> row as the rules need it, in µs of game time.</summary>
    public readonly record struct VitalsInfo(int Hp, int HpMax, int Mp, int MpMax, long At, long CombatUntil, long? DeadAt, ushort CastSkill, long CastStart, long CastEnd)
    {
        public bool Dead => DeadAt is not null;

        public int HpNow(long now) => Sim.Combat.HpNow(Hp, HpMax, At, CombatUntil, now, Dead);

        public int MpNow(long now) => Sim.Combat.MpNow(Mp, MpMax, At, CombatUntil, now, Dead);

        public bool InCombat(long now) => now < CombatUntil;

        /// <summary>How far the cast under way has got, 0 to 1, or null with none.</summary>
        public float? CastProgress(long now) =>
            CastSkill == 0 || CastEnd <= CastStart ? null : Math.Clamp((float)(now - CastStart) / (CastEnd - CastStart), 0.0f, 1.0f);
    }

    /// <summary>Something Tab could pick: an entity with vitals, where it's drawn.</summary>
    public readonly record struct Candidate(ulong Id, Vector3 At, bool Alive);

    /// <summary>What pressing a hotbar key does: send it, turn to face the target first, or refuse and say why.</summary>
    public readonly record struct SkillPlan(CombatRefusal? Refusal, bool Turn)
    {
        public bool Send => Refusal is null;
    }

    /// <summary>
    /// The combat UI's rules (docs/design/combat.md), kept apart from Godot so they can be tested:
    /// which skill each key uses, whom it lands on, what the client refuses without asking, the
    /// order Tab cycles targets in, cooldowns as the hotbar shows them, and the respawn countdown.
    /// </summary>
    public static class CombatModel
    {
        /// <summary>Tab picks targets this close.</summary>
        public const float TabRangeM = 40.0f;

        /// <summary>Keys 1–9 use the skills in content order.</summary>
        public static IReadOnlyList<SkillData> Hotbar => GameContent.Skills;

        /// <summary>The skill on hotbar key <paramref name="key"/> (1 for the first), if any.</summary>
        public static SkillData? Slot(int key) => key >= 1 && key <= Hotbar.Count ? Hotbar[key - 1] : null;

        /// <summary>
        /// Whom a skill used on <paramref name="target"/> lands on, as the server decides: an ally
        /// skill on anything but a living character lands on us, and so does a caster skill.
        /// </summary>
        public static ulong Resolve(SkillData s, ulong me, ulong target, Func<ulong, bool> livingCharacter) => s.Target switch
        {
            SkillTarget.Enemy => target,
            SkillTarget.Ally => target != 0 && livingCharacter(target) ? target : me,
            _ => me,
        };

        /// <summary>When <paramref name="skill"/> can be used again: the later of the global cooldown and its own.</summary>
        public static long ReadyAt(ushort skill, long gcdUntil, IEnumerable<(ushort Skill, long ReadyAt)> cooldowns)
        {
            long own = cooldowns.Where(c => c.Skill == skill).Select(c => c.ReadyAt).DefaultIfEmpty(long.MinValue).Max();
            return Math.Max(gcdUntil, own);
        }

        /// <summary>
        /// What the hotbar shows over a skill: the share of its wait left (0 when ready) and the
        /// seconds left, rounded up. The wait is its own cooldown when that's running, else the global one.
        /// </summary>
        public static (float Fraction, int Secs) CooldownShown(SkillData s, long gcdUntil, IEnumerable<(ushort Skill, long ReadyAt)> cooldowns, long now)
        {
            long own = cooldowns.Where(c => c.Skill == s.Wire).Select(c => c.ReadyAt).DefaultIfEmpty(long.MinValue).Max();
            long ready = Math.Max(gcdUntil, own);
            if (ready <= now)
                return (0, 0);
            long total = own > now && own >= gcdUntil ? Sim.Combat.CooldownMicros(s) : Sim.Combat.GcdMicros;
            float fraction = total <= 0 ? 1 : Math.Clamp((float)(ready - now) / total, 0, 1);
            return (fraction, (int)((ready - now + 999_999) / 1_000_000));
        }

        /// <summary>
        /// What a hotbar key does. Everything <see cref="Sim.Combat.Check"/> refuses is refused here
        /// too, except a target that isn't in front: we turn to face it first.
        /// </summary>
        public static SkillPlan Plan(SkillData s, Caster c, CombatTarget t, long now) =>
            Sim.Combat.Check(s, c, t, now) switch
            {
                null => new SkillPlan(null, false),
                CombatRefusal.NotInFront => new SkillPlan(null, true),
                var r => new SkillPlan(r, false),
            };

        /// <summary>
        /// The order Tab cycles through: living others within <see cref="TabRangeM"/>, those in
        /// front of <paramref name="yaw"/> first, nearest first within each.
        /// </summary>
        public static IReadOnlyList<ulong> TabOrder(IEnumerable<Candidate> candidates, ulong me, Vector3 at, float yaw) =>
            candidates
                .Where(c => c.Id != me && c.Alive && Sim.Combat.InRange(at, c.At, TabRangeM))
                .OrderBy(c => Sim.Combat.InFront(yaw, at, c.At) ? 0 : 1)
                .ThenBy(c => Vector3.DistanceSquared(at, c.At))
                .ThenBy(c => c.Id)
                .Select(c => c.Id)
                .ToList();

        /// <summary>The target after <paramref name="current"/> in <paramref name="order"/>, wrapping; the first if it isn't there; 0 if there's none.</summary>
        public static ulong NextTarget(IReadOnlyList<ulong> order, ulong current)
        {
            if (order.Count == 0)
                return 0;
            int i = order.ToList().IndexOf(current);
            return order[(i + 1) % order.Count];
        }

        /// <summary>When a character dead since <paramref name="deadAt"/> with <paramref name="deaths"/> counted deaths may respawn.</summary>
        public static long RespawnAt(long deadAt, uint deaths) => deadAt + Sim.Combat.RespawnWaitMicros(deaths);

        /// <summary>The death panel's line: the countdown, then the key.</summary>
        public static string RespawnLine(long deadAt, uint deaths, long now)
        {
            long left = RespawnAt(deadAt, deaths) - now;
            if (left <= 0)
                return "R: respawn at the nearest Wayshrine";
            long secs = (left + 999_999) / 1_000_000;
            return $"respawn in {secs / 60}:{secs % 60:D2}";
        }

        /// <summary><c>740 / 1000</c>.</summary>
        public static string Amounts(int now, int max) => FormattableString.Invariant($"{now:N0} / {max:N0}");

        /// <summary>What a floating number says for a combat event.</summary>
        public static string Floating(int amount, ushort flags)
        {
            bool heal = (flags & EventFlags.Heal) != 0;
            string text = heal ? $"+{amount}" : $"{amount}";
            return (flags & EventFlags.Kill) != 0 ? text + " killed" : text;
        }
    }

    /// <summary><c>combat_event.flags</c> bits (server/world/src/combat.rs).</summary>
    public static class EventFlags
    {
        public const ushort Hit = 1;
        public const ushort Heal = 2;
        public const ushort Kill = 4;
    }
}
