using System.Linq;
using System.Numerics;
using Mmo.Content;
using Mmo.Gameplay;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The trade prompts and coins (docs/design/trade.md).</summary>
    public class TradeTests
    {
        private const long T0 = 1_800_000_000_000_000;
        private static readonly ushort Bale = GameContent.Items.First(i => i.Id == "grain_bale").Wire;
        private static readonly PostData Gull = GameContent.Posts.First(p => p.Id == "gullhaven_post");

        private static PackInfo Pack(float x = 0, long madeAt = T0) => new(Bale, 1, x, 0, madeAt);

        [Fact]
        public void SellPromptShowsThePriceAndItsParts()
        {
            // 3 km and the island's bonus: 175% + 20%.
            string p = TradeModel.SellPrompt(Pack(), Gull, new Vector2(3000, 0), Trade.Full, T0);
            Assert.Equal("F: sell Grain Bale at Gullhaven Post for 2g 92s 50c (route 195%, demand 100%, fresh 100%)", p);
            string lower = TradeModel.SellPrompt(Pack(), Gull, new Vector2(3000, 0), 960, T0);
            Assert.Contains("demand 96%", lower);
            Assert.Contains($"for {Trade.Coins(Trade.Price(15000, 195, 960, 1000))} ", lower);
        }

        [Fact]
        public void SellPromptSaysWhyNot()
        {
            Assert.Equal("Gullhaven Post: this post is too close to where the pack was made", TradeModel.SellPrompt(Pack(), Gull, new Vector2(100, 0), Trade.Full, T0));
            long spoiled = T0 + GameContent.Trade.ExpireS * 1_000_000L;
            Assert.Equal("Gullhaven Post: this pack has spoiled", TradeModel.SellPrompt(Pack(), Gull, new Vector2(3000, 0), Trade.Full, spoiled));
        }

        [Fact]
        public void PickUpPromptMarksTheft()
        {
            Assert.Equal("F: pick up Grain Bale", TradeModel.PickUpPrompt(Pack(), ours: true, carrying: false, T0));
            Assert.Equal("F: pick up Grain Bale (theft)", TradeModel.PickUpPrompt(Pack(), ours: false, carrying: false, T0));
            Assert.Equal("Grain Bale: already carrying a pack", TradeModel.PickUpPrompt(Pack(), ours: true, carrying: true, T0));
            Assert.Contains("spoiled", TradeModel.PickUpPrompt(Pack(), ours: true, carrying: false, T0 + GameContent.Trade.ExpireS * 1_000_000L));
        }

        [Fact]
        public void PackLineCountsDownToSpoiling()
        {
            Assert.Equal("carrying Grain Bale, fresh 100%, spoils in 6d 0h (X: drop)", TradeModel.PackLine(Pack(), T0));
            long twoDays = T0 + 2 * 86_400_000_000L;
            Assert.Equal("carrying Grain Bale, fresh 96%, spoils in 4d 0h (X: drop)", TradeModel.PackLine(Pack(), twoDays));
            long late = T0 + GameContent.Trade.ExpireS * 1_000_000L - 90 * 60_000_000L;
            Assert.Contains("spoils in 1h 30m", TradeModel.PackLine(Pack(), late));
            Assert.Equal("carrying Grain Bale, spoiled (X: drop it)", TradeModel.PackLine(Pack(), T0 + GameContent.Trade.ExpireS * 1_000_000L));
        }

        [Fact]
        public void CoinsReadAsGoldSilverCopper()
        {
            Assert.Equal("0c", Trade.Coins(0));
            Assert.Equal("1s 50c", Trade.Coins(150));
            Assert.Equal("3g 22s 50c", Trade.Coins(32250));
            Assert.Equal("purse 1g 0s 0c", TradeModel.PurseLine(10_000));
        }

        [Fact]
        public void PackRecipesCraftOneAtATime()
        {
            var bag = new[] { new BagStack(1, 0, GameContent.Items.First(i => i.Id == "hearthgrain").Wire, 200), new BagStack(2, 1, GameContent.Items.First(i => i.Id == "reedcloth").Wire, 20) };
            var bench = GameContent.Stations.First(s => s.Id == "packwright_bench").Wire;
            var line = CraftModel.Lines(bench, bag, 5000).Single(l => l.Recipe.Id == "grain_bale");
            Assert.Equal(1u, line.Affordable);
        }
    }
}
