using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;

namespace Mmo.World
{
    /// <summary>A named area: a circle on the map (docs/design/coast.md).</summary>
    public sealed record Area(string Name, float X, float Z, float Radius);

    /// <summary>
    /// The world's named areas (<c>world/areas.tsv</c>, <c>worldtool::areas</c>): lines of
    /// <c>name x z radius</c>, tab-separated, with <c># comments</c>. The smallest area round a point
    /// names it.
    /// </summary>
    public static class Areas
    {
        public const string File = "areas.tsv";

        public static IReadOnlyList<Area> Parse(string text)
        {
            var list = new List<Area>();
            var lines = text.Split('\n');
            for (int i = 0; i < lines.Length; i++)
            {
                var line = lines[i].TrimEnd('\r');
                if (line.Trim().Length == 0 || line.StartsWith('#'))
                    continue;
                var f = line.Split('\t');
                if (f.Length != 4 || f[0].Trim().Length == 0)
                    throw new InvalidDataException($"{File} line {i + 1}: want a name, x, z and radius");
                float Num(string s) => float.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture, out var v) && float.IsFinite(v)
                    ? v
                    : throw new InvalidDataException($"{File} line {i + 1}: bad number {s}");
                var a = new Area(f[0], Num(f[1]), Num(f[2]), Num(f[3]));
                if (a.Radius <= 0)
                    throw new InvalidDataException($"{File} line {i + 1}: bad radius {a.Radius}");
                list.Add(a);
            }
            return list;
        }

        /// <summary>The world directory's areas, or none if it has no file.</summary>
        public static IReadOnlyList<Area> Load(string dir)
        {
            var path = Path.Combine(dir, File);
            return System.IO.File.Exists(path) ? Parse(System.IO.File.ReadAllText(path)) : Array.Empty<Area>();
        }

        /// <summary>
        /// Where (x, z) is: the smallest area round it, then the largest if that's another, as
        /// "Croaker Fen, Hearthshore"; empty outside every area.
        /// </summary>
        public static string Describe(IReadOnlyList<Area> areas, float x, float z)
        {
            var round = areas.Where(a => MathF.Sqrt((a.X - x) * (a.X - x) + (a.Z - z) * (a.Z - z)) <= a.Radius).OrderBy(a => a.Radius).ToList();
            if (round.Count == 0)
                return "";
            return round.Count == 1 ? round[0].Name : $"{round[0].Name}, {round[^1].Name}";
        }
    }
}
