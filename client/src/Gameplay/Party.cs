using System;
using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Net;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Our party as the server has it (docs/design/social.md), from <c>my_party</c> and
    /// <c>my_party_invites</c>, and the party's commands. Refusals come back through
    /// <see cref="Refused"/> for the chat box to show.
    /// </summary>
    public partial class Party : Node
    {
        private readonly Session session;

        public Party(Session session)
        {
            this.session = session;
            Name = "Party";
        }

        /// <summary>A party command or chat message the server refused, and why.</summary>
        public event Action<string>? Refused;

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            void Watch(ReducerEventContext ctx, string what)
            {
                if (ctx.Event.Status is Status.Failed(var reason))
                    Refused?.Invoke($"{what}: {reason}");
            }
            r.OnInviteToParty += (ctx, _) => Watch(ctx, "invite");
            r.OnAcceptPartyInvite += (ctx, _) => Watch(ctx, "accept");
            r.OnDeclinePartyInvite += (ctx, _) => Watch(ctx, "decline");
            r.OnLeaveParty += ctx => Watch(ctx, "leave");
            r.OnKickFromParty += (ctx, _) => Watch(ctx, "kick");
            r.OnMakePartyLeader += (ctx, _) => Watch(ctx, "lead");
            r.OnChat += (ctx, _, _) => Watch(ctx, "not sent");
            r.OnWhisper += (ctx, _, _) => Watch(ctx, "not sent");
        }

        /// <summary>Our character's membership, if we're in a party.</summary>
        public PartyMember? Mine =>
            session.Me is { } me ? session.Conn?.Db.MyParty.Iter().FirstOrDefault(m => m.CharId == me.CharId) : null;

        /// <summary>Everyone in our party, the oldest member first (the leader first of those who joined together, as the server orders them); empty with no party.</summary>
        public IReadOnlyList<PartyMember> Members
        {
            get
            {
                if (Mine is not { } mine)
                    return Array.Empty<PartyMember>();
                // View rows arrive as an insert and a delete, so a change may show twice for a moment.
                return session.Conn!.Db.MyParty.Iter().Where(m => m.PartyId == mine.PartyId)
                    .GroupBy(m => m.CharId).Select(g => g.Last())
                    .OrderBy(m => m.JoinedAt.MicrosecondsSinceUnixEpoch).ThenBy(m => !m.Leader).ThenBy(m => m.CharId).ToList();
            }
        }

        public bool Leading => Mine?.Leader == true;

        /// <summary>Whether a character is in our party with us.</summary>
        public bool IsMate(ulong charId) => charId != session.Me?.CharId && Members.Any(m => m.CharId == charId);

        /// <summary>The newest invitation to us that still stands.</summary>
        public PartyInvite? Invitation
        {
            get
            {
                if (session.Me is not { } me || session.Conn is null)
                    return null;
                long now = session.GameNowMicros;
                return session.Conn.Db.MyPartyInvites.Iter().Where(i => i.ToChar == me.CharId && !Sim.Party.Lapsed(i.MadeAt.MicrosecondsSinceUnixEpoch, now))
                    .OrderByDescending(i => i.MadeAt.MicrosecondsSinceUnixEpoch).FirstOrDefault();
            }
        }

        public void Invite(string name) => session.Conn?.Reducers.InviteToParty(name);

        /// <summary>Take the newest invitation; false if there's none.</summary>
        public bool Accept()
        {
            if (Invitation is not { } i)
                return false;
            session.Conn?.Reducers.AcceptPartyInvite(i.ScheduledId);
            return true;
        }

        public bool Decline()
        {
            if (Invitation is not { } i)
                return false;
            session.Conn?.Reducers.DeclinePartyInvite(i.ScheduledId);
            return true;
        }

        public void Leave() => session.Conn?.Reducers.LeaveParty();

        /// <summary>The member called <paramref name="name"/>, ignoring case.</summary>
        public PartyMember? Named(string name) => ChatModel.Named(Members, m => m.Name, name);

        public void Kick(PartyMember m) => session.Conn?.Reducers.KickFromParty(m.CharId);

        public void Lead(PartyMember m) => session.Conn?.Reducers.MakePartyLeader(m.CharId);
    }
}
