//! Parties (docs/design/social.md).

use super::*;
use crate::chat::{ChatMessage, chat_message};
use crate::party::{PartyInvite, accept_party_invite, decline_party_invite, invite_to_party, kick_from_party, lapse_party_invite, leave_party, make_party_leader, my_party, my_party_invites, party, party_invite};
use sim::party::PARTY;

/// Move `player`'s authoritative position, as accepted moves would.
pub(super) fn place(t: &TestContext, player: u128, x: f32, z: f32) {
    read(t, |ctx| {
        let p = ctx.db.presence().identity().find(identity(player)).unwrap();
        ctx.db.presence().identity().update(Presence { x, z, ..p });
    });
}

pub(super) fn invite(t: &TestContext, player: u128, name: &str) -> Result<(), String> {
    call(t, player, |ctx| invite_to_party(ctx, name.into()))
}

/// The invitation to `to` from `from`, if one stands.
fn invitation(t: &TestContext, from: u64, to: u64) -> Option<PartyInvite> {
    read(t, |ctx| ctx.db.party_invite().to_char().filter(to).find(|i| i.from_char == from))
}

pub(super) fn accept(t: &TestContext, player: u128, from: u64, to: u64) -> Result<(), String> {
    let id = invitation(t, from, to).ok_or("no invitation")?.scheduled_id;
    call(t, player, |ctx| accept_party_invite(ctx, id))
}

/// `players` (with their presences) in one party led by the first.
pub(super) fn group(t: &TestContext, players: &[(u128, &Presence, &str)]) {
    let (lead, lp, _) = players[0];
    for &(player, p, name) in &players[1..] {
        invite(t, lead, name).unwrap();
        accept(t, player, lp.char_id, p.char_id).unwrap();
    }
}

/// Members of `char_id`'s party as (name, leader, entity), the oldest first.
fn roster(t: &TestContext, char_id: u64) -> Vec<(String, bool, u64)> {
    read(t, |ctx| match crate::party::party_of(ctx, char_id) {
        Some(p) => crate::party::members(ctx, p).into_iter().map(|m| (m.name, m.leader, m.entity_id)).collect(),
        None => Vec::new(),
    })
}

fn notices(t: &TestContext, char_id: u64) -> Vec<String> {
    read(t, |ctx| {
        let mut all: Vec<ChatMessage> = ctx.db.chat_message().char_id().filter(char_id).filter(|m| m.from_char == 0).collect();
        all.sort_by_key(|m| m.id);
        all.into_iter().map(|m| m.text).collect()
    })
}

fn three() -> (TestContext, Presence, Presence, Presence) {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    let c = arrive(&t, 3, "Wren");
    (t, a, b, c)
}

#[test]
fn an_accepted_invitation_makes_a_party_led_by_the_inviter() {
    let (t, a, b, _) = three();
    invite(&t, 1, "tobin").unwrap();
    let i = invitation(&t, a.char_id, b.char_id).unwrap();
    assert_eq!((i.from_name.as_str(), i.to_name.as_str()), ("Mira", "Tobin"));
    assert!(roster(&t, a.char_id).is_empty(), "no party until it's taken");
    // Inviting again replaces the first invitation.
    invite(&t, 1, "Tobin").unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.party_invite().count()), 1);
    // Only the invitee can take it.
    let id = invitation(&t, a.char_id, b.char_id).unwrap().scheduled_id;
    assert_eq!(call(&t, 3, |ctx| accept_party_invite(ctx, id)).unwrap_err(), "no such invitation");
    accept(&t, 2, a.char_id, b.char_id).unwrap();
    assert_eq!(roster(&t, a.char_id), vec![("Mira".into(), true, a.entity_id), ("Tobin".into(), false, b.entity_id)]);
    assert_eq!(read(&t, |ctx| ctx.db.party_invite().count()), 0);
    assert_eq!(notices(&t, a.char_id), vec!["Tobin joined the party"]);
    assert!(read(&t, |ctx| crate::party::together(ctx, a.char_id, b.char_id)));
    assert!(!read(&t, |ctx| crate::party::together(ctx, a.char_id, a.char_id)));
}

