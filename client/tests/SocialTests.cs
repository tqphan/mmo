using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>Chat's rules, the chat box's commands and lines, and the party frame (docs/design/social.md).</summary>
    public class SocialTests
    {
        private const long T0 = 1_800_000_000_000_000;
        private const long S = 1_000_000;

        [Fact]
        public void RulesComeFromContent()
        {
            Assert.Equal(5, Sim.Party.Size);
            Assert.Equal(200, GameContent.Chat.MaxChars);
            Assert.Equal("Whisper", Chat.Name(Channel.Whisper));
            Assert.Equal(2, (byte)Channel.Whisper);
        }

        [Fact]
        public void MessagesAreCleanedAsTheServerDoes()
        {
            Assert.Equal("hello there", Chat.Clean("  hello there \t", out _));
            Assert.Null(Chat.Clean("   ", out var why));
            Assert.Equal("say something", why);
            Assert.Null(Chat.Clean("two\nlines", out why));
            Assert.Equal("messages are one line of text", why);
            var most = new string('é', 200);
            Assert.Equal(most, Chat.Clean(most, out _));
            Assert.Null(Chat.Clean(new string('a', 201), out why));
            Assert.Equal("that message is too long", why);
            // Characters, not UTF-16 units: an emoji outside the BMP counts once.
            Assert.NotNull(Chat.Clean(string.Concat(System.Linq.Enumerable.Repeat("\U0001F600", 200)), out _));
        }

        [Fact]
        public void LocalCarriesHorizontally()
        {
            var from = new Vector3(10, 0, 10);
            Assert.True(Chat.Hears(from, new Vector3(50, 0, 10)));
            Assert.False(Chat.Hears(from, new Vector3(39, 0, 39)));
            Assert.True(Chat.Hears(from, new Vector3(10, 300, 10)));
        }

        [Fact]
        public void InvitationsLapse()
        {
            Assert.Equal(T0 + 60 * S, Sim.Party.LapsesAt(T0));
            Assert.False(Sim.Party.Lapsed(T0, T0 + 60 * S - 1));
            Assert.True(Sim.Party.Lapsed(T0, T0 + 60 * S));
            Assert.Equal("Mira invites you to a party (60 s): /accept or /decline", ChatModel.InviteLine("Mira", T0, T0));
            Assert.Equal("Mira invites you to a party (1 s): /accept or /decline", ChatModel.InviteLine("Mira", T0, T0 + 59 * S + 1));
            Assert.Equal("", ChatModel.InviteLine("Mira", T0, T0 + 60 * S));
        }

        [Fact]
        public void PlainLinesGoToTheCurrentChannel()
        {
            Assert.Null(ChatModel.Parse("   ", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Say, Channel.Local, Text: "hello"), ChatModel.Parse(" hello ", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Say, Channel.Party, Text: "hello"), ChatModel.Parse("hello", Channel.Party, null));
            Assert.Equal(new Command(CommandKind.Say, Channel.Party, Text: "on my way"), ChatModel.Parse("/p on my way", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Say, Channel.Local, Text: "hi"), ChatModel.Parse("/S hi", Channel.Party, null));
            Assert.Equal(new Command(CommandKind.Switch, Channel.Party), ChatModel.Parse("/party", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Switch, Channel.Local), ChatModel.Parse("/s", Channel.Party, null));
        }

        [Fact]
        public void WhispersAndReplies()
        {
            Assert.Equal(new Command(CommandKind.Whisper, Channel.Whisper, "Tobin", "a word, please"), ChatModel.Parse("/w Tobin  a word, please", Channel.Local, null));
            Assert.Equal(CommandKind.Error, ChatModel.Parse("/w Tobin", Channel.Local, null)!.Kind);
            Assert.Equal(new Command(CommandKind.Error, Text: "nobody has whispered to you yet"), ChatModel.Parse("/r hi", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Whisper, Channel.Whisper, "Wren", "hi"), ChatModel.Parse("/r hi", Channel.Local, "Wren"));
        }

        [Fact]
        public void PartyCommands()
        {
            Assert.Equal(new Command(CommandKind.Invite, Name: "Tobin"), ChatModel.Parse("/invite Tobin", Channel.Local, null));
            Assert.Equal(CommandKind.Error, ChatModel.Parse("/invite", Channel.Local, null)!.Kind);
            Assert.Equal(CommandKind.Accept, ChatModel.Parse("/accept", Channel.Local, null)!.Kind);
            Assert.Equal(CommandKind.Decline, ChatModel.Parse("/decline", Channel.Local, null)!.Kind);
            Assert.Equal(CommandKind.Leave, ChatModel.Parse("/leave", Channel.Local, null)!.Kind);
            Assert.Equal(new Command(CommandKind.Kick, Name: "Wren"), ChatModel.Parse("/kick Wren", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Lead, Name: "Wren"), ChatModel.Parse("/lead Wren", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Help, Text: ChatModel.Help), ChatModel.Parse("/help", Channel.Local, null));
            Assert.Equal(new Command(CommandKind.Error, Text: "no command /dance; /help lists them"), ChatModel.Parse("/dance", Channel.Local, null));
            var members = new[] { "Mira", "Tobin" };
            Assert.Equal("Tobin", ChatModel.Named(members, m => m, "tobin"));
            Assert.Null(ChatModel.Named(members, m => m, "Wren"));
        }

        [Fact]
        public void LinesNameTheChannelAndTheSpeaker()
        {
            Assert.Equal("[Local] Mira: well met", ChatModel.Line(Channel.Local, "Mira", "", "well met", false));
            Assert.Equal("[Party] Tobin: on my way", ChatModel.Line(Channel.Party, "Tobin", "", "on my way", false));
            Assert.Equal("[Party] Tobin joined the party", ChatModel.Line(Channel.Party, "", "", "Tobin joined the party", false));
            Assert.Equal("[Whisper] Mira whispers: psst", ChatModel.Line(Channel.Whisper, "Mira", "Wren", "psst", false));
            Assert.Equal("[Whisper] to Wren: psst", ChatModel.Line(Channel.Whisper, "Mira", "Wren", "psst", true));
            Assert.Equal("Party:", ChatModel.Prompt(Channel.Party));
        }

        [Fact]
        public void TheFrameShowsWhereEachMemberIs()
        {
            var v = new VitalsInfo(820, 1000, 100, 100, T0, T0, null, 0, 0, 0, Level: 3);
            Assert.Equal("★ Mira  Lv 3  820/1,000", ChatModel.MemberLine("Mira", true, true, v with { CombatUntil = T0 + 60 * S }, T0));
            Assert.Equal("Tobin  (far)", ChatModel.MemberLine("Tobin", false, true, null, T0));
            Assert.Equal("Tobin  (away)", ChatModel.MemberLine("Tobin", false, false, null, T0));
            Assert.Equal("Tobin  Lv 3  dead", ChatModel.MemberLine("Tobin", false, true, v with { Hp = 0, DeadAt = T0 }, T0));
        }
    }
}
