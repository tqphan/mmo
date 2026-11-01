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
    /// <c>--quest-demo</c> (dev builds, docs/design/quests.md): once in the world beside a giver
    /// (a new character starts by Odda Fernwright in Furrowdale), walks up to the nearest, opens its
    /// panel with F's action and takes every quest it offers; then fights Shore Skitters it summons
    /// with <c>dev_spawn_npc</c> until Pinchers on the Strand is done, walks back, turns it in and
    /// opens the log. It pauses at each step for captures, and logs what it saw.
    /// </summary>
    public partial class QuestDemo : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Gameplay.Combat combat;
        private readonly Quests quests;
        private readonly QuestWindow window;
        private bool started;

        public QuestDemo(Session session, PlayerController player, EntityRegistry entities, Gameplay.Combat combat, Quests quests, QuestWindow window)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            this.combat = combat;
            this.quests = quests;
            this.window = window;
            Name = "QuestDemo";
        }

        public override void _Process(double delta)
        {
            if (started || session.Me is null || session.Near is not { Ready: true } || combat.Mine is null || quests.Nearest() is null)
                return;
            started = true;
            _ = Run();
        }

        private async Task Run()
        {
            var conn = session.Conn!;
            await Wait(2.0);
            var giver = quests.Nearest()!;
            session.Log($"quest demo: nearest giver {Content.GameContent.Giver(giver.Kind)?.Name}, {new Vector2(giver.X - player.GlobalPosition.X, giver.Z - player.GlobalPosition.Z).Length():F1} m off");
            await WalkTo(giver.X, giver.Z, 2.5f);
            await Wait(1.5);
            quests.Interact();
            await Wait(2.5);
            foreach (var o in QuestModel.Offers(giver.Kind, quests.Level, quests.Log).Where(o => o.Locked is null))
            {
                window.PressTake(o.Quest);
                await Until(() => quests.Log.Any(e => e.Quest == o.Quest.Wire), 5);
                await Wait(1.0);
            }
            session.Log($"quest demo: took {quests.Log.Count} quests; marker now {QuestModel.Marker(giver.Kind, quests.Level, quests.Log)}");
            await Wait(1.5);
            window.CloseGiver();

            // Skitters, summoned a few metres ahead and cut down, until the quest is done: first a few
            // steps away from the giver, so it isn't in the way.
            var away = new Vector2(player.GlobalPosition.X - giver.X, player.GlobalPosition.Z - giver.Z).Normalized();
            await WalkTo(giver.X + away.X * 9, giver.Z + away.Y * 9, 0.8f);
            await Wait(0.5);
            if (!combat.Picks.Contains((byte)1))
            {
                combat.PickSkillset(1);
                await Wait(0.5);
            }
            var pests = Content.GameContent.Quests.First(q => q.Id == "shore_pests");
            uint Kills() => quests.Log.FirstOrDefault(e => e.Quest == pests.Wire)?.Done.FirstOrDefault() ?? 0;
            var limit = DateTime.UtcNow.AddSeconds(150);
            while (Kills() < pests.Objectives[0].Count && DateTime.UtcNow < limit)
            {
                var (fx, fz) = Sim.Yaw.Facing(player.Facing);
                var at = player.GlobalPosition;
                var before = entities.Views.Where(v => v.Kind == EntityKind.Npc).Select(v => v.EntityId).ToHashSet();
                conn.Reducers.DevSpawnNpc("shore_skitter", at.X + 3 * fx, at.Z + 3 * fz);
                RemoteView? Fresh() => entities.Views.FirstOrDefault(v => v.Kind == EntityKind.Npc && v.Visible && !before.Contains(v.EntityId));
                if (!await Until(() => Fresh() is not null, 5))
                    break;
                ulong id = Fresh()!.EntityId;
                combat.SetTarget(id);
                var fight = DateTime.UtcNow.AddSeconds(30);
                while (combat.Info(id) is { Dead: false } && DateTime.UtcNow < fight)
                {
                    combat.Use(Skill("threefold_cut"));
                    await Wait(1.1);
                }
                await Wait(0.8);
                conn.Reducers.DevRemoveNpc(id);
                session.Log($"quest demo: skitter {id} down; {Kills()}/{pests.Objectives[0].Count}");
            }
            await Wait(1.5);

            // Back to the giver to turn it in.
            await WalkTo(giver.X, giver.Z, 2.5f);
            await Wait(1.0);
            quests.Interact();
            await Wait(2.5);
            int before2 = quests.TurnedIn;
            window.PressTurnIn(pests);
            await Until(() => quests.TurnedIn > before2, 5);
            await Wait(2.5);
            window.CloseGiver();
            window.ToggleLog();
            await Wait(3.0);
            window.ToggleLog();
            session.Log($"quest demo: done; level {combat.Level}, {combat.Fighter?.Xp} experience, {quests.TurnedIn} turned in, {quests.Log.Count(e => !e.TurnedIn)} open");
        }

        private static Content.SkillData Skill(string id) => Content.GameContent.Skills.First(s => s.Id == id);

        /// <summary>Walk until within <paramref name="m"/> of (x, z), facing it.</summary>
        private async Task WalkTo(float x, float z, float m)
        {
            var limit = DateTime.UtcNow.AddSeconds(20);
            while (new Vector2(x - player.GlobalPosition.X, z - player.GlobalPosition.Z) is var d && d.Length() > m && DateTime.UtcNow < limit)
            {
                player.DemoWish = d.Normalized();
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
                    session.Log("quest demo: timed out; " + quests.LastResult);
                    return false;
                }
                await Wait(0.1);
            }
            return true;
        }

        private async Task Wait(double secs) => await ToSignal(GetTree().CreateTimer(secs), SceneTreeTimer.SignalName.Timeout);
    }
}
