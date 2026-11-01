using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Godot;
using Mmo.World;
using TokisanGames;

namespace Mmo.Presentation
{
    /// <summary>
    /// The whole map as one Terrain3D node (ADR 0032, docs/spikes/terrain3d.md): all 64 regions of
    /// 1024 pixels at 2 m, built in memory from the world's files at start and never streamed. Heights
    /// and the land use bits come from <see cref="Terrain3DLayout"/>; the colour map carries the same
    /// placeholder colours as <see cref="Terrain"/>'s tiles, over one plain white texture.
    /// </summary>
    public static class Terrain3DWorld
    {
        /// <summary>Colour map alpha is a roughness modifier; half is none.</summary>
        private const byte NeutralRoughness = 128;

        /// <summary>
        /// Add the terrain under <paramref name="parent"/>, following <paramref name="target"/> with its
        /// level of detail and, if <paramref name="collide"/>, building collision around it.
        /// </summary>
        public static Terrain3D Create(Node parent, WorldData world, Node3D target, bool collide)
        {
            var clock = Stopwatch.StartNew();
            var terrain = Terrain3D.Instantiate();
            terrain.Name = "Terrain3D";
            terrain.VertexSpacing = HeightMap.Spacing;
            // Our assets are made in code, not saved to a file, so the editor can't reload them.
            terrain.FreeEditorTextures = false;
            terrain.CollisionMode = collide ? Terrain3DCollision.CollisionMode.DynamicGame : Terrain3DCollision.CollisionMode.Disabled;
            parent.AddChild(terrain);
            // Only once in the tree: its data takes the region size then, and starts at 256 otherwise.
            terrain.RegionSize = Terrain3D.RegionSizeEnum.Size1024;
            terrain.ClipmapTarget = target;
            terrain.CollisionTarget = target;

            var material = terrain.Material;
            material.WorldBackground = Terrain3DMaterial.WorldBackgroundEnum.None;
            material.AutoShaderEnabled = false;
            material.MaxRegions = Terrain3DMaterial.RegionMaximum.MaxRegions64;
            // Our own shader: the colour map alone, without texture blending (docs/spikes/terrain3d.md).
            material.ShaderOverride = GD.Load<Shader>("res://shaders/terrain3d.gdshader");
            material.ShaderOverrideEnabled = true;
            terrain.Assets.SetTextureAsset(0, WhiteTexture());

            // The regions' pixels in parallel; Godot images and regions on this thread.
            var regions = Terrain3DLayout.Regions();
            var built = new (byte[] Height, byte[] Control, byte[] Color)[regions.Length];
            var palette = new Palette();
            Parallel.For(0, regions.Length, k => built[k] = Pixels(world, palette, regions[k].Rx, regions[k].Rz));
            long pixelsMs = clock.ElapsedMilliseconds;

            var data = terrain.Data;
            const int px = Terrain3DLayout.RegionPx;
            for (int k = 0; k < regions.Length; k++)
            {
                var region = Terrain3DRegion.Instantiate();
                region.RegionSize = px;
                region.VertexSpacing = HeightMap.Spacing;
                region.Location = new Vector2I(regions[k].Rx, regions[k].Rz);
                region.SetMap(Terrain3DRegion.MapType.Height, Image.CreateFromData(px, px, false, Image.Format.Rf, built[k].Height));
                region.SetMap(Terrain3DRegion.MapType.Control, Image.CreateFromData(px, px, false, Image.Format.Rf, built[k].Control));
                region.SetMap(Terrain3DRegion.MapType.Color, Image.CreateFromData(px, px, false, Image.Format.Rgba8, built[k].Color));
                region.CalcHeightRange();
                var err = data.AddRegion(region, update: false);
                if (err != Error.Ok)
                    GD.PushError($"terrain3d: region {region.Location} not added: {err}");
                built[k] = default;
            }
            data.UpdateMaps(Terrain3DRegion.MapType.Max, allRegions: true, generateMipmaps: false);
            // The pixel arrays (768 MB) are in Godot's images now.
            GC.Collect();
            GD.Print($"terrain3d: {data.GetRegionCount()} regions of {px} px in {clock.ElapsedMilliseconds} ms ({pixelsMs} ms of it pixels)");
            CheckHeights(data, world.Heights);
            return terrain;
        }

