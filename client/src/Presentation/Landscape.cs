using System.Collections.Generic;
using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// The world's terrain, sea, flora, sun and sky, as the game and the world test both draw them
    /// (docs/design/art.md). <see cref="Follow"/> keeps them all centred on the camera.
    /// </summary>
    public partial class Landscape : Node3D
    {
        public readonly Terrain Terrain;
        public readonly Node3D Sea;
        public readonly Greenery Greenery;

        public Landscape(World.HeightMap map, World.LandRaster? land, IReadOnlyList<World.Area> areas)
        {
            Name = "Landscape";
            Terrain = new Terrain(map, land);
            AddChild(Terrain);
            Sea = Water.Create(faceted: false, new Palette(), horizon: 80000.0f, masks: Terrain.Masks);
            AddChild(Sea);
            Greenery = new Greenery(new World.FloraPlan(map, land, Terrain.MaskBytes, areas));
            AddChild(Greenery);
            AddChild(new DirectionalLight3D
            {
                Name = "Sun",
                RotationDegrees = new Vector3(-38, -35, 0),
                LightColor = new Color(1.0f, 0.93f, 0.82f),
                LightEnergy = 1.35f,
                ShadowEnabled = true,
                DirectionalShadowMaxDistance = 200,
            });
            AddChild(new WorldEnvironment { Environment = Environment() });
        }

        public void Follow(Vector3 eye)
        {
            Terrain.Focus = eye;
            Greenery.Focus = eye;
            Water.Follow(Sea, eye);
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
