using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Gameplay;

namespace Mmo.UI
{
    /// <summary>
    /// Combat's frames (docs/design/combat.md): our HP, mana and cast bars and our buffs and
    /// debuffs above the hotbar, with our level and experience; the target's name, level, HP,
    /// statuses and cast bar at the top; the hotbar, one row per skillset picked, with each skill's
    /// key, name and cooldown; the last refusal; the skillset picker (K); and the death panel.
    /// Everything is settled to the present from <c>vitals</c> and <c>my_fighter</c> every frame.
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
        private readonly Label levelLine = Text(15);
        private readonly Label pickLine = Text(15);
        private readonly Label noSkills = Text(18);
        private readonly Chips myChips = new();
        private readonly Chips targetChips = new();
        private readonly Picker picker;
        private readonly List<Slot> slots = new();
        private string slotsFor = "-";
        private readonly ColorRect deathShade = new() { Color = new Color(0.25f, 0.0f, 0.0f, 0.35f), MouseFilter = Control.MouseFilterEnum.Ignore };
        private readonly Label deathTitle = Text(40);
        private readonly Label deathLine = Text(22);

        public CombatHud(Combat combat)
        {
            this.combat = combat;
            picker = new Picker(combat);
            Name = "CombatHud";
            Layer = 2;
        }

