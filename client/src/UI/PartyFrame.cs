using Godot;
using Mmo.Gameplay;
using Mmo.Net;

namespace Mmo.UI
{
    /// <summary>
    /// The party frame at the top right (docs/design/social.md): each member with a star for the
    /// leader, and its level and HP while it's in view (from <c>vitals</c> in the Near tier), "far"
    /// when it's in the world out of view, "away" when it isn't. Below it, an invitation to us while
    /// it stands.
    /// </summary>
    public partial class PartyFrame : CanvasLayer
    {
        private readonly Session session;
        private readonly Party party;
        private readonly Gameplay.Combat combat;
        private readonly Label label = new();

        public PartyFrame(Session session, Party party, Gameplay.Combat combat)
        {
            this.session = session;
            this.party = party;
            this.combat = combat;
            Name = "PartyFrame";
            Layer = 2;
        }

        public override void _Ready()
        {
            label.AddThemeFontSizeOverride("font_size", 16);
            label.AddThemeColorOverride("font_color", new Color(0.7f, 0.88f, 1.0f));
            label.AddThemeColorOverride("font_outline_color", Colors.Black);
            label.AddThemeConstantOverride("outline_size", 4);
            label.HorizontalAlignment = HorizontalAlignment.Right;
            AddChild(label);
        }

        public override void _Process(double delta)
        {
            long now = session.GameNowMicros;
            var text = "";
            var members = party.Members;
            if (members.Count > 0)
            {
                text = $"Party ({members.Count}/{Sim.Party.Size})";
                foreach (var m in members)
                    text += "\n" + ChatModel.MemberLine(m.Name, m.Leader, m.EntityId != 0, combat.Info(m.EntityId), now);
            }
            if (party.Invitation is { } i && ChatModel.InviteLine(i.FromName, i.MadeAt.MicrosecondsSinceUnixEpoch, now) is { Length: > 0 } invite)
                text += (text.Length > 0 ? "\n\n" : "") + invite;
            label.Text = text;
            label.Visible = text.Length > 0;
            var size = GetViewport().GetVisibleRect().Size;
            label.Size = label.GetMinimumSize();
            label.Position = new Vector2(size.X - label.Size.X - 16, 96);
        }
    }
}
