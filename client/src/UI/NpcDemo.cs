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
    /// <c>--npc-demo</c> (dev builds, docs/design/npcs.md): once in the world, goes to level 5 with
    /// Ironvow and Emberlore, puts a Greymane Wolf 14 m ahead with <c>dev_spawn_npc</c> and lets it
    /// notice us, come and bite; targets it and fights it down through the hotbar; walks to the
    /// corpse and loots it with F's action; then removes the wolf and its spawn point. It pauses at
    /// each step for captures, and logs what it saw.
    /// </summary>
    public partial class NpcDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Gameplay.Combat combat;
        private readonly Looting looting;
        private bool started;
        /// <summary>Where the fight happens: (x, z), an open patch of the town south of spawn.</summary>
        private static readonly Vector2 Spot = new(-20, 30);
        private static readonly string[] Rotation = { "frostspike", "shield_bash", "rending_blow", "threefold_cut", "cinder_dart" };

        public NpcDemo(Session session, PlayerController player, EntityRegistry entities, Gameplay.Combat combat, Looting looting)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.combat = combat;
            this.looting = looting;
            Name = "NpcDemo";
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
            conn.Reducers.DevSetLevel(5);
            await Until(() => combat.Level == 5, 5);
            foreach (byte set in new byte[] { 1, 0 }.Where(s => !combat.Picks.Contains(s)))
            {
                combat.PickSkillset(set);
                await Wait(0.5);
            }
            player.DevTeleport(Spot.X, Spot.Y);
            await Wait(2.0);
            var (fx, fz) = Sim.Yaw.Facing(player.Facing);
            var at = player.GlobalPosition;
            conn.Reducers.DevSpawnNpc("greymane_wolf", at.X + 14 * fx, at.Z + 14 * fz);
            RemoteView? Wolf() => entities.Views.Where(v => v.Kind == EntityKind.Npc && v.Visible && v.Label == "Greymane Wolf")
                .OrderBy(v => v.GlobalPosition.DistanceSquaredTo(player.GlobalPosition)).FirstOrDefault();
            if (!await Until(() => Wolf() is not null, 5))
                return;
            ulong wolf = Wolf()!.EntityId;
            session.Log($"npc demo: Greymane Wolf {wolf} placed 14 m ahead");
            // It notices us and comes; we watch it close in and bite.
            await Until(() => conn.Db.Vitals.EntityId.Find(wolf)?.Target == combat.Me, 5);
            session.Log("npc demo: the wolf noticed us");
            await Wait(1.2);
            combat.SetTarget(wolf);
            await Until(() => combat.PositionOf(wolf) is { } p && p.DistanceTo(player.GlobalPosition) < 4.0f, 6);
            await Wait(2.0);
            session.Log($"npc demo: bitten down to {combat.Mine?.HpNow(combat.Now)} HP");

            // Fight it down through the hotbar.
            var limit = DateTime.UtcNow.AddSeconds(60);
            while (combat.Info(wolf) is { Dead: false } && DateTime.UtcNow < limit)
            {
                foreach (var id in Rotation)
                {
                    int before = combat.Accepted;
                    combat.Use(Skill(id));
                    if (combat.Accepted > before)
                        break;
                }
                await Wait(1.1);
            }
            if (combat.Info(wolf) is not { Dead: true })
            {
                session.Log("npc demo: the wolf is still standing");
                return;
            }
            session.Log($"npc demo: the wolf is dead; level {combat.Level}, {combat.Fighter?.Xp} experience");
            await Wait(2.5);

            // Loot the corpse, if it dropped anything.
            if (looting.Corpses.Any(c => c.Corpse == wolf))
            {
                await RunTo(wolf, 2.0f);
                await Wait(1.5);
                session.Log($"npc demo: {looting.Prompt}");
                int before = looting.Looted;
                looting.Take(wolf);
                await Until(() => looting.Looted > before, 5);
                await Wait(2.0);
            }
            else
            {
                session.Log("npc demo: the wolf dropped nothing");
            }
            conn.Reducers.DevRemoveNpc(wolf);
            session.Log($"npc demo: done; {combat.Accepted} skills accepted, {combat.EventsSeen} combat events seen");
        }

        private static Content.SkillData Skill(string id) => Content.GameContent.Skills.First(s => s.Id == id);

        private async Task RunTo(ulong id, float m)
        {
            var limit = DateTime.UtcNow.AddSeconds(8);
            while (combat.PositionOf(id) is { } to && to.DistanceTo(player.GlobalPosition) > m && DateTime.UtcNow < limit)
            {
                var d = to - player.GlobalPosition;
                player.DemoWish = new Vector2(d.X, d.Z).Normalized();
                await Wait(0.05);
            }
            player.DemoWish = null;
        }

        private async Task<bool> Until(Func<bool> cond, double secs)
        {
            var limit = DateTime.UtcNow.AddSeconds(secs);
            while (!cond())
            {
                if (DateTime.UtcNow > limit)
                {
                    session.Log("npc demo: timed out; " + combat.Message);
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
