using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;

namespace Mmo.UI
{
    /// <summary>
    /// Combat's frames (docs/design/combat.md): our HP, mana and cast bars above the hotbar, the
    /// target's name, HP and cast bar at the top, the hotbar with each skill's key, name and
    /// cooldown, the last refusal, and the death panel. Everything is settled to the present from
    /// <c>vitals</c> and <c>my_fighter</c> every frame.
    /// </summary>
    public partial class CombatHud : CanvasLayer
    {
        private const float SlotPx = 76;
        private const float GapPx = 6;
        private const float BarW = 360;

        private readonly Combat combat;
        private readonly Bar hp = new(new Color(0.78f, 0.2f, 0.18f));
        private readonly Bar mp = new(new Color(0.22f, 0.42f, 0.85f));
        private readonly Bar cast = new(new Color(0.95f, 0.78f, 0.3f));
        private readonly Bar targetHp = new(new Color(0.78f, 0.2f, 0.18f));
        private readonly Bar targetCast = new(new Color(0.95f, 0.78f, 0.3f));
        private readonly Label targetName = Text(20);
        private readonly Label message = Text(18);
        private readonly Label combatTag = Text(15);
        private readonly List<Slot> slots = new();
        private readonly ColorRect deathShade = new() { Color = new Color(0.25f, 0.0f, 0.0f, 0.35f), MouseFilter = Control.MouseFilterEnum.Ignore };
        private readonly Label deathTitle = Text(40);
        private readonly Label deathLine = Text(22);

        public CombatHud(Combat combat)
        {
            this.combat = combat;
            Name = "CombatHud";
            Layer = 2;
        }

        public override void _Ready()
        {
            AddChild(deathShade);
            foreach (var c in new Control[] { hp, mp, cast, targetHp, targetCast, targetName, message, combatTag, deathTitle, deathLine })
                AddChild(c);
            foreach (var s in CombatModel.Hotbar.Select((skill, i) => new Slot(skill, i + 1)))
            {
                slots.Add(s);
                AddChild(s);
            }
            deathTitle.Text = "You have died";
            deathTitle.AddThemeColorOverride("font_color", new Color(1.0f, 0.55f, 0.5f));
            message.AddThemeColorOverride("font_color", new Color(1.0f, 0.85f, 0.5f));
            combatTag.AddThemeColorOverride("font_color", new Color(1.0f, 0.5f, 0.4f));
        }

        public override void _Process(double delta)
        {
            var size = GetViewport().GetVisibleRect().Size;
            long now = combat.Now;
            var mine = combat.Mine;
            Visible = mine is not null;
            if (mine is not { } me)
                return;

            // The hotbar along the bottom, our bars above it.
            float barW = CombatModel.Hotbar.Count * (SlotPx + GapPx) - GapPx;
            float left = (size.X - barW) / 2;
            float top = size.Y - SlotPx - 16;
            var fighter = combat.Fighter;
            long gcd = fighter?.GcdUntil.MicrosecondsSinceUnixEpoch ?? 0;
            var cds = fighter?.Cooldowns.Select(c => (c.Skill, c.ReadyAt.MicrosecondsSinceUnixEpoch)).ToList() ?? new();
            int mpNow = me.MpNow(now);
            foreach (var s in slots)
            {
                s.Position = new Vector2(left + (s.Key - 1) * (SlotPx + GapPx), top);
                var (fraction, secs) = CombatModel.CooldownShown(s.Skill, gcd, cds, now);
                s.Show(fraction, secs, usable: !me.Dead && mpNow >= s.Skill.CostMp, casting: me.CastSkill == s.Skill.Wire);
            }
            float barX = (size.X - BarW) / 2;
            int hpNow = me.HpNow(now);
            mp.Place(new Vector2(barX, top - 30), BarW, 20, (float)mpNow / Mathf.Max(1, me.MpMax), $"MP {CombatModel.Amounts(mpNow, me.MpMax)}");
            hp.Place(new Vector2(barX, top - 56), BarW, 22, (float)hpNow / Mathf.Max(1, me.HpMax), $"HP {CombatModel.Amounts(hpNow, me.HpMax)}");
            combatTag.Text = me.InCombat(now) ? "in combat" : "";
            combatTag.Position = new Vector2(barX + BarW + 10, top - 54);
            cast.Visible = me.CastProgress(now) is not null;
            if (me.CastProgress(now) is float p)
                cast.Place(new Vector2(barX + 40, top - 100), BarW - 80, 18, p, GameContent.Skill(me.CastSkill)?.Name ?? "");
            message.Text = combat.Message;
            message.Position = new Vector2((size.X - message.GetMinimumSize().X) / 2, top - 132);

            // The target at the top.
            var target = combat.Target != 0 ? combat.Info(combat.Target) : null;
            targetName.Visible = targetHp.Visible = target is not null;
            targetCast.Visible = target?.CastProgress(now) is not null;
            if (target is { } t)
            {
                float tx = (size.X - BarW) / 2;
                targetName.Text = combat.NameOf(combat.Target) + (t.Dead ? " (dead)" : "");
                targetName.Position = new Vector2(tx, 16);
                int thp = t.HpNow(now);
                targetHp.Place(new Vector2(tx, 46), BarW, 22, (float)thp / Mathf.Max(1, t.HpMax), CombatModel.Amounts(thp, t.HpMax));
                if (t.CastProgress(now) is float tp)
                    targetCast.Place(new Vector2(tx + 40, 74), BarW - 80, 16, tp, GameContent.Skill(t.CastSkill)?.Name ?? "");
            }

            // The death panel.
            bool dead = me.Dead;
            deathShade.Visible = deathTitle.Visible = deathLine.Visible = dead;
            if (dead)
            {
                deathShade.Size = size;
                deathLine.Text = CombatModel.RespawnLine(me.DeadAt ?? now, fighter?.Deaths ?? 1, now);
                deathTitle.Position = new Vector2((size.X - deathTitle.GetMinimumSize().X) / 2, size.Y * 0.36f);
                deathLine.Position = new Vector2((size.X - deathLine.GetMinimumSize().X) / 2, size.Y * 0.36f + 56);
            }
        }

