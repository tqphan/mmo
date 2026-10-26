using Godot;

namespace Mmo
{
    /// <summary>Local monotonic time in seconds, for movement timing and interpolation.</summary>
    public static class Clock
    {
        public static double Now => Time.GetTicksUsec() / 1_000_000.0;
    }

    /// <summary>Placeholder meshes until the art style test (BACKLOG).</summary>
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

        /// <summary>A stable, distinct colour per entity id.</summary>
        public static Color ColorFor(ulong id) => Color.FromHsv((id * 0.618034f) % 1.0f, 0.55f, 0.85f);
    }
}
