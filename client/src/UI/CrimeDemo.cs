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
    /// <c>--crime-demo</c> (dev builds, docs/design/crime.md), with <c>bot visitor</c> nearby for a
    /// victim: once in the world, goes to the top level with every skillset, finds the nearest other
    /// character, runs up and hits it with Cinder Darts (a Bloodstain at its feet, Bloodmarked on us),
    /// walks up to the stain and tries to report it (refused: it's our own), darts the visitor until
    /// it dies (a Blood Pool), then <c>dev_set_crime</c>
    /// makes us Sought. It pauses at each step for captures, and logs what it saw.
    /// </summary>
    public partial class CrimeDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Gameplay.Combat combat;
        private readonly Gameplay.Crime crime;
        private readonly EvidenceRegistry evidence;
        private bool started;

        public CrimeDemo(Session session, PlayerController player, EntityRegistry entities, Gameplay.Combat combat, Gameplay.Crime crime, EvidenceRegistry evidence)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.combat = combat;
            this.crime = crime;
            this.evidence = evidence;
            Name = "CrimeDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null || session.Near is not { Ready: true } || combat.Mine is null)
                return;
            started = true;
            _ = Run();
        }

        private async Task Run()
        {
            var conn = session.Conn!;
            if (combat.Dead)
            {
                conn.Reducers.DevSetVitals(1000, 800);
                await Wait(1.0);
            }
            conn.Reducers.DevSetLevel(Sim.Combat.LevelMax);
            await Until(() => combat.Level == Sim.Combat.LevelMax, 5);
            foreach (var set in Content.GameContent.Skillsets.Where(s => !s.Npc && !combat.Picks.Contains(s.Wire)))
            {
                combat.PickSkillset(set.Wire);
                await Wait(0.5);
            }
            RemoteView? Victim() => entities.Views.Where(v => v.Kind == EntityKind.Character && v.Visible && !v.Dead)
                .OrderBy(v => v.GlobalPosition.DistanceSquaredTo(player.GlobalPosition)).FirstOrDefault();
            if (!await Until(() => Victim() is not null, 10))
            {
                session.Log("crime demo: nobody to hit; run `cargo xtask bot visitor` alongside");
                return;
            }
            var victim = Victim()!;
            var at = victim.GlobalPosition;
            player.DevTeleport(at.X + 8, at.Z + 8);
            await Wait(2.0);
            combat.SetTarget(victim.EntityId);
            session.Log($"crime demo: targeting {victim.Label}");
            int before = evidence.Count;
            // Cinder Darts until the first lands: it reaches the visitor as it runs.
            var dart = Content.GameContent.Skills.First(s => s.Id == "cinder_dart");
            for (int i = 0; i < 4 && evidence.Count == before; i++)
            {
                combat.Use(dart);
                await Wait(2.5);
            }
            if (evidence.All.Where(e => e.Kind == Sim.EvidenceKind.Bloodstain).OrderByDescending(e => e.Id).FirstOrDefault() is not { Id: > 0 } stain)
            {
                session.Log($"crime demo: no Bloodstain appeared; {combat.Message}");
                return;
            }
            bool marked = combat.Mine is { } me && me.Slots.Any(s => s.Status == Content.GameContent.Crime.Mark && s.Active(combat.Now));
            session.Log($"crime demo: a Bloodstain at ({stain.At.X:F1}, {stain.At.Z:F1}); we are {(marked ? "" : "not ")}Bloodmarked");
            combat.SetTarget(0);
            await Wait(2.0);

            // Walk up to the stain, stopping short so it's in view, and report it: refused, it's ours.
            await WalkTo(stain.At, 3.0f);
            session.Log($"crime demo: standing by it, the prompt says {crime.Prompt.Split('\n')[0]}");
            crime.Report();
            await Until(() => crime.LastResult.Length > 0, 5);
            session.Log($"crime demo: our report: {crime.LastResult}");
            await Wait(3.0);

            // Darts until it dies: a Blood Pool where it fell.
            combat.SetTarget(victim.EntityId);
            for (int i = 0; i < 30 && !victim.Dead; i++)
            {
                combat.Use(dart);
                await Wait(1.3);
            }
            combat.SetTarget(0);
            if (evidence.All.FirstOrDefault(e => e.Kind == Sim.EvidenceKind.BloodPool && e.Id > stain.Id) is { Id: > 0 } pool)
            {
                session.Log($"crime demo: {victim.Label} died; a Blood Pool at ({pool.At.X:F1}, {pool.At.Z:F1})");
                await WalkTo(pool.At, 3.5f);
                await Wait(3.0);
            }
            else
                session.Log($"crime demo: no Blood Pool; {victim.Label} is {(victim.Dead ? "dead" : "alive")}");

            // Sought.
            conn.Reducers.DevSetCrime((uint)Content.GameContent.Crime.SoughtPoints, (uint)Content.GameContent.Crime.SoughtPoints);
            await Until(() => crime.RecordLine.Contains("Sought"), 5);
            session.Log($"crime demo: {crime.RecordLine}");
            await Wait(4.0);
            conn.Reducers.DevSetCrime(0, 0);
            session.Log("crime demo: done");
        }

        /// <summary>Walk towards <paramref name="to"/> until within <paramref name="m"/> of it.</summary>
        private async Task WalkTo(System.Numerics.Vector3 to, float m)
        {
            var limit = DateTime.UtcNow.AddSeconds(8);
            while (new Vector2(to.X - player.GlobalPosition.X, to.Z - player.GlobalPosition.Z).Length() > m && DateTime.UtcNow < limit)
            {
                player.DemoWish = new Vector2(to.X - player.GlobalPosition.X, to.Z - player.GlobalPosition.Z).Normalized();
                await Wait(0.05);
            }
            player.DemoWish = null;
            await Wait(1.5);
        }

        private async Task<bool> Until(Func<bool> cond, double secs)
        {
            var limit = DateTime.UtcNow.AddSeconds(secs);
            while (!cond())
            {
                if (DateTime.UtcNow > limit)
                {
                    session.Log("crime demo: timed out");
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