        private static Label Text(int px)
        {
            var l = new Label { MouseFilter = Control.MouseFilterEnum.Ignore };
            l.AddThemeFontSizeOverride("font_size", px);
            l.AddThemeColorOverride("font_outline_color", Colors.Black);
            l.AddThemeConstantOverride("outline_size", 5);
            return l;
        }

        /// <summary>A filled bar with its numbers on it.</summary>
        private partial class Bar : Control
        {
            private readonly ColorRect back = new() { Color = new Color(0.05f, 0.05f, 0.06f, 0.8f) };
            private readonly ColorRect fill = new();
            private readonly Label label = Text(14);

            public Bar(Color color)
            {
                fill.Color = color;
                MouseFilter = MouseFilterEnum.Ignore;
                AddChild(back);
                AddChild(fill);
                AddChild(label);
            }

            public void Place(Vector2 at, float w, float h, float fraction, string text)
            {
                Position = at;
                back.Size = new Vector2(w, h);
                fill.Position = new Vector2(2, 2);
                fill.Size = new Vector2((w - 4) * Mathf.Clamp(fraction, 0, 1), h - 4);
                label.Text = text;
                label.Position = new Vector2((w - label.GetMinimumSize().X) / 2, (h - label.GetMinimumSize().Y) / 2);
            }
        }

        /// <summary>One hotbar key: its number, the skill's name, and its cooldown sweeping down.</summary>
        private partial class Slot : Control
        {
            private readonly ColorRect back = new() { Color = new Color(0.1f, 0.1f, 0.12f, 0.85f) };
            private readonly ColorRect border = new();
            private readonly ColorRect shade = new() { Color = new Color(0, 0, 0, 0.6f) };
            private readonly Label key = Text(14);
            private readonly Label name = Text(13);
            private readonly Label secs = Text(22);

            public Slot(SkillData skill, int k)
            {
                Skill = skill;
                Key = k;
                MouseFilter = MouseFilterEnum.Ignore;
                border.Size = new Vector2(SlotPx, SlotPx);
                back.Position = new Vector2(2, 2);
                back.Size = new Vector2(SlotPx - 4, SlotPx - 4);
                border.Color = SkillsetColor(skill.Skillset);
                key.Text = k.ToString();
                key.Position = new Vector2(6, 2);
                name.Text = skill.Name.Replace(' ', '\n');
                name.HorizontalAlignment = HorizontalAlignment.Center;
                name.Size = new Vector2(SlotPx, 40);
                name.Position = new Vector2(0, 26);
                secs.HorizontalAlignment = HorizontalAlignment.Center;
                secs.Size = new Vector2(SlotPx, 30);
                secs.Position = new Vector2(0, 20);
                foreach (var c in new Control[] { border, back, key, name, shade, secs })
                    AddChild(c);
            }

            public SkillData Skill { get; }
            public int Key { get; }

            public void Show(float fraction, int seconds, bool usable, bool casting)
            {
                shade.Position = new Vector2(2, 2);
                shade.Size = new Vector2(SlotPx - 4, (SlotPx - 4) * fraction);
                secs.Text = seconds > 1 ? seconds.ToString() : "";
                name.Visible = secs.Text.Length == 0;
                name.Modulate = usable ? Colors.White : new Color(0.6f, 0.6f, 0.65f);
                back.Color = casting ? new Color(0.35f, 0.3f, 0.1f, 0.9f) : new Color(0.1f, 0.1f, 0.12f, 0.85f);
            }

            private static Color SkillsetColor(byte set) => set switch
            {
                0 => new Color(0.9f, 0.45f, 0.2f),
                1 => new Color(0.7f, 0.7f, 0.75f),
                _ => new Color(0.3f, 0.75f, 0.6f),
            };
        }
    }
}
