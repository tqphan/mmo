using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Godot;
using Mmo.World;

namespace Mmo.Presentation
{
    /// <summary>
    /// The world-scale spike (docs/design/worldstream.md, docs/spikes/world-scale.md): the whole 16 km
    /// map from a <c>.mmoh</c> file, drawn as terrain tiles with the sea, while the camera follows a
    /// scripted route. It logs frame and GPU times, triangles, memory and streaming once a second,
    /// and a summary on exit. Offline; no server.
    /// </summary>
    public partial class WorldTest : Node3D
    {
        /// <summary>Seconds at the start left out of the summary: the first tiles are still streaming in.</summary>
        private const double WarmupS = 5.0;

        private readonly ClientOptions options;
        private HeightMap map = null!;
        private Terrain terrain = null!;
        private Node3D water = null!;
        private Camera3D camera = null!;
        private Rid viewport;
        private double time;
        private ulong lastTicks;
        private double initialLoadS = -1;
        private readonly List<double> frameMs = new(), gpuMs = new();
        private readonly Second second = new();
        private readonly Totals totals = new();

        public WorldTest(ClientOptions options)
        {
            this.options = options;
            Name = "WorldTest";
        }

        public override void _Ready()
        {
            GetWindow().Size = new Vector2I(1920, 1080);
            if (options.NoVsync)
                DisplayServer.WindowSetVsyncMode(DisplayServer.VSyncMode.Disabled);
            // Relative paths are from the repo root, where cargo xtask runs.
            string path = System.IO.Path.IsPathRooted(options.WorldTest)
                ? options.WorldTest!
                : System.IO.Path.GetFullPath(System.IO.Path.Combine(ProjectSettings.GlobalizePath("res://"), "..", options.WorldTest!));
            if (!System.IO.File.Exists(path))
            {
                GD.PushError($"world: no map at {path}; write one with `cargo xtask bot world-gen`");
                SetProcess(false);
                GetTree().Quit(2);
                return;
            }
            var clock = Stopwatch.StartNew();
            map = HeightMap.Load(path);
            GD.Print($"world: read {path} ({map.Width} × {map.Depth} samples) in {clock.ElapsedMilliseconds} ms");
            clock.Restart();
            terrain = new Terrain(map);
            GD.Print($"world: {terrain.Plan.Count} tiles, {terrain.Plan.DeepCount} in deep water, planned in {clock.ElapsedMilliseconds} ms");
            AddChild(terrain);

            AddChild(new WorldEnvironment { Environment = StyleTest.Environment(fog: 0.00012f) });
            AddChild(new DirectionalLight3D
            {
                Name = "Sun",
                RotationDegrees = new Vector3(-42, -35, 0),
                LightColor = new Color(1.0f, 0.95f, 0.86f),
                LightEnergy = 1.15f,
                ShadowEnabled = true,
                DirectionalShadowMaxDistance = 200,
            });
            water = Water.Create(faceted: false, new Palette(), horizon: 80000.0f);
            AddChild(water);
            camera = new Camera3D { Name = "Camera", Fov = 60, Near = 0.2f, Far = 40000 };
            AddChild(camera);
            Place();

            viewport = GetViewport().GetViewportRid();
            GD.Print($"world: rendering at {GetViewport().GetVisibleRect().Size} on {RenderingServer.GetVideoAdapterName()}");
            RenderingServer.ViewportSetMeasureRenderTime(viewport, true);
            lastTicks = Time.GetTicksUsec();
        }

        public override void _Process(double delta)
        {
            time += delta;
            Place();
            Measure();
            if (options.Secs > 0 && time >= options.Secs)
                GetTree().Quit();
        }

        public override void _ExitTree()
        {
            if (map != null)
                Summary();
        }

        /// <summary>Move the camera along the route, and the terrain's focus and the sea with it.</summary>
        private void Place()
        {
            float t = (float)time;
            Vector3 eye, ahead;
            switch (options.Route)
            {
                case "cross":
                    // West to east across both continents and the trade island, 250 m up, looping.
                    eye = new Vector3(-7800 + (150 * t) % 15600, 250, 1200);
                    ahead = new Vector3(1, -0.22f, 0);
                    break;
                case "walk":
                    // North across the west continent at run speed, eyes 1.7 m above the ground.
                    eye = new Vector3(-5800, 0, 4000 - 5.4f * t);
                    eye.Y = Mathf.Max(map.Height(eye.X, eye.Z), 0) + 1.7f;
                    ahead = new Vector3(0, -0.08f, -1);
                    break;
                case "high":
                    // Circling the whole map from 3 km up.
                    float a = 0.05f * t;
                    eye = new Vector3(Mathf.Sin(a) * 7000, 3000, Mathf.Cos(a) * 7000);
                    ahead = -eye;
                    break;
                default:
                    // "sail": north up the central sea at a clipper's 11 m/s, 4 m above the water, looking towards the east coast.
                    eye = new Vector3(2000, 4, 7500 - 11 * t);
                    ahead = new Vector3(0.35f, -0.04f, -1);
                    break;
            }
            camera.LookAtFromPosition(eye, eye + ahead, Vector3.Up);
            terrain.Focus = eye;
            Water.Follow(water, eye);
        }

