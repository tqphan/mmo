using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.Sim;
using Mmo.World;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Plots nearby (docs/design/land.md): F within reach of one opens its window, where its owner
    /// pays tax, sets access or abandons it. The server decides each; refusals are logged and shown.
    /// </summary>
    public partial class Plots : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly PlotRegistry registry;

        public Plots(Session session, PlayerController player, PlotRegistry registry)
        {
            this.session = session;
            this.player = player;
            this.registry = registry;
            Name = "PlotControls";
        }

        /// <summary>The plot window; set by <see cref="Main"/>.</summary>
        public UI.PlotWindow? Window { get; set; }

        public PlotRegistry Registry => registry;

        public string Prompt { get; private set; } = "";

        /// <summary>The server's answer to the last request from the window.</summary>
        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        private string lastResult = "";
        private System.DateTime resultUntil;

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnPayTax += (ctx, _) => Answer(ctx, "paid a week of tax");
            r.OnSetPlotAccess += (ctx, _, a) => Answer(ctx, a == 1 ? "the plot is public" : "the plot is private");
            r.OnAbandonPlot += (ctx, _) => Answer(ctx, "abandoned the plot; the deed is back in the bag");
        }

        private void Answer(ReducerEventContext ctx, string ok)
        {
            lastResult = ctx.Event.Status is Status.Failed(var reason) ? $"refused: {reason}" : ok;
            // Shown for a few seconds.
            resultUntil = System.DateTime.UtcNow.AddSeconds(6);
            session.Log($"land: {lastResult}");
        }

        /// <summary>F on land: open (or close) the window for the nearest plot in reach. False if there's none.</summary>
        public bool Interact()
        {
            if (Window is not { } w || player.Parent != 0)
                return false;
            if (w.Open)
            {
                w.Close();
                return true;
            }
            if (NearestInReach() is not { } p)
                return false;
            w.Show(p);
            return true;
        }

        public void PayTax(Plot p) => session.Conn?.Reducers.PayTax(p.Id);

        public void SetAccess(Plot p, byte access) => session.Conn?.Reducers.SetPlotAccess(p.Id, access);

        public void Abandon(Plot p) => session.Conn?.Reducers.AbandonPlot(p.Id);

        public bool InReach(Plot p) =>
            GameContent.Plot(p.Kind) is { } k && Land.InReach(Sim.Rect.Plot(k, p.X, p.Z), player.GlobalPosition.X, player.GlobalPosition.Z);

        public override void _Process(double delta)
        {
            if (Window is { Open: true } w && (w.Plot is not { } shown || registry.Find(shown.Id) is null || !InReach(shown)))
                w.Close();
            Prompt = MakePrompt();
        }

        private string MakePrompt()
        {
            if (player.Parent != 0 || session.Me is null || NearestInReach() is not { } p || GameContent.Plot(p.Kind) is not { } k)
                return LastResult.Length > 0 ? $"land: {LastResult}" : "";
            if (Window is { Open: true })
                return $"F: close the {k.Name}";
            long now = session.GameNowMicros, paid = p.TaxPaidUntil.MicrosecondsSinceUnixEpoch;
            string state = Land.StandingOf(paid, now) == Sim.Standing.Paid
                ? $"paid for {Land.Span(paid - now)}"
                : $"overdue, demolished in {Land.Span(Land.DemolishAt(paid) - now)}";
            return $"F: {k.Name} of {registry.OwnerName(p)} ({state})";
        }

        private Plot? NearestInReach()
        {
            var at = player.GlobalPosition;
            return registry.Standing.Where(InReach).OrderBy(p => new Vector2(p.X - at.X, p.Z - at.Z).LengthSquared()).FirstOrDefault();
        }
    }
}
