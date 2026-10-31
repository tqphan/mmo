//! Parties (docs/design/social.md): invitations, members and a leader, kept while members are out
//! of the world. Party members can't harm each other, share a kill's experience when near it, and
//! share the loot one of them owns. Who is in a party is private: members read theirs through
//! `my_party`, and invitations through `my_party_invites`.

use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp, ViewContext};

use crate::session::{Presence, character_private__view, character_public, presence};
use crate::time::now;

/// Private: a party. Its members are `party_member` rows.
#[spacetimedb::table(accessor = party)]
pub struct Party {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub made_at: Timestamp,
}

/// Private: a character in a party. Members read their party's rows through `my_party`.
#[spacetimedb::table(accessor = party_member)]
#[derive(Clone, Debug, PartialEq)]
pub struct PartyMember {
    #[primary_key]
    pub char_id: u64,
    #[index(btree)]
    pub party_id: u64,
    pub name: String,
    /// The character's entity while it's in the world, or 0, so members can find each other in view.
    pub entity_id: u64,
    pub leader: bool,
    pub joined_at: Timestamp,
}

/// Private: an invitation from one character to another, which lapses by itself when its time
/// comes. Both read it through `my_party_invites`.
#[spacetimedb::table(accessor = party_invite, scheduled(lapse_party_invite))]
#[derive(Clone, Debug, PartialEq)]
pub struct PartyInvite {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    #[index(btree)]
    pub from_char: u64,
    pub from_name: String,
    #[index(btree)]
    pub to_char: u64,
    pub to_name: String,
    pub made_at: Timestamp,
}

/// The members of every party the caller's characters are in.
#[spacetimedb::view(accessor = my_party, public)]
pub(crate) fn my_party(ctx: &ViewContext) -> Vec<PartyMember> {
    let mut out = Vec::new();
    for c in ctx.db.character_private().owner().filter(ctx.sender()) {
        if let Some(m) = ctx.db.party_member().char_id().find(c.char_id) {
            out.extend(ctx.db.party_member().party_id().filter(m.party_id));
        }
    }
    out
}

