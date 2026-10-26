using System;
using System.Globalization;
using System.Linq;

namespace Mmo
{
    /// <summary>
    /// Command-line options, given after <c>--</c>: <c>godot --path client -- --profile alpha</c>.
    /// <list type="bullet">
    /// <item><c>--uri URI</c>, <c>--db NAME</c>: the server (default the local dev server).</item>
    /// <item><c>--profile NAME</c>: keeps its own identity token, and names the character it makes.</item>
    /// <item><c>--autopilot circle:X,Z,R | pace:X,Z,LEN | none</c>: run a scripted route instead of reading the keyboard.</item>
    /// <item><c>--walk</c>: walk instead of run.</item>
    /// <item><c>--cam-yaw DEG</c>: where the camera starts, 0 looking north (-z).</item>
    /// <item><c>--trace</c>: log where each other entity is drawn every frame, to measure smoothness.</item>
    /// <item><c>--test-cheat</c>: 3 s after entering, jump 30 m in one frame, to check that the server's correction snaps us back.</item>
    /// <item><c>--style-test flat|gradient</c>: show the offline art style test scene instead of connecting (docs/design/art.md).</item>
    /// <item><c>--outline</c>: in the style test, outline the characters.</item>
    /// <item><c>--shot follow|wide</c>: in the style test, orbit the walking character or the whole island.</item>
    /// </list>
    /// </summary>
    public sealed class ClientOptions
    {
        public string Uri = "http://127.0.0.1:3000";
        public string Db = "mmo-world";
        public string Profile = "player";
        public Gameplay.Autopilot? Autopilot;
        public bool Walk;
        public float CamYawDeg;
        public bool Trace;
        public bool TestCheat;
        public string? StyleTest;
        public bool Outline;
        public string Shot = "follow";

        /// <summary>The profile as a legal character name: 3–16 letters, capitalised.</summary>
        public string CharacterName
        {
            get
            {
                var letters = new string(Profile.Where(char.IsAsciiLetter).ToArray()).ToLowerInvariant();
                if (letters.Length < 3)
                    letters = (letters + "player")[..Math.Max(3, letters.Length)];
                letters = letters.Length > 16 ? letters[..16] : letters;
                return char.ToUpperInvariant(letters[0]) + letters[1..];
            }
        }

        public static ClientOptions Parse(string[] args)
        {
            var o = new ClientOptions();
            for (int i = 0; i < args.Length; i++)
            {
                string Next() => i + 1 < args.Length ? args[++i] : throw new ArgumentException($"{args[i]} needs a value");
                switch (args[i])
                {
                    case "--uri": o.Uri = Next(); break;
                    case "--db": o.Db = Next(); break;
                    case "--profile": o.Profile = Next(); break;
                    case "--autopilot": o.Autopilot = Gameplay.Autopilot.Parse(Next()); break;
                    case "--walk": o.Walk = true; break;
                    case "--trace": o.Trace = true; break;
                    case "--test-cheat": o.TestCheat = true; break;
                    case "--style-test": o.StyleTest = OneOf(Next(), "flat", "gradient"); break;
                    case "--outline": o.Outline = true; break;
                    case "--shot": o.Shot = OneOf(Next(), "follow", "wide"); break;
                    case "--cam-yaw": o.CamYawDeg = float.Parse(Next(), CultureInfo.InvariantCulture); break;
                    default: throw new ArgumentException($"unknown option {args[i]}");
                }
            }
            return o;
        }

        private static string OneOf(string value, params string[] allowed) =>
            Array.IndexOf(allowed, value) >= 0 ? value : throw new ArgumentException($"{value} is not one of {string.Join(", ", allowed)}");
    }
}
