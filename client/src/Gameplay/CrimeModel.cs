using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>An <c>evidence</c> row as the rules need it.</summary>
    public readonly record struct EvidenceInfo(ulong Id, EvidenceKind Kind, Vector3 At, long MadeAt);

    /// <summary>
    /// The crime UI's rules (docs/design/crime.md), kept apart from Godot so they can be tested:
    /// which evidence J reports, the prompt, and the HUD's record line.
    /// </summary>
    public static class CrimeModel
    {
        /// <summary>The nearest unfaded evidence in reach of <paramref name="at"/>, if any.</summary>
        public static EvidenceInfo? Nearest(IEnumerable<EvidenceInfo> all, Vector3 at, long now) =>
            all.Where(e => !Sim.Crime.Faded(e.MadeAt, now) && Sim.Crime.InReach(e.At, at))
                .OrderBy(e => Vector2.Distance(new Vector2(e.At.X, e.At.Z), new Vector2(at.X, at.Z)))
                .Select(e => (EvidenceInfo?)e)
                .FirstOrDefault();

        /// <summary><c>J: report Footprints</c>.</summary>
        public static string Prompt(EvidenceInfo e) => $"J: report {Sim.Crime.Name(e.Kind)}";

        /// <summary><c>crime 11 · notoriety 11</c>, with <c>· Sought</c> when it is; empty with no record.</summary>
        public static string RecordLine(uint? crimePoints, uint notoriety)
        {
            if (crimePoints is not uint points)
                return "";
            var line = $"crime {points:N0} · notoriety {notoriety:N0}";
            return Sim.Crime.Sought(points) ? line + " · Sought" : line;
        }

        /// <summary>A name tag or frame's name, with <c>Sought</c> under or after it.</summary>
        public static string Tagged(string name, bool sought, string separator = "\n") => sought ? $"{name}{separator}Sought" : name;
    }
}
