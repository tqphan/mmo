using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// The style test's colours (docs/design/art.md): bright, saturated and few, so shapes read from
    /// a distance. Ground colour depends on height and on slope (the normal's y).
    /// </summary>
    public sealed class Palette
    {
        public Color SeaFloor = new(0.80f, 0.74f, 0.52f);
        public Color Sand = new(0.93f, 0.84f, 0.58f);
        public Color GrassLow = new(0.49f, 0.74f, 0.31f);
        public Color GrassHigh = new(0.33f, 0.58f, 0.25f);
        public Color Rock = new(0.55f, 0.51f, 0.47f);
        public Color RockLight = new(0.70f, 0.67f, 0.62f);
        public Color Trunk = new(0.45f, 0.30f, 0.20f);
        public Color Leaves = new(0.30f, 0.62f, 0.27f);
        public Color LeavesAutumn = new(0.90f, 0.62f, 0.20f);
        public Color Pine = new(0.18f, 0.45f, 0.30f);
        public Color WaterShallow = new(0.25f, 0.85f, 0.80f);
        public Color WaterDeep = new(0.07f, 0.35f, 0.60f);

        public Color Ground(float height, float up, bool banded)
        {
            if (banded)
            {
                if (up < 0.72f)
                    return height > 12 ? RockLight : Rock;
                if (height < 0.2f)
                    return height < -1.5f ? SeaFloor : Sand;
                return height < 7 ? GrassLow : GrassHigh;
            }
            var grass = GrassLow.Lerp(GrassHigh, Mathf.SmoothStep(3, 10, height));
            var rock = Rock.Lerp(RockLight, Mathf.SmoothStep(8, 16, height));
            var shore = SeaFloor.Lerp(Sand, Mathf.SmoothStep(-2.5f, -0.5f, height));
            var c = shore.Lerp(grass, Mathf.SmoothStep(0.0f, 1.0f, height));
            return c.Lerp(rock, Mathf.SmoothStep(0.80f, 0.68f, up));
        }
    }
}
