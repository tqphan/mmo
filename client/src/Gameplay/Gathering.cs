using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Gathering from resource nodes (docs/design/gathering.md). On land, F gathers the nearest
    /// ready node in reach; the server times the gather and decides what it yields. While our
    /// entity is a node's gatherer, <see cref="Bar"/> says how far it has got. Walking away cancels
    /// it on the server; we send <c>cancel_gather</c> as soon as we start moving, so the bar stops at once.
    /// </summary>
    public partial class Gathering : Node
    {
        /// <summary>How far to look for a node to name in the prompt when none is in reach.</summary>
        private const float NoticeM = 12.0f;

        private readonly Session session;
        private readonly PlayerController player;
        private readonly NodeRegistry nodes;
        /// <summary>The node we're gathering, as the server last showed it, and our bag's count of its item then.</summary>
        private ResourceNode? active;
        private uint heldAtStart;
        private bool cancelSent;

        public Gathering(Session session, PlayerController player, NodeRegistry nodes)
        {
            this.session = session;
            this.player = player;
            this.nodes = nodes;
            Name = "Gathering";
        }

        /// <summary>What the HUD says about nodes nearby.</summary>
        public string Prompt { get; private set; } = "";

        /// <summary>The node kind being gathered and how far it has got, while we're gathering.</summary>
        public (NodeData Kind, float Progress)? Bar { get; private set; }

        public int Gathered { get; private set; }

        /// <summary>F was pressed on land: start gathering the nearest ready node in reach. False if there's none, so F can climb aboard instead.</summary>
        public bool Interact()
        {
            if (session.Conn is not { } conn || player.Parent != 0 || player.Mode == MoveMode.Swim || active is not null)
                return false;
            if (NearestInReach(ready: true) is not { } n)
                return false;
            conn.Reducers.Gather(n.Id);
            session.Log($"gathering {GameContent.Node(n.Kind)?.Name} (node {n.Id})");
            return true;
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            ulong me = session.Me?.EntityId ?? 0;
            Track(me);
            Bar = active is { } a && GameContent.Node(a.Kind) is { } k ? (k, Sim.Gather.Progress(k, a.DoneAt.MicrosecondsSinceUnixEpoch, now)) : null;
            if (active is { } busy && !cancelSent && new Vector2(player.Velocity.X, player.Velocity.Z).Length() > MoveSender.StillMps)
            {
                session.Conn?.Reducers.CancelGather();
                cancelSent = true;
                session.Log($"moved: cancelled gathering node {busy.Id}");
            }
            Prompt = MakePrompt(now);
        }

        /// <summary>Follow the node we're gathering: a gather starts when the server makes us its gatherer, and ends when it stops.</summary>
        private void Track(ulong me)
        {
            if (active is { } a)
            {
                var row = nodes.Find(a.Id);
                if (row is not null && row.Gatherer == me && me != 0)
                {
                    active = row;
                    return;
                }
                var kind = GameContent.Node(a.Kind);
                uint got = kind is null ? 0 : Held(kind.Item) - heldAtStart;
                if (got > 0)
                {
                    Gathered++;
                    session.Log($"gathered {got} {GameContent.Item(kind!.Item)?.Name} from {kind.Name}; labor now {LaborNow()}");
                }
                else
                {
                    session.Log($"gathering node {a.Id} stopped with nothing gathered");
                }
                active = null;
                return;
            }
            if (me == 0)
                return;
            var mine = nodes.Rows.FirstOrDefault(n => n.Gatherer == me);
            if (mine is not null && GameContent.Node(mine.Kind) is { } k)
            {
                active = mine;
                cancelSent = false;
                heldAtStart = Held(k.Item);
            }
        }

        private string MakePrompt(long now)
        {
            if (Bar is { } b)
                return $"gathering {b.Kind.Name}… {b.Progress * 100:F0}% (move to cancel)";
            if (player.Parent != 0 || session.Me is null)
                return "";
            if (NearestInReach(ready: true) is { } n && GameContent.Node(n.Kind) is { } k)
            {
                if (player.Mode == MoveMode.Swim)
                    return $"{k.Name}: can't gather while swimming";
                return n.Gatherer != 0 ? $"{k.Name}: someone is gathering it" : $"F: gather {k.Name} ({k.Labor} labor)";
            }
            var here = PlayerController.ToNumerics(player.GlobalPosition);
            var near = nodes.Rows
                .Where(r => System.Numerics.Vector2.Distance(new(r.X, r.Z), new(here.X, here.Z)) < NoticeM)
                .OrderBy(r => System.Numerics.Vector2.Distance(new(r.X, r.Z), new(here.X, here.Z)))
                .FirstOrDefault();
            if (near is null || GameContent.Node(near.Kind) is not { } nk)
                return "";
            long left = near.ReadyAt.MicrosecondsSinceUnixEpoch - now;
            if (left <= 0)
                return $"{nk.Name} nearby";
            long secs = (left + 999_999) / 1_000_000;
            return $"{nk.Name} regrows in {secs / 60}:{secs % 60:D2}";
        }

        /// <summary>The nearest node in reach, ready ones only if <paramref name="ready"/>.</summary>
        private ResourceNode? NearestInReach(bool ready)
        {
            var at = PlayerController.ToNumerics(player.GlobalPosition);
            long now = session.GameNowMicros;
            return nodes.Rows
                .Where(n => Sim.Gather.InReach(new System.Numerics.Vector3(n.X, n.Y, n.Z), at))
                .Where(n => !ready || Sim.Gather.Ready(n.ReadyAt.MicrosecondsSinceUnixEpoch, now))
                .OrderBy(n => System.Numerics.Vector3.DistanceSquared(new System.Numerics.Vector3(n.X, n.Y, n.Z), at))
                .FirstOrDefault();
        }

        private uint Held(ushort item) => (uint)session.Bag.Where(s => s.Def == item).Sum(s => (long)s.Count);

        private int LaborNow() => session.Labor?.Current(GameContent.Labor, session.GameNowMicros) ?? -1;
    }
}