        public override void _Ready()
        {
            AddChild(deathShade);
            foreach (var c in new Control[] { hp, mp, cast, targetHp, targetCast, targetName, message, combatTag, levelLine, pickLine, noSkills, myChips, targetChips, deathTitle, deathLine, picker })
                AddChild(c);
            noSkills.Text = "no skills yet: press K to pick a skillset";
            levelLine.AddThemeColorOverride("font_color", new Color(1.0f, 0.9f, 0.55f));
            pickLine.AddThemeColorOverride("font_color", new Color(1.0f, 0.9f, 0.55f));
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

            // The hotbar along the bottom, a row per skillset with the first lowest; our bars above it.
            var picks = combat.Picks;
            Rebuild(picks);
            int rows = Mathf.Max(1, picks.Count);
            float barW = CombatModel.RowKeys * (SlotPx + GapPx) - GapPx;
            float left = (size.X - barW) / 2;
            float top = size.Y - 16 - rows * (SlotPx + GapPx) + GapPx;
            var fighter = combat.Fighter;
            long gcd = fighter?.GcdUntil.MicrosecondsSinceUnixEpoch ?? 0;
            var cds = fighter?.Cooldowns.Select(c => (c.Skill, c.ReadyAt.MicrosecondsSinceUnixEpoch)).ToList() ?? new();
            int mpNow = me.MpNow(now);
            byte level = combat.Level;
            foreach (var s in slots)
            {
                s.Position = new Vector2(left + (s.Key - 1) * (SlotPx + GapPx), size.Y - 16 - (s.Row + 1) * (SlotPx + GapPx) + GapPx);
                var (fraction, secs) = CombatModel.CooldownShown(s.Skill, gcd, cds, now);
                bool learned = level >= s.Skill.Level;
                s.Show(fraction, secs, usable: learned && !me.Dead && !me.Controlled(now) && mpNow >= s.Skill.CostMp, casting: me.CastSkill == s.Skill.Wire, learned);
            }
            noSkills.Visible = picks.Count == 0;
            noSkills.Position = new Vector2((size.X - noSkills.GetMinimumSize().X) / 2, top + SlotPx / 2 - 12);
            float barX = (size.X - BarW) / 2;
            int hpNow = me.HpNow(now);
            mp.Place(new Vector2(barX, top - 30), BarW, 20, (float)mpNow / Mathf.Max(1, me.MpMax), $"MP {CombatModel.Amounts(mpNow, me.MpMax)}");
            hp.Place(new Vector2(barX, top - 56), BarW, 22, (float)hpNow / Mathf.Max(1, me.HpMax), $"HP {CombatModel.Amounts(hpNow, me.HpMax)}");
            combatTag.Text = me.InCombat(now) ? "in combat" : "";
            combatTag.Position = new Vector2(barX + BarW + 10, top - 54);
            levelLine.Text = CombatModel.LevelLine(level, fighter?.Xp ?? 0);
            levelLine.Position = new Vector2(barX + BarW + 10, top - 30);
            pickLine.Text = CombatModel.PickLine(level, picks.Count);
            pickLine.Position = new Vector2(barX - 10 - pickLine.GetMinimumSize().X, top - 30);
            myChips.Place(new Vector2(barX, top - 84), me.Slots, now);
            cast.Visible = me.CastProgress(now) is not null;
            if (me.CastProgress(now) is float p)
                cast.Place(new Vector2(barX + 40, top - 122), BarW - 80, 18, p, GameContent.Skill(me.CastSkill)?.Name ?? "");
            message.Text = combat.Message;
            message.Position = new Vector2((size.X - message.GetMinimumSize().X) / 2, top - 156);

            // The target at the top.
            var target = combat.Target != 0 ? combat.Info(combat.Target) : null;
            targetName.Visible = targetHp.Visible = targetChips.Visible = target is not null;
            targetCast.Visible = target?.CastProgress(now) is not null;
            if (target is { } t)
            {
                float tx = (size.X - BarW) / 2;
                targetName.Text = $"{combat.NameOf(combat.Target)}  ·  level {t.Level}" + (t.Dead ? " (dead)" : "");
                targetName.Position = new Vector2(tx, 16);
                int thp = t.HpNow(now);
                targetHp.Place(new Vector2(tx, 46), BarW, 22, (float)thp / Mathf.Max(1, t.HpMax), CombatModel.Amounts(thp, t.HpMax));
                targetChips.Place(new Vector2(tx, 74), t.Slots, now);
                if (t.CastProgress(now) is float tp)
                    targetCast.Place(new Vector2(tx + 40, 104), BarW - 80, 16, tp, GameContent.Skill(t.CastSkill)?.Name ?? "");
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

        /// <summary>Make the hotbar's slots again when the skillsets picked change.</summary>
        private void Rebuild(IReadOnlyList<byte> picks)
        {
            string key = string.Join(",", picks);
            if (key == slotsFor)
                return;
            slotsFor = key;
            foreach (var s in slots)
                s.QueueFree();
            slots.Clear();
            var rows = CombatModel.Rows(picks);
            for (int r = 0; r < rows.Count; r++)
                for (int k = 0; k < rows[r].Count; k++)
                {
                    var s = new Slot(rows[r][k], r, k + 1);
                    slots.Add(s);
                    AddChild(s);
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

            public Slot(SkillData skill, int row, int k)
            {
                Skill = skill;
                Row = row;
                Key = k;
                MouseFilter = MouseFilterEnum.Ignore;
                border.Size = new Vector2(SlotPx, SlotPx);
                back.Position = new Vector2(2, 2);
                back.Size = new Vector2(SlotPx - 4, SlotPx - 4);
                border.Color = SkillsetColor(skill.Skillset);
                key.Text = CombatModel.KeyLabel(row, k);
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
            public int Row { get; }
            public int Key { get; }

            public void Show(float fraction, int seconds, bool usable, bool casting, bool learned)
            {
                shade.Position = new Vector2(2, 2);
                shade.Size = new Vector2(SlotPx - 4, (SlotPx - 4) * (learned ? fraction : 1));
                secs.Text = !learned ? $"L{Skill.Level}" : seconds > 1 ? seconds.ToString() : "";
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
    
        /// <summary>A row of buffs and debuffs: each its name and seconds left, buffs edged green and debuffs red.</summary>
        private partial class Chips : Control
        {
            private const float W = 96, H = 22;
            private readonly List<(ColorRect Edge, Label Text)> pool = new();

            public Chips() => MouseFilter = MouseFilterEnum.Ignore;

            public void Place(Vector2 at, IReadOnlyList<Sim.Slot> slots, long now)
            {
                Position = at;
                var chips = CombatModel.Chips(slots, now);
                while (pool.Count < chips.Count)
                {
                    var edge = new ColorRect { Size = new Vector2(W, H), MouseFilter = MouseFilterEnum.Ignore };
                    edge.AddChild(new ColorRect { Position = new Vector2(2, 2), Size = new Vector2(W - 4, H - 4), Color = new Color(0.06f, 0.06f, 0.08f, 0.9f), MouseFilter = MouseFilterEnum.Ignore });
                    var text = Text(13);
                    text.Size = new Vector2(W, H);
                    text.HorizontalAlignment = HorizontalAlignment.Center;
                    text.VerticalAlignment = VerticalAlignment.Center;
                    edge.AddChild(text);
                    AddChild(edge);
                    pool.Add((edge, text));
                }
                for (int i = 0; i < pool.Count; i++)
                {
                    var (edge, text) = pool[i];
                    edge.Visible = i < chips.Count;
                    if (i >= chips.Count)
                        continue;
                    edge.Position = new Vector2(i * (W + 4), 0);
                    edge.Color = chips[i].Debuff ? new Color(0.85f, 0.3f, 0.35f) : new Color(0.35f, 0.85f, 0.5f);
                    text.Text = chips[i].Text;
                }
            }
        }

        /// <summary>The skillset picker (K): a button per skillset, open while there's a pick to make.</summary>
        private partial class Picker : PanelContainer
        {
            private readonly Combat combat;
            private readonly Label title = Text(20);
            private readonly List<(byte Set, Button Button)> buttons = new();

            public Picker(Combat combat)
            {
                this.combat = combat;
                Visible = false;
                var box = new VBoxContainer();
                box.AddThemeConstantOverride("separation", 8);
                AddChild(box);
                box.AddChild(title);
                foreach (var set in GameContent.Skillsets.Where(s => !s.Npc))
                {
                    var skills = string.Join(", ", GameContent.Skills.Where(s => s.Skillset == set.Wire).Take(3).Select(s => s.Name));
                    var b = new Button { Text = $"{set.Name}: {skills}, …", CustomMinimumSize = new Vector2(460, 40) };
                    byte wire = set.Wire;
                    b.Pressed += () => combat.PickSkillset(wire);
                    box.AddChild(b);
                    buttons.Add((wire, b));
                }
                var close = new Button { Text = "Close (K)" };
                close.Pressed += () => combat.Picking = false;
                box.AddChild(close);
            }

            public override void _Process(double delta)
            {
                Visible = combat.Picking && combat.Mine is not null;
                if (!Visible)
                    return;
                var picks = combat.Picks;
                int free = Sim.Combat.SkillsetSlots(combat.Level) - picks.Count;
                title.Text = free > 0 ? "Pick a skillset" : CombatModel.PickLine(combat.Level, picks.Count) is { Length: > 0 } line ? $"Skillsets: {line}" : "Skillsets: all picked";
                foreach (var (set, b) in buttons)
                    b.Disabled = free <= 0 || picks.Contains(set);
                var size = GetViewport().GetVisibleRect().Size;
                Position = new Vector2((size.X - Size.X) / 2, size.Y * 0.18f);
            }
        }
}
}
