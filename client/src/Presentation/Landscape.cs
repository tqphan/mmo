using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// The world's terrain, sea, sun and sky, as the game and the world test both draw them
    /// (docs/design/art.md).
    /// </summary>
    public static class Landscape
    {
        public static (Terrain Terrain, Node3D Water) Add(Node parent, World.HeightMap map, World.LandRaster? land)
        {
            var terrain = new Terrain(map, land);
            parent.AddChild(terrain);
            var water = Water.Create(faceted: false, new Palette(), horizon: 80000.0f, masks: terrain.Masks);
            parent.AddChild(water);
            parent.AddChild(new DirectionalLight3D
            {
                Name = "Sun",
                RotationDegrees = new Vector3(-38, -35, 0),
                LightColor = new Color(1.0f, 0.93f, 0.82f),
                LightEnergy = 1.35f,
                ShadowEnabled = true,
                DirectionalShadowMaxDistance = 200,
            });
            parent.AddChild(new WorldEnvironment { Environment = Environment() });
            return (terrain, water);
        }

        private static Godot.Environment Environment()
        {
            var horizon = new Color(0.74f, 0.85f, 0.95f);
            return new Godot.Environment
            {
                BackgroundMode = Godot.Environment.BGMode.Sky,
                Sky = new Sky
                {
                    SkyMaterial = new ProceduralSkyMaterial
                    {
                        SkyTopColor = new Color(0.22f, 0.47f, 0.86f),
                        SkyHorizonColor = horizon,
                        SkyCurve = 0.1f,
                        // Below the horizon is sea, past the edge of the water mesh.
                        GroundHorizonColor = new Color(0.55f, 0.70f, 0.82f),
                        GroundBottomColor = new Color(0.04f, 0.22f, 0.42f),
                        SunAngleMax = 20,
                    },
                },
                AmbientLightSource = Godot.Environment.AmbientSource.Sky,
                AmbientLightEnergy = 0.55f,
                TonemapMode = Godot.Environment.ToneMapper.Filmic,
                TonemapExposure = 1.0f,
                TonemapWhite = 6.0f,
                FogEnabled = true,
                FogLightColor = horizon,
                FogDensity = 0.0001f,
                FogAerialPerspective = 0.35f,
                FogSkyAffect = 0.0f,
                GlowEnabled = true,
                GlowIntensity = 0.25f,
                AdjustmentEnabled = true,
                AdjustmentSaturation = 1.12f,
                AdjustmentContrast = 1.05f,
            };
        }
    }
}
