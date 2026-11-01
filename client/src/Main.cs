using System;
using Godot;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.UI;
using Mmo.World;

namespace Mmo
{
    /// <summary>
    /// The composition root: reads the command line (<see cref="ClientOptions"/>), then builds the
    /// session, the world's terrain and sea, the entity registry, the player, the camera and the HUD.
    /// </summary>
    public partial class Main : Node3D
    {
        public override void _Ready()
        {
            ClientOptions options;
            try
            {
                options = ClientOptions.Parse(OS.GetCmdlineUserArgs());
            }
            catch (ArgumentException e)
            {
                GD.PushError(e.Message);
                GetTree().Quit(2);
                return;
            }
            AddInputActions();
            if (options.StyleTest != null)
            {
                GetWindow().Title = $"mmo — style test ({options.StyleTest})";
                AddChild(new Presentation.StyleTest(options));
                return;
            }
            if (options.WorldTest != null)
            {
                GetWindow().Title = $"mmo — world test ({options.Route})";
                AddChild(new Presentation.WorldTest(options));
                return;
            }
            GetWindow().Title = $"mmo — {options.Profile}";

            if (LoadWorld(options.World) is not { } world)
            {
                GetTree().Quit(2);
                return;
            }
            var map = world.Heights;

            var session = new Session(options, world.TerrainHash);
            AddChild(session);
            var terrain = new Presentation.Terrain(map, world.Land);
            AddChild(terrain);
            var water = Presentation.Water.Create(faceted: false, new Presentation.Palette(), horizon: 80000.0f);
            AddChild(water);
            AddChild(new DirectionalLight3D
            {
                Name = "Sun",
                RotationDegrees = new Vector3(-42, -35, 0),
                LightColor = new Color(1.0f, 0.95f, 0.86f),
                LightEnergy = 1.15f,
                ShadowEnabled = true,
                DirectionalShadowMaxDistance = 200,
            });
            AddChild(new WorldEnvironment { Environment = Presentation.StyleTest.Environment(fog: 0.00012f) });
            var entities = new EntityRegistry(session, options.Trace, map);
            AddChild(entities);
            var ground = new TerrainCollision(map);
            AddChild(ground);
            var player = new PlayerController(session, options, ground, entities);
            AddChild(player);
            var controls = new ShipControls(session, player, entities, map, options);
            AddChild(controls);
            player.Controls = controls;
            var nodes = new NodeRegistry(session);
            AddChild(nodes);
            var gathering = new Gathering(session, player, nodes);
            AddChild(gathering);
            player.Gathering = gathering;
            var stations = new StationRegistry(session);
            AddChild(stations);
            var crafting = new Crafting(session, player, stations);
            AddChild(crafting);
            player.Crafting = crafting;
            var craftWindow = new CraftWindow(session, crafting);
            AddChild(craftWindow);
            crafting.Window = craftWindow;
            AddChild(new GatherBar(gathering, crafting));
            var plotRegistry = new PlotRegistry(session, map);
            AddChild(plotRegistry);
            var placement = new Placement(session, player, map, world.Land, plotRegistry);
            AddChild(placement);
            player.Placement = placement;
            var crops = new CropRegistry(session, plotRegistry);
            AddChild(crops);
            var farming = new Farming(session, player, map, world.Land, plotRegistry, crops);
            AddChild(farming);
            player.Farming = farming;
            var vehicles = new Vehicles(session, player, entities);
            AddChild(vehicles);
            player.Vehicles = vehicles;
            var yardRegistry = new ShipyardRegistry(session);
            AddChild(yardRegistry);
            var shipyards = new Shipyards(session, player, yardRegistry);
            AddChild(shipyards);
            player.Shipyards = shipyards;
            var tradeRegistry = new TradeRegistry(session);
            AddChild(tradeRegistry);
            var trade = new Trade(session, player, tradeRegistry);
            AddChild(trade);
            player.Trade = trade;
            AddChild(new ShrineRegistry(session));
            var combat = new Combat(session, player, entities);
            AddChild(combat);
            player.Combat = combat;
            var givers = new GiverRegistry(session);
            AddChild(givers);
            var quests = new Quests(session, player, givers, combat);
            AddChild(quests);
            player.Quests = quests;
            var questWindow = new QuestWindow(session, quests);
            AddChild(questWindow);
            quests.Window = questWindow;
            AddChild(new CombatHud(combat));
            var evidence = new EvidenceRegistry(session);
            AddChild(evidence);
            var crime = new Crime(session, player, evidence);
            AddChild(crime);
            var party = new Party(session);
            AddChild(party);
            entities.IsMate = party.IsMate;
            AddChild(new PartyFrame(session, party, combat));
            var chat = new ChatBox(session, player, party);
            AddChild(chat);
            var looting = new Looting(session, player, entities);
            AddChild(looting);
            player.Looting = looting;
            var plots = new Plots(session, player, plotRegistry);
            AddChild(plots);
            player.Plots = plots;
            var plotWindow = new PlotWindow(session, plots);
            AddChild(plotWindow);
            plots.Window = plotWindow;
            var camera = new FollowCamera(player, options.CamYawDeg, map, options.CamDistance, options.CamPitchDeg);
            AddChild(camera);
            var hud = new Hud(session, player, entities, world.Land, world.Areas, controls, gathering, crafting);
            hud.Prompts.Add(() => placement.Prompt);
            hud.Prompts.Add(() => quests.Prompt);
            hud.Prompts.Add(() => farming.Prompt);
            hud.Prompts.Add(() => looting.Prompt);
            hud.Prompts.Add(() => crime.RecordLine);
            hud.Prompts.Add(() => crime.Prompt);
            hud.Prompts.Add(() => vehicles.Prompt);
            hud.Prompts.Add(() => shipyards.Prompt);
            hud.Prompts.Add(() => trade.Status);
            hud.Prompts.Add(() => trade.Prompt);
            hud.Prompts.Add(() => placement.Active is null && farming.Active is null ? plots.Prompt : "");
            AddChild(hud);
            // Right-clicking a stack uses it: deeds place plots, seeds plant, vehicles are summoned
            // (or mended, if wrecked), designs place shipyards.
            bool Use(BagStack s)
            {
                if (vehicles.Begin(s) || shipyards.Begin(s))
                {
                    placement.Cancel();
                    farming.Cancel();
                }
                else if (placement.Begin(s))
                    farming.Cancel();
                else if (farming.Begin(s))
                    placement.Cancel();
                else
                    return false;
                return true;
            }
            var bag = new BagWindow(session) { Use = Use, Note = vehicles.Note };
            AddChild(bag);
            if (options.BagDemo)
                AddChild(new BagDemo(session, bag));
            if (options.GatherDemo)
                AddChild(new GatherDemo(session, player, nodes, gathering));
            if (options.CraftDemo)
                AddChild(new CraftDemo(session, player, stations, crafting, craftWindow));
            if (options.LandDemo)
                AddChild(new LandDemo(session, player, map, world.Land, bag, placement, plots, plotWindow));
            if (options.FarmDemo)
                AddChild(new FarmDemo(session, player, map, world.Land, bag, placement, plotRegistry, farming));
            if (options.TradeDemo)
                AddChild(new TradeDemo(session, player, stations, crafting, craftWindow, tradeRegistry, trade));
            if (options.VehicleDemo)
                AddChild(new VehicleDemo(session, player, entities, controls, vehicles, map));
            if (options.ShipyardDemo)
                AddChild(new ShipyardDemo(session, player, entities, vehicles, shipyards, yardRegistry, trade, tradeRegistry, map));
            if (options.CombatDemo)
                AddChild(new CombatDemo(session, player, entities, combat));
            if (options.NpcDemo)
                AddChild(new NpcDemo(session, player, entities, combat, looting));
            if (options.CrimeDemo)
                AddChild(new CrimeDemo(session, player, entities, combat, crime, evidence));
            if (options.QuestDemo)
                AddChild(new QuestDemo(session, player, entities, combat, quests, questWindow));
            if (options.PartyDemo)
                AddChild(new PartyDemo(session, player, entities, party, chat));
            // Last, so it sees where the camera went this frame.
            AddChild(new Scenery(camera, terrain, water));
            if (OS.HasFeature("movie"))
                AddChild(new RealTime());
            if (options.Secs > 0)
                GetTree().CreateTimer(options.Secs).Timeout += () => GetTree().Quit();
        }

