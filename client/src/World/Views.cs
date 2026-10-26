using Godot;

namespace Mmo
{
    /// <summary>Local monotonic time in seconds, for movement timing and interpolation.</summary>
    public static class Clock
    {
        public static double Now => Time.GetTicksUsec() / 1_000_000.0;
    }

    /// <summary>Placeholder meshes until the look is chosen (ADR 0004) and characters get models.</summary>
    public static class Views
    {
        /// <summary>A capsule standing on its origin, with a nose pointing along its facing (-z).</summary>
        public static Node3D Capsule(Color color)
        {
            var root = new Node3D { Name = "Body" };
            var material = new StandardMaterial3D { AlbedoColor = color, Roughness = 0.8f };
            root.AddChild(new MeshInstance3D
            {
                Mesh = new CapsuleMesh { Radius = 0.35f, Height = 1.8f, Material = material },
                Position = new Vector3(0, 0.9f, 0),
            });
            root.AddChild(new MeshInstance3D
            {
                Mesh = new BoxMesh { Size = new Vector3(0.18f, 0.12f, 0.3f), Material = new StandardMaterial3D { AlbedoColor = color.Darkened(0.5f) } },
                Position = new Vector3(0, 1.45f, -0.35f),
            });
            return root;
        }

        /// <summary>A hull-sized box for a ship, deck at its origin.</summary>
        public static Node3D Ship(Vector3 size)
        {
            var root = new Node3D { Name = "Hull" };
            root.AddChild(new MeshInstance3D
            {
                Mesh = new BoxMesh { Size = size, Material = new StandardMaterial3D { AlbedoColor = new Color(0.45f, 0.3f, 0.18f) } },
                Position = new Vector3(0, -size.Y / 2, 0),
            });
            return root;
        }

        /// <summary>The helm: a post with a wheel, standing on the deck at <paramref name="at"/>.</summary>
        public static Node3D Helm(Vector3 at)
        {
            var root = new Node3D { Name = "Helm", Position = at };
            var wood = new StandardMaterial3D { AlbedoColor = new Color(0.3f, 0.2f, 0.12f) };
            root.AddChild(new MeshInstance3D { Mesh = new CylinderMesh { TopRadius = 0.08f, BottomRadius = 0.1f, Height = 1.0f, Material = wood }, Position = new Vector3(0, 0.5f, 0) });
            root.AddChild(new MeshInstance3D
            {
                Mesh = new TorusMesh { InnerRadius = 0.32f, OuterRadius = 0.4f, Material = wood },
                Position = new Vector3(0, 1.1f, 0),
                // Upright, facing the bow.
                RotationDegrees = new Vector3(90, 0, 0),
            });
            return root;
        }

        /// <summary>A cannon on deck at <paramref name="at"/>, its barrel along <paramref name="facing"/> (a yaw on deck).</summary>
        public static Node3D Cannon(Vector3 at, float facing)
        {
            var root = new Node3D { Name = "Cannon", Position = at, Rotation = new Vector3(0, facing, 0) };
            var iron = new StandardMaterial3D { AlbedoColor = new Color(0.15f, 0.15f, 0.17f), Metallic = 0.6f, Roughness = 0.5f };
            root.AddChild(new MeshInstance3D { Mesh = new BoxMesh { Size = new Vector3(0.7f, 0.4f, 0.8f), Material = new StandardMaterial3D { AlbedoColor = new Color(0.35f, 0.24f, 0.14f) } }, Position = new Vector3(0, 0.2f, 0) });
            root.AddChild(new MeshInstance3D
            {
                Mesh = new CylinderMesh { TopRadius = 0.13f, BottomRadius = 0.18f, Height = 1.6f, Material = iron },
                // Lying along -z, the muzzle 1 m up and over the rail.
                Position = new Vector3(0, 0.75f, -0.5f),
                RotationDegrees = new Vector3(-90, 0, 0),
            });
            return root;
        }

        /// <summary>A stable, distinct colour per entity id.</summary>
        public static Color ColorFor(ulong id) => Color.FromHsv((id * 0.618034f) % 1.0f, 0.55f, 0.85f);
    }
}
