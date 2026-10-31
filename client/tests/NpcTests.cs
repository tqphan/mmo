using System.Linq;
using Mmo.Content;
using Mmo.Gameplay;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>NPC labels, colours and the loot prompt (docs/design/npcs.md).</summary>
    public class NpcTests
    {
        private const long T0 = 1_800_000_000_000_000;
        private static readonly ushort Fang = GameContent.Items.First(i => i.Id == "wolf_fang").Wire;
        private static readonly ushort Hide = GameContent.Items.First(i => i.Id == "coarse_hide").Wire;

        [Fact]
        public void KindsComeFromTheRefIdWithTheirLook()
        {
            var wolf = NpcModel.Kind(GameContent.NpcKinds.First(k => k.Id == "greymane_wolf").Wire)!;
            Assert.Equal("Greymane Wolf", wolf.Name);
            Assert.True(NpcModel.Aggressive(wolf));
            Assert.False(NpcModel.Aggressive(NpcModel.Kind(0)!));
            Assert.Null(NpcModel.Kind(999));
            var (r, g, b) = NpcModel.Color(wolf);
            Assert.Equal((0x8a / 255f, 0x8f / 255f, 0x96 / 255f), (r, g, b));
            Assert.DoesNotContain(GameContent.Skillsets.Where(s => s.Npc), s => s.Wire < 3);
        }

        [Fact]
        public void OursAtOnceAndAnyonesWhenFree()
        {
            var mine = new LootInfo(Fang, 1, 7, T0 + 30_000_000);
            Assert.True(NpcModel.MayTake(mine, 7, T0));
            Assert.False(NpcModel.MayTake(mine, 8, T0));
            Assert.True(NpcModel.MayTake(mine, 8, T0 + 30_000_000));
            var nobodys = mine with { Owner = 0 };
            Assert.False(NpcModel.MayTake(nobodys, 7, T0));
        }

        [Fact]
        public void PromptListsTheLootOrTheWait()
        {
            var loot = new[] { new LootInfo(Fang, 1, 7, T0 + 30_000_000), new LootInfo(Hide, 2, 7, T0 + 30_000_000) };
            Assert.Equal("F: loot the Greymane Wolf (Wolf Fang, Coarse Hide ×2)", NpcModel.Prompt("Greymane Wolf", loot, 7, T0));
            Assert.Equal("Greymane Wolf: not yours to loot for 30 s", NpcModel.Prompt("Greymane Wolf", loot, 8, T0));
            Assert.Equal("Greymane Wolf: not yours to loot for 1 s", NpcModel.Prompt("Greymane Wolf", loot, 8, T0 + 29_500_000));
            Assert.Equal("", NpcModel.Prompt("Greymane Wolf", System.Array.Empty<LootInfo>(), 7, T0));
        }
    }
}