        /// <summary>The world's files, or a bare height file (docs/design/world.md); null, with the reason logged, if they can't be read.</summary>
        public static WorldData? LoadWorld(string option)
        {
            string path = ClientOptions.RepoPath(option);
            var clock = System.Diagnostics.Stopwatch.StartNew();
            try
            {
                var world = WorldFiles.Load(path);
                GD.Print($"world: {path}, {world.Heights.Width} × {world.Heights.Depth} samples, terrain hash {world.TerrainHash:x16}, read in {clock.ElapsedMilliseconds} ms");
                return world;
            }
            catch (Exception e) when (e is System.IO.IOException or System.IO.InvalidDataException or UnauthorizedAccessException)
            {
                GD.PushError($"world: can't read {path}: {e.Message}");
                return null;
            }
        }

        /// <summary>Keeps the terrain's levels of detail and the sea centred on the camera.</summary>
        private partial class Scenery : Node
        {
            private readonly Camera3D camera;
            private readonly Presentation.Terrain terrain;
            private readonly Node3D water;

            public Scenery(Camera3D camera, Presentation.Terrain terrain, Node3D water)
            {
                this.camera = camera;
                this.terrain = terrain;
                this.water = water;
                Name = "Scenery";
            }

            public override void _Process(double delta)
            {
                terrain.Focus = camera.GlobalPosition;
                Presentation.Water.Follow(water, camera.GlobalPosition);
            }
        }

