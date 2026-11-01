using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;
using Godot;
using Mmo.World;

namespace Mmo.Presentation
{
    /// <summary>
    /// The world's trees, bushes, stones and ground cover round the camera (docs/design/art.md,
    /// ADR 0041), placed by <see cref="FloraPlan"/>. Each layer keeps the cells within its radius
    /// of <see cref="Focus"/>; cells are planned on worker threads and drawn as one MultiMesh per
    /// kind and variant, swapped in on the main thread within <see cref="ApplyBudgetMs"/> a frame.
    /// Cosmetic only: nothing collides with them.
    /// </summary>
    public partial class Greenery : Node3D
    {
        public const int MaxWorkers = 3;
        public const double ApplyBudgetMs = 2.0;

        public Vector3 Focus;
        private readonly FloraPlan plan;
        private readonly Mesh[,] meshes;
        private readonly ShaderMaterial trees, shrubs, stones, cover;
        private readonly FloraPlan.Layer[] layers = { FloraPlan.Big, FloraPlan.Small };
        private readonly Dictionary<(FloraPlan.Layer, int, int), Node3D> shown = new();
        private readonly HashSet<(FloraPlan.Layer, int, int)> building = new();
        private readonly ConcurrentQueue<Built> done = new();
        private int running;

        /// <summary>Instances on screen, for the world test's readout.</summary>
        public int Instances { get; private set; }
        private readonly Dictionary<(FloraPlan.Layer, int, int), int> counts = new();

        private sealed record Built(FloraPlan.Layer Layer, int Cx, int Cz, List<(Flora Kind, int Variant, float[] Buffer, int Count)> Groups);

        public Greenery(FloraPlan plan)
        {
            this.plan = plan;
            Name = "Greenery";
            var palette = new Palette();
            var kinds = Enum.GetValues<Flora>();
            meshes = new Mesh[kinds.Length, FloraPlan.Variants];
            foreach (var kind in kinds)
                for (int v = 0; v < FloraPlan.Variants; v++)
                    meshes[(int)kind, v] = new Props(false, palette, (ulong)(101 + (int)kind * 7 + v)).Kind(kind, v);
            var shader = GD.Load<Shader>("res://shaders/flora.gdshader");
            ShaderMaterial Material(float sway, bool ground)
            {
                var m = new ShaderMaterial { Shader = shader };
                m.SetShaderParameter("sway", sway);
                m.SetShaderParameter("ground_cover", ground);
                return m;
            }
            trees = Material(0.0025f, false);
            shrubs = Material(0.02f, false);
            stones = Material(0.0f, false);
            cover = Material(0.08f, true);
        }

        private ShaderMaterial MaterialOf(Flora kind) => kind switch
        {
            Flora.Pine or Flora.Oak or Flora.Birch or Flora.Autumn or Flora.DeadTree => trees,
            Flora.Bush => shrubs,
            Flora.Boulder or Flora.Rock or Flora.Pebbles or Flora.Driftwood => stones,
            _ => cover,
        };

        public override void _Process(double delta)
        {
            foreach (var layer in layers)
                Plan(layer);
            Apply();
        }

        private static (int, int) CellOf(FloraPlan.Layer layer, float x, float z) =>
            ((int)MathF.Floor((x + HeightMap.WorldHalf) / layer.CellM), (int)MathF.Floor((z + HeightMap.WorldHalf) / layer.CellM));

        private float Distance(FloraPlan.Layer layer, int cx, int cz)
        {
            float x = (cx + 0.5f) * layer.CellM - HeightMap.WorldHalf, z = (cz + 0.5f) * layer.CellM - HeightMap.WorldHalf;
            return MathF.Sqrt((x - Focus.X) * (x - Focus.X) + (z - Focus.Z) * (z - Focus.Z));
        }

        /// <summary>Drop cells that have fallen out of range, and start building the nearest missing ones.</summary>
        private void Plan(FloraPlan.Layer layer)
        {
            float reach = layer.RadiusM + layer.CellM * 0.71f;
            var gone = new List<(FloraPlan.Layer, int, int)>();
            foreach (var key in shown.Keys)
                if (key.Item1 == layer && Distance(layer, key.Item2, key.Item3) > reach * 1.15f)
                    gone.Add(key);
            foreach (var key in gone)
            {
                shown[key].QueueFree();
                shown.Remove(key);
                Instances -= counts[key];
                counts.Remove(key);
            }
            while (Volatile.Read(ref running) < MaxWorkers && Nearest(layer, reach) is { } next)
            {
                building.Add(next);
                Interlocked.Increment(ref running);
                Task.Run(() =>
                {
                    try
                    {
                        done.Enqueue(Build(next.Item1, next.Item2, next.Item3));
                    }
                    finally
                    {
                        Interlocked.Decrement(ref running);
                    }
                });
            }
        }