/// Invitations to and from the caller's characters.
#[spacetimedb::view(accessor = my_party_invites, public)]
pub(crate) fn my_party_invites(ctx: &ViewContext) -> Vec<PartyInvite> {
    let mut out = Vec::new();
    for c in ctx.db.character_private().owner().filter(ctx.sender()) {
        out.extend(ctx.db.party_invite().to_char().filter(c.char_id));
        out.extend(ctx.db.party_invite().from_char().filter(c.char_id));
    }
    out
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

fn me(ctx: &ReducerContext) -> Result<Presence, String> {
    ctx.db.presence().identity().find(ctx.sender()).ok_or_else(|| "not in the world".into())
}

/// The party `char_id` is in.
pub fn party_of(ctx: &ReducerContext, char_id: u64) -> Option<u64> {
    ctx.db.party_member().char_id().find(char_id).map(|m| m.party_id)
}

/// Whether two different characters are in the same party.
pub fn together(ctx: &ReducerContext, a: u64, b: u64) -> bool {
    a != b && party_of(ctx, a).is_some_and(|p| party_of(ctx, b) == Some(p))
}

/// Everyone in `party_id`, the oldest member first, and the leader first of those who joined
/// together (the party's maker and its first guest).
pub fn members(ctx: &ReducerContext, party_id: u64) -> Vec<PartyMember> {
    let mut all: Vec<PartyMember> = ctx.db.party_member().party_id().filter(party_id).collect();
    all.sort_by_key(|m| (micros(m.joined_at), !m.leader, m.char_id));
    all
}

/// `char_id` and everyone in its party.
pub fn mates(ctx: &ReducerContext, char_id: u64) -> Vec<u64> {
    match party_of(ctx, char_id) {
        Some(p) => members(ctx, p).into_iter().map(|m| m.char_id).collect(),
        None => vec![char_id],
    }
}

/// Who shares a kill made by `killer` at `kill`: the killer, and every member of its party in the
/// world, alive and near enough (docs/design/social.md).
pub fn sharers(ctx: &ReducerContext, killer: &Presence, kill: Option<[f32; 3]>) -> Vec<Presence> {
    let mut out = Vec::new();
    for char_id in mates(ctx, killer.char_id) {
        let Some(p) = ctx.db.presence().char_id().find(char_id) else { continue };
        let near = |p: &Presence| kill.is_none_or(|k| sim::party::shares(k, crate::ship::world_position(ctx, p)));
        if p.char_id == killer.char_id || (crate::combat::alive(ctx, p.entity_id) && near(&p)) {
            out.push(p);
        }
    }
    out
}

/// Called on entering the world: members see where `char_id` is.
pub fn entered(ctx: &ReducerContext, char_id: u64, entity_id: u64) {
    if let Some(m) = ctx.db.party_member().char_id().find(char_id) {
        ctx.db.party_member().char_id().update(PartyMember { entity_id, ..m });
    }
}

/// Called on leaving the world: `char_id` stays in its party, out of sight, and its invitations go.
pub fn left(ctx: &ReducerContext, char_id: u64) {
    if let Some(m) = ctx.db.party_member().char_id().find(char_id) {
        ctx.db.party_member().char_id().update(PartyMember { entity_id: 0, ..m });
    }
    for i in ctx.db.party_invite().from_char().filter(char_id).chain(ctx.db.party_invite().to_char().filter(char_id)).collect::<Vec<_>>() {
        ctx.db.party_invite().scheduled_id().delete(i.scheduled_id);
    }
}

fn name_of(ctx: &ReducerContext, char_id: u64) -> String {
    ctx.db.character_public().char_id().find(char_id).map(|c| c.name).unwrap_or_default()
}

/// Tell `party_id`'s members something, in party chat.
fn notice(ctx: &ReducerContext, party_id: u64, text: &str) {
    crate::chat::notice(ctx, members(ctx, party_id).iter().map(|m| m.char_id), text);
}

/// Invite the character called `name`, who must be in the world and in no party. Inviting from a
/// party takes its leader; inviting with none makes one when the invitation is taken.
#[spacetimedb::reducer]
pub fn invite_to_party(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let p = me(ctx)?;
    let target = crate::session::character_named(ctx, &name).ok_or_else(|| format!("no one called {name} is in the world"))?;
    let them = ctx.db.presence().char_id().find(target.char_id).ok_or_else(|| format!("no one called {} is in the world", target.name))?;
    if them.char_id == p.char_id {
        return Err("you can't invite yourself".into());
    }
    if party_of(ctx, them.char_id).is_some() {
        return Err(format!("{} is already in a party", target.name));
    }
    if let Some(m) = ctx.db.party_member().char_id().find(p.char_id) {
        if !m.leader {
            return Err("only the party's leader can invite".into());
        }
        if members(ctx, m.party_id).len() >= sim::party::size() {
            return Err("your party is full".into());
        }
    }
    // One standing invitation from one character to another: a second replaces the first.
    for i in ctx.db.party_invite().from_char().filter(p.char_id).filter(|i| i.to_char == them.char_id).collect::<Vec<_>>() {
        ctx.db.party_invite().scheduled_id().delete(i.scheduled_id);
    }
    let made_at = now(ctx);
    let lapse = sim::party::lapses_at(micros(made_at)) - micros(made_at);
    ctx.db.party_invite().insert(PartyInvite {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + TimeDuration::from_micros(lapse)).into(),
        from_char: p.char_id,
        from_name: name_of(ctx, p.char_id),
        to_char: them.char_id,
        to_name: target.name,
        made_at,
    });
    Ok(())
}

/// An invitation's time has come by the host's clock. Game time may have run ahead (the dev
/// clock), but never behind, so it has lapsed either way.
#[spacetimedb::reducer]
pub fn lapse_party_invite(ctx: &ReducerContext, invite: PartyInvite) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("lapse_party_invite runs on the server's schedule only".into());
    }
    ctx.db.party_invite().scheduled_id().delete(invite.scheduled_id);
    Ok(())
}

/// The invitation `id` to the caller's character in the world.
fn invite_to_me(ctx: &ReducerContext, p: &Presence, id: u64) -> Result<PartyInvite, String> {
    ctx.db.party_invite().scheduled_id().find(id).filter(|i| i.to_char == p.char_id).ok_or_else(|| "no such invitation".into())
}

