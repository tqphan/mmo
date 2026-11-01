using System;
using System.IO;
using System.Runtime.InteropServices;
using Godot;
using Mmo.World;
using TokisanGames;

namespace Mmo.Presentation
{
    /// <summary>
    /// The editing round trip between <c>world/</c> and Terrain3D (ADR 0032, docs/spikes/terrain3d.md).
    /// <c>--terrain3d-import</c> writes the world's 64 regions into <see cref="DataDir"/>, for the Godot
    /// editor; <c>--terrain3d-export</c> reads them back and writes the whole map's heights and land
    /// use classes to <c>target/terrain3d/export/</c>, which <c>worldtool apply-terrain3d</c> takes
    /// into <c>world/</c>. Both quit when done. Offline; no server.
    /// </summary>
    public partial class Terrain3DFiles : Node3D
    {
        /// <summary>Terrain3D's own files, inside the project so the editor can open them; not committed.</summary>
        public const string DataDir = "res://terrain3d";
        public const string ExportDir = "target/terrain3d/export";

        private readonly ClientOptions options;
        private readonly bool import;

        public Terrain3DFiles(ClientOptions options, bool import)
        {
            this.options = options;
            this.import = import;
            Name = "Terrain3DFiles";
        }

        public override void _Ready()
        {
            int code;
            try
            {
                code = import ? Import() : Export();
            }
            catch (Exception e) when (e is IOException or InvalidDataException or ArgumentException)
            {
                GD.PushError($"terrain3d: {e.Message}");
                code = 1;
            }
            GetTree().Quit(code);
        }

        private int Import()
        {
            if (Main.LoadWorld(options.World) is not { } world)
                return 2;
            var anchor = new Node3D { Name = "Anchor" };
            AddChild(anchor);
            var terrain = Terrain3DWorld.Create(this, world, anchor, collide: false);
            // Full 32-bit heights: 16-bit halves would lose the 1/32 m steps.
            terrain.Save16Bit = false;
            string dir = ProjectSettings.GlobalizePath(DataDir);
            Directory.CreateDirectory(dir);
            foreach (var f in Directory.GetFiles(dir, "terrain3d*.res"))
                File.Delete(f);
            // The colour map is placeholder colour, rebuilt from the heights on every start; it's
            // saved only because Terrain3D's regions always carry one.
            terrain.Data.SaveDirectory(DataDir);
            GD.Print($"terrain3d: wrote {Directory.GetFiles(dir, "terrain3d*.res").Length} regions to {dir}");
            return 0;
        }

        private int Export()
        {
            var terrain = Terrain3D.Instantiate();
            terrain.Name = "Terrain3D";
            terrain.VertexSpacing = HeightMap.Spacing;
            AddChild(terrain);
            terrain.RegionSize = Terrain3D.RegionSizeEnum.Size1024;
            terrain.ClipmapTarget = terrain;
            terrain.Data.LoadDirectory(DataDir);
            int n = Terrain3DLayout.Samples;
            var samples = new ushort[n * n];
            var uses = new byte[LandRaster.Side * LandRaster.Side];
            const int px = Terrain3DLayout.RegionPx;
            var heights = new float[px * px];
            var control = new uint[px * px];
            foreach (var (rx, rz) in Terrain3DLayout.Regions())
            {
                var region = terrain.Data.GetRegion(new Vector2I(rx, rz));
                if (region is null)
                    throw new InvalidDataException($"region ({rx}, {rz}) is missing from {DataDir}; every region of the map is needed");
                if (region.RegionSize != px || !Mathf.IsEqualApprox(region.VertexSpacing, HeightMap.Spacing))
                    throw new InvalidDataException($"region ({rx}, {rz}) is {region.RegionSize} px at {region.VertexSpacing} m, not {px} px at {HeightMap.Spacing} m");
                Pixels(region.GetMap(Terrain3DRegion.MapType.Height), Image.Format.Rf, MemoryMarshal.AsBytes(heights.AsSpan()));
                Pixels(region.GetMap(Terrain3DRegion.MapType.Control), Image.Format.Rf, MemoryMarshal.AsBytes(control.AsSpan()));
                Terrain3DLayout.ReadHeights(heights, rx, rz, samples);
                Terrain3DLayout.ReadUses(control, rx, rz, uses);
            }
            string dir = ClientOptions.RepoPath(ExportDir);
            Directory.CreateDirectory(dir);
            Terrain3DLayout.WriteHeightFile(Path.Combine(dir, "heights.mmoh"), samples);
            File.WriteAllBytes(Path.Combine(dir, "uses.raw"), uses);
            GD.Print($"terrain3d: exported {DataDir} to {dir}; run `cargo run --release -p worldtool -- apply-terrain3d`");
            return 0;
        }

        private static void Pixels(Image image, Image.Format format, Span<byte> dst)
        {
            if (image.GetFormat() != format || image.GetWidth() != Terrain3DLayout.RegionPx || image.GetHeight() != Terrain3DLayout.RegionPx)
                throw new InvalidDataException($"a {image.GetWidth()} × {image.GetHeight()} {image.GetFormat()} map, expected {Terrain3DLayout.RegionPx}² {format}");
            image.GetData().CopyTo(dst);
        }
    }
}
