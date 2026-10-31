using System;

namespace Mmo.Sim
{
    /// <summary>The ground under a point: the lower and higher of the two triangulations' heights, and the gentler triangle's slope (rise over run). A port of <c>sim::terrain::Ground</c>.</summary>
    public readonly record struct Ground(float Lo, float Hi, float Gradient);

    /// <summary>
    /// The server's reading of the heightmap (docs/design/terrain.md): a port of
    /// <c>sim::terrain::{quad, ground}</c>, checked against generated vectors, so the client judges
    /// ground the way the server does (a cart's footing, docs/design/vehicles.md).
    /// </summary>
    public static class Terrain
    {
        public const float SpacingM = 2.0f;
        public const float WorldHalfM = 8000.0f;
        public const int QuadsPerSide = 250 * 32;

        /// <summary>
        /// The ground at (<paramref name="x"/>, <paramref name="z"/>), or null off the map or where
        /// there is no terrain. <paramref name="corner"/>(qx, qz, dx, dz) reads a corner of quad
        /// (qx, qz) from that quad's cell, null where the cell is unknown
        /// (<see cref="World.HeightMap.QuadCorner"/>).
        /// </summary>
        public static Ground? Ground(Func<int, int, int, int, float?> corner, float x, float z)
        {
            if (Axis(x) is not (int qx, float u) || Axis(z) is not (int qz, float v))
                return null;
            if (corner(qx, qz, 0, 0) is not float h00 || corner(qx, qz, 1, 0) is not float h10
                || corner(qx, qz, 0, 1) is not float h01 || corner(qx, qz, 1, 1) is not float h11)
                return null;
            var (a, ga) = Triangle(h00, h10, h01, h11, u, v, main: true);
            var (b, gb) = Triangle(h00, h10, h01, h11, u, v, main: false);
            return new Ground(MathF.Min(a, b), MathF.Max(a, b), MathF.Min(ga, gb));
        }

        private static (int, float)? Axis(float c)
        {
            float g = (c + WorldHalfM) / SpacingM;
            if (!(g >= 0.0f && g <= 2.0f * WorldHalfM / SpacingM))
                return null;
            int q = Math.Min((int)MathF.Floor(g), QuadsPerSide - 1);
            return (q, g - q);
        }

        /// <summary>Height and gradient of the triangle under (u, v) when the quad is split along its main (−x −z to +x +z) or anti diagonal.</summary>
        private static (float, float) Triangle(float h00, float h10, float h01, float h11, float u, float v, bool main)
        {
            float height, dx, dz;
            if (main && u >= v)
                (height, dx, dz) = (h00 + u * (h10 - h00) + v * (h11 - h10), h10 - h00, h11 - h10);
            else if (main)
                (height, dx, dz) = (h00 + v * (h01 - h00) + u * (h11 - h01), h11 - h01, h01 - h00);
            else if (u + v <= 1.0f)
                (height, dx, dz) = (h00 + u * (h10 - h00) + v * (h01 - h00), h10 - h00, h01 - h00);
            else
                (height, dx, dz) = (h11 + (1.0f - u) * (h01 - h11) + (1.0f - v) * (h10 - h11), h11 - h01, h11 - h10);
            return (height, MathF.Sqrt(dx * dx + dz * dz) / SpacingM);
        }
    }
}
