using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Shipyards (docs/design/vehicles.md §2B). Right-clicking a design in the bag places a yard on
    /// the water ahead of us; F by a yard does its next stage when we have what it needs, and the
    /// prompt says what that is. The server decides each.
    /// </summary>
    public partial class Shipyards : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly ShipyardRegistry registry;
        private string lastResult = "";
        private System.DateTime resultUntil;

        public Shipyards(Session session, PlayerController player, ShipyardRegistry registry)
        {
            this.session = session;
            this.player = player;
            this.registry = registry;
            Name = "Shipyards";
        }

        /// <summary>What the HUD says about the yard in reach.</summary>
        public string Prompt { get; private set; } = "";

        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        public int StagesDone { get; private set; }

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnPlaceShipyard += (ctx, _) => Answer(ctx, "placed the shipyard");
            r.OnWorkShipyard += (ctx, _) =>
            {
                if (ctx.Event.Status is SpacetimeDB.Status.Committed)
                    StagesDone++;
                Answer(ctx, "stage done");
            };
        }

        private void Answer(ReducerEventContext ctx, string ok)
        {
            lastResult = ctx.Event.Status is SpacetimeDB.Status.Failed(var reason) ? $"refused: {reason}" : ok;
            resultUntil = System.DateTime.UtcNow.AddSeconds(6);
            session.Log($"shipyards: {lastResult}");
        }

        private System.Numerics.Vector3 At => PlayerController.ToNumerics(player.GlobalPosition);

        private ushort Carrying => session.Conn is { } c && session.Me is { } me ? c.Db.MyPack.Iter().FirstOrDefault(p => p.Carrier == me.CharId)?.Def ?? 0 : (ushort)0;

        /// <summary>A design from the bag: place its yard ahead of us. False for anything else.</summary>
        public bool Begin(BagStack s)
        {
            if (GameContent.ShipyardOfDesign(s.Def) is not { } kind)
                return false;
            session.Conn?.Reducers.PlaceShipyard(s.Id);
            session.Log($"placing a {kind.Name}");
            return true;
        }

        /// <summary>The yard action in reach now, if any.</summary>
        public YardAction? Action => ShipyardModel.Pick(registry.Seen(session.Me?.CharId ?? 0), At, Carrying);

        /// <summary>F was pressed: do the next stage of the yard in reach. False if there's nothing F can do there.</summary>
        public bool Interact()
        {
            if (Action is not { Can: true } a)
                return false;
            Work(a.Yard);
            return true;
        }

        public void Work(ulong yard)
        {
            session.Conn?.Reducers.WorkShipyard(yard);
            session.Log($"working on shipyard {yard}");
        }

        public override void _Process(double delta)
        {
            var lines = Action is { } a ? a.Prompt : "";
            if (LastResult.Length > 0)
                lines = lines.Length > 0 ? lines + "\n" + LastResult : LastResult;
            Prompt = lines;
        }
    }
}
