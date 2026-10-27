using System.Collections.Generic;
using System.Linq;
using Mmo.Content;
using Mmo.Gameplay;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The craft window's rules (docs/design/crafting.md).</summary>
    public class CraftTests
    {
        private static ushort Wire(string id) => GameContent.Items.First(i => i.Id == id).Wire;

        private static RecipeData Recipe(string id) => GameContent.Recipes.First(r => r.Id == id);

        private static byte Hearth => GameContent.Stations.First(s => s.Id == "cookhearth").Wire;

        private static byte Bloomery => GameContent.Stations.First(s => s.Id == "bloomery").Wire;

        private static List<BagStack> Bag(params (string Id, uint Count)[] stacks) =>
            stacks.Select((s, i) => new BagStack((ulong)i + 1, (ushort)i, Wire(s.Id), s.Count)).ToList();

        [Fact]
        public void LinesListTheStationsRecipesWithHaveAndNeed()
        {
            // Two stacks of logs count together.
            var lines = CraftModel.Lines(Hearth, Bag(("driftpine_log", 3), ("emberleaf", 2), ("driftpine_log", 4)), 5000);
            Assert.All(lines, l => Assert.Equal(Hearth, l.Recipe.Station));
            Assert.Equal(GameContent.Recipes.Count(r => r.Station == Hearth), lines.Count);
            var coal = lines.Single(l => l.Recipe.Id == "charcoal");
            Assert.Equal("Charcoal", coal.Output!.Name);
            var logs = Assert.Single(coal.Inputs);
            Assert.Equal((2u, 7u, false), (logs.Need, logs.Have, logs.Short));
            Assert.Equal(3u, coal.Affordable);
            var tea = lines.Single(l => l.Recipe.Id == "emberleaf_tea");
            Assert.True(tea.Inputs[0].Short);
            Assert.Equal(0u, tea.Affordable);
        }

        [Fact]
        public void AffordableUnitsAreLimitedByEveryInputAndLabor()
        {
            var bar = Recipe("rustiron_bar"); // 3 ore + 1 charcoal, 10 labor
            var bag = Bag(("rustvein_ore", 10), ("charcoal", 5));
            Assert.Equal(3u, CraftModel.Lines(Bloomery, bag, 5000).Single(l => l.Recipe == bar).Affordable);
            Assert.Equal(2u, CraftModel.Lines(Bloomery, bag, 29).Single(l => l.Recipe == bar).Affordable);
            Assert.Equal(1u, CraftModel.Lines(Bloomery, Bag(("rustvein_ore", 10), ("charcoal", 1)), 5000).Single(l => l.Recipe == bar).Affordable);
            var cap = CraftModel.Lines(Hearth, Bag(("driftpine_log", 1000)), 5000).Single(l => l.Recipe.Id == "charcoal");
            Assert.Equal(GameContent.CraftMaxCount, cap.Affordable);
        }

        [Fact]
        public void TheCountStaysWithinWhatIsAffordable()
        {
            var lines = CraftModel.Lines(Hearth, Bag(("driftpine_log", 7)), 5000);
            var m = new CraftModel();
            Assert.Null(m.CraftIntent(lines, busy: false));
            m.Select(Recipe("charcoal").Wire);
            m.Step(lines, -5);
            Assert.Equal(1u, m.Count);
            m.Step(lines, +10);
            Assert.Equal(3u, m.Count);
            Assert.Equal((Recipe("charcoal").Wire, 3u), m.CraftIntent(lines, busy: false));
            Assert.Null(m.CraftIntent(lines, busy: true));

            // The logs run low: the count follows.
            var fewer = CraftModel.Lines(Hearth, Bag(("driftpine_log", 4)), 5000);
            m.Refresh(fewer);
            Assert.Equal(2u, m.Count);

            // A new recipe starts again at 1; Max takes everything affordable.
            m.Select(Recipe("emberleaf_tea").Wire);
            Assert.Equal(1u, m.Count);
            Assert.Null(m.CraftIntent(fewer, busy: false));
            m.Select(Recipe("charcoal").Wire);
            m.Max(lines);
            Assert.Equal(3u, m.Count);
        }
    }
}
