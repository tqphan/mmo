using System.Collections.Generic;
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
        private static readonly SkillData Dart = GameContent.Skills[0];
        private static readonly SkillData Spike = GameContent.Skills[1];
        private static readonly SkillData Mend = GameContent.Skills[6];

        private static readonly Caster Ready = new(Vector3.Zero, 0, Alive: true, Casting: false, Mp: 800, ReadyAt: 0);

        [Fact]
        public void TheHotbarIsTheSkillsInContentOrder()
        {
            Assert.Equal(9, CombatModel.Hotbar.Count);
            Assert.Equal("cinder_dart", CombatModel.Slot(1)!.Id);
            Assert.Equal("brine_lash", CombatModel.Slot(9)!.Id);
            Assert.Null(CombatModel.Slot(0));
            Assert.Null(CombatModel.Slot(10));
            Assert.Equal(SkillTarget.Ally, Mend.Target);
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
