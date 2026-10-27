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
    /// <c>--combat-demo</c> (dev builds, docs/design/combat.md): once in the world, puts a Straw
    /// Dummy 9 m ahead and targets it, casts the Emberlore skills from range, runs in and works
    /// through Ironvow's, has a Brine Lash interrupted by a step, runs off and lets the hotbar turn
    /// us back to face the dummy, heals with a Quick Balm aimed at the dummy (it lands on us) and a
    /// Mending Word, then dies by <c>dev_set_vitals</c>, waits out the countdown and respawns at the
    /// nearest Wayshrine. It pauses at each step for captures, and logs what it saw.
    /// </summary>
    public partial class CombatDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Gameplay.Combat combat;
        private bool started;
        /// <summary>Where the fight happens: (x, z).</summary>
        private static readonly Vector2 Spot = new(-20, 30);

        public CombatDemo(Session session, PlayerController player, EntityRegistry entities, Gameplay.Combat combat)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.combat = combat;
            Name = "CombatDemo";
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
            // An open patch of the town south of spawn, clear of the Wayshrine and the stations.
            player.DevTeleport(Spot.X, Spot.Y);
            await Wait(2.0);
            var (fx, fz) = Sim.Yaw.Facing(player.Facing);
            var at = player.GlobalPosition;
            conn.Reducers.DevSpawnDummy(at.X + 9 * fx, at.Z + 9 * fz);
            RemoteView? Dummy() => entities.Views.Where(v => v.Kind == EntityKind.Dummy && v.Visible)
                .OrderBy(v => v.GlobalPosition.DistanceSquaredTo(player.GlobalPosition)).FirstOrDefault();
            if (!await Until(() => Dummy() is not null, 5))
                return;
            ulong dummy = Dummy()!.EntityId;
            combat.SetTarget(dummy);
            session.Log($"combat demo: Straw Dummy {dummy} placed 9 m ahead and targeted");
            await Wait(1.5);

            // Emberlore from range: a bolt, a spike and a burst.
            await Cast(1, 2.2);
            await Cast(2, 1.3);
            await Cast(3, 2.8);
            Report("after Emberlore", dummy);

            // Run in and work through Ironvow.
            if (!await RunTo(dummy, 2.5f))
                return;
            await Cast(4, 1.3);
            await Cast(5, 1.3);
            await Cast(6, 2.4);
            Report("after Ironvow", dummy);

            // A cast cut short by a step.
            combat.Use(9);
            await Wait(0.4);
            var (bx, bz) = Sim.Yaw.Facing(player.Facing);
            player.DemoWish = new Vector2(-bx, -bz);
            await Wait(0.35);
            player.DemoWish = null;
            await Wait(1.5);
            session.Log($"combat demo: stepped during Brine Lash; cast now {(combat.Mine?.CastSkill ?? 0) != 0}");

            // Run off with our back to it; the hotbar turns us round before the next skill.
            player.DemoWish = new Vector2(-bx, -bz);
            await Wait(1.2);
            player.DemoWish = null;
            await Wait(1.0);
            session.Log("combat demo: our back is to the dummy; Brine Lash turns us to face it");
            await Cast(9, 2.2);

            // Heals: a Quick Balm aimed at the dummy lands on us, then a Mending Word.
            conn.Reducers.DevSetVitals(350, combat.Mine?.MpNow(combat.Now) ?? 800);
            await Wait(1.2);
            await Cast(8, 1.4);
            await Cast(7, 2.5);
            session.Log($"combat demo: healed to {combat.Mine?.HpNow(combat.Now)} HP");

            // Die, wait out the countdown and get up at a Wayshrine.
            conn.Reducers.DevSetVitals(0, combat.Mine?.MpNow(combat.Now) ?? 0);
            if (!await Until(() => combat.Dead, 5))
                return;
            session.Log("combat demo: dead");
            var deadAt = player.GlobalPosition;
            await Until(() => combat.Mine is { DeadAt: long d } && CombatModel.RespawnAt(d, combat.Fighter?.Deaths ?? 1) <= combat.Now, 200);
            await Wait(1.5);
            combat.Respawn();
            if (!await Until(() => !combat.Dead, 5))
                return;
            await Wait(2.0);
            session.Log($"combat demo: respawned {player.GlobalPosition.DistanceTo(deadAt):F0} m away at ({player.GlobalPosition.X:F0}, {player.GlobalPosition.Z:F0}) with {combat.Mine?.HpNow(combat.Now)} HP");
            await Wait(3.0);
            conn.Reducers.DevRemoveDummy(dummy);
            session.Log($"combat demo: done; {combat.Accepted} skills accepted, {combat.EventsSeen} combat events seen");
        }

        private async Task Cast(int key, double wait)
        {
            int before = combat.Accepted;
            combat.Use(key);
            await Wait(wait);
            var name = CombatModel.Slot(key)?.Name;
            session.Log(combat.Accepted > before ? $"combat demo: used {name}" : $"combat demo: {name} not used: {combat.Message}");
        }

        private void Report(string when, ulong dummy) =>
            session.Log($"combat demo: {when}, the dummy has {combat.Info(dummy)?.HpNow(combat.Now)} HP and we have {combat.Mine?.MpNow(combat.Now)} mana");

        /// <summary>Run until within <paramref name="m"/> of an entity.</summary>
        private async Task<bool> RunTo(ulong id, float m)
        {
            var limit = DateTime.UtcNow.AddSeconds(8);
            while (combat.PositionOf(id) is { } to && to.DistanceTo(player.GlobalPosition) > m)
            {
                if (DateTime.UtcNow > limit)
                {
                    player.DemoWish = null;
                    session.Log("combat demo: couldn't reach the target");
                    return false;
                }
                var d = to - player.GlobalPosition;
                player.DemoWish = new Vector2(d.X, d.Z).Normalized();
                await Wait(0.05);
            }
            player.DemoWish = null;
            await Wait(0.5);
            return true;
        }

        private async Task<bool> Until(Func<bool> cond, double secs)
        {
            var limit = DateTime.UtcNow.AddSeconds(secs);
            while (!cond())
            {
                if (DateTime.UtcNow > limit)
                {
                    session.Log("combat demo: timed out; " + combat.Message);
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
