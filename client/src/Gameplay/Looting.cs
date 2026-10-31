using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Loot on corpses (docs/design/npcs.md): <c>loot</c> rows arrive with the Near tier. A corpse
    /// with loot we may take sparkles, and F within <c>loot_m</c> takes it all (before stations,
    /// packs and plots). The server decides.
    /// </summary>
    public partial class Looting : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly Dictionary<ulong, Loot> rows = new();
        private string lastResult = "";
        private System.DateTime resultUntil;

        public Looting(Session session, PlayerController player, EntityRegistry entities)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            Name = "Looting";
        }

        /// <summary>What the HUD says about the nearest corpse with loot.</summary>
        public string Prompt { get; private set; } = "";

        /// <summary>Corpses looted, for the demo.</summary>
        public int Looted { get; private set; }

        public override void _Ready()
        {
            var db = session.Conn!.Db;
            db.Loot.OnInsert += (_, l) => rows[l.Id] = l;
            db.Loot.OnUpdate += (_, _, l) => rows[l.Id] = l;
            db.Loot.OnDelete += (_, l) => rows.Remove(l.Id);
            session.Conn.Reducers.OnLootCorpse += (ctx, corpse) =>
            {
                if (ctx.Event.Status is Status.Committed)
                    Looted++;
                lastResult = ctx.Event.Status is Status.Failed(var reason) ? $"refused: {reason}" : $"looted the {CorpseName(corpse)}";
                resultUntil = System.DateTime.UtcNow.AddSeconds(5);
                session.Log($"loot: {lastResult}");
            };
        }

        private ulong MyChar => session.Me?.CharId ?? 0;

        private string CorpseName(ulong corpse) => entities.View(corpse)?.Label is { Length: > 0 } l ? l : "corpse";

        /// <summary>The stacks on each corpse in view.</summary>
        public IEnumerable<(ulong Corpse, List<LootInfo> Loot)> Corpses =>
            rows.Values.GroupBy(r => r.Corpse).Select(g => (g.Key, g.Select(r => new LootInfo(r.Def, r.Count, r.Owner, r.FreeAt.MicrosecondsSinceUnixEpoch)).ToList()));

        /// <summary>The nearest corpse with loot within reach, if any.</summary>
        public ulong? Nearest()
        {
            var at = player.GlobalPosition;
            float reach = GameContent.Npc.LootM;
            return Corpses
                .Select(c => (c.Corpse, View: entities.View(c.Corpse)))
                .Where(c => c.View is { Visible: true } v && Flat(v.GlobalPosition - at) <= reach && Mathf.Abs(v.GlobalPosition.Y - at.Y) <= reach)
                .OrderBy(c => Flat(c.View!.GlobalPosition - at))
                .Select(c => (ulong?)c.Corpse)
                .FirstOrDefault();
        }

        private static float Flat(Vector3 d) => new Vector2(d.X, d.Z).Length();

        /// <summary>F was pressed: loot the nearest corpse in reach. False if there's none.</summary>
        public bool Interact()
        {
            if (Nearest() is not { } corpse)
                return false;
            Take(corpse);
            return true;
        }

        public void Take(ulong corpse)
        {
            session.Conn?.Reducers.LootCorpse(corpse);
            session.Log($"looting {corpse}");
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            ulong me = MyChar;
            var corpses = Corpses.ToList();
            foreach (var v in entities.Views.Where(v => v.Kind == EntityKind.Npc))
                v.SetSparkle(v.Dead && corpses.Any(c => c.Corpse == v.EntityId && c.Loot.Any(l => NpcModel.MayTake(l, me, now))));
            Prompt = Nearest() is { } corpse ? NpcModel.Prompt(CorpseName(corpse), corpses.First(c => c.Corpse == corpse).Loot, me, now) : "";
            if (System.DateTime.UtcNow < resultUntil)
                Prompt = Prompt.Length > 0 ? Prompt + "\n" + lastResult : lastResult;
        }
    }
}
