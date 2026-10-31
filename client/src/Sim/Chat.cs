using System.Linq;
using System.Numerics;
using Mmo.Content;

namespace Mmo.Sim
{
    /// <summary>Where a message goes. The value is the wire value in the message rows.</summary>
    public enum Channel : byte
    {
        Local = 0,
        Party = 1,
        Whisper = 2,
    }

    /// <summary>
    /// Chat's rules (docs/design/social.md): channel names, what a message may hold and how far
    /// Local carries. A port of <c>sim::chat</c>; the server decides, and keeps the rate limit.
    /// </summary>
    public static class Chat
    {
        private static ChatRules R => GameContent.Chat;

        /// <summary>What players see.</summary>
        public static string Name(Channel c) => c switch
        {
            Channel.Local => "Local",
            Channel.Party => "Party",
            _ => "Whisper",
        };

        /// <summary><paramref name="text"/> trimmed, or null with <paramref name="refusal"/> saying why it would be refused.</summary>
        public static string? Clean(string text, out string refusal)
        {
            refusal = "";
            // Rust's trim and is_control: Unicode white space, and the C0 and C1 controls.
            var t = text.Trim();
            if (t.Length == 0)
                refusal = "say something";
            else if (t.Any(char.IsControl))
                refusal = "messages are one line of text";
            else if (t.EnumerateRunes().Count() > R.MaxChars)
                refusal = "that message is too long";
            return refusal.Length == 0 ? t : null;
        }

        /// <summary>Whether someone at <paramref name="at"/> hears Local chat said at <paramref name="from"/>: horizontally within reach.</summary>
        public static bool Hears(Vector3 from, Vector3 at)
        {
            float dx = at.X - from.X, dz = at.Z - from.Z;
            return dx * dx + dz * dz <= R.LocalM * R.LocalM;
        }
    }

    /// <summary>
    /// Party rules (docs/design/social.md): the size, and when an invitation lapses. A port of
    /// <c>sim::party</c>; the server decides.
    /// </summary>
    public static class Party
    {
        private const long MicrosPerS = 1_000_000;

        public static int Size => GameContent.Party.Size;

        public static long LapsesAt(long madeAt) => madeAt + GameContent.Party.InviteS * MicrosPerS;

        public static bool Lapsed(long madeAt, long now) => now >= LapsesAt(madeAt);
    }
}
