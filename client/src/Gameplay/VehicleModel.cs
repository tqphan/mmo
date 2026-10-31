using System.Collections.Generic;
using System.Numerics;
using Mmo.Content;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>What F would do at a pack slot: load our pack into it, or unload the pack there.</summary>
    public readonly record struct SlotAction(ulong Vehicle, int Slot, bool Load, ushort Def);

    /// <summary>
    /// The rules for vehicles that the client applies before asking the server
    /// (docs/design/vehicles.md), kept free of Godot so xUnit can check them.
    /// </summary>
    public static class VehicleModel
    {
        /// <summary>A vehicle as these rules see it.</summary>
        public readonly record struct Seen(ulong Id, HullData Hull, ShipPose Pose, IReadOnlyList<ushort> Cargo, bool Ours);

        /// <summary>
        /// The nearest slot in reach of <paramref name="at"/> (world) where F does something: an empty
        /// one when we carry a pack, or a full one on our own vehicle when our back is free. Null if none.
        /// </summary>
        public static SlotAction? Pick(IEnumerable<Seen> vehicles, Vector3 at, bool carrying)
        {
            SlotAction? best = null;
            float bestD = float.PositiveInfinity;
            foreach (var v in vehicles)
                for (int i = 0; i < v.Hull.Slots.Count; i++)
                {
                    if (Ship.SlotPoint(v.Hull, v.Pose, i) is not Vector3 p || !Ship.CargoReach(p, at))
                        continue;
                    ushort def = i < v.Cargo.Count ? v.Cargo[i] : (ushort)0;
                    bool useful = carrying ? def == 0 : def != 0 && v.Ours;
                    float d = Vector3.DistanceSquared(p, at);
                    if (useful && d < bestD)
                        (best, bestD) = (new SlotAction(v.Id, i, carrying, def), d);
                }
            return best;
        }

        /// <summary>The vehicle V stows: ours that we're aboard, else our nearest in sight. Null if none.</summary>
        public static ulong? StowTarget(IEnumerable<Seen> vehicles, ulong aboard, Vector3 at)
        {
            ulong? best = null;
            float bestD = float.PositiveInfinity;
            foreach (var v in vehicles)
            {
                if (!v.Ours)
                    continue;
                if (v.Id == aboard)
                    return v.Id;
                float d = Vector2.DistanceSquared(new Vector2(v.Pose.X, v.Pose.Z), new Vector2(at.X, at.Z));
                if (d < bestD)
                    (best, bestD) = (v.Id, d);
            }
            return best;
        }

        /// <summary>The cargo line for the HUD: each slot's pack, or "empty".</summary>
        public static string CargoLine(IReadOnlyList<ushort> cargo)
        {
            if (cargo.Count == 0)
                return "";
            var parts = new List<string>();
            foreach (var def in cargo)
                parts.Add(def == 0 ? "empty" : GameContent.Item(def)?.Name ?? $"item {def}");
            return "cargo: " + string.Join(", ", parts);
        }
    }
}
