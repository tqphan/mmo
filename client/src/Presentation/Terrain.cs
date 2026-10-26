using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading.Tasks;
using Godot;
using Mmo.World;

namespace Mmo.Presentation
{
    /// <summary>
    /// Terrain drawn from the shared heightmap in 256 m tiles at six levels of detail
    /// (docs/design/worldstream.md, ADR 0017). <see cref="TerrainPlan"/> picks each tile's level
    /// from the distance to <see cref="Focus"/>; meshes are built on worker threads, nearest first,
    /// and swapped in on the main thread within <see cref="ApplyBudgetMs"/> a frame. A tile keeps its
    /// old mesh until the new one is in. Placeholder colours only (ADR 0004).
    /// </summary>
    public partial class Terrain : Node3D
    {
        public const int MaxWorkers = 4;
        public const double ApplyBudgetMs = 2.0;
        /// <summary>Tiles at this level or finer cast shadows; the sun's shadows don't reach further.</summary>
        private const int ShadowLevels = 1;

        public Vector3 Focus;
        public readonly TerrainPlan Plan;
        private readonly HeightMap map;
        private readonly Palette palette = new();
        private readonly StandardMaterial3D material = new()
        {
            VertexColorUseAsAlbedo = true,
            VertexColorIsSrgb = true,
            Roughness = 1.0f,
            SpecularMode = BaseMaterial3D.SpecularModeEnum.Disabled,
            // Skirts face either way.
            CullMode = BaseMaterial3D.CullModeEnum.Disabled,
        };
        private readonly Dictionary<int, MeshInstance3D> shown = new();
        private readonly Dictionary<int, int> shownLevel = new();
        private readonly Dictionary<int, long> shownBytes = new();
        /// <summary>Tiles waiting for a worker, with the level wanted.</summary>
        private readonly Dictionary<int, int> pending = new();
        private readonly HashSet<int> building = new();
        private readonly ConcurrentQueue<TileMesh> done = new();

        /// <summary>Counters for the world test's readout.</summary>
        public int Built { get; private set; }
        public double LastApplyMs { get; private set; }
        public int Queued => pending.Count + building.Count;
        /// <summary>Vertex and index data of the meshes on screen, as uploaded to the GPU.</summary>
        public long MeshBytes { get; private set; }

        private sealed record TileMesh(int Tile, int Level, Vector3[] Vertices, Vector3[] Normals, Color[] Colors, int[] Indices);

        public Terrain(HeightMap map)
        {
            this.map = map;
            Plan = new TerrainPlan(map);
            Name = "Terrain";
        }

        public int[] TilesPerLevel()
        {
            var n = new int[TerrainPlan.Levels];
            foreach (int level in shownLevel.Values)
                n[level]++;
            return n;
        }

        public override void _Process(double delta)
        {
            foreach (var (tile, level) in Plan.Update(Focus.X, Focus.Z))
            {
                if (level == TerrainPlan.None)
                {
                    pending.Remove(tile);
                    if (shown.Remove(tile, out var gone))
                    {
                        shownLevel.Remove(tile);
                        MeshBytes -= shownBytes[tile];
                        shownBytes.Remove(tile);
                        gone.QueueFree();
                    }
                }
                else if (shownLevel.TryGetValue(tile, out int has) && has == level)
                {
                    // Back to the level already on screen before its replacement was built.
                    pending.Remove(tile);
                }
                else
                {
                    pending[tile] = level;
                }
            }
            StartWorkers();
            Apply();
        }

        private void StartWorkers()
        {
            if (building.Count >= MaxWorkers || pending.Count == 0)
                return;
            var order = new List<int>(pending.Keys);
            order.Sort((a, b) => Plan.Distance(a, Focus.X, Focus.Z).CompareTo(Plan.Distance(b, Focus.X, Focus.Z)));
            foreach (int tile in order)
            {
                if (building.Count >= MaxWorkers)
                    break;
                // One build per tile at a time; a newer level waits for it.
                if (building.Contains(tile))
                    continue;
                int level = pending[tile];
                pending.Remove(tile);
                building.Add(tile);
                Task.Run(() => done.Enqueue(Build(tile, level)));
            }
        }

