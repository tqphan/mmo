using System;
using System.Collections.Generic;
using System.Linq;
using Godot;
using Mmo.Content;
using Mmo.Net;
using Mmo.Sim;
using Mmo.World;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace Mmo.Gameplay
{
    /// <summary>
    /// Combat (docs/design/combat.md): Tab cycles targets, F1 targets ourselves and Escape clears;
    /// keys 1–9 use the hotbar's skills, turning us to face the target first when it's behind us;
    /// R respawns when the wait is over. What the client can see is wrong is refused here by the
    /// same rules as the server's (<see cref="CombatModel"/>); the server decides. A ring marks the
    /// target, and each combat event in view floats its number above whoever it hit.
    /// </summary>
    public partial class Combat : Node3D
    {
        private const double MessageS = 4.0;
        private const double FloatS = 1.6;

        private readonly Session session;
        private readonly PlayerController player;
        private readonly EntityRegistry entities;
        private readonly List<(Label3D Label, double Born)> floating = new();
        private MeshInstance3D ring = null!;
        private string message = "";
        private double messageUntil;

        public Combat(Session session, PlayerController player, EntityRegistry entities)
        {
            this.session = session;
            this.player = player;
            this.entities = entities;
            Name = "Combat";
        }

        /// <summary>The entity we target, or 0.</summary>
        public ulong Target { get; private set; }

        /// <summary>Skills the server accepted and events we saw, for the demo.</summary>
        public int Accepted { get; private set; }
        public int EventsSeen { get; private set; }

        /// <summary>The last refusal or notice, while it's fresh.</summary>
        public string Message => Clock.Now < messageUntil ? message : "";

        public ulong Me => session.Me?.EntityId ?? 0;

        public VitalsInfo? Mine => Info(Me);

        public bool Dead => Mine?.Dead == true;

        /// <summary>Our fighter row: cooldowns and deaths.</summary>
        public Fighter? Fighter =>
            session.Conn is { } c && session.Me is { } me ? c.Db.MyFighter.Iter().FirstOrDefault(f => f.CharId == me.CharId) : null;

        public long Now => session.GameNowMicros;

        public override void _Ready()
        {
            ring = new MeshInstance3D
            {
                Name = "TargetRing",
                Mesh = new TorusMesh { InnerRadius = 0.75f, OuterRadius = 0.9f, Rings = 32, RingSegments = 6 },
                MaterialOverride = new StandardMaterial3D
                {
                    AlbedoColor = new Color(1.0f, 0.35f, 0.25f),
                    ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded,
                    NoDepthTest = true,
                },
                Visible = false,
            };
            AddChild(ring);
            var conn = session.Conn!;
            conn.Db.CombatEvent.OnInsert += (_, e) => Float(e);
            conn.Reducers.OnUseSkill += (ctx, skill, _) =>
            {
                if (ctx.Event.Status is Status.Failed(var reason))
                    Say($"{GameContent.Skill(skill)?.Name ?? "skill"}: {reason}");
                else
                    Accepted++;
            };
            conn.Reducers.OnRespawn += ctx =>
            {
                if (ctx.Event.Status is Status.Failed(var reason))
                    Say($"respawn: {reason}");
                else
                    session.Log("respawned");
            };
            conn.Reducers.OnSetTarget += (ctx, _) =>
            {
                if (ctx.Event.Status is Status.Failed(var reason))
                    Say($"target: {reason}");
            };
        }

        public override void _Process(double delta)
        {
            if (session.Me is null)
                return;
            if (Input.IsActionJustPressed("target_next"))
                TabNext();
            if (Input.IsActionJustPressed("target_self"))
                SetTarget(Me);
            if (Input.IsActionJustPressed("target_clear"))
                SetTarget(0);
            for (int k = 1; k <= CombatModel.Hotbar.Count; k++)
                if (Input.IsActionJustPressed($"skill_{k}"))
                    Use(k);
            if (Dead && Input.IsActionJustPressed("respawn"))
                Respawn();
            if (Target != 0 && Info(Target) is null)
                Target = 0;
            ring.Visible = Target != 0 && PositionOf(Target) is not null;
            if (ring.Visible)
                ring.GlobalPosition = PositionOf(Target)!.Value + new Vector3(0, 0.08f, 0);
            Drift();
        }

        /// <summary>An entity's vitals, if it has any in view.</summary>
        public VitalsInfo? Info(ulong id)
        {
            if (id == 0 || session.Conn?.Db.Vitals.EntityId.Find(id) is not { } v)
                return null;
            return new VitalsInfo(v.Hp, v.HpMax, v.Mp, v.MpMax, v.At.MicrosecondsSinceUnixEpoch, v.CombatUntil.MicrosecondsSinceUnixEpoch,
                v.DeadAt?.MicrosecondsSinceUnixEpoch, v.CastSkill, v.CastStart.MicrosecondsSinceUnixEpoch, v.CastEnd.MicrosecondsSinceUnixEpoch);
        }

        /// <summary>Where an entity is drawn: us, or another we can see.</summary>
        public Vector3? PositionOf(ulong id)
        {
            if (id == 0)
                return null;
            if (id == Me)
                return player.GlobalPosition;
            return entities.View(id) is { Visible: true } v ? v.GlobalPosition : null;
        }

        public string NameOf(ulong id)
        {
            if (id == Me && session.Me is { } me)
                return session.Conn?.Db.CharacterPublic.CharId.Find(me.CharId)?.Name ?? "You";
            return entities.View(id)?.Label ?? $"#{id}";
        }

        private bool LivingCharacter(ulong id) =>
            (id == Me || entities.View(id)?.Kind == EntityKind.Character) && Info(id) is { Dead: false };

        public void SetTarget(ulong id)
        {
            if (id == Target)
                return;
            Target = id;
            session.Conn?.Reducers.SetTarget(id);
        }

        /// <summary>The next target in front, nearest first (<see cref="CombatModel.TabOrder"/>).</summary>
        public void TabNext()
        {
            var at = PlayerController.ToNumerics(player.GlobalPosition);
            var candidates = entities.Views
                .Where(v => v.Visible && Info(v.EntityId) is not null)
                .Select(v => new Candidate(v.EntityId, PlayerController.ToNumerics(v.GlobalPosition), Info(v.EntityId)?.Dead == false));
            var next = CombatModel.NextTarget(CombatModel.TabOrder(candidates, Me, at, player.Facing), Target);
            if (next == 0)
                Say("nothing to target nearby");
            SetTarget(next);
        }

        /// <summary>Use hotbar key <paramref name="key"/> on the target.</summary>
        public void Use(int key)
        {
            if (CombatModel.Slot(key) is { } s)
                Use(s);
        }

        public void Use(SkillData s)
        {
            if (session.Conn is not { } conn || Mine is not { } mine)
                return;
            long now = Now;
            ulong to = CombatModel.Resolve(s, Me, Target, LivingCharacter);
            if (to == 0 || PositionOf(to) is not { } where || Info(to) is not { } them)
            {
                Say($"{s.Name}: {Sim.Combat.Message(CombatRefusal.NoTarget)}");
                return;
            }
            var f = Fighter;
            long ready = f is null ? 0 : CombatModel.ReadyAt(s.Wire, f.GcdUntil.MicrosecondsSinceUnixEpoch,
                f.Cooldowns.Select(c => (c.Skill, c.ReadyAt.MicrosecondsSinceUnixEpoch)));
            var here = PlayerController.ToNumerics(player.GlobalPosition);
            var caster = new Caster(here, player.Facing, !mine.Dead, mine.CastSkill != 0, mine.MpNow(now), ready);
            var plan = CombatModel.Plan(s, caster, new CombatTarget(PlayerController.ToNumerics(where), !them.Dead, to == Me), now);
            if (plan.Refusal is { } r)
            {
                Say($"{s.Name}: {Sim.Combat.Message(r)}");
                return;
            }
            if (plan.Turn)
                player.Face(Yaw.Of(where.X - here.X, where.Z - here.Z));
            conn.Reducers.UseSkill(s.Wire, Target);
        }

        public void Respawn() => session.Conn?.Reducers.Respawn();

        private void Say(string text)
        {
            message = text;
            messageUntil = Clock.Now + MessageS;
            session.Log($"combat: {text}");
        }

        /// <summary>A number floating up from whoever the event hit.</summary>
        private void Float(CombatEvent e)
        {
            EventsSeen++;
            if (e.Target == 0 || PositionOf(e.Target) is not { } at || e.SkillId < 100)
                return;
            bool heal = (e.Flags & EventFlags.Heal) != 0;
            var label = new Label3D
            {
                Text = CombatModel.Floating(e.Amount, e.Flags),
                Billboard = BaseMaterial3D.BillboardModeEnum.Enabled,
                FixedSize = true,
                FontSize = 44,
                PixelSize = 0.0011f,
                OutlineSize = 10,
                NoDepthTest = true,
                Modulate = heal ? new Color(0.45f, 1.0f, 0.5f) : e.Target == Me ? new Color(1.0f, 0.3f, 0.25f) : new Color(1.0f, 0.75f, 0.3f),
            };
            AddChild(label);
            // Above the nameplate, spread a little so several hits in a row don't stack on one spot.
            label.GlobalPosition = at + new Vector3((floating.Count % 3 - 1) * 0.4f, 2.9f, 0);
            floating.Add((label, Clock.Now));
            if (GameContent.Skill((ushort)e.SkillId) is { } s)
                session.Log($"combat: {NameOf(e.Source)} {(heal ? "healed" : "hit")} {NameOf(e.Target)} with {s.Name} for {e.Amount}{((e.Flags & EventFlags.Kill) != 0 ? ", killing them" : "")}");
        }

        /// <summary>Floating numbers rise and fade.</summary>
        private void Drift()
        {
            double now = Clock.Now;
            for (int i = floating.Count - 1; i >= 0; i--)
            {
                var (label, born) = floating[i];
                float age = (float)(now - born);
                if (age > FloatS)
                {
                    label.QueueFree();
                    floating.RemoveAt(i);
                    continue;
                }
                label.GlobalPosition += new Vector3(0, 0.9f * (float)GetProcessDeltaTime(), 0);
                label.Modulate = label.Modulate with { A = 1.0f - Mathf.Max(0, age - (float)FloatS * 0.5f) / ((float)FloatS * 0.5f) };
            }
        }
    }
}
