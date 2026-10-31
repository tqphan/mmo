//! Chat (docs/design/social.md). Local chat is public speech, an event broadcast to the speaker's
//! cell that clients in range hear through their Near tier. Party chat and whispers are private:
//! each goes into the inbox of everyone it's for, read through `my_chat`, which keeps the latest
//! `inbox` messages per character. Every message counts against its account's rate limit.

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext};
use sim::chat::Channel;

use crate::session::{character_private, character_public, presence};
use crate::time::now;

/// Near tier, public, an event: something said aloud, broadcast to the cell the speaker stands in.
/// Clients hear it within `local_m` of where it was said.
#[spacetimedb::table(accessor = local_chat, public, event)]
#[derive(Debug)]
pub struct LocalChat {
    pub cell: u64,
    pub entity_id: u64,
    pub char_id: u64,
    pub name: String,
    /// Where it was said, in the world.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub text: String,
}

/// Private: a party message or whisper in one character's inbox, read through `my_chat`.
#[spacetimedb::table(accessor = chat_message)]
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    /// Whose inbox it's in.
    #[index(btree)]
    pub char_id: u64,
    /// A `sim::chat::Channel`.
    pub channel: u8,
    /// Who sent it, or 0 for the party's own notices (someone joined, left, leads).
    pub from_char: u64,
    pub from_name: String,
    /// A whisper's recipient, so the sender's copy can say who it went to; otherwise empty.
    pub to_name: String,
    pub text: String,
    pub sent_at: Timestamp,
}

/// Private: an account's rate limit, as the time its bucket is full again (`sim::chat::send`).
#[spacetimedb::table(accessor = chat_rate)]
pub struct ChatRate {
    #[primary_key]
    pub identity: Identity,
    pub due: Timestamp,
}

/// The inboxes of the caller's characters.
#[spacetimedb::view(accessor = my_chat, public)]
pub(crate) fn my_chat(ctx: &ViewContext) -> Vec<ChatMessage> {
    ctx.db.chat_message().owner().filter(ctx.sender()).collect()
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

/// Check `text` and the caller's rate limit, spending from it.
fn spend(ctx: &ReducerContext, text: &str) -> Result<String, String> {
    let text = sim::chat::clean(text).map_err(|r| r.message().to_string())?.to_string();
    let at = micros(now(ctx));
    let row = ctx.db.chat_rate().identity().find(ctx.sender());
    let due = sim::chat::send(row.as_ref().map_or(0, |r| micros(r.due)), at).map_err(|r| r.message().to_string())?;
    let due = Timestamp::from_micros_since_unix_epoch(due);
    if row.is_some() {
        ctx.db.chat_rate().identity().update(ChatRate { identity: ctx.sender(), due });
    } else {
        ctx.db.chat_rate().insert(ChatRate { identity: ctx.sender(), due });
    }
    Ok(text)
}

/// Put a message into `char_id`'s inbox, and let the oldest go past `inbox`.
fn deliver(ctx: &ReducerContext, char_id: u64, channel: Channel, from: (u64, &str), to_name: &str, text: &str) {
    let Some(c) = ctx.db.character_private().char_id().find(char_id) else { return };
    ctx.db.chat_message().insert(ChatMessage {
        id: 0,
        owner: c.owner,
        char_id,
        channel: channel as u8,
        from_char: from.0,
        from_name: from.1.into(),
        to_name: to_name.into(),
        text: text.into(),
        sent_at: now(ctx),
    });
    let keep = sim::chat::CHAT.inbox.unsigned_abs() as usize;
    let mut ids: Vec<u64> = ctx.db.chat_message().char_id().filter(char_id).map(|m| m.id).collect();
    if ids.len() > keep {
        ids.sort_unstable();
        for id in &ids[..ids.len() - keep] {
            ctx.db.chat_message().id().delete(id);
        }
    }
}

/// A party notice, from nobody, to each of `to`.
pub fn notice(ctx: &ReducerContext, to: impl IntoIterator<Item = u64>, text: &str) {
    for char_id in to {
        deliver(ctx, char_id, Channel::Party, (0, ""), "", text);
    }
}

fn name_of(ctx: &ReducerContext, char_id: u64) -> String {
    ctx.db.character_public().char_id().find(char_id).map(|c| c.name).unwrap_or_default()
}

/// Say `text` in Local chat (`channel` 0) or to our party (1).
#[spacetimedb::reducer]
pub fn chat(ctx: &ReducerContext, channel: u8, text: String) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let channel = Channel::from_u8(channel).filter(|c| *c != Channel::Whisper).ok_or("no such channel")?;
    let party = crate::party::party_of(ctx, p.char_id);
    if channel == Channel::Party && party.is_none() {
        return Err("you're not in a party".into());
    }
    let text = spend(ctx, &text)?;
    let name = name_of(ctx, p.char_id);
    match (channel, party) {
        (Channel::Party, Some(party)) => {
            for m in crate::party::members(ctx, party) {
                deliver(ctx, m.char_id, Channel::Party, (p.char_id, &name), "", &text);
            }
        }
        _ => {
            let [x, y, z] = crate::ship::world_position(ctx, &p);
            let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z);
            ctx.db.local_chat().insert(LocalChat { cell, entity_id: p.entity_id, char_id: p.char_id, name, x, y, z, text });
        }
    }
    Ok(())
}

/// Whisper `text` to the character called `to`, who must be in the world. Both get a copy.
#[spacetimedb::reducer]
pub fn whisper(ctx: &ReducerContext, to: String, text: String) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let target = crate::session::character_named(ctx, &to).ok_or_else(|| format!("no one called {to} is in the world"))?;
    if ctx.db.presence().char_id().find(target.char_id).is_none() {
        return Err(format!("no one called {} is in the world", target.name));
    }
    if target.char_id == p.char_id {
        return Err("you can't whisper to yourself".into());
    }
    let text = spend(ctx, &text)?;
    let name = name_of(ctx, p.char_id);
    deliver(ctx, target.char_id, Channel::Whisper, (p.char_id, &name), &target.name, &text);
    deliver(ctx, p.char_id, Channel::Whisper, (p.char_id, &name), &target.name, &text);
    Ok(())
}
