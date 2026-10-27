using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.World;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Crafting at stations (docs/design/crafting.md). On land, F at a station opens the craft
    /// window for it; the server times each unit and decides what it takes and makes. While
    /// <c>my_craft</c> has our job, <see cref="Bar"/> says how far the current unit has got. Walking
    /// away cancels it on the server; we send <c>cancel_craft</c> as soon as we start moving, so the bar stops at once.
    /// </summary>
    public partial class Crafting : Node
    {
        private readonly Session session;
        private readonly PlayerController player;
        private readonly StationRegistry stations;
        /// <summary>The job as last seen, and our bag's count of its output when it started.</summary>
        private CraftJob? active;
        private uint heldAtStart;
        private bool cancelSent;

        public Crafting(Session session, PlayerController player, StationRegistry stations)
        {
            this.session = session;
            this.player = player;
            this.stations = stations;
            Name = "Crafting";
        }

        /// <summary>The craft window; set by <see cref="Main"/>.</summary>
        public UI.CraftWindow? Window { get; set; }

        /// <summary>What the HUD says about stations nearby.</summary>
        public string Prompt { get; private set; } = "";

        /// <summary>The recipe being crafted, how far its current unit has got, and units made of units asked for.</summary>
        public (RecipeData Recipe, float Progress, uint Made, uint Count)? Bar { get; private set; }

        /// <summary>Crafts that have ended, however they ended.</summary>
        public int Finished { get; private set; }

        /// <summary>Our craft under way, as the server has it.</summary>
        public CraftJob? Job =>
            session.Conn is { } c && session.Me is { } me ? c.Db.MyCraft.Iter().FirstOrDefault(j => j.CharId == me.CharId) : null;

        /// <summary>F was pressed on land: open (or close) the craft window for the nearest station in reach. False if there's none, so F can climb aboard instead.</summary>
        public bool Interact()
        {
            if (Window is not { } w || session.Conn is null || player.Parent != 0 || player.Mode == MoveMode.Swim)
                return false;
            if (w.Open)
            {
                w.Close();
                return true;
            }
            if (NearestInReach() is not { } s)
                return false;
            w.Show(s);
            return true;
        }

        /// <summary>Ask the server to craft <paramref name="count"/> of <paramref name="recipe"/> at <paramref name="station"/>.</summary>
        public void Start(SpacetimeDB.Types.Station station, RecipeData recipe, uint count)
        {
            session.Conn?.Reducers.Craft(station.Id, recipe.Wire, count);
            session.Log($"crafting {count} × {recipe.Id} at station {station.Id}");
        }

        public void Cancel()
        {
            session.Conn?.Reducers.CancelCraft();
            cancelSent = true;
        }

        public override void _Process(double delta)
        {
            Track(Job);
            long now = session.GameNowMicros;
            Bar = active is { } a && GameContent.Recipe(a.Recipe) is { } r ? (r, Sim.Craft.Progress(r, a.DoneAt.MicrosecondsSinceUnixEpoch, now), a.Made, a.Count) : null;
            if (active is not null && !cancelSent && new Vector2(player.Velocity.X, player.Velocity.Z).Length() > MoveSender.StillMps)
            {
                Cancel();
                session.Log("moved: cancelled crafting");
            }
            if (Window is { Open: true } w && (w.Station is not { } st || stations.Find(st.Id) is null || !InReach(st)))
                w.Close();
            Prompt = MakePrompt();
        }

        /// <summary>Follow our job: a craft starts when <c>my_craft</c> shows it and ends when the row goes.</summary>
        private void Track(CraftJob? job)
        {
            if (job is not null)
            {
                if (active is null)
                {
                    cancelSent = false;
                    heldAtStart = GameContent.Recipe(job.Recipe) is { } r ? Held(r.Output) : 0;
                }
                active = job;
                return;
            }
            if (active is not { } a)
                return;
            var recipe = GameContent.Recipe(a.Recipe);
            uint got = recipe is null ? 0 : Held(recipe.Output) - heldAtStart;
            session.Log(got > 0
                ? $"crafted {got} {GameContent.Item(recipe!.Output)?.Name}; labor now {LaborNow()}"
                : $"crafting {recipe?.Id} stopped with nothing made");
            Finished++;
            active = null;
        }

        private string MakePrompt()
        {
            if (Bar is { } b)
                return $"crafting {GameContent.Item(b.Recipe.Output)?.Name} {b.Made + 1} / {b.Count}… {b.Progress * 100:F0}% (move to cancel)";
            if (player.Parent != 0 || session.Me is null || NearestInReach() is not { } s || GameContent.Station(s.Kind) is not { } k)
                return "";
            if (player.Mode == MoveMode.Swim)
                return $"{k.Name}: can't craft while swimming";
            return Window is { Open: true } ? $"F: close the {k.Name}" : $"F: use {k.Name}";
        }

        private bool InReach(SpacetimeDB.Types.Station s) => Sim.Craft.InReach(new System.Numerics.Vector3(s.X, s.Y, s.Z), PlayerController.ToNumerics(player.GlobalPosition));

        /// <summary>The nearest station in reach.</summary>
        private SpacetimeDB.Types.Station? NearestInReach()
        {
            var at = PlayerController.ToNumerics(player.GlobalPosition);
            return stations.Rows
                .Where(InReach)
                .OrderBy(s => System.Numerics.Vector3.DistanceSquared(new System.Numerics.Vector3(s.X, s.Y, s.Z), at))
                .FirstOrDefault();
        }

        private uint Held(ushort item) => (uint)session.Bag.Where(s => s.Def == item).Sum(s => (long)s.Count);

        private int LaborNow() => session.Labor?.Current(GameContent.Labor, session.GameNowMicros) ?? -1;
    }
}