        /// <summary>
        /// Movie Maker runs as fast as it can render, which can be faster than real time. The server
        /// checks movement against its own clock, so hold each frame back to real time.
        /// </summary>
        private partial class RealTime : Node
        {
            private double start = -1;
            private double gameTime;

            public override void _Process(double delta)
            {
                double now = Clock.Now;
                if (start < 0)
                    start = now;
                // Movie Maker passes the fixed frame time as delta.
                gameTime += delta;
                double due = start + gameTime;
                if (now < due)
                    OS.DelayUsec((int)((due - now) * 1_000_000));
            }
        }

        private static void AddInputActions()
        {
            void Bind(string action, params Key[] keys)
            {
                if (InputMap.HasAction(action))
                    return;
                InputMap.AddAction(action);
                foreach (var k in keys)
                    InputMap.ActionAddEvent(action, new InputEventKey { PhysicalKeycode = k });
            }
            Bind("forward", Key.W, Key.Up);
            Bind("back", Key.S, Key.Down);
            Bind("left", Key.A);
            Bind("right", Key.D);
            Bind("turn_left", Key.Q, Key.Left);
            Bind("turn_right", Key.E, Key.Right);
            Bind("jump", Key.Space);
            Bind("interact", Key.F);
            Bind("dev_spawn_ship", Key.F9);
            Bind("dev_remove_ship", Key.F10);
            Bind("dev_spend_labor", Key.F8);
            Bind("bag", Key.I, Key.B);
            Bind("rotate", Key.R);
            Bind("uproot", Key.G);
            Bind("drop_pack", Key.X);
            Bind("target_next", Key.Tab);
            Bind("target_self", Key.F1);
            Bind("target_clear", Key.Escape);
            Bind("respawn", Key.R);
            Bind("skillsets", Key.K);
            Bind("stow_vehicle", Key.V);
            Bind("repair_vehicle", Key.H);
            Bind("report", Key.J);
            Bind("quest_log", Key.L);
        }
    }
}