/// Join the party of the character who invited us, making it if they have none.
#[spacetimedb::reducer]
pub fn accept_party_invite(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let p = me(ctx)?;
    let i = invite_to_me(ctx, &p, id)?;
    let at = now(ctx);
    if sim::party::lapsed(micros(i.made_at), micros(at)) {
        ctx.db.party_invite().scheduled_id().delete(id);
        return Err("that invitation has lapsed".into());
    }
    if party_of(ctx, p.char_id).is_some() {
        return Err("you're already in a party".into());
    }
    let party_id = match ctx.db.party_member().char_id().find(i.from_char) {
        Some(m) if !m.leader => return Err(format!("{} no longer leads a party", i.from_name)),
        Some(m) if members(ctx, m.party_id).len() >= sim::party::size() => return Err(format!("{}'s party is full", i.from_name)),
        Some(m) => m.party_id,
        None => {
            let party = ctx.db.party().insert(Party { id: 0, made_at: at });
            let entity_id = ctx.db.presence().char_id().find(i.from_char).map_or(0, |q| q.entity_id);
            ctx.db.party_member().insert(PartyMember { char_id: i.from_char, party_id: party.id, name: i.from_name.clone(), entity_id, leader: true, joined_at: at });
            party.id
        }
    };
    // In a party now, so every other invitation to us goes.
    for other in ctx.db.party_invite().to_char().filter(p.char_id).collect::<Vec<_>>() {
        ctx.db.party_invite().scheduled_id().delete(other.scheduled_id);
    }
    ctx.db.party_member().insert(PartyMember { char_id: p.char_id, party_id, name: i.to_name.clone(), entity_id: p.entity_id, leader: false, joined_at: at });
    notice(ctx, party_id, &format!("{} joined the party", i.to_name));
    log::info!("char {} joined party {party_id}", p.char_id);
    Ok(())
}

/// Turn down an invitation to us, or take back one we made.
#[spacetimedb::reducer]
pub fn decline_party_invite(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let p = me(ctx)?;
    let i = ctx.db.party_invite().scheduled_id().find(id).filter(|i| i.to_char == p.char_id || i.from_char == p.char_id).ok_or("no such invitation")?;
    ctx.db.party_invite().scheduled_id().delete(i.scheduled_id);
    Ok(())
}

/// Take `char_id` out of its party, `kicked` by the leader or leaving. A leader's going passes the
/// lead to the oldest member left, and a party of one is no party.
fn remove(ctx: &ReducerContext, char_id: u64, kicked: bool) {
    let Some(m) = ctx.db.party_member().char_id().find(char_id) else { return };
    ctx.db.party_member().char_id().delete(char_id);
    let (told, went) = if kicked { ("you were removed from the party", "was removed from") } else { ("you left the party", "left") };
    crate::chat::notice(ctx, [char_id], told);
    let rest = members(ctx, m.party_id);
    if rest.len() < 2 {
        for r in rest {
            ctx.db.party_member().char_id().delete(r.char_id);
            crate::chat::notice(ctx, [r.char_id], "the party has broken up");
        }
        ctx.db.party().id().delete(m.party_id);
        return;
    }
    notice(ctx, m.party_id, &format!("{} {went} the party", m.name));
    if m.leader {
        promote(ctx, &rest[0]);
    }
}

fn promote(ctx: &ReducerContext, to: &PartyMember) {
    for o in members(ctx, to.party_id).into_iter().filter(|o| o.leader) {
        ctx.db.party_member().char_id().update(PartyMember { leader: false, ..o });
    }
    ctx.db.party_member().char_id().update(PartyMember { leader: true, ..to.clone() });
    notice(ctx, to.party_id, &format!("{} leads the party", to.name));
}

#[spacetimedb::reducer]
pub fn leave_party(ctx: &ReducerContext) -> Result<(), String> {
    let p = me(ctx)?;
    party_of(ctx, p.char_id).ok_or("you're not in a party")?;
    remove(ctx, p.char_id, false);
    Ok(())
}

/// The caller's own membership, as leader, and `char_id`'s in the same party.
fn lead_over(ctx: &ReducerContext, char_id: u64) -> Result<PartyMember, String> {
    let p = me(ctx)?;
    let mine = ctx.db.party_member().char_id().find(p.char_id).ok_or("you're not in a party")?;
    if !mine.leader {
        return Err("only the party's leader can do that".into());
    }
    let them = ctx.db.party_member().char_id().find(char_id).filter(|t| t.party_id == mine.party_id).ok_or("they're not in your party")?;
    if them.char_id == mine.char_id {
        return Err("that's you".into());
    }
    Ok(them)
}

/// The leader removes a member, in the world or not.
#[spacetimedb::reducer]
pub fn kick_from_party(ctx: &ReducerContext, char_id: u64) -> Result<(), String> {
    lead_over(ctx, char_id)?;
    remove(ctx, char_id, true);
    Ok(())
}

/// The leader hands the lead to another member.
#[spacetimedb::reducer]
pub fn make_party_leader(ctx: &ReducerContext, char_id: u64) -> Result<(), String> {
    let them = lead_over(ctx, char_id)?;
    promote(ctx, &them);
    Ok(())
}
