using System;
using System.Collections.Generic;
using System.Linq;
using Mmo.Net;
using Mmo.Sim;
using Xunit;

namespace Mmo.Tests
{
    /// <summary>The director's rules (docs/design/interest.md §2) against a fake server.</summary>
    public class SubscriptionDirectorTests
    {
        private sealed class FakeChannel : ICellChannel
        {
            public sealed class Sub
            {
                public ulong Cell;
                public string[] Queries = Array.Empty<string>();
                public Action OnApplied = () => { };
                public bool Applied, Ended;
            }

            public readonly List<Sub> Subs = new();

            public object Subscribe(ulong cell, string[] queries, Action onApplied)
            {
                var s = new Sub { Cell = cell, Queries = queries, OnApplied = onApplied };
                Subs.Add(s);
                return s;
            }

            public void Unsubscribe(object handle)
            {
                var s = (Sub)handle;
                Assert.True(s.Applied, "unsubscribed before it was applied");
                Assert.False(s.Ended, "unsubscribed twice");
                s.Ended = true;
            }

            /// <summary>The server applies every pending subscription.</summary>
            public void ApplyAll()
            {
                foreach (var s in Subs.Where(s => !s.Applied).ToList())
                {
                    s.Applied = true;
                    s.OnApplied();
                }
            }

            public HashSet<ulong> Live => Subs.Where(s => !s.Ended).Select(s => s.Cell).ToHashSet();
        }

        private static readonly string[] Tables = { "entity", "transform" };
        private static readonly ulong Home = Cell.CellOf(Cell.OpenWorld, 32, 32);

        [Fact]
        public void SubscribesOneQuerySetPerCell()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            Assert.True(d.Update(32, 32));
            Assert.Equal(25, ch.Subs.Count);
            Assert.Equal(Cell.WindowCells(Home, 2).ToHashSet(), ch.Live);
            var s = ch.Subs.Single(x => x.Cell == Home);
            Assert.Equal(new[] { $"SELECT * FROM entity WHERE cell = {Home}", $"SELECT * FROM transform WHERE cell = {Home}" }, s.Queries);
            Assert.False(d.Ready);
            ch.ApplyAll();
            Assert.True(d.Ready);
            Assert.True(d.Settled);
        }

        [Fact]
        public void MovingOneCellFetchesOnlyTheNewColumnAndDropsTheOldOneAfter()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            d.Update(32, 32);
            ch.ApplyAll();
            Assert.False(d.Update(70, 32)); // inside the 8 m margin
            Assert.True(d.Update(73, 32));
            Assert.Equal(30, ch.Subs.Count);
            // New before old: nothing is dropped until the new column is applied.
            Assert.Equal(30, ch.Live.Count);
            Assert.False(d.Ready);
            ch.ApplyAll();
            Assert.True(d.Settled);
            Assert.Equal(Cell.WindowCells(Home + 1, 2).ToHashSet(), ch.Live);
            Assert.Equal(1, d.Recenters);
        }

        [Fact]
        public void PacingOverABorderNeverResubscribes()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            d.Update(32, 32);
            ch.ApplyAll();
            for (int i = 0; i < 50; i++)
                Assert.False(d.Update(i % 2 == 0 ? 70 : 58, 32));
            Assert.Equal(25, ch.Subs.Count);
            Assert.Equal(0, d.Recenters);
        }

        [Fact]
        public void ComingBackBeforeTheNewWindowAppliesDropsNothing()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            d.Update(32, 32);
            ch.ApplyAll();
            d.Update(73, 32); // new column pending
            Assert.True(d.Update(32, 32)); // back west past the margin: the old window again
            Assert.True(d.Ready); // every cell of it is still held
            // The pending east column left the window before it applied; once it applies it's dropped.
            ch.ApplyAll();
            Assert.Equal(Cell.WindowCells(Home, 2).ToHashSet(), ch.Live);
            Assert.True(d.Settled);
        }

        [Fact]
        public void ATeleportReplacesTheWholeWindowWithoutAGap()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            d.Update(32, 32);
            ch.ApplyAll();
            d.Update(2000, 2000);
            Assert.Equal(50, ch.Live.Count);
            ch.ApplyAll();
            Assert.Equal(Cell.WindowCells(Cell.CellOf(0, 2000, 2000), 2).ToHashSet(), ch.Live);
        }

        [Fact]
        public void NonFinitePositionsKeepTheWindow()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            d.Update(32, 32);
            Assert.False(d.Update(float.NaN, 32));
            Assert.False(d.Update(32, float.PositiveInfinity));
            Assert.Equal(Home, d.Center);
        }

        [Fact]
        public void TheMapCornerHasAClippedWindow()
        {
            var ch = new FakeChannel();
            var d = new SubscriptionDirector(ch, Tables, 2);
            d.Update(-Cell.WorldHalfM + 1, -Cell.WorldHalfM + 1);
            Assert.Equal(9, ch.Subs.Count);
        }
    }
}