        private sealed class Second
        {
            public double Start, MaxApplyMs;
            public List<double> Frames = new();
        }

        private sealed class Totals
        {
            public double MaxApplyMs, MaxVramMb, MaxPrims, MaxDraws, MaxMeshMb;
            public int MaxQueued, Over33;
        }

        private void Measure()
        {
            ulong now = Time.GetTicksUsec();
            double ms = (now - lastTicks) / 1000.0;
            lastTicks = now;
            double gpu = RenderingServer.ViewportGetMeasuredRenderTimeGpu(viewport);
            double prims = Performance.GetMonitor(Performance.Monitor.RenderTotalPrimitivesInFrame);
            double draws = Performance.GetMonitor(Performance.Monitor.RenderTotalDrawCallsInFrame);
            double vram = Performance.GetMonitor(Performance.Monitor.RenderVideoMemUsed) / 1048576.0;

            if (initialLoadS < 0 && time > 0.5 && terrain.Queued == 0)
            {
                initialLoadS = time;
                GD.Print($"world: first tiles all in after {initialLoadS:F1} s ({terrain.Built} built)");
            }
            second.Frames.Add(ms);
            second.MaxApplyMs = Math.Max(second.MaxApplyMs, terrain.LastApplyMs);
            if (time >= WarmupS)
            {
                frameMs.Add(ms);
                gpuMs.Add(gpu);
                totals.Over33 += ms > 33.3 ? 1 : 0;
                totals.MaxApplyMs = Math.Max(totals.MaxApplyMs, terrain.LastApplyMs);
                totals.MaxVramMb = Math.Max(totals.MaxVramMb, vram);
                totals.MaxPrims = Math.Max(totals.MaxPrims, prims);
                totals.MaxDraws = Math.Max(totals.MaxDraws, draws);
                totals.MaxQueued = Math.Max(totals.MaxQueued, terrain.Queued);
                totals.MaxMeshMb = Math.Max(totals.MaxMeshMb, terrain.MeshBytes / 1048576.0);
            }
            if (time - second.Start < 1.0)
                return;
            var f = second.Frames;
            f.Sort();
            var levels = string.Join("/", terrain.TilesPerLevel());
            GD.Print($"world t={time:F0}s fps={f.Count / (time - second.Start):F0} frame p50={f[f.Count / 2]:F1} max={f[^1]:F1} ms gpu={gpu:F2} ms " +
                     $"tris={prims / 1e6:F2}M draws={draws:F0} vram={vram:F0}MB meshes={terrain.MeshBytes / 1048576}MB managed={GC.GetTotalMemory(false) / 1048576}MB " +
                     $"rss={System.Environment.WorkingSet / 1048576}MB tiles={levels} queued={terrain.Queued} built={terrain.Built} apply-max={second.MaxApplyMs:F1}ms");
            second.Start = time;
            second.Frames.Clear();
            second.MaxApplyMs = 0;
        }

        private void Summary()
        {
            if (frameMs.Count == 0)
                return;
            static string Stats(List<double> v)
            {
                var s = v.OrderBy(x => x).ToList();
                return $"p50 {s[s.Count / 2]:F2}  p99 {s[(int)(s.Count * 0.99)]:F2}  max {s[^1]:F2} ms";
            }
            GD.Print($"world summary ({options.Route}, {frameMs.Count} frames after {WarmupS:F0} s, vsync {(options.NoVsync ? "off" : "on")}, " +
                     $"{RenderingServer.GetVideoAdapterName()}):");
            GD.Print($"  frame  {Stats(frameMs)}  over 33 ms: {totals.Over33}");
            GD.Print($"  gpu    {Stats(gpuMs)}");
            GD.Print($"  peak   {totals.MaxPrims / 1e6:F2}M triangles  {totals.MaxDraws:F0} draw calls  {totals.MaxVramMb:F0} MB video memory, {totals.MaxMeshMb:F0} MB of it terrain meshes");
            GD.Print($"  stream first tiles in {initialLoadS:F1} s  {terrain.Built} meshes built  longest apply {totals.MaxApplyMs:F1} ms  most queued {totals.MaxQueued}");
        }
    }
}