        /// <summary>The nearest cell in reach that's neither drawn nor being built.</summary>
        private (FloraPlan.Layer, int, int)? Nearest(FloraPlan.Layer layer, float reach)
        {
            var (fx, fz) = CellOf(layer, Focus.X, Focus.Z);
            int span = (int)MathF.Ceiling(reach / layer.CellM);
            (FloraPlan.Layer, int, int)? best = null;
            float bestD = float.MaxValue;
            for (int cz = fz - span; cz <= fz + span; cz++)
            {
                for (int cx = fx - span; cx <= fx + span; cx++)
                {
                    var key = (layer, cx, cz);
                    float d = Distance(layer, cx, cz);
                    if (d > reach || d >= bestD || shown.ContainsKey(key) || building.Contains(key))
                        continue;
                    bestD = d;
                    best = key;
                }
            }
            return best;
        }

        /// <summary>A cell's pieces, grouped by kind and variant into MultiMesh buffers. Runs on a worker thread.</summary>
        private Built Build(FloraPlan.Layer layer, int cx, int cz)
        {
            var groups = new Dictionary<(Flora, int), List<FloraItem>>();
            foreach (var item in plan.Cell(layer, cx, cz))
            {
                if (!groups.TryGetValue((item.Kind, item.Variant), out var list))
                    groups[(item.Kind, item.Variant)] = list = new List<FloraItem>();
                list.Add(item);
            }
            var result = new List<(Flora, int, float[], int)>();
            foreach (var ((kind, variant), items) in groups)
            {
                // A 3 × 4 transform, rows first, then a colour.
                var buffer = new float[items.Count * 16];
                for (int i = 0; i < items.Count; i++)
                {
                    var it = items[i];
                    float c = MathF.Cos(it.Yaw) * it.Scale, s = MathF.Sin(it.Yaw) * it.Scale;
                    int o = i * 16;
                    buffer[o] = c; buffer[o + 1] = 0; buffer[o + 2] = s; buffer[o + 3] = it.X;
                    buffer[o + 4] = 0; buffer[o + 5] = it.Scale; buffer[o + 6] = 0; buffer[o + 7] = it.Y;
                    buffer[o + 8] = -s; buffer[o + 9] = 0; buffer[o + 10] = c; buffer[o + 11] = it.Z;
                    // A tint round white: brighter or darker, warmer or cooler.
                    float t = it.Tint;
                    buffer[o + 12] = 0.84f + 0.16f * t;
                    buffer[o + 13] = 0.88f + 0.12f * MathF.Abs(t - 0.4f) * 1.6f;
                    buffer[o + 14] = 0.80f + 0.18f * (1 - t);
                    buffer[o + 15] = 1;
                }
                result.Add((kind, variant, buffer, items.Count));
            }
            return new Built(layer, cx, cz, result);
        }

        private void Apply()
        {
            var clock = Stopwatch.StartNew();
            while (clock.Elapsed.TotalMilliseconds < ApplyBudgetMs && done.TryDequeue(out var b))
            {
                var key = (b.Layer, b.Cx, b.Cz);
                building.Remove(key);
                // Stale: the camera has moved on.
                if (Distance(b.Layer, b.Cx, b.Cz) > (b.Layer.RadiusM + b.Layer.CellM * 0.71f) * 1.15f)
                    continue;
                var node = new Node3D { Name = $"{b.Layer.Name}_{b.Cx}_{b.Cz}" };
                int total = 0;
                foreach (var (kind, variant, buffer, count) in b.Groups)
                {
                    var mm = new MultiMesh
                    {
                        TransformFormat = MultiMesh.TransformFormatEnum.Transform3D,
                        UseColors = true,
                        Mesh = meshes[(int)kind, variant],
                        InstanceCount = count,
                        Buffer = buffer,
                    };
                    node.AddChild(new MultiMeshInstance3D
                    {
                        Multimesh = mm,
                        MaterialOverride = MaterialOf(kind),
                        CastShadow = b.Layer.Ground ? GeometryInstance3D.ShadowCastingSetting.Off : GeometryInstance3D.ShadowCastingSetting.On,
                    });
                    total += count;
                }
                AddChild(node);
                shown[key] = node;
                counts[key] = total;
                Instances += total;
            }
        }
    }
}
