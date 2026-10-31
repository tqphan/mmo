using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Gameplay;
using Mmo.Net;
using Mmo.Sim;

namespace Mmo.UI
{
    /// <summary>
    /// Chat (docs/design/social.md): the log at the bottom left, with Local chat heard through the
    /// Near tier within <c>local_m</c>, and party messages, notices and whispers from <c>my_chat</c>.
    /// Enter (or /) opens the line; Enter sends it, Escape drops it. Plain lines go to the channel
    /// last chosen with /s or /p; the rest are commands (<see cref="ChatModel"/>).
    /// </summary>
    public partial class ChatBox : CanvasLayer
    {
        private const int Shown = 12;
        private const int Kept = 200;
        private const float MaxWidth = 560;
        /// <summary>The hotbar's width (eight 76 px slots, 6 px apart), centred at the bottom: the box stays left of it.</summary>
        private const float HotbarPx = 8 * 76 + 7 * 6;

        private static readonly Dictionary<Channel, Color> Colors = new()
        {
            [Channel.Local] = new Color(0.95f, 0.95f, 0.92f),
            [Channel.Party] = new Color(0.55f, 0.8f, 1.0f),
            [Channel.Whisper] = new Color(1.0f, 0.62f, 0.9f),
        };
        private static readonly Color Notice = new(1.0f, 0.88f, 0.5f);
        private static readonly Color Refusal = new(1.0f, 0.5f, 0.42f);

        private readonly Session session;
        private readonly PlayerController player;
        private readonly Gameplay.Party party;
        private readonly List<(string Text, Color Color)> lines = new();
        private readonly VBoxContainer log = new() { MouseFilter = Control.MouseFilterEnum.Ignore };
        private readonly ColorRect back = new() { Color = new Color(0.03f, 0.03f, 0.05f, 0.45f), MouseFilter = Control.MouseFilterEnum.Ignore };
        private readonly Label prompt = new();
        private readonly LineEdit input = new() { Visible = false, MaxLength = 400, ContextMenuEnabled = false };
        private readonly HashSet<ulong> shownMessages = new();
        private bool dirty = true;
        private float width = MaxWidth;
        private ulong closedAt;

        public ChatBox(Session session, PlayerController player, Gameplay.Party party)
        {
            this.session = session;
            this.player = player;
            this.party = party;
            Name = "ChatBox";
            Layer = 3;
        }

        /// <summary>Where plain lines go: Local or Party.</summary>
        public Channel Current { get; private set; } = Channel.Local;

        /// <summary>The last character to whisper to us, for /r.</summary>
        public string? ReplyTo { get; private set; }

        public bool Open => input.Visible;

        /// <summary>Every line in the log, oldest first, for the demo.</summary>
        public IEnumerable<string> Lines => lines.Select(l => l.Text);

        public override void _Ready()
        {
            AddChild(back);
            AddChild(log);
            prompt.AddThemeFontSizeOverride("font_size", 16);
            prompt.AddThemeColorOverride("font_outline_color", Godot.Colors.Black);
            prompt.AddThemeConstantOverride("outline_size", 4);
            prompt.Visible = false;
            AddChild(prompt);
            input.AddThemeFontSizeOverride("font_size", 16);
            input.TextSubmitted += text =>
            {
                Submit(text);
                Close();
            };
            input.GuiInput += e =>
            {
                if (e is InputEventKey { Pressed: true, Keycode: Key.Escape })
                {
                    Close();
                    input.AcceptEvent();
                }
            };
            AddChild(input);
            session.Conn!.Db.LocalChat.OnInsert += (_, e) => Heard(e);
            party.Refused += why => Add(why, Refusal);
            Add("Enter to chat · /help lists the commands", Notice);
        }

        /// <summary>Enter, or /, opens the line when nothing else has the keyboard.</summary>
        public override void _UnhandledKeyInput(InputEvent e)
        {
            if (Open || session.Me is null || e is not InputEventKey { Pressed: true, Echo: false } k)
                return;
            if (k.Keycode is Key.Enter or Key.KpEnter)
                Begin("");
            else if (k.Keycode == Key.Slash)
                Begin("/");
            else
                return;
            GetViewport().SetInputAsHandled();
        }

        /// <summary>Open the line with <paramref name="text"/> typed, as Enter or / does.</summary>
        public void Begin(string text)
        {
            input.Text = text;
            input.Visible = prompt.Visible = true;
            GameInput.Typing = true;
            input.GrabFocus();
            input.CaretColumn = text.Length;
        }

        /// <summary>Send what's typed on the open line, as Enter does.</summary>
        public void Send()
        {
            Submit(input.Text);
            Close();
        }

        private void Close()
        {
            input.Visible = prompt.Visible = false;
            input.ReleaseFocus();
            // The keys stay quiet until next frame, so Escape doesn't also close a window.
            closedAt = Engine.GetProcessFrames();
        }

