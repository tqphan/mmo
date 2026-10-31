using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Owned vehicles and pack slots (docs/design/vehicles.md). Right-clicking a vehicle item in the
    /// bag summons it ahead of us; V stows ours (the one we're aboard, else our nearest in sight).
    /// F by a pack slot loads our pack into it, or unloads the pack there from our own vehicle. The
    /// server decides each; the prompt says what F would do.
    /// </summary>
    public partial class Vehicles : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private string lastResult = "";
        private System.DateTime resultUntil;

        public Vehicles(Session session, PlayerController player, EntityRegistry entities)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            Name = "Vehicles";
        }

        /// <summary>What the HUD says about vehicles and slots nearby.</summary>
        public string Prompt { get; private set; } = "";

        public string LastResult => System.DateTime.UtcNow < resultUntil ? lastResult : "";

        /// <summary>F would load or unload at a slot now, so it does nothing else.</summary>
        public bool HasSlotAction { get; private set; }

        public int Loaded { get; private set; }
        public int Unloaded { get; private set; }

        public override void _Ready()
        {
            var r = session.Conn!.Reducers;
            r.OnSummonVehicle += (ctx, _) => Answer(ctx, "summoned");
            r.OnStowVehicle += (ctx, _) => Answer(ctx, "stowed");
            r.OnRepairVehicle += (ctx, _) => Answer(ctx, "repaired");
            r.OnMendVehicle += (ctx, _) => Answer(ctx, "mending it");
            r.OnLoadPack += (ctx, _, slot) =>
            {
                if (ctx.Event.Status is SpacetimeDB.Status.Committed)
                    Loaded++;
                Answer(ctx, $"loaded the pack into slot {slot}");
            };
            r.OnUnloadPack += (ctx, _, slot) =>
            {
                if (ctx.Event.Status is SpacetimeDB.Status.Committed)
                    Unloaded++;
                Answer(ctx, $"unloaded slot {slot}");
            };
        }

        private void Answer(SpacetimeDB.Types.ReducerEventContext ctx, string ok)
        {
            lastResult = ctx.Event.Status is SpacetimeDB.Status.Failed(var reason) ? $"refused: {reason}" : ok;
            resultUntil = System.DateTime.UtcNow.AddSeconds(6);
            session.Log($"vehicles: {lastResult}");
        }

        /// <summary>Every vehicle in sight, as the rules see it.</summary>
        public IEnumerable<VehicleModel.Seen> Seen()
        {
            ulong me = session.Me?.CharId ?? 0;
            foreach (var s in entities.Ships)
                if (s.Vehicle is { } v)
                    yield return new VehicleModel.Seen(s.Id, s.Hull, s.Pose, v.Cargo, me != 0 && v.Owner == me, v.Hp, v.HitAt.MicrosecondsSinceUnixEpoch);
        }

        private System.Numerics.Vector3 At => PlayerController.ToNumerics(player.GlobalPosition);

        private bool Carrying => session.Conn is { } c && session.Me is { } me && c.Db.MyPack.Iter().Any(p => p.Carrier == me.CharId);

        /// <summary>Our wreck of item <paramref name="item"/>, if it is one.</summary>
        public Wreck? WreckOf(ulong item) => session.Conn?.Db.MyWrecks.Iter().FirstOrDefault(w => w.ItemId == item);

        /// <summary>The bag's note for a stack: a wreck's state, or nothing.</summary>
        public string Note(BagStack s) =>
            WreckOf(s.Id) is { } w ? ShipyardModel.WreckLabel(w.Mending, w.ReadyAt.MicrosecondsSinceUnixEpoch, session.GameNowMicros) : "";

        /// <summary>A vehicle item from the bag: summon it, or mend it if it's a wreck. False for anything else.</summary>
        public bool Begin(BagStack s)
        {
            if (GameContent.HullOfItem(s.Def) is not { } hull)
                return false;
            if (WreckOf(s.Id) is { Mending: false })
            {
                session.Conn?.Reducers.MendVehicle(s.Id);
                session.Log($"mending the {hull.Name}");
                return true;
            }
            session.Conn?.Reducers.SummonVehicle(s.Id);
            session.Log($"summoning a {hull.Name}");
            return true;
        }

        public void Repair(ulong vehicle)
        {
            session.Conn?.Reducers.RepairVehicle(vehicle);
            session.Log($"repairing vehicle {vehicle}");
        }

        /// <summary>Our vehicles wherever they are (<c>my_vehicles</c>), for V out of sight.</summary>
        private IEnumerable<ulong> Owned =>
            session.Conn is { } c && session.Me is { } me ? c.Db.MyVehicles.Iter().Where(v => v.Owner == me.CharId).Select(v => v.EntityId).ToList() : Enumerable.Empty<ulong>();

        /// <summary>F was pressed: load or unload at a slot in reach. False if there's nothing to do.</summary>
        public bool Interact()
        {
            if (VehicleModel.Pick(Seen(), At, Carrying) is not SlotAction a)
                return false;
            Use(a);
            return true;
        }

        public void Use(SlotAction a)
        {
            if (a.Load)
                session.Conn?.Reducers.LoadPack(a.Vehicle, (byte)a.Slot);
            else
                session.Conn?.Reducers.UnloadPack(a.Vehicle, (byte)a.Slot);
            session.Log($"{(a.Load ? "loading into" : "unloading")} slot {a.Slot} of vehicle {a.Vehicle}");
        }

        public void Stow(ulong vehicle)
        {
            session.Conn?.Reducers.StowVehicle(vehicle);
            session.Log($"stowing vehicle {vehicle}");
        }

        public override void _Process(double delta)
        {
            var seen = Seen().ToList();
            if (Input.IsActionJustPressed("stow_vehicle"))
            {
                if (VehicleModel.StowTarget(seen, player.Parent, At, Owned) is ulong v)
                    Stow(v);
                else
                    (lastResult, resultUntil) = ("no vehicle of yours in sight", System.DateTime.UtcNow.AddSeconds(4));
            }
            if (Input.IsActionJustPressed("repair_vehicle"))
            {
                if (VehicleModel.RepairTarget(seen, player.Parent, At) is ulong v)
                    Repair(v);
                else
                    (lastResult, resultUntil) = ("nothing to repair in reach", System.DateTime.UtcNow.AddSeconds(4));
            }
            var lines = new List<string>();
            // At the helm or a cannon, F works that instead (PlayerController).
            var action = player.Controls?.AtStation == true ? null : VehicleModel.Pick(seen, At, Carrying);
            HasSlotAction = action is not null;
            if (action is SlotAction a)
            {
                string pack = GameContent.Item(a.Def)?.Name ?? "pack";
                lines.Add(a.Load ? $"F: load your pack into slot {a.Slot}" : $"F: unload the {pack} from slot {a.Slot}");
            }
            if (seen.FirstOrDefault(s => s.Id == player.Parent) is { Hull: not null } aboard)
            {
                string cargo = VehicleModel.CargoLine(aboard.Cargo);
                if (cargo.Length > 0)
                    lines.Add(cargo);
                string state = VehicleModel.StateLine(aboard, session.GameNowMicros);
                if (state.Length > 0)
                    lines.Add(state);
                if (aboard.Ours && aboard.Hp > 0)
                    lines.Add($"V: stow your {aboard.Hull.Name}");
            }
            if (LastResult.Length > 0)
                lines.Add(LastResult);
            Prompt = string.Join("\n", lines);
        }
    }
}
