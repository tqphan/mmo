using System.Collections.Generic;
using Godot;
using Mmo.Sim;

namespace Mmo.World
{
    /// <summary>
    /// Ground for the local player to stand on: a <see cref="HeightMapShape3D"/> for each 64 m cell in
    /// the 3 × 3 window around it, holding the same samples as the server's chunks
    /// (docs/design/worldstream.md). The window moves like the Near tier's, once the player is 8 m
    /// past its centre cell, so there's always at least 56 m of ground ahead. Nothing else collides
    /// with terrain on the client: other entities are drawn where the server puts them.
    /// </summary>
    public partial class TerrainCollision : StaticBody3D
    {
        public const ushort Radius = 1;
        /// <summary>
        /// Heightmap shapes have a sample every unit, so each is scaled by the 2 m spacing, uniformly
        /// (physics engines handle non-uniform scale badly), with its heights divided to match.
        /// </summary>
        private const float ShapeScale = HeightMap.Spacing;

        public readonly HeightMap Map;
        private readonly Dictionary<ulong, CollisionShape3D> shapes = new();
        private ulong center = ulong.MaxValue;

        public TerrainCollision(HeightMap map)
        {
            Map = map;
            Name = "TerrainCollision";
        }

        public int Shapes => shapes.Count;

        /// <summary>
        /// Keep the window around (x, z). Returns true if it moved, which adds shapes that the physics
        /// engine may not see until its next step.
        /// </summary>
        public bool Follow(float x, float z)
        {
            ulong next = center == ulong.MaxValue
                ? Cell.CellOf(Cell.OpenWorld, x, z)
                : Cell.Recenter(center, x, z, Cell.HysteresisM);
            if (next == center)
                return false;
            center = next;
            var want = new HashSet<ulong>(Cell.WindowCells(center, Radius));
            foreach (var (cell, shape) in new List<KeyValuePair<ulong, CollisionShape3D>>(shapes))
            {
                if (want.Contains(cell))
                    continue;
                shapes.Remove(cell);
                shape.QueueFree();
            }
            foreach (ulong cell in want)
            {
                if (!shapes.ContainsKey(cell) && Shape(cell) is CollisionShape3D shape)
                {
                    shapes[cell] = shape;
                    AddChild(shape);
                }
            }
            return true;
        }

        private CollisionShape3D? Shape(ulong cell)
        {
            var (_, cy, cx) = Cell.Unpack(cell);
            if (Map.CellHeights(cx, cy) is not float[] heights)
                return null;
            for (int k = 0; k < heights.Length; k++)
                heights[k] /= ShapeScale;
            const int side = HeightMap.QuadsPerCell + 1;
            // The shape is centred on its node.
            float half = HeightMap.CellSize / 2;
            return new CollisionShape3D
            {
                Name = $"Cell{cx}_{cy}",
                Shape = new HeightMapShape3D { MapWidth = side, MapDepth = side, MapData = heights },
                Position = new Vector3(cx * HeightMap.CellSize - HeightMap.WorldHalf + half, 0, cy * HeightMap.CellSize - HeightMap.WorldHalf + half),
                Scale = new Vector3(ShapeScale, ShapeScale, ShapeScale),
            };
        }
    }
}