        /// <summary>Run a typed line: send it, or carry out its command.</summary>
        public void Submit(string line)
        {
            if (ChatModel.Parse(line, Current, ReplyTo) is not { } c)
                return;
            var conn = session.Conn;
            if (conn is null || session.Me is null)
                return;
            string? text = null;
            if (c.Kind is CommandKind.Say or CommandKind.Whisper)
            {
                text = Chat.Clean(c.Text, out var refusal);
                if (text is null)
                {
                    Add(refusal, Refusal);
                    return;
                }
            }
            switch (c.Kind)
            {
                case CommandKind.Say when c.Channel == Channel.Party && party.Mine is null:
                    Add("you're not in a party", Refusal);
                    break;
                case CommandKind.Say:
                    conn.Reducers.Chat((byte)c.Channel, text!);
                    break;
                case CommandKind.Whisper:
                    conn.Reducers.Whisper(c.Name, text!);
                    break;
                case CommandKind.Switch:
                    Current = c.Channel;
                    Add($"now talking in {Chat.Name(Current)}", Notice);
                    break;
                case CommandKind.Invite:
                    party.Invite(c.Name);
                    Add($"invited {c.Name}", Notice);
                    break;
                case CommandKind.Accept:
                    if (!party.Accept())
                        Add("no invitation to accept", Refusal);
                    break;
                case CommandKind.Decline:
                    if (!party.Decline())
                        Add("no invitation to decline", Refusal);
                    break;
                case CommandKind.Leave:
                    party.Leave();
                    break;
                case CommandKind.Kick or CommandKind.Lead:
                    if (party.Named(c.Name) is not { } m)
                        Add($"{c.Name} isn't in your party", Refusal);
                    else if (c.Kind == CommandKind.Kick)
                        party.Kick(m);
                    else
                        party.Lead(m);
                    break;
                case CommandKind.Help:
                    Add(c.Text, Notice);
                    break;
                default:
                    Add(c.Text, Refusal);
                    break;
            }
        }

        private void Heard(SpacetimeDB.Types.LocalChat e)
        {
            var at = PlayerController.ToNumerics(player.GlobalPosition);
            if (session.Me is null || !Chat.Hears(new System.Numerics.Vector3(e.X, e.Y, e.Z), at))
                return;
            Add(ChatModel.Line(Channel.Local, e.Name, "", e.Text, false), Colors[Channel.Local]);
        }

        private void Add(string text, Color color)
        {
            lines.Add((text, color));
            if (lines.Count > Kept)
                lines.RemoveRange(0, lines.Count - Kept);
            dirty = true;
        }

        public override void _Process(double delta)
        {
            if (!Open && GameInput.Typing && Engine.GetProcessFrames() > closedAt)
                GameInput.Typing = false;
            // Our inbox: anything not shown yet, in the order it came. A reconnect replays it.
            if (session.Me is { } me && session.Conn is { } conn)
            {
                foreach (var m in conn.Db.MyChat.Iter().Where(m => m.CharId == me.CharId && !shownMessages.Contains(m.Id)).OrderBy(m => m.Id).ToList())
                {
                    shownMessages.Add(m.Id);
                    var channel = (Channel)m.Channel;
                    bool mine = m.FromChar == me.CharId;
                    if (channel == Channel.Whisper && !mine)
                        ReplyTo = m.FromName;
                    Add(ChatModel.Line(channel, m.FromName, m.ToName, m.Text, mine), m.FromChar == 0 ? Notice : Colors.GetValueOrDefault(channel, Notice));
                }
            }
            var size = GetViewport().GetVisibleRect().Size;
            float fits = Mathf.Clamp((size.X - HotbarPx) / 2 - 32, 280, MaxWidth);
            if (fits != width)
            {
                width = fits;
                dirty = true;
            }
            float bottom = size.Y - 16;
            prompt.Text = ChatModel.Prompt(Current);
            prompt.Position = new Vector2(16, bottom - 30);
            input.Position = new Vector2(16 + prompt.GetMinimumSize().X + 8, bottom - 34);
            input.Size = new Vector2(width - prompt.GetMinimumSize().X - 8, 32);
            if (dirty)
                Rebuild();
            // The newest lines that fit in 40% of the screen; wrapped lines take more room.
            float room = size.Y * 0.4f, used = 0;
            for (int i = log.GetChildCount() - 1; i >= 0; i--)
            {
                var l = log.GetChild<Label>(i);
                float h = l.GetMinimumSize().Y + 4;
                l.Visible = used >= 0 && (used == 0 || used + h <= room);
                // Once one doesn't fit, none older shows either.
                used = l.Visible ? used + h : -1;
            }
            float top = bottom - 40 - log.GetCombinedMinimumSize().Y;
            log.Position = new Vector2(16, top);
            log.Size = new Vector2(width, 0);
            back.Position = new Vector2(10, top - 6);
            back.Size = new Vector2(width + 12, bottom - top + 6);
            back.Visible = Open || lines.Count > 0;
        }

        private void Rebuild()
        {
            dirty = false;
            foreach (var child in log.GetChildren())
            {
                log.RemoveChild(child);
                child.QueueFree();
            }
            foreach (var (text, color) in lines.Skip(System.Math.Max(0, lines.Count - Shown)))
            {
                var l = new Label { Text = text, AutowrapMode = TextServer.AutowrapMode.WordSmart, CustomMinimumSize = new Vector2(width, 0), MouseFilter = Control.MouseFilterEnum.Ignore };
                l.AddThemeFontSizeOverride("font_size", 15);
                l.AddThemeColorOverride("font_color", color);
                l.AddThemeColorOverride("font_outline_color", Godot.Colors.Black);
                l.AddThemeConstantOverride("outline_size", 4);
                log.AddChild(l);
            }
        }
    }
}
