using Godot;

namespace Mmo.Gameplay
{
    /// <summary>
    /// The game's polled keys, which go quiet while the chat box has the keyboard. Godot's
    /// <see cref="Input"/> follows the keys whatever has focus, so everything the game polls goes
    /// through here; key events (<c>_UnhandledInput</c>) already stop at the focused text box.
    /// </summary>
    public static class GameInput
    {
        /// <summary>True while a line is being typed (docs/design/social.md).</summary>
        public static bool Typing { get; set; }

        public static bool JustPressed(StringName action) => !Typing && Input.IsActionJustPressed(action);

        public static bool Pressed(StringName action) => !Typing && Input.IsActionPressed(action);

        public static float Strength(StringName action) => Typing ? 0.0f : Input.GetActionStrength(action);

        public static Vector2 Vector(StringName left, StringName right, StringName up, StringName down) =>
            Typing ? Vector2.Zero : Input.GetVector(left, right, up, down);

        /// <summary>A drag with the left mouse button that began in the world (not over a window): turns the camera.</summary>
        public static bool LeftDrag { get; set; }

        /// <summary>A drag with the right mouse button that began in the world: turns the camera, and the character to face its way.</summary>
        public static bool RightDrag { get; set; }

        /// <summary>Both mouse buttons held in the world: run forward, the way the camera faces, as in most MMOs.</summary>
        public static bool MouseRun => LeftDrag && RightDrag;
    }
}
