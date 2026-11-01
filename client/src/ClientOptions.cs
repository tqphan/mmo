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
    /// <item><c>--vehicle-demo</c>: dev builds; summon a Furrow Cart near spawn, load a pack, climb aboard and drive it, unload and stow it, then summon a Driftpine Skiff off the west beach, row it and stow it (docs/design/vehicles.md).</item>
    /// <item><c>--shipyard-demo</c>: dev builds; place a Swiftgull Shipyard off the west beach, build it through every stage with dev packs, launch and summon the Swiftgull, load a pack, sink it with dev damage, take the floating pack from the water, mend the wreck by the dev clock and summon it again (docs/design/vehicles.md §2B).</item>
    /// <item><c>--trade-demo</c>: dev builds; on entering, teleport to the Packwright's Bench, craft a Grain Bale, walk a few steps with it, drop it and pick it up, then teleport to Gullhaven Post and sell it (docs/design/trade.md).</item>
    /// <item><c>--combat-demo</c>: dev builds; on entering, put a Straw Dummy ahead and fight it through the hotbar, have a cast interrupted, turn to face it, heal, then die and respawn at a Wayshrine (docs/design/combat.md).</item>
    /// <item><c>--crime-demo</c>: dev builds, with <c>bot visitor</c> nearby; hit the nearest other character for a Bloodstain and the Bloodmarked status, try to report our own stain (refused), then become Sought by <c>dev_set_crime</c> (docs/design/crime.md).</item>
    /// <item><c>--party-demo</c>: with <c>bot visitor</c> nearby; invite it to a party, then talk to it aloud, in the party and in a whisper, and see its answers (docs/design/social.md).</item>
    /// <item><c>--npc-demo</c>: dev builds; on entering, go to level 5, put a Greymane Wolf ahead, let it come and bite, fight it down through the hotbar and loot the corpse (docs/design/npcs.md).</item>
    /// <item><c>--quest-demo</c>: dev builds; on entering, walk to the nearest quest giver, take what it offers, cut down summoned Shore Skitters until Pinchers on the Strand is done, turn it in and open the log (docs/design/quests.md).</item>
    /// <item><c>--land-demo</c>: dev builds; on entering, teleport beside the nearest housing site, place a garden through the placement mode, pay its tax, make it public, then move the dev clock past its demolition (docs/design/land.md).</item>
    /// <item><c>--craft-demo</c>: dev builds; on entering, teleport to the Cookhearth, grant logs and ore, craft Charcoal, then a Rustiron Bar at the Bloomery (docs/design/crafting.md).</item>
    /// <item><c>--walk</c>: walk instead of run.</item>
    /// <item><c>--cam-yaw DEG</c>: where the camera starts, 0 looking north (-z).</item>
    /// <item><c>--cam-distance M</c>: how far behind the player the camera starts, 3 to 400 m (8; the mouse wheel changes it, up to 30).</item>
    /// <item><c>--cam-pitch DEG</c>: the camera's pitch, −89 (straight down) to 30 (−20).</item>
    /// <item><c>--teleport X,Z</c>: dev builds; a second after entering, go to (X, Z) by dev teleport.</item>
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
        /// <summary><c>--teleport X,Z</c> (dev builds): a second after entering, go there by dev teleport.</summary>
        public (float X, float Z)? Teleport;
        public bool BagDemo;
        public bool GatherDemo;
        public bool CraftDemo;
        public bool LandDemo;
        public bool FarmDemo;
        public bool TradeDemo;
        public bool VehicleDemo;
        public bool ShipyardDemo;
        public bool CombatDemo;
        public bool CrimeDemo;
        public bool PartyDemo;
        public bool NpcDemo;
        public bool QuestDemo;
        public bool Walk;
        public float CamYawDeg;
        public float CamDistance = 8.0f;
        public float CamPitchDeg = -20.0f;
        public bool Trace;
        public bool TestCheat;
        public string World = "world";
        public string? StyleTest;
        public bool Outline;
        public string Shot = "follow";
        public string? WorldTest;
        public string Route = "sail";
        /// <summary>World test: a fixed camera at X, Z, a height above the ground (or sea), yaw and pitch in degrees, instead of a route.</summary>
        public float[]? View;
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
                    case "--trade-demo": o.TradeDemo = true; break;
                    case "--vehicle-demo": o.VehicleDemo = true; break;
                    case "--shipyard-demo": o.ShipyardDemo = true; break;
                    case "--combat-demo": o.CombatDemo = true; break;
                    case "--crime-demo": o.CrimeDemo = true; break;
                    case "--party-demo": o.PartyDemo = true; break;
                    case "--npc-demo": o.NpcDemo = true; break;
                    case "--quest-demo": o.QuestDemo = true; break;
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
                    case "--view": o.View = Floats(Next(), 5, "X,Z,HEIGHT,YAW,PITCH"); break;
                    case "--no-vsync": o.NoVsync = true; break;
                    case "--secs": o.Secs = double.Parse(Next(), CultureInfo.InvariantCulture); break;
                    case "--cam-yaw": o.CamYawDeg = float.Parse(Next(), CultureInfo.InvariantCulture); break;
                    case "--cam-distance": o.CamDistance = Math.Clamp(float.Parse(Next(), CultureInfo.InvariantCulture), 3.0f, 400.0f); break;
                    case "--cam-pitch": o.CamPitchDeg = Math.Clamp(float.Parse(Next(), CultureInfo.InvariantCulture), -89.0f, 30.0f); break;
                    case "--teleport": o.Teleport = Pair(Next()); break;
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

        private static (float, float) Pair(string value)
        {
            var nums = value.Split(',');
            if (nums.Length != 2 || !nums.All(n => float.TryParse(n, NumberStyles.Float, CultureInfo.InvariantCulture, out _)))
                throw new ArgumentException($"{value}: expected X,Z");
            return (float.Parse(nums[0], CultureInfo.InvariantCulture), float.Parse(nums[1], CultureInfo.InvariantCulture));
        }

        private static (float, float, float) Triple(string value)
        {
            var nums = value.Split(',');
            if (nums.Length != 3 || !nums.All(n => float.TryParse(n, NumberStyles.Float, CultureInfo.InvariantCulture, out _)))
                throw new ArgumentException($"{value}: expected X,Z,YAW");
            float N(int i) => float.Parse(nums[i], CultureInfo.InvariantCulture);
            return (N(0), N(1), N(2));
        }

        private static float[] Floats(string value, int count, string expected)
        {
            var nums = value.Split(',');
            if (nums.Length != count || !nums.All(n => float.TryParse(n, NumberStyles.Float, CultureInfo.InvariantCulture, out _)))
                throw new ArgumentException($"{value}: expected {expected}");
            return nums.Select(n => float.Parse(n, CultureInfo.InvariantCulture)).ToArray();
        }

        private static string OneOf(string value, params string[] allowed) =>
            Array.IndexOf(allowed, value) >= 0 ? value : throw new ArgumentException($"{value} is not one of {string.Join(", ", allowed)}");
    }
}
