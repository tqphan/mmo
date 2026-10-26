using System;
using System.Collections.Generic;
using System.Numerics;
using Mmo.Content;
using Mmo.Sim;

namespace Mmo.Gameplay
{
    /// <summary>A ship as the player's rules see it: its entity id, hull and pose now.</summary>
    public readonly record struct Deck(ulong Ship, HullData Hull, ShipPose Pose, float Speed);

    /// <summary>
    /// Which frame the local player is in, and climbing aboard (docs/design/ships.md, "Moving decks").
    /// The server checks moves on a deck in the ship's frame, with the deck as flat ground and the
    /// 1.5 m past its edge as rail with no ground, and refuses positions further out. So the client
    /// joins a ship's frame when it stands on the deck, keeps it over the rail and in the air, and
    /// leaves it before the server's limit, or when it has dropped below the deck.
    /// </summary>
    public static class Boarding
    {
        /// <summary>Within this of the deck's height, standing on it counts as being on it.</summary>
        public const float OnDeckM = 0.25f;
        /// <summary>Leave the ship's frame this far inside the server's rail limit, so no move lands outside it.</summary>
        public const float RailInsetM = 0.25f;
        /// <summary>Dropped this far below the deck, you've gone over the side.</summary>
        public const float FallenM = 1.0f;
        /// <summary>How far from the deck's edge you can climb aboard: alongside the hull, in the water.</summary>
        public const float ClimbReachM = 1.5f;
        /// <summary>How far below the deck you can climb from: from the sea floor beside a hull.</summary>
        public const float ClimbDepthM = 8.0f;
        /// <summary>A climb lands this far in from the deck's edge.</summary>
        public const float ClimbInsetM = 0.5f;

        /// <summary>
        /// The frame after a physics step: the ship whose deck we stand on; else the ship we're
        /// aboard while we're over its deck or rail and not below it; else the world (0).
        /// </summary>
        public static ulong FrameAfter(ulong parent, Vector3 world, bool grounded, IEnumerable<Deck> decks)
        {
            Deck? current = null;
            foreach (var d in decks)
            {
                var l = d.Pose.ToLocal(world);
                if (grounded && MathF.Abs(l.Y) <= OnDeckM && Ship.OnDeck(d.Hull, l.X, l.Z, 0))
                    return d.Ship;
                if (d.Ship == parent)
                    current = d;
            }
            if (current is Deck c)
            {
                var l = c.Pose.ToLocal(world);
                if (l.Y > -FallenM && Ship.OnDeck(c.Hull, l.X, l.Z, Ship.RailM - RailInsetM))
                    return c.Ship;
            }
            return 0;
        }

        /// <summary>Where on the deck a climb from <paramref name="world"/> lands, in the ship's frame; null if it's out of reach.</summary>
        public static Vector3? ClimbTarget(Deck d, Vector3 world)
        {
            var l = d.Pose.ToLocal(world);
            if (l.Y < -ClimbDepthM || l.Y > OnDeckM || !Ship.OnDeck(d.Hull, l.X, l.Z, ClimbReachM))
                return null;
            float hx = d.Hull.DeckHalfWidthM - ClimbInsetM, hz = d.Hull.DeckHalfLengthM - ClimbInsetM;
            return new Vector3(Math.Clamp(l.X, -hx, hx), 0, Math.Clamp(l.Z, -hz, hz));
        }

        /// <summary>
        /// Whether a frame change from <paramref name="from"/> (world coordinates, where the server
        /// last had us, <paramref name="since"/> seconds ago) to <paramref name="to"/> is inside the
        /// server's allowance, with <paramref name="marginM"/> to spare for our view of the ship lagging.
        /// </summary>
        public static bool WithinAllowance(Vector3 from, Vector3 to, float since, float ownSpeedMps, float shipSpeedMps, float marginM = 0.3f)
        {
            float moved = new Vector2(to.X - from.X, to.Z - from.Z).Length();
            return moved + marginM <= Movement.FrameChangeAllowance(ownSpeedMps, shipSpeedMps, since);
        }

        /// <summary>The helm or cannon mount within reach of a ship-local position, with <paramref name="marginM"/> to spare.</summary>
        public static Station? StationAt(HullData hull, Vector3 local, float marginM = 0.3f)
        {
            float reach = Ship.ReachM - marginM;
            if (Ship.Reach(local, hull.HelmX, hull.HelmZ) <= reach)
                return new Station(StationKind.Helm, 0);
            int best = -1;
            float bestD = reach;
            for (int i = 0; i < hull.Mounts.Count; i++)
            {
                float d = Ship.Reach(local, hull.Mounts[i].X, hull.Mounts[i].Z);
                if (d <= bestD)
                    (best, bestD) = (i, d);
            }
            return best >= 0 ? new Station(StationKind.Cannon, (byte)best) : null;
        }
    }

    public enum StationKind { Helm, Cannon }

    /// <summary>The helm, or cannon mount <see cref="Mount"/>.</summary>
    public readonly record struct Station(StationKind Kind, byte Mount);
}