        /// <summary>Terrain3D must give the map's own height at sample points; log the worst of 4,096 random ones.</summary>
        private static void CheckHeights(Terrain3DData data, HeightMap map)
        {
            var rng = new Random(1);
            double worst = 0;
            Vector3 at = default;
            for (int k = 0; k < 4096; k++)
            {
                var p = new Vector3(rng.Next(-4000, 4001) * HeightMap.Spacing, 0, rng.Next(-4000, 4001) * HeightMap.Spacing);
                double d = Math.Abs(data.GetHeight(p) - map.Height(p.X, p.Z));
                if (d > worst)
                    (worst, at) = (d, p);
            }
            GD.Print($"terrain3d: heights at 4096 samples differ from the map's by at most {worst:F3} m (at {at.X}, {at.Z}: {data.GetHeight(at):F2} against {map.Height(at.X, at.Z):F2})");
        }

        /// <summary>One region's height (RF), control (RF holding bits) and colour (RGBA8) pixels.</summary>
        private static (byte[], byte[], byte[]) Pixels(WorldData world, Palette palette, int rx, int rz)
        {
            const int px = Terrain3DLayout.RegionPx;
            var heights = new float[px * px];
            var control = new uint[px * px];
            var color = new byte[px * px * 4];
            Terrain3DLayout.FillHeights(world.Heights, rx, rz, heights);
            Terrain3DLayout.FillControl(world.Land, rx, rz, control);
            const float spacing = HeightMap.Spacing;
            for (int v = 0; v < px; v++)
            {
                for (int u = 0; u < px; u++)
                {
                    float x = (rx * px + u) * spacing, z = (rz * px + v) * spacing;
                    // Slopes from the neighbouring samples, for the rock on steep ground.
                    float gx = (world.Heights.Height(x + spacing, z) - world.Heights.Height(x - spacing, z)) / (2 * spacing);
                    float gz = (world.Heights.Height(x, z + spacing) - world.Heights.Height(x, z - spacing)) / (2 * spacing);
                    float up = 1.0f / MathF.Sqrt(1 + gx * gx + gz * gz);
                    var c = Terrain.GroundColor(palette, world.Land, heights[v * px + u], up, x, z);
                    int at = (v * px + u) * 4;
                    color[at] = (byte)c.R8;
                    color[at + 1] = (byte)c.G8;
                    color[at + 2] = (byte)c.B8;
                    color[at + 3] = NeutralRoughness;
                }
            }
            return (MemoryMarshal.AsBytes(heights.AsSpan()).ToArray(), MemoryMarshal.AsBytes(control.AsSpan()).ToArray(), color);
        }

        /// <summary>A small plain white texture with a flat normal, so the colour map is all there is to see.</summary>
        private static Terrain3DTextureAsset WhiteTexture()
        {
            var albedo = Image.CreateEmpty(64, 64, true, Image.Format.Rgba8);
            albedo.Fill(new Color(1, 1, 1, 0.5f));
            albedo.GenerateMipmaps();
            var normal = Image.CreateEmpty(64, 64, true, Image.Format.Rgba8);
            normal.Fill(new Color(0.5f, 0.5f, 1.0f, 1.0f));
            normal.GenerateMipmaps();
            var asset = Terrain3DTextureAsset.Instantiate();
            asset.Name = "White";
            asset.AlbedoTexture = ImageTexture.CreateFromImage(albedo);
            asset.NormalTexture = ImageTexture.CreateFromImage(normal);
            return asset;
        }
    }
}
