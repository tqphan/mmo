using System;
using System.Collections.Generic;
using System.Linq;
using Mmo.Sim;

namespace Mmo.Net
{
    /// <summary>Subscribes to one cell's queries. The real one wraps the SDK; tests use a fake.</summary>
    public interface ICellChannel
    {
        /// <summary>Start a subscription; <paramref name="onApplied"/> runs once the server has applied it.</summary>
        object Subscribe(ulong cell, string[] queries, Action onApplied);

        /// <summary>End an applied subscription.</summary>
        void Unsubscribe(object handle);
    }

    /// <summary>
    /// Keeps one tier's window of cells subscribed around the player (PLAN §4.3,
    /// docs/design/interest.md). A port of loadbot's director.
    ///
    /// Each cell is its own subscription, so moving the window fetches only the cells that came into
    /// it. Cells that left are dropped only once every cell of the new window is applied, so there's
    /// never a gap. The window follows the player with hysteresis (<see cref="Cell.Recenter"/>).
    /// Single-threaded: call it, and let the SDK run its callbacks, on one thread.
    /// </summary>
    public sealed class SubscriptionDirector
    {
        private sealed class Held
        {
            public object? Handle;
            public bool Applied;
            /// <summary>Dropped from the window before it was applied: unsubscribe once it is.</summary>
            public bool Dropped;
        }

        private readonly ICellChannel channel;
        private readonly string[] tables;
        private readonly ushort radius;
        private HashSet<ulong> wanted = new();
        private readonly Dictionary<ulong, Held> held = new();

        /// <summary>A window of (2·radius+1)² cells over <paramref name="tables"/>, each of which has a <c>cell</c> column.</summary>
        public SubscriptionDirector(ICellChannel channel, string[] tables, ushort radius)
        {
            this.channel = channel;
            this.tables = tables;
            this.radius = radius;
        }

        public ulong? Center { get; private set; }

        /// <summary>How many times the window has moved since the first update.</summary>
        public int Recenters { get; private set; }

        /// <summary>Cells subscribed or being subscribed, including ones about to be dropped.</summary>
        public int HeldCount => held.Count;

        public int WantedCount => wanted.Count;

        /// <summary>Whether every cell of the current window is applied.</summary>
        public bool Ready => Center.HasValue && wanted.All(c => held.TryGetValue(c, out var h) && h.Applied);

        /// <summary>Whether the window is applied and nothing outside it is still held.</summary>
        public bool Settled => Ready && held.Count == wanted.Count;

        /// <summary>Whether <paramref name="cell"/> is in the current window.</summary>
        public bool Wants(ulong cell) => wanted.Contains(cell);

        /// <summary>Call with the player's predicted position every frame. Returns whether the window moved.</summary>
        public bool Update(float x, float z)
        {
            ulong center = Center is ulong c
                ? Cell.Recenter(c, x, z, Cell.HysteresisM)
                : Cell.CellOf(Cell.OpenWorld, x, z);
            if (Center == center)
                return false;
            if (Center.HasValue)
                Recenters++;
            Center = center;
            wanted = Cell.WindowCells(center, radius).ToHashSet();
            foreach (ulong cell in wanted.Where(c => !held.ContainsKey(c)).ToList())
            {
                var h = new Held();
                held[cell] = h;
                string[] queries = tables.Select(t => $"SELECT * FROM {t} WHERE cell = {cell}").ToArray();
                h.Handle = channel.Subscribe(cell, queries, () =>
                {
                    h.Applied = true;
                    if (h.Dropped)
                        channel.Unsubscribe(h.Handle!);
                    else
                        Retire();
                });
            }
            // Coming back to cells still held can complete the window straight away.
            Retire();
            return true;
        }

        /// <summary>Drop every held cell outside the window, once the whole window is applied.</summary>
        private void Retire()
        {
            if (!Ready)
                return;
            foreach (var (cell, h) in held.Where(kv => !wanted.Contains(kv.Key)).ToList())
            {
                held.Remove(cell);
                if (h.Applied)
                    channel.Unsubscribe(h.Handle!);
                else
                    h.Dropped = true;
            }
        }
    }
}
