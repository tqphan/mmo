using System.Collections.Generic;
using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// The art style test (docs/design/art.md, ADR 0004): an offline scene with a procedural island,
    /// the sea, low-poly trees and rocks and CC0 characters walking about, in one of two looks.
    /// <c>flat</c> is faceted with banded colours; <c>gradient</c> has smooth normals and blended
    /// colours. Run it with <c>cargo xtask client --style-test flat</c>; no server is needed.
    /// </summary>
    public partial class StyleTest : Node3D
    {
        private readonly bool faceted;
        private readonly bool outline;
        private readonly string shot;
        private readonly Island island = new();
        private readonly Palette palette = new();
        private readonly List<Walker> walkers = new();
        private Camera3D camera = null!;
        private double time;

        public StyleTest(ClientOptions options)
        {
            faceted = options.StyleTest == "flat";
            outline = options.Outline;
            shot = options.Shot;
            Name = "StyleTest";
        }

        public override void _Ready()
        {
            AddChild(new WorldEnvironment { Environment = Environment() });
            AddChild(new DirectionalLight3D
            {
                Name = "Sun",
                RotationDegrees = new Vector3(-42, -35, 0),
                LightColor = new Color(1.0f, 0.95f, 0.86f),
                LightEnergy = 1.15f,
                ShadowEnabled = true,
                DirectionalShadowMaxDistance = 140,
            });
            AddChild(new MeshInstance3D
            {
                Name = "Island",
                Mesh = island.Mesh(faceted, palette),
                MaterialOverride = VertexColored(),
            });
            AddChild(Water.Create(faceted, palette));
            Scatter();

            var spot = ShoreSpot();
            AddWalker(new CharacterModel("res://assets/kaykit/Knight.glb", outline, "1H_Sword", "Round_Shield"), spot, 6.0f, 2.0f, "Walking_A");
            AddWalker(new CharacterModel("res://assets/kaykit/Rogue.glb", outline, "Knife"), spot, 11.0f, -5.4f, "Running_A");
            var idle = new CharacterModel("res://assets/kaykit/Rogue.glb", outline);
            AddChild(idle);
            idle.Position = Ground(spot + new Vector3(2, 0, 3));
            idle.RotationDegrees = new Vector3(0, 200, 0);
            idle.Play("Idle");

            camera = new Camera3D { Name = "Camera", Fov = 55, Far = 8000 };
            AddChild(camera);
        }

        public override void _Process(double delta)
        {
            time += delta;
            foreach (var w in walkers)
                w.Step(time, this);
            var target = walkers[0].Model.Position + new Vector3(0, 1.2f, 0);
            // The follow shot starts inland looking out to sea, and orbits.
            float yaw = Mathf.Atan2(-0.6f, -0.8f) + (float)time * 0.12f;
            Vector3 eye = shot switch
            {
                "wide" => new Vector3(Mathf.Sin(yaw) * 190, 85, Mathf.Cos(yaw) * 190),
                _ => target + new Basis(Vector3.Up, yaw) * new Vector3(0, 2.5f, 10),
            };
            camera.LookAtFromPosition(eye, shot == "wide" ? new Vector3(0, 0, 0) : target, Vector3.Up);
        }

        private Vector3 Ground(Vector3 p) => new(p.X, island.Height(p.X, p.Z), p.Z);

        /// <summary>A point 16 m inland from the beach, south-east of the centre; the walkers circle it.</summary>
        private Vector3 ShoreSpot()
        {
            var dir = new Vector3(0.6f, 0, 0.8f);
            float r = 0;
            while (r < Island.HalfSize && island.Height(dir.X * r, dir.Z * r) > 0.5f)
                r += 1;
            return dir * (r - 16);
        }

        private void AddWalker(CharacterModel model, Vector3 centre, float radius, float speed, string animation)
        {
            AddChild(model);
            model.Play(animation);
            walkers.Add(new Walker(model, centre, radius, speed));
        }

        /// <summary>Walks a circle on the ground; a negative speed goes the other way round.</summary>
        private sealed record Walker(CharacterModel Model, Vector3 Centre, float Radius, float Speed)
        {
            public void Step(double time, StyleTest scene)
            {
                float angle = (float)time * Speed / Radius;
                var p = Centre + new Vector3(Mathf.Cos(angle), 0, Mathf.Sin(angle)) * Radius;
                var ahead = new Vector3(-Mathf.Sin(angle), 0, Mathf.Cos(angle)) * Mathf.Sign(Speed);
                Model.Position = scene.Ground(p);
                // Facing -z along the direction of travel.
                Model.Rotation = new Vector3(0, Mathf.Atan2(-ahead.X, -ahead.Z), 0);
            }
        }

        private StandardMaterial3D VertexColored() => new()
        {
            VertexColorUseAsAlbedo = true,
            VertexColorIsSrgb = true,
            Roughness = 1.0f,
            SpecularMode = BaseMaterial3D.SpecularModeEnum.Disabled,
            DiffuseMode = faceted ? BaseMaterial3D.DiffuseModeEnum.Lambert : BaseMaterial3D.DiffuseModeEnum.LambertWrap,
        };

        /// <summary>Trees in noisy clumps on gentle grass, and rocks anywhere above the sea floor.</summary>
        private void Scatter()
        {
            var props = new Props(faceted, palette);
            var kinds = new[] { props.Pine(), props.Broadleaf(false), props.Broadleaf(true), props.Rock() };
            var placed = new List<Transform3D>[kinds.Length];
            for (int k = 0; k < kinds.Length; k++)
                placed[k] = new List<Transform3D>();
            var forest = new FastNoiseLite { Seed = 3, Frequency = 0.025f };
            var rng = new RandomNumberGenerator { Seed = 5 };
            var clearing = ShoreSpot();
            for (int i = 0; i < 6000; i++)
            {
                float x = rng.RandfRange(-Island.HalfSize, Island.HalfSize);
                float z = rng.RandfRange(-Island.HalfSize, Island.HalfSize);
                float h = island.Height(x, z);
                float up = island.Normal(x, z).Y;
                var at = new Vector3(x, h, z);
                if (at.DistanceTo(clearing) < 20)
                    continue;
                int kind;
                if (h > 1.2f && up > 0.86f && forest.GetNoise2D(x, z) > 0.1f && placed[0].Count + placed[1].Count + placed[2].Count < 320)
                    kind = h > 8 ? 0 : rng.Randf() < 0.15f ? 2 : rng.Randf() < 0.3f ? 0 : 1;
                else if (h > -1.0f && placed[3].Count < 70 && rng.Randf() < 0.02f + (1 - up))
                    kind = 3;
                else
                    continue;
                float scale = kind == 3 ? rng.RandfRange(0.5f, 2.2f) : rng.RandfRange(0.8f, 1.35f);
                var basis = new Basis(Vector3.Up, rng.Randf() * Mathf.Tau).Scaled(Vector3.One * scale);
                if (kind == 3)
                    basis = new Basis(new Vector3(1, 0, 0), rng.RandfRange(-0.3f, 0.3f)) * basis;
                placed[kind].Add(new Transform3D(basis, at - new Vector3(0, kind == 3 ? 0.3f * scale : 0.1f, 0)));
            }
            var material = VertexColored();
            for (int k = 0; k < kinds.Length; k++)
            {
                var multi = new MultiMesh { TransformFormat = MultiMesh.TransformFormatEnum.Transform3D, Mesh = kinds[k], InstanceCount = placed[k].Count };
                for (int i = 0; i < placed[k].Count; i++)
                    multi.SetInstanceTransform(i, placed[k][i]);
                AddChild(new MultiMeshInstance3D { Name = $"Props{k}", Multimesh = multi, MaterialOverride = material });
            }
        }

        /// <summary>Sky, ambient light, tonemap, fog and glow; the world test reuses it with thinner fog.</summary>
        internal static Godot.Environment Environment(float fog = 0.0008f)
        {
            var horizon = new Color(0.78f, 0.89f, 0.96f);
            return new Godot.Environment
            {
                BackgroundMode = Godot.Environment.BGMode.Sky,
                Sky = new Sky
                {
                    SkyMaterial = new ProceduralSkyMaterial
                    {
                        SkyTopColor = new Color(0.28f, 0.55f, 0.90f),
                        SkyHorizonColor = horizon,
                        SkyCurve = 0.12f,
                        // Below the horizon is sea, past the edge of the water mesh.
                        GroundHorizonColor = new Color(0.66f, 0.79f, 0.90f),
                        GroundBottomColor = new Color(0.07f, 0.35f, 0.60f),
                        SunAngleMax = 20,
                    },
                },
                AmbientLightSource = Godot.Environment.AmbientSource.Sky,
                AmbientLightEnergy = 0.7f,
                TonemapMode = Godot.Environment.ToneMapper.Filmic,
                TonemapExposure = 0.95f,
                FogEnabled = true,
                FogLightColor = horizon,
                FogDensity = fog,
                FogAerialPerspective = 0.2f,
                FogSkyAffect = 0.0f,
                GlowEnabled = true,
                GlowIntensity = 0.3f,
            };
        }
    }
}
