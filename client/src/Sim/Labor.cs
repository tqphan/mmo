using System;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>
    /// A labor pool as stored: <see cref="Points"/> as of <see cref="At"/>, in µs since the Unix
    /// epoch (docs/design/labor.md). A port of <c>sim::labor</c>, checked against generated vectors;
    /// the client only uses it to show the pool, and the server's value is the one that counts.
    /// </summary>
    public readonly record struct LaborPool(int Points, long At)
    {
        private const long MicrosPerS = 1_000_000;

        /// <summary>
        /// The pool as of <paramref name="now"/>: every whole period since <see cref="At"/> adds the
        /// regen points, up to the cap. Below the cap, progress toward the next points is kept; at the
        /// cap the pool is anchored at <paramref name="now"/>.
        /// </summary>
        public LaborPool Settle(LaborRules rules, long now)
        {
            if (Points >= rules.Cap)
                return new LaborPool(Points, now);
            long period = rules.RegenPeriodS * MicrosPerS;
            long periods = Math.Max(SaturatingSub(now, At), 0) / period;
            long points = Points + SaturatingMul(periods, rules.RegenPoints);
            return points >= rules.Cap ? new LaborPool(rules.Cap, now) : new LaborPool((int)points, At + periods * period);
        }

        public int Current(LaborRules rules, long now) => Settle(rules, now).Points;

        /// <summary>When the next points arrive, or null while the pool is full.</summary>
        public long? NextRegenAt(LaborRules rules, long now)
        {
            var s = Settle(rules, now);
            return s.Points < rules.Cap ? s.At + rules.RegenPeriodS * MicrosPerS : null;
        }

        /// <summary>Settle, then take <paramref name="cost"/> off; null (with what's there) if it can't.</summary>
        public LaborPool? Spend(LaborRules rules, long now, int cost, out int have)
        {
            var s = Settle(rules, now);
            have = s.Points;
            if (cost <= 0 || cost > s.Points)
                return null;
            return new LaborPool(s.Points - cost, s.At);
        }

        private static long SaturatingSub(long a, long b)
        {
            long d = unchecked(a - b);
            // Overflow when the operands' signs differ and the result's sign differs from a's.
            return ((a ^ b) & (a ^ d)) < 0 ? (a < 0 ? long.MinValue : long.MaxValue) : d;
        }

        private static long SaturatingMul(long a, long b)
        {
            try
            {
                return checked(a * b);
            }
            catch (OverflowException)
            {
                return (a < 0) == (b < 0) ? long.MaxValue : long.MinValue;
            }
        }
    }
}