#[test]
fn invitations_are_refused_when_they_cant_be_made() {
    let (t, a, b, c) = three();
    assert_eq!(invite(&t, 1, "Nobody").unwrap_err(), "no one called Nobody is in the world");
    assert_eq!(invite(&t, 1, "Mira").unwrap_err(), "you can't invite yourself");
    call(&t, 3, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(invite(&t, 1, "Wren").unwrap_err(), "no one called Wren is in the world");
    group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
    // Only the leader invites, and nobody already in a party is invited.
    call(&t, 3, |ctx| enter_world(ctx, c.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    assert_eq!(invite(&t, 2, "Wren").unwrap_err(), "only the party's leader can invite");
    assert_eq!(invite(&t, 3, "Tobin").unwrap_err(), "Tobin is already in a party");
    assert_eq!(call(&t, 1, |ctx| invite_to_party(ctx, "x".into())).unwrap_err(), "no one called x is in the world");
}

#[test]
fn invitations_lapse() {
    let (t, a, b, _) = three();
    invite(&t, 1, "Tobin").unwrap();
    let i = invitation(&t, a.char_id, b.char_id).unwrap();
    let at = read(&t, |ctx| ctx.timestamp) + TimeDuration::from_micros(i64::from(PARTY.invite_s) * 1_000_000);
    assert!(i.scheduled_at == spacetimedb::ScheduleAt::Time(at));
    advance_ms(&t, i64::from(PARTY.invite_s) * 1000);
    assert_eq!(accept(&t, 2, a.char_id, b.char_id).unwrap_err(), "that invitation has lapsed");
    // The refusal rolled back, so the scheduler still finds it and takes it away.
    t.with_reducer_tx::<_, String>(TestAuth::internal(), |ctx| lapse_party_invite(ctx, i.clone())).unwrap();
    assert!(invitation(&t, a.char_id, b.char_id).is_none());
    assert!(call(&t, 1, |ctx| lapse_party_invite(ctx, i.clone())).is_err(), "players can't run the schedule");
}

#[test]
fn declining_or_leaving_the_world_ends_an_invitation() {
    let (t, a, b, _) = three();
    invite(&t, 1, "Tobin").unwrap();
    let id = invitation(&t, a.char_id, b.char_id).unwrap().scheduled_id;
    call(&t, 2, |ctx| decline_party_invite(ctx, id)).unwrap();
    assert!(invitation(&t, a.char_id, b.char_id).is_none());
    // The inviter can take one back too.
    invite(&t, 1, "Tobin").unwrap();
    let id = invitation(&t, a.char_id, b.char_id).unwrap().scheduled_id;
    assert!(call(&t, 3, |ctx| decline_party_invite(ctx, id)).is_err());
    call(&t, 1, |ctx| decline_party_invite(ctx, id)).unwrap();
    invite(&t, 1, "Tobin").unwrap();
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert!(invitation(&t, a.char_id, b.char_id).is_none());
}

#[test]
fn joining_takes_away_other_invitations_and_a_party_fills_up() {
    let t = world();
    let names = ["Mira", "Tobin", "Wren", "Alda", "Brin", "Cass"];
    let ps: Vec<Presence> = names.iter().enumerate().map(|(i, n)| arrive(&t, i as u128 + 1, n)).collect();
    invite(&t, 3, "Tobin").unwrap();
    group(&t, &[(1, &ps[0], "Mira"), (2, &ps[1], "Tobin")]);
    assert!(invitation(&t, ps[2].char_id, ps[1].char_id).is_none(), "Wren's invitation went when Tobin joined Mira");
    for (i, n) in names.iter().enumerate().take(5).skip(2) {
        invite(&t, 1, n).unwrap();
        accept(&t, i as u128 + 1, ps[0].char_id, ps[i].char_id).unwrap();
    }
    assert_eq!(roster(&t, ps[0].char_id).len(), PARTY.size as usize);
    assert_eq!(invite(&t, 1, "Cass").unwrap_err(), "your party is full");
}

#[test]
fn an_invitation_from_a_full_party_or_a_former_leader_is_no_good() {
    let (t, a, b, c) = three();
    invite(&t, 1, "Wren").unwrap();
    group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
    call(&t, 1, |ctx| make_party_leader(ctx, b.char_id)).unwrap();
    assert_eq!(accept(&t, 3, a.char_id, c.char_id).unwrap_err(), "Mira no longer leads a party");
}

#[test]
fn leaders_kick_and_hand_over_and_a_leaving_leader_passes_the_lead() {
    let (t, a, b, c) = three();
    group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin"), (3, &c, "Wren")]);
    assert_eq!(call(&t, 2, |ctx| kick_from_party(ctx, c.char_id)).unwrap_err(), "only the party's leader can do that");
    assert_eq!(call(&t, 1, |ctx| kick_from_party(ctx, a.char_id)).unwrap_err(), "that's you");
    call(&t, 1, |ctx| make_party_leader(ctx, c.char_id)).unwrap();
    let leaders = |t: &TestContext| roster(t, a.char_id).into_iter().filter(|m| m.1).map(|m| m.0).collect::<Vec<_>>();
    assert_eq!(leaders(&t), vec!["Wren"]);
    // The leader leaves: the oldest member left leads.
    call(&t, 3, leave_party).unwrap();
    assert_eq!(leaders(&t), vec!["Mira"]);
    assert_eq!(notices(&t, c.char_id).last().unwrap(), "you left the party");
    assert!(notices(&t, b.char_id).contains(&"Wren left the party".to_string()));
    // Kicked: and a party of one is no party.
    call(&t, 1, |ctx| kick_from_party(ctx, b.char_id)).unwrap();
    assert!(roster(&t, a.char_id).is_empty());
    assert_eq!(read(&t, |ctx| ctx.db.party().count()), 0);
    assert_eq!(notices(&t, b.char_id).last().unwrap(), "you were removed from the party");
    assert_eq!(notices(&t, a.char_id).last().unwrap(), "the party has broken up");
    assert_eq!(call(&t, 1, leave_party).unwrap_err(), "you're not in a party");
}

#[test]
fn members_stay_while_out_of_the_world() {
    let (t, a, b, _) = three();
    group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
    call(&t, 2, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(roster(&t, a.char_id)[1], ("Tobin".into(), false, 0));
    call(&t, 2, |ctx| enter_world(ctx, b.char_id, CONTENT_HASH, server_terrain(ctx))).unwrap();
    let back = presence_of(&t, 2);
    assert_eq!(roster(&t, a.char_id)[1], ("Tobin".into(), false, back.entity_id));
    // The leader can kick one who's away.
    call(&t, 2, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    call(&t, 1, |ctx| kick_from_party(ctx, b.char_id)).unwrap();
    assert!(roster(&t, a.char_id).is_empty());
}

#[test]
fn my_party_and_my_invites_show_only_our_own() {
    let (t, a, b, c) = three();
    group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
    let names = |player: u128| {
        let mut n: Vec<String> = my_party(&t.view_context(auth(player))).into_iter().map(|m| m.name).collect();
        n.sort();
        n
    };
    assert_eq!(names(1), vec!["Mira", "Tobin"]);
    assert_eq!(names(2), vec!["Mira", "Tobin"]);
    assert!(names(3).is_empty());
    arrive(&t, 4, "Alda");
    invite(&t, 1, "Wren").unwrap();
    let seen = |player: u128| my_party_invites(&t.view_context(auth(player))).into_iter().map(|i| (i.from_char, i.to_char)).collect::<Vec<_>>();
    assert_eq!(seen(1), vec![(a.char_id, c.char_id)]);
    assert_eq!(seen(3), vec![(a.char_id, c.char_id)]);
    assert!(seen(2).is_empty() && seen(4).is_empty());
}

#[cfg(feature = "dev")]
mod fighting {
    use super::super::combat::{put, use_on, veteran, vitals_of};
    use super::*;
    use crate::combat::{Vitals, dev_spawn_dummy, fighter, pick_skillset, vitals};

    fn dummy(t: &TestContext, player: u128, x: f32, z: f32) -> u64 {
        call(t, player, |ctx| dev_spawn_dummy(ctx, x, z)).unwrap();
        read(t, |ctx| ctx.db.entity().iter().filter(|e| e.kind == entity_kind::DUMMY).map(|e| e.id).max().unwrap())
    }

    fn xp(t: &TestContext, char_id: u64) -> u32 {
        read(t, |ctx| ctx.db.fighter().char_id().find(char_id).unwrap().xp)
    }

    #[test]
    fn party_members_cant_harm_each_other() {
        let (t, a, b, _) = three();
        veteran(&t, 1);
        put(&t, 2, 0.0, -3.0);
        group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
        assert_eq!(use_on(&t, 1, "threefold_cut", b.entity_id).unwrap_err(), "you can't attack a party member");
        assert_eq!(vitals_of(&t, b.entity_id).hp, vitals_of(&t, b.entity_id).hp_max);
        // Out of the party, the blow lands.
        call(&t, 2, leave_party).unwrap();
        use_on(&t, 1, "threefold_cut", b.entity_id).unwrap();
        assert!(vitals_of(&t, b.entity_id).hp < vitals_of(&t, b.entity_id).hp_max);
    }

    #[test]
    fn a_kill_is_shared_by_members_alive_and_near() {
        let t = world();
        let a = arrive(&t, 1, "Mira");
        let b = arrive(&t, 2, "Tobin");
        let c = arrive(&t, 3, "Wren");
        let d = arrive(&t, 4, "Alda");
        call(&t, 1, |ctx| pick_skillset(ctx, 1)).unwrap();
        group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin"), (3, &c, "Wren"), (4, &d, "Alda")]);
        put(&t, 2, 50.0, 0.0);
        put(&t, 3, 0.0, PARTY.share_m + 10.0);
        let target = dummy(&t, 1, 0.0, -3.0);
        let kill = |t: &TestContext| {
            read(t, |ctx| {
                let v = ctx.db.vitals().entity_id().find(target).unwrap();
                ctx.db.vitals().entity_id().update(Vitals { hp: 10, ..v });
            });
            advance_ms(t, 1000);
            use_on(t, 1, "threefold_cut", target).unwrap();
        };
        // Alda is dead, Wren too far: Mira and Tobin share 110% of the kill.
        read(&t, |ctx| {
            let v = ctx.db.vitals().entity_id().find(d.entity_id).unwrap();
            ctx.db.vitals().entity_id().update(Vitals { hp: 0, dead_at: Some(crate::time::now(ctx)), ..v });
        });
        kill(&t);
        let whole = sim::combat::kill_xp(1, 1);
        assert_eq!((xp(&t, a.char_id), xp(&t, b.char_id), xp(&t, c.char_id), xp(&t, d.char_id)), (sim::party::share(whole, 2), sim::party::share(whole, 2), 0, 0));
        // Alone, all of it.
        call(&t, 1, leave_party).unwrap();
        let before = xp(&t, a.char_id);
        kill(&t);
        assert_eq!(xp(&t, a.char_id), before + whole);
        assert_eq!(xp(&t, b.char_id), sim::party::share(whole, 2));
    }
}
