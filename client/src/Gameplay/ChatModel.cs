using System;
using System.Linq;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>What a typed line asks for.</summary>
    public enum CommandKind
    {
        /// <summary>Say <c>Text</c> on <c>Channel</c> (Local or Party).</summary>
        Say,
        /// <summary>Make <c>Channel</c> the one plain lines go to.</summary>
        Switch,
        /// <summary>Whisper <c>Text</c> to <c>Name</c>.</summary>
        Whisper,
        Invite,
        Accept,
        Decline,
        Leave,
        /// <summary>Remove <c>Name</c> from the party.</summary>
        Kick,
        /// <summary>Hand the lead to <c>Name</c>.</summary>
        Lead,
        Help,
        /// <summary>Nothing sent; <c>Text</c> says why.</summary>
        Error,
    }

    public sealed record Command(CommandKind Kind, Channel Channel = Channel.Local, string Name = "", string Text = "");

    /// <summary>
    /// The chat box's rules (docs/design/social.md), kept out of Godot so they can be tested: what a
    /// typed line means, how a message is shown, and the party frame's lines.
    /// </summary>
    public static class ChatModel
    {
        public const string Help = "/s Local · /p party · /w Name whisper · /r reply · /invite Name · /accept · /decline · /leave · /kick Name · /lead Name";

        /// <summary>
        /// What <paramref name="line"/> asks for, with plain lines going to <paramref name="current"/>
        /// and /r to <paramref name="replyTo"/>, the last character to whisper to us. Null for a blank line.
        /// </summary>
        public static Command? Parse(string line, Channel current, string? replyTo)
        {
            var t = line.Trim();
            if (t.Length == 0)
                return null;
            if (!t.StartsWith('/'))
                return new Command(CommandKind.Say, current, Text: t);
            int space = t.IndexOf(' ');
            string verb = (space < 0 ? t[1..] : t[1..space]).ToLowerInvariant();
            string rest = space < 0 ? "" : t[(space + 1)..].Trim();
            var (first, after) = Split(rest);
            switch (verb)
            {
                case "s" or "say" or "l" or "local":
                    return rest.Length == 0 ? new Command(CommandKind.Switch, Channel.Local) : new Command(CommandKind.Say, Channel.Local, Text: rest);
                case "p" or "party":
                    return rest.Length == 0 ? new Command(CommandKind.Switch, Channel.Party) : new Command(CommandKind.Say, Channel.Party, Text: rest);
                case "w" or "whisper" or "tell":
                    return first.Length == 0 || after.Length == 0 ? Error("whisper with /w Name message") : new Command(CommandKind.Whisper, Channel.Whisper, first, after);
                case "r" or "reply":
                    if (replyTo is null)
                        return Error("nobody has whispered to you yet");
                    return rest.Length == 0 ? Error("reply with /r message") : new Command(CommandKind.Whisper, Channel.Whisper, replyTo, rest);
                case "invite":
                    return first.Length == 0 ? Error("invite with /invite Name") : new Command(CommandKind.Invite, Name: first);
                case "accept":
                    return new Command(CommandKind.Accept);
                case "decline":
                    return new Command(CommandKind.Decline);
                case "leave":
                    return new Command(CommandKind.Leave);
                case "kick":
                    return first.Length == 0 ? Error("kick with /kick Name") : new Command(CommandKind.Kick, Name: first);
                case "lead":
                    return first.Length == 0 ? Error("hand the lead over with /lead Name") : new Command(CommandKind.Lead, Name: first);
                case "help" or "?":
                    return new Command(CommandKind.Help, Text: Help);
                default:
                    return Error($"no command /{verb}; /help lists them");
            }
        }

        private static Command Error(string why) => new(CommandKind.Error, Text: why);

        private static (string First, string After) Split(string s)
        {
            int space = s.IndexOf(' ');
            return space < 0 ? (s, "") : (s[..space], s[(space + 1)..].Trim());
        }

        /// <summary>
        /// A message as the log shows it. <paramref name="from"/> is empty for the party's own notices;
        /// <paramref name="mine"/> is our own copy of a whisper we sent, to <paramref name="to"/>.
        /// </summary>
        public static string Line(Channel channel, string from, string to, string text, bool mine) => channel switch
        {
            Channel.Whisper when mine => $"[{Chat.Name(channel)}] to {to}: {text}",
            Channel.Whisper => $"[{Chat.Name(channel)}] {from} whispers: {text}",
            _ when from.Length == 0 => $"[{Chat.Name(channel)}] {text}",
            _ => $"[{Chat.Name(channel)}] {from}: {text}",
        };

        /// <summary>The box's prompt while typing, naming where a plain line goes.</summary>
        public static string Prompt(Channel current) => $"{Chat.Name(current)}:";

        /// <summary>
        /// A party member's line in the frame: a star for the leader, then the level and HP when we
        /// can see it, or where it is otherwise.
        /// </summary>
        public static string MemberLine(string name, bool leader, bool inWorld, VitalsInfo? seen, long now)
        {
            var head = (leader ? "★ " : "") + name;
            if (!inWorld)
                return head + "  (away)";
            if (seen is not { } v)
                return head + "  (far)";
            if (v.Dead)
                return head + $"  Lv {v.Level}  dead";
            return head + $"  Lv {v.Level}  {v.HpNow(now):N0}/{v.HpMax:N0}";
        }

        /// <summary>An invitation to us, while it stands; empty once it has lapsed.</summary>
        public static string InviteLine(string from, long madeAt, long now)
        {
            long left = Sim.Party.LapsesAt(madeAt) - now;
            if (left <= 0)
                return "";
            return $"{from} invites you to a party ({(left + 999_999) / 1_000_000} s): /accept or /decline";
        }

        /// <summary>The member called <paramref name="name"/>, ignoring case, if any.</summary>
        public static T? Named<T>(System.Collections.Generic.IEnumerable<T> members, Func<T, string> nameOf, string name) where T : class =>
            members.FirstOrDefault(m => string.Equals(nameOf(m), name, StringComparison.OrdinalIgnoreCase));
    }
}
