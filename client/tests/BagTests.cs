using Mmo.Content;
using Mmo.Gameplay;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The bag window's click rules (docs/design/items.md §2, Client).</summary>
    public class BagTests
    {
        private static BagStack?[] Bag(params BagStack[] rows) => BagModel.Grid(rows, GameContent.BagSlots);

        private static readonly BagStack Reed = new(11, 0, 1, 500);
        private static readonly BagStack Knife = new(12, 3, 200, 1);

        [Fact]
        public void TheGridPutsEachStackInItsSlotAndIgnoresOthers()
        {
            var grid = BagModel.Grid(new[] { Reed, Knife, new BagStack(13, 60, 1, 5) }, 50);
            Assert.Equal(50, grid.Length);
            Assert.Equal(Reed, grid[0]);
            Assert.Equal(Knife, grid[3]);
            Assert.Null(grid[1]);
        }

        [Fact]
        public void ClickPickUpThenClickASlotMoves()
        {
            var m = new BagModel();
            var grid = Bag(Reed, Knife);
            Assert.Null(m.Click(grid, 0, shift: false));
            Assert.Equal(new BagModel.Held(11, 0, null), m.Holding);
            Assert.Equal(new MoveIntent(11, 7), m.Click(grid, 7, shift: false));
            Assert.Null(m.Holding);
            // Onto another stack too: the server decides between merging and swapping.
            m.Click(grid, 3, shift: false);
            Assert.Equal(new MoveIntent(12, 0), m.Click(grid, 0, shift: false));
        }

        [Fact]
        public void ShiftClickSplitsHalfRoundedDown()
        {
            var m = new BagModel();
            var grid = Bag(Reed with { Count = 7 });
            m.Click(grid, 0, shift: true);
            Assert.Equal(new SplitIntent(11, 3, 9), m.Click(grid, 9, shift: false));
        }

        [Fact]
        public void ShiftClickOnASingleItemOrAnEmptySlotPicksNothingUp()
        {
            var m = new BagModel();
            var grid = Bag(Reed, Knife);
            Assert.Null(m.Click(grid, 3, shift: true));
            Assert.Null(m.Holding);
            Assert.Null(m.Click(grid, 20, shift: false));
            Assert.Null(m.Holding);
        }

        [Fact]
        public void TheSameSlotOrCancelPutsItDown()
        {
            var m = new BagModel();
            var grid = Bag(Reed);
            m.Click(grid, 0, shift: false);
            Assert.Null(m.Click(grid, 0, shift: false));
            Assert.Null(m.Holding);
            m.Click(grid, 0, shift: true);
            m.Cancel();
            Assert.Null(m.Click(grid, 5, shift: false));
        }

        [Fact]
        public void AStackTheServerMovedIsDropped()
        {
            var m = new BagModel();
            m.Click(Bag(Reed), 0, shift: false);
            var moved = Bag(Reed with { Slot = 4 });
            Assert.Null(m.Click(moved, 9, shift: false));
            m.Click(Bag(Reed), 0, shift: true);
            m.Refresh(Bag(Reed with { Count = 251 }));
            Assert.NotNull(m.Holding);
            m.Refresh(Bag(Reed with { Count = 1 }));
            Assert.Null(m.Holding);
            m.Click(Bag(Reed), 0, shift: false);
            m.Refresh(Bag());
            Assert.Null(m.Holding);
        }

        [Fact]
        public void CtrlRightClickDestroysTheWholeStack()
        {
            var m = new BagModel();
            var grid = Bag(Reed, Knife);
            m.Click(grid, 3, shift: false);
            Assert.Equal(new DestroyIntent(11, 500), m.DestroyClick(grid, 0));
            Assert.Null(m.Holding);
            Assert.Null(m.DestroyClick(grid, 30));
        }

        [Fact]
        public void ContentLooksUpItemsByWire()
        {
            Assert.Equal("Saltreed", GameContent.Item(1)?.Name);
            Assert.Equal(1u, GameContent.Item(200)?.Stack);
            Assert.Null(GameContent.Item(0));
            Assert.Equal(50, GameContent.BagSlots);
        }
    }
}
