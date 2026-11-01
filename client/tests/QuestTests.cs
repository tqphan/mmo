using System.Collections.Generic;
using System.Linq;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The quest window's, the log's and the markers' rules (docs/design/quests.md).</summary>
    public class QuestTests
    {
        private static QuestData Q(string id) => GameContent.Quests.First(q => q.Id == id);

        private static byte G(string id) => GameContent.Givers.First(g => g.Id == id).Wire;

        private static QuestEntry Open(string id, params uint[] done) => new(Q(id).Wire, done, false);

        private static QuestEntry Done(string id) => new(Q(id).Wire, new uint[Q(id).Objectives.Count], true);

        [Fact]
        public void RulesComeFromContent()
        {
            Assert.Equal(50, GameContent.Quest.Log);
            Assert.Equal(50, GameContent.Quest.EarlyPct);
            Assert.Equal(150, GameContent.Quest.OverPct);
            Assert.Equal(10, GameContent.Quests.Count);
            Assert.Equal(9u, Quest.Cap(6));
            Assert.Equal(1u, Quest.Cap(1));
        }

        [Fact]
        public void AGiverOffersWhatWeMayTakeFirstAndSaysWhyTheRestWait()
        {
            var odda = G("odda_fernwright");
            var offers = QuestModel.Offers(odda, 1, new List<QuestEntry>());
            Assert.Equal(new[] { "shore_pests", "first_furrows", "reed_and_twine" }, offers.Where(o => o.Locked is null).Select(o => o.Quest.Id));
            Assert.Equal("needs level 2", offers.First(o => o.Quest.Id == "cairnwick_road").Locked);
            var later = QuestModel.Offers(odda, 2, new List<QuestEntry> { Done("first_furrows") });
            Assert.Equal("after Pinchers on the Strand", later.First(o => o.Quest.Id == "cairnwick_road").Locked);
            Assert.Equal("after Reed and Twine", later.First(o => o.Quest.Id == "first_bale").Locked);
            // Taken or turned in, a quest isn't offered again.
            Assert.DoesNotContain(later, o => o.Quest.Id == "first_furrows");
            Assert.DoesNotContain(QuestModel.Offers(odda, 1, new List<QuestEntry> { Open("shore_pests", 0) }), o => o.Quest.Id == "shore_pests");
        }

        [Fact]
        public void MarkersShowWhatAGiverHasForUs()
        {
            var (odda, brannoch) = (G("odda_fernwright"), G("brannoch_hale"));
            Assert.Equal("!", QuestModel.Marker(odda, 1, new List<QuestEntry>()));
            Assert.Equal("", QuestModel.Marker(brannoch, 1, new List<QuestEntry>()));
            // Everything of Odda's at level 1 taken, one under way: an ellipsis, then a ? once it's half done.
            var log = new List<QuestEntry> { Open("shore_pests", 2), Done("first_furrows"), Done("reed_and_twine") };
            Assert.Equal("…", QuestModel.Marker(odda, 1, log));
            log[0] = Open("shore_pests", 3);
            Assert.Equal("?", QuestModel.Marker(odda, 1, log));
            // An errand to Brannoch puts the ? over him, not over Odda.
            var errand = new List<QuestEntry> { Done("shore_pests"), Open("cairnwick_road") };
            Assert.Equal("?", QuestModel.Marker(brannoch, 2, errand));
            Assert.Single(QuestModel.TurnIns(brannoch, errand));
            Assert.Empty(QuestModel.TurnIns(odda, errand));
        }

        [Fact]
        public void ObjectivesAndPayReadAsWords()
        {
            Assert.Equal(new[] { "Shore Skitter slain 4/6" }, QuestModel.Objectives(Q("shore_pests"), new uint[] { 4 }));
            Assert.Equal(new[] { "Saltreed gathered 0/32", "Reedcloth crafted 0/2" }, QuestModel.Objectives(Q("reed_and_twine"), new uint[0]));
            Assert.Equal(new[] { "Grain Bale sold at Gullhaven Post 1/1" }, QuestModel.Objectives(Q("across_the_water"), new uint[] { 1 }));
            Assert.Equal(new[] { "Speak to Brannoch Hale" }, QuestModel.Objectives(Q("cairnwick_road"), new uint[0]));
            Assert.Equal("60 experience, 2 Emberleaf Salve", QuestModel.Pays(Q("shore_pests"), new uint[] { 3 }));
            Assert.Equal("180 experience, 2 Emberleaf Salve", QuestModel.Pays(Q("shore_pests"), new uint[] { 9 }));
            Assert.Equal("100 experience", QuestModel.Pays(Q("cairnwick_road"), new uint[0]));
        }

        [Fact]
        public void TheTrackerListsOpenQuestsWithTheirStanding()
        {
            var log = new List<QuestEntry> { Open("reed_and_twine", 20, 1), Open("shore_pests", 7), Done("first_furrows"), Open("cairnwick_road") };
            Assert.Equal(new[]
            {
                "Pinchers on the Strand (done, extra)",
                "  Shore Skitter slain 7/6",
                "Reed and Twine (can turn in early)",
                "  Saltreed gathered 20/32",
                "  Reedcloth crafted 1/2",
                "The Cairnwick Road (done)",
                "  Speak to Brannoch Hale",
            }, QuestModel.Tracker(log));
            Assert.Equal("overachieved", QuestModel.StandingLabel(Quest.Standing(Q("shore_pests").Objectives, new uint[] { 9 })));
        }
    }
}
