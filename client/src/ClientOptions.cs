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
    /// <item><c>--autopilot circle:X,Z,R | pace:X,Z,LEN | deck:X,Z,S | helm:X,Z,R | gun:X,Z,M | none</c>: run a scripted route instead of reading the keyboard (<see cref="Gameplay.Autopilot"/>).</item>
    /// <item><c>--spawn-ship X,Z,YAW</c>: dev builds; once within 60 m of (X, Z), replace any ship within 30 m of it with a fresh clipper facing YAW degrees.</item>
    /// <item><c>--labor N</c>: dev builds; on entering, set our account's labor pool to N (docs/design/labor.md). F8 spends 100.</item>
    /// <item><c>--bag-demo</c>: dev builds; on entering, empty the bag, grant some items and work through a move, a merge, a split, a swap and a destroy with the bag window open (docs/design/items.md).</item>
    /// <item><c>--gather-demo</c>: dev builds; on entering, teleport beside the nearest ready plant and gather it, then the nearest tree (docs/design/gathering.md).</item>
    /// <item><c>--farm-demo</c>: dev builds; on entering, place a farm on the nearest housing site, plant a row of Hearthgrain, a Woolback and an Amberpear Tree on it and Hearthgrain beside it, ripen them with the dev clock, harvest, then let the crop beside it wither (docs/design/farming.md).</item>
    /// <item><c>--land-demo</c>: dev builds; on entering, teleport beside the nearest housing site, place a garden through the placement mode, pay its tax, make it public, then move the dev clock past its demolition (docs/design/land.md).</item>
    /// <item><c>--craft-demo</c>: dev builds; on entering, teleport to the Cookhearth, grant logs and ore, craft Charcoal, then a Rustiron Bar at the Bloomery (docs/design/crafting.md).</item>
    /// <item><c>--walk</c>: walk instead of run.</item>
    /// <item><c>--cam-yaw DEG</c>: where the camera starts, 0 looking north (-z).</item>
    /// <item><c>--trace</c>: log where each other entity is drawn every frame, to measure smoothness.</item>
    /// <item><c>--test-cheat</c>: 3 s after entering, jump 30 m in one frame, to check that the server's correction snaps us back.</item>
    /// <item><c>--world DIR</c>: the world's files (default <c>world</c>), the same ones uploaded to the server (docs/design/world.md); or a bare <c>.mmoh</c> height file.</item>
    /// <item><c>--style-test flat|gradient</c>: show the offline art style test scene instead of connecting (docs/design/art.md).</item>
    /// <item><c>--outline</c>: in the style test, outline the characters.</item>
    /// <item><c>--shot follow|wide</c>: in the style test, orbit the walking character or the whole island.</item>
    /// <item><c>--world-test DIR</c>: show the offline world-scale test with this world (a directory or a <c>.mmoh</c> file) instead of connecting (docs/design/worldstream.md).</item>
    /// <item><c>--route sail|cross|walk|high</c>: the world test's camera route.</item>
    /// <item><c>--no-vsync</c>: in the world test, render as fast as possible, to measure frame times.</item>
    /// <item><c>--secs S</c>: quit after S seconds.</item>
    /// </list>
    /// </summary>
    public sealed class ClientOptions
    {
        public string Uri = "http://127.0.0.1:3000";
        public string Db = "mmo-world";
        public string Profile = "player";
        public Gameplay.Autopilot? Autopilot;
        public (float X, float Z, float YawDeg)? SpawnShip;
        public int? Labor;
        public bool BagDemo;
        public bool GatherDemo;
        public bool CraftDemo;
        public bool LandDemo;
        public bool FarmDemo;
        public bool Walk;
        public float CamYawDeg;
        public bool Trace;
        public bool TestCheat;
        public string World = "world";
        public string? StyleTest;
        public bool Outline;
        public string Shot = "follow";
        public string? WorldTest;
        public string Route = "sail";
        public bool NoVsync;
        public double Secs;

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
                    case "--bag-demo": o.BagDemo = true; break;
                    case "--gather-demo": o.GatherDemo = true; break;
                    case "--craft-demo": o.CraftDemo = true; break;
                    case "--land-demo": o.LandDemo = true; break;
                    case "--farm-demo": o.FarmDemo = true; break;
                    case "--labor": o.Labor = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                    case "--spawn-ship": o.SpawnShip = Triple(Next()); break;
                    case "--trace": o.Trace = true; break;
                    case "--test-cheat": o.TestCheat = true; break;
                    case "--world": o.World = Next(); break;
                    case "--style-test": o.StyleTest = OneOf(Next(), "flat", "gradient"); break;
                    case "--outline": o.Outline = true; break;
                    case "--shot": o.Shot = OneOf(Next(), "follow", "wide"); break;
                    case "--world-test": o.WorldTest = Next(); break;
                    case "--route": o.Route = OneOf(Next(), "sail", "cross", "walk", "high"); break;
                    case "--no-vsync": o.NoVsync = true; break;
                    case "--secs": o.Secs = double.Parse(Next(), CultureInfo.InvariantCulture); break;
                    case "--cam-yaw": o.CamYawDeg = float.Parse(Next(), CultureInfo.InvariantCulture); break;
                    default: throw new ArgumentException($"unknown option {args[i]}");
                }
            }
            return o;
        }

        /// <summary>A path from the command line: relative paths are from the repo root, where cargo xtask runs.</summary>
        public static string RepoPath(string path) =>
            System.IO.Path.IsPathRooted(path)
                ? path
                : System.IO.Path.GetFullPath(System.IO.Path.Combine(Godot.ProjectSettings.GlobalizePath("res://"), "..", path));

        private static (float, float, float) Triple(string value)
        {
            var nums = value.Split(',');
            if (nums.Length != 3 || !nums.All(n => float.TryParse(n, NumberStyles.Float, CultureInfo.InvariantCulture, out _)))
                throw new ArgumentException($"{value}: expected X,Z,YAW");
            float N(int i) => float.Parse(nums[i], CultureInfo.InvariantCulture);
            return (N(0), N(1), N(2));
        }

        private static string OneOf(string value, params string[] allowed) =>
            Array.IndexOf(allowed, value) >= 0 ? value : throw new ArgumentException($"{value} is not one of {string.Join(", ", allowed)}");
    }
}
