using System.Linq;
using Godot;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Reporting crimes (docs/design/crime.md): J reports the nearest unfaded evidence in reach. The
    /// client can't tell whose it is, so the server's answer is shown for a few seconds. Also our
    /// own record for the HUD.
    /// </summary>
    public partial class Crime : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EvidenceRegistry evidence;
        private string lastResult = "";
        private System.DateTime resultUntil;

        public Crime(Session session, PlayerController player, EvidenceRegistry evidence)
        {
            this.session = session;
            this.player = player;
            this.evidence = evidence;
            Name = "Crime";
        }

        /// <summary>What the HUD says about the nearest evidence, and the last report's answer.</summary>
        public string Prompt { get; private set; } = "";

        /// <summary>Our crime points and Notoriety, once we have a record.</summary>
        public string RecordLine { get; private set; } = "";

        /// <summary>The last report's answer, for the demo.</summary>
        public string LastResult => lastResult;

        public override void _Ready()
        {
            session.Conn!.Reducers.OnReportEvidence += (ctx, _) =>
            {
                lastResult = ctx.Event.Status is Status.Failed(var reason) ? $"refused: {reason}" : "reported";
                resultUntil = System.DateTime.UtcNow.AddSeconds(5);
                session.Log($"report: {lastResult}");
            };
        }

        public EvidenceInfo? Nearest() => CrimeModel.Nearest(evidence.All, PlayerController.ToNumerics(player.GlobalPosition), session.GameNowMicros);

        /// <summary>Report the nearest evidence in reach; false if there's none.</summary>
        public bool Report()
        {
            if (Nearest() is not { } e)
                return false;
            session.Conn?.Reducers.ReportEvidence(e.Id);
            session.Log($"reporting {Sim.Crime.Name(e.Kind)} {e.Id}");
            return true;
        }

        public override void _Process(double delta)
        {
            if (GameInput.JustPressed("report"))
                Report();
            Prompt = Nearest() is { } e ? CrimeModel.Prompt(e) : "";
            if (System.DateTime.UtcNow < resultUntil)
                Prompt = Prompt.Length > 0 ? Prompt + "\n" + lastResult : lastResult;
            var mine = session.Me is { } me ? session.Conn?.Db.MyCrime.Iter().FirstOrDefault(r => r.CharId == me.CharId) : null;
            RecordLine = CrimeModel.RecordLine(mine?.CrimePoints, mine?.Notoriety ?? 0);
        }
    }
}
