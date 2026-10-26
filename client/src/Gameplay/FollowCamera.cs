using Godot;

namespace Mmo.Gameplay
{
    /// <summary>
    /// A third-person camera behind the player. Q and E, or dragging with the right mouse button,
    /// turn it; the wheel zooms.
    /// </summary>
    public partial class FollowCamera : Camera3D
    {
        private const float Pitch = -0.35f;
        private const float TurnSpeed = 2.0f;
        private const float MouseTurn = 0.005f;

        private readonly PlayerController player;
        private float yaw;
        private float distance = 8.0f;

        public FollowCamera(PlayerController player, float yawDeg)
        {
            this.player = player;
            yaw = Mathf.DegToRad(yawDeg);
            Name = "Camera";
            Fov = 60;
        }

        public override void _UnhandledInput(InputEvent e)
        {
            if (e is InputEventMouseMotion m && Input.IsMouseButtonPressed(MouseButton.Right))
                yaw -= m.Relative.X * MouseTurn;
            else if (e is InputEventMouseButton { Pressed: true } b)
            {
                if (b.ButtonIndex == MouseButton.WheelUp)
                    distance = Mathf.Max(3.0f, distance - 1.0f);
                else if (b.ButtonIndex == MouseButton.WheelDown)
                    distance = Mathf.Min(30.0f, distance + 1.0f);
            }
        }

        public override void _Process(double delta)
        {
            yaw += (Input.GetActionStrength("turn_left") - Input.GetActionStrength("turn_right")) * TurnSpeed * (float)delta;
            player.CameraYaw = yaw;
            var target = player.GlobalPosition + new Vector3(0, 1.6f, 0);
            var offset = new Basis(Vector3.Up, yaw) * new Basis(Vector3.Right, Pitch) * new Vector3(0, 0, distance);
            LookAtFromPosition(target + offset, target, Vector3.Up);
        }
    }
}
