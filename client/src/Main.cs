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
    /// session, a flat placeholder ground, the entity registry, the player, the camera and the HUD.
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

            var session = new Session(options);
            AddChild(session);
            AddChild(Ground());
            AddChild(new DirectionalLight3D
            {
                Name = "Sun",
                RotationDegrees = new Vector3(-50, -30, 0),
                ShadowEnabled = true,
            });
            AddChild(new WorldEnvironment { Environment = Sky() });
            var entities = new EntityRegistry(session, options.Trace);
            AddChild(entities);
            var player = new PlayerController(session, options);
            AddChild(player);
            AddChild(new FollowCamera(player, options.CamYawDeg));
            AddChild(new Hud(session, player, entities));
            if (OS.HasFeature("movie"))
                AddChild(new RealTime());
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
        }

        /// <summary>A flat 2 km checkerboard at y = 0 until there's terrain (BACKLOG: world exporter, spike 4).</summary>
        private static Node3D Ground()
        {
            const float size = 2000.0f;
            var checker = Image.CreateEmpty(2, 2, false, Image.Format.Rgb8);
            var light = new Color(0.55f, 0.62f, 0.45f);
            var dark = new Color(0.47f, 0.54f, 0.38f);
            checker.SetPixel(0, 0, light);
            checker.SetPixel(1, 1, light);
            checker.SetPixel(1, 0, dark);
            checker.SetPixel(0, 1, dark);
            var material = new StandardMaterial3D
            {
                AlbedoTexture = ImageTexture.CreateFromImage(checker),
                TextureFilter = BaseMaterial3D.TextureFilterEnum.Nearest,
                // One checker square per 4 m.
                Uv1Scale = new Vector3(size / 8.0f, size / 8.0f, 1),
                Roughness = 1.0f,
            };
            var ground = new StaticBody3D { Name = "Ground" };
            ground.AddChild(new CollisionShape3D
            {
                Shape = new BoxShape3D { Size = new Vector3(size, 1, size) },
                Position = new Vector3(0, -0.5f, 0),
            });
            ground.AddChild(new MeshInstance3D { Mesh = new PlaneMesh { Size = new Vector2(size, size), Material = material } });
            return ground;
        }

        private static Godot.Environment Sky() => new()
        {
            BackgroundMode = Godot.Environment.BGMode.Sky,
            Sky = new Sky { SkyMaterial = new ProceduralSkyMaterial() },
            AmbientLightSource = Godot.Environment.AmbientSource.Sky,
            TonemapMode = Godot.Environment.ToneMapper.Filmic,
            FogEnabled = true,
            FogDensity = 0.002f,
            FogSkyAffect = 0.0f,
        };
    }
}
