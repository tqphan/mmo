using System.Collections.Generic;

namespace Mmo.Gameplay
{
    /// <summary>One stack in the bag, as the server has it (the <c>item</c> row).</summary>
    public readonly record struct BagStack(ulong Id, ushort Slot, ushort Def, uint Count);

    /// <summary>What a click asks the server to do.</summary>
    public abstract record BagIntent;

    /// <summary>Move a whole stack: into an empty slot, merging, or swapping (docs/design/items.md).</summary>
    public sealed record MoveIntent(ulong Id, ushort To) : BagIntent;

    public sealed record SplitIntent(ulong Id, uint Count, ushort To) : BagIntent;

    public sealed record DestroyIntent(ulong Id, uint Count) : BagIntent;

    /// <summary>
    /// The bag window's click rules, with no Godot dependency (docs/design/items.md §2, Client).
    /// Click a stack to pick it up, or shift-click to pick up half of it; click another slot to
    /// put it there, or the same slot (or <see cref="Cancel"/>) to put it down. Nothing is predicted:
    /// the server's rows are the only truth, and a pick-up only marks the slot.
    /// </summary>
    public sealed class BagModel
    {
        /// <summary>What's held: the stack, where it was, and how many to split off (null: all of it).</summary>
        public readonly record struct Held(ulong Id, ushort Slot, uint? Split);

        public Held? Holding { get; private set; }

        /// <summary>The bag as <paramref name="slots"/> slots, empty ones null. Rows outside the bag are ignored.</summary>
        public static BagStack?[] Grid(IEnumerable<BagStack> rows, int slots)
        {
            var grid = new BagStack?[slots];
            foreach (var r in rows)
                if (r.Slot < slots)
                    grid[r.Slot] = r;
            return grid;
        }

        /// <summary>A left click on <paramref name="slot"/>; returns what to send, if anything.</summary>
        public BagIntent? Click(BagStack?[] grid, ushort slot, bool shift)
        {
            if (Holding is not { } h)
            {
                if (grid[slot] is { } s)
                {
                    if (!shift)
                        Holding = new Held(s.Id, slot, null);
                    else if (s.Count >= 2)
                        Holding = new Held(s.Id, slot, s.Count / 2);
                }
                return null;
            }
            Holding = null;
            if (slot == h.Slot || grid[h.Slot]?.Id != h.Id)
                return null;
            return h.Split is uint n ? new SplitIntent(h.Id, n, slot) : new MoveIntent(h.Id, slot);
        }

        /// <summary>Ctrl + right click: destroy the whole stack, once the player confirms.</summary>
        public BagIntent? DestroyClick(BagStack?[] grid, ushort slot)
        {
            Holding = null;
            return grid[slot] is { } s ? new DestroyIntent(s.Id, s.Count) : null;
        }

        public void Cancel() => Holding = null;

        /// <summary>Drop what's held if the server moved or removed it, or a split no longer fits its stack.</summary>
        public void Refresh(BagStack?[] grid)
        {
            if (Holding is { } h && (grid[h.Slot] is not { } s || s.Id != h.Id || (h.Split is uint n && n >= s.Count)))
                Holding = null;
        }
    }
}
