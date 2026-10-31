using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>What a piece of evidence is. The value is the wire value in <c>evidence.kind</c>.</summary>
    public enum EvidenceKind : byte
    {
        Footprints = 0,
        Bloodstain = 1,
        BloodPool = 2,
    }

    /// <summary>
    /// Crime's rules (docs/design/crime.md): evidence names and points, when it fades, the reach to
    /// report it, and the Sought threshold. A port of <c>sim::crime</c>; the server decides.
    /// </summary>
    public static class Crime
    {
        private const long MicrosPerS = 1_000_000;

        private static CrimeRules R => GameContent.Crime;

        /// <summary>What players see.</summary>
        public static string Name(EvidenceKind kind) => kind switch
        {
            EvidenceKind.Footprints => "Footprints",
            EvidenceKind.Bloodstain => "Bloodstain",
            _ => "Blood Pool",
        };

        /// <summary>The crime points (and Notoriety) a report of it adds.</summary>
        public static int Points(EvidenceKind kind) => kind switch
        {
            EvidenceKind.Footprints => R.FootprintPoints,
            EvidenceKind.Bloodstain => R.BloodstainPoints,
            _ => R.BloodPoolPoints,
        };

        public static long FadesAt(long madeAt) => madeAt + R.EvidenceS * MicrosPerS;

        public static bool Faded(long madeAt, long now) => now >= FadesAt(madeAt);

        /// <summary>A character at <paramref name="at"/> (its feet) can report evidence at <paramref name="thing"/>.</summary>
        public static bool InReach(Vector3 thing, Vector3 at)
        {
            float dx = at.X - thing.X, dz = at.Z - thing.Z;
            float r = R.ReachM;
            return dx * dx + dz * dz <= r * r && System.MathF.Abs(at.Y - thing.Y) <= r;
        }

        public static bool Sought(uint crimePoints) => crimePoints >= R.SoughtPoints;
    }
}
