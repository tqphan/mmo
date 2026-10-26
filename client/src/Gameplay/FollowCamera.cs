using Godot;
using Mmo.World;

namespace Mmo.Gameplay
{
    /// <summary>
    /// A third-person camera behind the player. Q and E, or dragging with the right mouse button,
    /// turn it; the wheel zooms. It stays above the ground, turns with the ship we're aboard, and
    /// looks along the cannon we're manning.
    /// </summary>
    public partial class FollowCamera : Camera3D
    {
        private const float Pitch = -0.35f;
        private const float TurnSpeed = 2.0f;
        private const float MouseTurn = 0.005f;
        /// <summary>The least height above the ground at the camera, so it doesn't look out from inside a hill.</summary>
        private const float Clearance = 0.5f;

        /// <summary>How fast the camera swings round to look along a cannon, per second.</summary>
        private const float AimFollowRate = 6.0f;

        private readonly PlayerController player;
        private readonly HeightMap map;
        private float yaw;
        private float distance = 8.0f;
        private ulong lastFrame;
        private float lastFrameYaw;

        public FollowCamera(PlayerController player, float yawDeg, HeightMap map)
        {
            this.player = player;
            this.map = map;
            yaw = Mathf.DegToRad(yawDeg);
            Name = "Camera";
            Fov = 60;
            Far = 20000;
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
            // Aboard, the view turns with the ship.
            float frameYaw = player.FrameYaw;
            if (player.Parent != 0 && player.Parent == lastFrame)
                yaw += Sim.Yaw.Delta(lastFrameYaw, frameYaw);
            (lastFrame, lastFrameYaw) = (player.Parent, frameYaw);
            // Manning a cannon, look where it points.
            if (player.Controls?.AimWorldYaw is float aim)
                yaw += Sim.Yaw.Delta(yaw, aim) * (1 - Mathf.Exp(-AimFollowRate * (float)delta));
            player.CameraYaw = yaw;
            var target = player.GlobalPosition + new Vector3(0, 1.6f, 0);
            var offset = new Basis(Vector3.Up, yaw) * new Basis(Vector3.Right, Pitch) * new Vector3(0, 0, distance);
            var eye = target + offset;
            eye.Y = Mathf.Max(eye.Y, map.Height(eye.X, eye.Z) + Clearance);
            LookAtFromPosition(eye, target, Vector3.Up);
        }
    }
}
