using System;
using System.Linq;
using System.Threading.Tasks;
using Godot;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.World;

namespace Mmo.UI
{
    /// <summary>
    /// <c>--party-demo</c> (dev builds, docs/design/social.md), with <c>bot visitor</c> nearby, which
    /// takes invitations and answers what's said to it: once in the world, goes up to the nearest
    /// other character, then types each step into the chat box as a player would. It invites the
    /// visitor and waits for it to join, says hello aloud, in the party and in a whisper, waiting for
    /// each answer, replies with /r, and leaves the party. It pauses at each step for captures, and
    /// logs what it saw.
    /// </summary>
    public partial class PartyDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Party party;
        private readonly ChatBox chat;
        private bool started;

        public PartyDemo(Session session, PlayerController player, EntityRegistry entities, Party party, ChatBox chat)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.party = party;
            this.chat = chat;
            Name = "PartyDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null || session.Near is not { Ready: true })
                return;
            started = true;
            _ = Run();
        }

        private async Task Run()
        {
            RemoteView? Visitor() => entities.Views.Where(v => v.Kind == EntityKind.Character && v.Visible)
                .OrderBy(v => v.GlobalPosition.DistanceSquaredTo(player.GlobalPosition)).FirstOrDefault();
            if (!await Until(() => Visitor() is not null, 10))
            {
                session.Log("party demo: nobody to talk to; run `cargo xtask bot visitor` alongside");
                return;
            }
            var visitor = Visitor()!;
            if (party.Mine is not null)
            {
                party.Leave();
                await Until(() => party.Mine is null, 5);
            }
            var at = visitor.GlobalPosition;
            player.DevTeleport(at.X + 6, at.Z + 6);
            await Wait(2.0);
            string name = visitor.Label;
            session.Log($"party demo: talking to {name}");

            await Type($"/invite {name}");
            if (!await Until(() => party.Members.Count == 2, 8))
            {
                session.Log($"party demo: {name} didn't join; {string.Join(" | ", chat.Lines.TakeLast(3))}");
                return;
            }
            session.Log($"party demo: a party of {party.Members.Count}, led by {party.Members.First(m => m.Leader).Name}");
            await Wait(2.0);

            foreach (var (line, answer) in new[] { ("well met, traveller", "heard: well met"), ($"/p party check", "heard: party check"), ($"/w {name} a word in private", "heard: a word in private") })
            {
                await Type(line);
                bool answered = await Until(() => chat.Lines.Any(l => l.Contains(answer)), 6);
                session.Log($"party demo: said \"{line}\"; {(answered ? "answered" : "no answer")}: {chat.Lines.LastOrDefault()}");
                await Wait(2.0);
            }
            session.Log($"party demo: /r goes to {chat.ReplyTo}");
            await Type("/r thanks");
            await Wait(3.0);
            await Type("/leave");
            await Until(() => party.Mine is null, 5);
            await Wait(2.0);
            session.Log($"party demo: done; the log ends: {string.Join(" | ", chat.Lines.TakeLast(4))}");
        }

        /// <summary>Open the chat line, type <paramref name="text"/>, hold it there a moment, and send it.</summary>
        private async Task Type(string text)
        {
            chat.Begin("");
            for (int i = 1; i <= text.Length; i += 3)
            {
                chat.Begin(text[..i]);
                await Wait(0.03);
            }
            chat.Begin(text);
            await Wait(1.2);
            chat.Send();
        }

        private async Task<bool> Until(Func<bool> cond, double secs)
        {
            var limit = DateTime.UtcNow.AddSeconds(secs);
            while (!cond())
            {
                if (DateTime.UtcNow > limit)
                {
                    session.Log("party demo: timed out");
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
