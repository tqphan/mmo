using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The combat UI's rules (docs/design/combat.md).</summary>
    public class CombatTests
    {
        private const long S = 1_000_000;
        private static SkillData Skill(string id) => GameContent.Skills.First(s => s.Id == id);
        private static byte Status(string id) => GameContent.Statuses.First(s => s.Id == id).Wire;

        private static readonly SkillData Dart = Skill("cinder_dart");
        private static readonly SkillData Spike = Skill("frostspike");
        private static readonly SkillData Mend = Skill("mending_word");

        private static readonly Caster Ready = new(Vector3.Zero, 0, Alive: true, Controlled: false, Knows: true, Casting: false, Mp: 800, ReadyAt: 0);

        [Fact]
        public void TheHotbarHasARowPerSkillsetInTheOrderPicked()
        {
            var picks = new byte[] { 2, 0 };
            var rows = CombatModel.Rows(picks);
            Assert.Equal(2, rows.Count);
            Assert.All(rows, r => Assert.Equal(CombatModel.RowKeys, r.Count));
            Assert.Equal("mending_word", CombatModel.Slot(picks, 0, 1)!.Id);
            Assert.Equal("cinder_dart", CombatModel.Slot(picks, 1, 1)!.Id);
            Assert.Equal("frostspike", CombatModel.Slot(picks, 1, 2)!.Id);
            Assert.Null(CombatModel.Slot(picks, 2, 1));
            Assert.Null(CombatModel.Slot(picks, 0, 0));
            Assert.Null(CombatModel.Slot(picks, 0, 9));
            Assert.Empty(CombatModel.Rows(new byte[0]));
            Assert.Equal((0, 1, 2, 2), (CombatModel.RowOf(false, false), CombatModel.RowOf(true, false), CombatModel.RowOf(false, true), CombatModel.RowOf(true, true)));
            Assert.Equal(("4", "S-4", "C-4"), (CombatModel.KeyLabel(0, 4), CombatModel.KeyLabel(1, 4), CombatModel.KeyLabel(2, 4)));
            Assert.Equal(SkillTarget.Ally, Mend.Target);
        }

        [Fact]
        public void SkillsetPicksAndLevelsAreSpelledOut()
        {
            Assert.Equal("K: pick a skillset", CombatModel.PickLine(1, 0));
            Assert.Equal("next skillset at level 5", CombatModel.PickLine(4, 1));
            Assert.Equal("K: pick 2 skillsets", CombatModel.PickLine(10, 1));
            Assert.Equal("", CombatModel.PickLine(15, 3));
            Assert.Equal("Level 3 · 120 / 220 xp", CombatModel.LevelLine(3, 120));
            Assert.Equal("Level 15 (top)", CombatModel.LevelLine(15, 0));
            Assert.False(Sim.Combat.Knows(Skill("shield_bash"), 4, new byte[] { 1 }));
            Assert.True(Sim.Combat.Knows(Skill("shield_bash"), 5, new byte[] { 1 }));
            Assert.Equal(CombatRefusal.Unlearned, CombatModel.Plan(Spike, Ready with { Knows = false }, new CombatTarget(new Vector3(0, 0, -5), true, false), 0).Refusal);
            Assert.Equal(CombatRefusal.Controlled, CombatModel.Plan(Spike, Ready with { Controlled = true }, new CombatTarget(new Vector3(0, 0, -5), true, false), 0).Refusal);
        }

        [Fact]
        public void FramesShowBuffsThenDebuffsWithSecondsLeftAndControlsSlowOrStopUs()
        {
            var slots = new List<Slot>
            {
                new(Status("slowed"), 0, 4 * S),
                new(Status("stunned"), 0, 2 * S - 1),
                new(Status("fervor"), 0, 10 * S),
                new(Status("scorched"), 5 * S, 8 * S),
            };
            Assert.Equal(new[] { ("Fervor 10", false), ("Stunned 2", true), ("Slowed 4", true) }, CombatModel.Chips(slots, 0));
            var v = new VitalsInfo(500, 1000, 100, 800, 0, 0, null, 0, 0, 0, 3, slots);
            Assert.True(v.Controlled(S));
            Assert.Equal(0.0f, v.MoveScale(S));
            Assert.Equal(GameContent.Statuses[Status("slowed")].SpeedPct / 100.0f, v.MoveScale(3 * S));
            Assert.Equal(1.0f, v.MoveScale(5 * S));
            Assert.False(v.KnockedDown(S));
            Assert.Equal(3, v.Level);
            Assert.Equal("Stunned", CombatModel.Floating(2000, EventFlags.Status, Status("stunned")));
            Assert.Equal("immune to Felled", CombatModel.Floating(0, EventFlags.Immune, Status("felled")));
            Assert.Equal("+40 xp", CombatModel.Floating(40, EventFlags.Xp));
            Assert.Equal("level 5!", CombatModel.Floating(5, EventFlags.Level));
        }

        [Fact]
        public void AllySkillsOnAnythingButALivingCharacterLandOnUs()
        {
            bool Character(ulong id) => id == 7;
            Assert.Equal(7ul, CombatModel.Resolve(Mend, 1, 7, Character));
            Assert.Equal(1ul, CombatModel.Resolve(Mend, 1, 9, Character));
            Assert.Equal(1ul, CombatModel.Resolve(Mend, 1, 0, Character));
            Assert.Equal(9ul, CombatModel.Resolve(Spike, 1, 9, Character));
        }

        [Fact]
        public void ATargetBehindUsIsTurnedToNotRefused()
        {
            var ahead = new CombatTarget(new Vector3(0, 0, -10), true, false);
            var behind = ahead with { At = new Vector3(0, 0, 10) };
            Assert.Equal(new SkillPlan(null, false), CombatModel.Plan(Spike, Ready, ahead, 0));
            Assert.Equal(new SkillPlan(null, true), CombatModel.Plan(Spike, Ready, behind, 0));
            Assert.Equal(CombatRefusal.OutOfRange, CombatModel.Plan(Spike, Ready, ahead with { At = new Vector3(0, 0, -30) }, 0).Refusal);
            Assert.Equal(CombatRefusal.NotReady, CombatModel.Plan(Spike, Ready with { ReadyAt = 5 }, ahead, 0).Refusal);
            Assert.False(CombatModel.Plan(Spike, Ready with { Mp = 1 }, behind, 0).Send);
        }

        [Fact]
        public void CooldownsShowTheLongerWait()
        {
            var cds = new List<(ushort, long)> { (Spike.Wire, 10 * S) };
            Assert.Equal(10 * S, CombatModel.ReadyAt(Spike.Wire, 1 * S, cds));
            Assert.Equal(1 * S, CombatModel.ReadyAt(Dart.Wire, 1 * S, cds));
            // Spike's own 8 s cooldown, 6 s left.
            var (f, secs) = CombatModel.CooldownShown(Spike, 1 * S, cds, 4 * S);
            Assert.Equal(6.0f / 8.0f, f, 3);
            Assert.Equal(6, secs);
            // Dart only waits on the global cooldown, half gone.
            (f, secs) = CombatModel.CooldownShown(Dart, 1 * S, cds, S / 2);
            Assert.Equal(0.5f, f, 3);
            Assert.Equal(1, secs);
            Assert.Equal((0f, 0), CombatModel.CooldownShown(Dart, 1 * S, cds, 2 * S));
        }

        [Fact]
        public void TabPicksLivingOthersInFrontFirstThenNearest()
        {
            var cands = new[]
            {
                new Candidate(1, Vector3.Zero, true),                  // us
                new Candidate(2, new Vector3(0, 0, -20), true),        // ahead, far
                new Candidate(3, new Vector3(0, 0, -5), true),         // ahead, near
                new Candidate(4, new Vector3(0, 0, 3), true),          // behind, nearest of all
                new Candidate(5, new Vector3(0, 0, -2), false),        // dead
                new Candidate(6, new Vector3(0, 0, -50), true),        // too far
            };
            var order = CombatModel.TabOrder(cands, 1, Vector3.Zero, 0);
            Assert.Equal(new ulong[] { 3, 2, 4 }, order);
            Assert.Equal(3ul, CombatModel.NextTarget(order, 0));
            Assert.Equal(2ul, CombatModel.NextTarget(order, 3));
            Assert.Equal(3ul, CombatModel.NextTarget(order, 4));
            Assert.Equal(0ul, CombatModel.NextTarget(new List<ulong>(), 3));
        }

        [Fact]
        public void TheDeathPanelCountsDownThenOffersTheKey()
        {
            long dead = 100 * S;
            uint deaths = 2;
            long ready = dead + Combat.RespawnWaitMicros(deaths);
            Assert.Equal("respawn in 0:30", CombatModel.RespawnLine(dead, deaths, ready - 30 * S));
            Assert.Equal("respawn in 0:01", CombatModel.RespawnLine(dead, deaths, ready - 1));
            Assert.Equal("R: respawn at the nearest Wayshrine", CombatModel.RespawnLine(dead, deaths, ready));
        }

        [Fact]
        public void VitalsSettleAndCastsProgress()
        {
            var r = GameContent.Combat;
            var v = new VitalsInfo(500, 1000, 100, 800, 0, 2 * S, null, Dart.Wire, 0, 1 * S);
            Assert.Equal(500 + 2 * r.HpRegenIn + 3 * r.HpRegenOut, v.HpNow(5 * S));
            Assert.Equal(0.5f, v.CastProgress(S / 2));
            Assert.Null((v with { CastSkill = 0 }).CastProgress(S / 2));
            Assert.Equal(500, (v with { DeadAt = 0 }).HpNow(100 * S));
            Assert.True(v.InCombat(S));
            Assert.Equal("80", CombatModel.Floating(80, EventFlags.Hit));
            Assert.Equal("+120", CombatModel.Floating(120, EventFlags.Heal));
            Assert.Equal("70 killed", CombatModel.Floating(70, EventFlags.Hit | EventFlags.Kill));
        }
    }
}
