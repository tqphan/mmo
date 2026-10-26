using Godot;

namespace Mmo.Presentation
{
    /// <summary>
    /// A rigged character from <c>assets/</c> (docs/design/art.md), standing on its origin and facing
    /// -z like the rest of the client. Plays one looping animation at a time.
    /// </summary>
    public partial class CharacterModel : Node3D
    {
        private readonly string path;
        private readonly bool outline;
        private readonly string[] held;
        private AnimationPlayer? animations;

        /// <param name="path">A glTF scene such as <c>res://assets/kaykit/Knight.glb</c>.</param>
        /// <param name="outline">Draw a dark inverted-hull outline around every mesh.</param>
        /// <param name="held">The props to keep in its hands, by mesh name; it comes holding all of them.</param>
        public CharacterModel(string path, bool outline, params string[] held)
        {
            this.path = path;
            this.outline = outline;
            this.held = held;
            Name = System.IO.Path.GetFileNameWithoutExtension(path);
        }

        public override void _Ready()
        {
            var model = GD.Load<PackedScene>(path).Instantiate<Node3D>();
            // glTF characters face +z.
            model.RotationDegrees = new Vector3(0, 180, 0);
            AddChild(model);
            animations = model.FindChild("AnimationPlayer", true, false) as AnimationPlayer;
            foreach (var node in model.FindChildren("*", nameof(MeshInstance3D), true, false))
            {
                var mesh = (MeshInstance3D)node;
                if (mesh.GetParent() is BoneAttachment3D { BoneName: var bone } && bone.StartsWith("handslot", System.StringComparison.Ordinal)
                    && System.Array.IndexOf(held, mesh.Name.ToString()) < 0)
                {
                    mesh.Visible = false;
                    continue;
                }
                if (outline)
                    mesh.MaterialOverlay = Outline;
            }
        }

        /// <summary>Loop <paramref name="name"/> (one of the clips in the glTF) at <paramref name="speed"/>.</summary>
        public void Play(string name, float speed = 1.0f)
        {
            if (animations == null || !animations.HasAnimation(name))
            {
                GD.PushWarning($"{Name} has no animation {name}");
                return;
            }
            animations.GetAnimation(name).LoopMode = Animation.LoopModeEnum.Linear;
            animations.Play(name, customBlend: 0.2, customSpeed: speed);
        }

        private static readonly StandardMaterial3D Outline = new()
        {
            ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded,
            CullMode = BaseMaterial3D.CullModeEnum.Front,
            AlbedoColor = new Color(0.12f, 0.09f, 0.08f),
            Grow = true,
            GrowAmount = 0.025f,
        };
    }
}