        private void Apply()
        {
            var clock = Stopwatch.StartNew();
            while (clock.Elapsed.TotalMilliseconds < ApplyBudgetMs && done.TryDequeue(out var b))
            {
                building.Remove(b.Tile);
                // Stale: the plan has moved on, or dropped the tile.
                if (Plan.Level(b.Tile) != b.Level)
                    continue;
                var arrays = new Godot.Collections.Array();
                arrays.Resize((int)Mesh.ArrayType.Max);
                arrays[(int)Mesh.ArrayType.Vertex] = b.Vertices;
                arrays[(int)Mesh.ArrayType.Normal] = b.Normals;
                arrays[(int)Mesh.ArrayType.Color] = b.Colors;
                arrays[(int)Mesh.ArrayType.Index] = b.Indices;
                var mesh = new ArrayMesh();
                mesh.AddSurfaceFromArrays(Mesh.PrimitiveType.Triangles, arrays);
                if (!shown.TryGetValue(b.Tile, out var node))
                {
                    var (i0, j0) = Plan.Origin(b.Tile);
                    node = new MeshInstance3D
                    {
                        Name = $"Tile{b.Tile}",
                        MaterialOverride = material,
                        Position = new Vector3(map.X0 + i0 * HeightMap.Spacing, 0, map.Z0 + j0 * HeightMap.Spacing),
                    };
                    AddChild(node);
                    shown[b.Tile] = node;
                }
                node.Mesh = mesh;
                node.CastShadow = b.Level <= ShadowLevels ? GeometryInstance3D.ShadowCastingSetting.On : GeometryInstance3D.ShadowCastingSetting.Off;
                shownLevel[b.Tile] = b.Level;
                // Position, normal and colour per vertex (Godot packs normals smaller; this is an upper bound).
                long bytes = b.Vertices.Length * (12L + 12 + 16) + b.Indices.Length * 4L;
                MeshBytes += bytes - shownBytes.GetValueOrDefault(b.Tile);
                shownBytes[b.Tile] = bytes;
                Built++;
            }
            LastApplyMs = clock.Elapsed.TotalMilliseconds;
        }

        /// <summary>
        /// A tile's mesh at <paramref name="level"/>, in the tile's own frame: a grid with a vertex
        /// every 2^level samples, alternating diagonals, and a skirt hanging from its four edges.
        /// Runs on a worker thread, so it touches only the heightmap and plain structs.
        /// </summary>
        private TileMesh Build(int tile, int level)
        {
            int step = 1 << level, n = TerrainPlan.TileQuads / step;
            float spacing = step * HeightMap.Spacing, skirt = spacing + 2.0f;
            var (i0, j0) = Plan.Origin(tile);
            int grid = (n + 1) * (n + 1);
            var vertices = new Vector3[grid + 4 * (n + 1)];
            var normals = new Vector3[vertices.Length];
            var colors = new Color[vertices.Length];
            var indices = new List<int>(n * n * 6 + 4 * n * 6);
            float H(int i, int j) => map.Sample(i0 + i * step, j0 + j * step);
            for (int b = 0; b <= n; b++)
            {
                for (int a = 0; a <= n; a++)
                {
                    int v = b * (n + 1) + a;
                    float h = H(a, b);
                    vertices[v] = new Vector3(a * spacing, h, b * spacing);
                    normals[v] = new Vector3(H(a - 1, b) - H(a + 1, b), 2 * spacing, H(a, b - 1) - H(a, b + 1)).Normalized();
                    colors[v] = Ground(h, normals[v].Y);
                }
            }
            for (int b = 0; b < n; b++)
            {
                for (int a = 0; a < n; a++)
                {
                    int p = b * (n + 1) + a, q = p + 1, r = p + n + 2, s = p + n + 1;
                    // Alternate the diagonal, as the server allows for both (ADR 0013).
                    if (((a + b) & 1) == 0)
                        indices.AddRange(new[] { p, q, r, p, r, s });
                    else
                        indices.AddRange(new[] { p, q, s, q, r, s });
                }
            }
            // Skirts: each edge's vertices again, lowered, joined to the edge by a strip.
            int k = grid;
            void Skirt(System.Func<int, int> edge)
            {
                int first = k;
                for (int t = 0; t <= n; t++, k++)
                {
                    int v = edge(t);
                    vertices[k] = vertices[v] - new Vector3(0, skirt, 0);
                    normals[k] = normals[v];
                    colors[k] = colors[v];
                }
                for (int t = 0; t < n; t++)
                    indices.AddRange(new[] { edge(t), edge(t + 1), first + t + 1, edge(t), first + t + 1, first + t });
            }
            Skirt(t => t);
            Skirt(t => n * (n + 1) + t);
            Skirt(t => t * (n + 1));
            Skirt(t => t * (n + 1) + n);
            return new TileMesh(tile, level, vertices, normals, colors, indices.ToArray());
        }

        /// <summary>Placeholder ground colours: sea floor, sand at the waterline, grass, rock when steep or high.</summary>
        private Color Ground(float height, float up)
        {
            var low = palette.SeaFloor.Lerp(palette.Sand, Mathf.SmoothStep(-3.0f, -0.5f, height));
            var grass = palette.GrassLow.Lerp(palette.GrassHigh, Mathf.SmoothStep(20, 120, height));
            var c = low.Lerp(grass, Mathf.SmoothStep(1.0f, 3.0f, height));
            float rocky = Mathf.Max(Mathf.SmoothStep(0.80f, 0.68f, up), Mathf.SmoothStep(160, 260, height));
            var rock = palette.Rock.Lerp(palette.RockLight, Mathf.SmoothStep(150, 350, height));
            return c.Lerp(rock, rocky);
        }
    }
}
