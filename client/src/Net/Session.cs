using System;
using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Net
{
    /// <summary>
    /// The connection to the world module: connects with Gzip (ADR 0011), keeps a token per profile,
    /// subscribes to our own views, makes a character if there isn't one and enters the world with
    /// our content hash and the terrain hash of the world we loaded (docs/design/world.md). Once we're in, it runs the Near tier's subscription director.
    /// A dropped connection is recovered by the SDK, which keeps the cache and replays every
    /// subscription, the director's included; the server despawned us, so we enter again.
    /// The SDK's callbacks run inside <see cref="DbConnection.FrameTick"/>, on the main thread.
    /// </summary>
    public partial class Session : Node
    {
        /// <summary>The Near tier's tables (PLAN §4.3), 5×5 cells. <c>vehicle</c> carries ships' helm input and HP.</summary>
        public static readonly string[] NearTables = { "entity", "transform", "vehicle" };
        public const ushort NearRadius = 2;
        /// <summary>The Static tier's tables (PLAN §4.3), 9×9 cells: things that rarely change, such as resource nodes (docs/design/gathering.md), crafting stations (docs/design/crafting.md), plots (docs/design/land.md), crops (docs/design/farming.md), and trade posts and dropped packs (docs/design/trade.md).</summary>
        public static readonly string[] StaticTables = { "resource_node", "station", "plot", "crop", "trade_post", "dropped_pack" };
        public const ushort StaticRadius = 4;

        private readonly ClientOptions options;
        private readonly ulong terrainHash;
        private bool entering;
        /// <summary>Reconnected: enter again even if a presence from the old connection is still in the cache.</summary>
        private bool reenter;

        public Session(ClientOptions options, ulong terrainHash)
        {
            this.options = options;
            this.terrainHash = terrainHash;
            Name = "Session";
        }

        public DbConnection? Conn { get; private set; }
        public string Status { get; private set; } = "connecting";
        public Presence? Me { get; private set; }
        public SubscriptionDirector? Near { get; private set; }
        public SubscriptionDirector? Static { get; private set; }
        public int Reconnects { get; private set; }

        /// <summary>
        /// Game time in µs since the Unix epoch: our clock plus the dev test clock's offset from
        /// <c>game_clock</c>, which production servers never set (docs/design/labor.md).
        /// </summary>
        public long GameNowMicros => (DateTime.UtcNow - DateTime.UnixEpoch).Ticks / 10 + (Conn?.Db.GameClock.Id.Find(0)?.OffsetMicros ?? 0);

        /// <summary>Our account's labor pool as stored; settle it at <see cref="GameNowMicros"/> to show it.</summary>
        public Sim.LaborPool? Labor =>
            Conn?.Db.MyLabor.Iter().FirstOrDefault() is { } l ? new Sim.LaborPool(l.Points, l.At.MicrosecondsSinceUnixEpoch) : null;

        /// <summary>The bag of the character we're in the world as, as the server has it (docs/design/items.md).</summary>
        public IEnumerable<Gameplay.BagStack> Bag =>
            Conn is { } c && Me is { } me
                ? c.Db.MyInventory.Iter().Where(i => i.Holder == me.CharId && i.Place == BagPlace).Select(i => new Gameplay.BagStack(i.Id, i.Slot, i.Def, i.Count))
                : Enumerable.Empty<Gameplay.BagStack>();

        /// <summary><c>place::BAG</c> on the server.</summary>
        public const byte BagPlace = 1;

        /// <summary>We're in the world: snap to this presence.</summary>
        public event Action<Presence>? Entered;

        /// <summary>The server rejected a move: snap to this presence (docs/design/movement.md).</summary>
        public event Action<Presence>? Corrected;

        /// <summary>The connection dropped and we're out of the world until <see cref="Entered"/> fires again.</summary>
        public event Action? Dropped;

        public override void _Ready()
        {
            var builder = DbConnection.Builder()
                .WithUri(options.Uri)
                .WithDatabaseName(options.Db)
                .WithCompression(Compression.Gzip)
                .WithAutomaticReconnect(new AutomaticReconnectOptions { MinDelay = TimeSpan.FromSeconds(1), MaxDelay = TimeSpan.FromSeconds(15) })
                .OnConnect(OnConnected)
                .OnAutomaticReconnect(OnReconnected)
                .OnConnectError((e, next) => SetStatus(next is { } n ? $"reconnect failed: {e.Message}; {Retry(n)}" : $"connect failed: {e.Message}", error: true))
                .OnDisconnect((_, e, next) => OnDisconnected(e, next));
            if (LoadToken() is string token)
                builder = builder.WithToken(token);
            Conn = builder.Build();
        }

        public override void _Process(double delta)
        {
            if (Conn is null)
                return;
            Conn.FrameTick();
            if (Me is not null && Conn.Db.MyPresence.Count == 0)
            {
                Me = null;
                SetStatus("left the world");
            }
            // Dev builds: spend labor as work would, until there is work (docs/design/labor.md).
            if (Me is not null && Input.IsActionJustPressed("dev_spend_labor"))
            {
                Conn.Reducers.DevSpendLabor(100);
                Log("F8: spent 100 labor");
            }
        }

        public override void _ExitTree()
        {
            if (Reconnects > 0)
                Log($"reconnected {Reconnects}x");
            if (Conn is { IsActive: true })
                Conn.Disconnect();
        }

        private void OnConnected(DbConnection conn, Identity identity, string token)
        {
            SaveToken(token);
            SetStatus($"connected as {identity.ToString()[..8]}");
            conn.OnUnhandledReducerError += (ctx, e) => SetStatus($"{ctx.Event.Reducer.GetType().Name} failed: {e.Message}", error: true);
            conn.Db.MyCharacters.OnInsert += (_, _) => EnterWorld();
            // my_presence is a view, and view rows have no primary key, so the SDK delivers each change
            // as an insert of the new row and a delete of the old one, in either order. A row for the
            // entity we already have is an update; we've left only when no row is left (see _Process).
            conn.Db.MyPresence.OnInsert += (_, p) =>
            {
                if (Me is { } old && old.EntityId == p.EntityId)
                    OnPresenceChanged(old, p);
                else
                    OnEntered(p);
            };
            conn.SubscriptionBuilder()
                .OnApplied(_ => EnterWorld())
                .OnError((_, e) => SetStatus($"subscription failed: {e.Message}", error: true))
                .Subscribe(new[]
                {
                    "SELECT * FROM my_characters", "SELECT * FROM my_presence", "SELECT * FROM character_public",
                    "SELECT * FROM my_labor", "SELECT * FROM game_clock", "SELECT * FROM my_inventory", "SELECT * FROM my_craft",
                    "SELECT * FROM my_pack", "SELECT * FROM my_purse", "SELECT * FROM trade_demand",
                });
        }

        /// <summary>
        /// Recovered after a drop. The SDK replays our subscriptions next, and the views' <c>OnApplied</c>
        /// enters again. The server despawns a character when the connection that entered it closes
        /// (ADR 0020), which it may not have noticed yet, so we enter even if the old presence is still there.
        /// </summary>
        private void OnReconnected(DbConnection conn, Identity identity, string token)
        {
            SaveToken(token);
            Reconnects++;
            reenter = true;
            SetStatus("reconnected");
        }

        private void OnDisconnected(Exception? e, NextReconnect? next)
        {
            bool wasIn = Me is not null;
            Me = null;
            entering = false;
            string why = e is null ? "" : $": {e.Message}";
            if (next is { } n)
                SetStatus($"connection lost{why}; {Retry(n)}", error: true);
            else
                SetStatus($"disconnected{why}", error: e is not null);
            if (wasIn)
                Dropped?.Invoke();
        }

        private static string Retry(NextReconnect n) => $"retrying in {n.Delay.TotalSeconds:F1} s (attempt {n.Attempt})";

        /// <summary>Make a character named after the profile if we have none, then enter with the first one.</summary>
        private void EnterWorld()
        {
            if (Conn is null || (Me is not null && !reenter) || entering)
                return;
            var mine = Conn.Db.MyCharacters.Iter().OrderBy(c => c.CharId).FirstOrDefault();
            if (mine is null)
            {
                SetStatus($"creating character {options.CharacterName}");
                Conn.Reducers.CreateCharacter(options.CharacterName);
                return;
            }
            entering = true;
            reenter = false;
            Me = null;
            SetStatus("entering the world");
            Conn.Reducers.EnterWorld(mine.CharId, GameContent.Hash, terrainHash);
        }

        private void OnPresenceChanged(Presence old, Presence p)
        {
            Me = p;
            if (p.Corrections != old.Corrections)
            {
                Log($"correction {p.Corrections} at ({p.X:F1}, {p.Y:F1}, {p.Z:F1})");
                Corrected?.Invoke(p);
            }
        }

        private void OnEntered(Presence p)
        {
            entering = false;
            Me = p;
            var name = Conn!.Db.CharacterPublic.CharId.Find(p.CharId)?.Name ?? "?";
            SetStatus($"in the world as {name}");
            Log($"entered as {name}, entity {p.EntityId}, at ({p.X:F1}, {p.Y:F1}, {p.Z:F1})");
            Near ??= new SubscriptionDirector(new SdkCellChannel(Conn), NearTables, NearRadius);
            Near.Update(p.X, p.Z);
            Static ??= new SubscriptionDirector(new SdkCellChannel(Conn), StaticTables, StaticRadius);
            Static.Update(p.X, p.Z);
            if (options.Labor is int labor)
            {
                Conn.Reducers.DevSetLabor(labor);
                Log($"--labor: set the pool to {labor}");
            }
            Entered?.Invoke(p);
        }

        public void SendMove(Gameplay.MoveMsg m)
        {
            if (Conn is null || Me is null)
                return;
            Conn.Reducers.MoveEvent(m.Seq, m.Parent, m.Pos.X, m.Pos.Y, m.Pos.Z, m.Yaw, m.SpeedCmS, m.Mode);
        }

        private void SetStatus(string status, bool error = false)
        {
            Status = status;
            if (error)
                GD.PushError($"[{options.Profile}] {status}");
            else
                Log(status);
        }

        public void Log(string line) => GD.Print($"[{options.Profile}] {line}");

        private string TokenPath => $"user://tokens/{options.Profile}.token";

        private string? LoadToken()
        {
            if (!FileAccess.FileExists(TokenPath))
                return null;
            using var f = FileAccess.Open(TokenPath, FileAccess.ModeFlags.Read);
            var token = f?.GetAsText().Trim();
            return string.IsNullOrEmpty(token) ? null : token;
        }

        private void SaveToken(string token)
        {
            DirAccess.MakeDirRecursiveAbsolute("user://tokens");
            using var f = FileAccess.Open(TokenPath, FileAccess.ModeFlags.Write);
            f?.StoreString(token);
        }

        /// <summary>Subscribes cells through the SDK for the director.</summary>
        private sealed class SdkCellChannel : ICellChannel
        {
            private readonly DbConnection conn;

            public SdkCellChannel(DbConnection conn) => this.conn = conn;

            public object Subscribe(ulong cell, string[] queries, Action onApplied) =>
                conn.SubscriptionBuilder()
                    .OnApplied(_ => onApplied())
                    .OnError((_, e) => GD.PushError($"subscription error for cell {cell}: {e.Message}"))
                    .Subscribe(queries);

            public void Unsubscribe(object handle) => ((SubscriptionHandle)handle).Unsubscribe();
        }
    }
}
