//! The `party` scenario (docs/design/social.md): bots A and B on the trade island and C 300 m off.
//! A invites B, who declines, then accepts a second invitation; an invitation lapses by the dev
//! clock; B, not the leader, can't invite, and A brings in C. A can't harm B. Local chat reaches B
//! and not C; party chat reaches all three; a whisper only its two ends. A runs into the rate
//! limit. A hands the lead to B, B kicks C, and A's leaving breaks the party up.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sim::chat::{CHAT, Channel};
use sim::combat::{level_max, skill_by_id};
use sim::party::PARTY;

use crate::gather::{advance, call, subscribe};
use crate::land::refused;
use crate::module_bindings::*;
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, EventTable, Table};

/// On the trade island south-east of spawn, where the combat and crime scenarios fight.
const SCENE: (f32, f32) = (200.0, -150.0);
/// How far off C waits: out of earshot, and out of A's cells.
const AWAY_M: f32 = 300.0;

fn name_of(bot: &Bot, char_id: u64) -> String {
    bot.conn.db.character_public().char_id().find(&char_id).map(|c| c.name).unwrap_or_default()
}

fn invite(bot: &Bot, name: &str) -> Check {
    call(bot, |tx| bot.conn.reducers.invite_to_party_then(name.into(), tx))
}

/// The invitation `bot` sees to it from `from`.
fn invitation(bot: &Bot, from: u64) -> Option<PartyInvite> {
    let me = bot.presence()?.char_id;
    bot.conn.db.my_party_invites().iter().find(|i| i.from_char == from && i.to_char == me)
}

/// The party `bot` sees as (name, leader), sorted by name.
fn roster(bot: &Bot) -> Vec<(String, bool)> {
    let mut all: Vec<(String, bool)> = bot.conn.db.my_party().iter().map(|m| (m.name, m.leader)).collect();
    all.sort();
    all.dedup();
    all
}

/// Whether `bot`'s inbox has `text` on `channel`.
fn got(bot: &Bot, channel: Channel, text: &str) -> bool {
    bot.conn.db.my_chat().iter().any(|m| m.channel == channel as u8 && m.text == text)
}

fn say(bot: &Bot, channel: Channel, text: &str) -> Check {
    call(bot, |tx| bot.conn.reducers.chat_then(channel as u8, text.into(), tx))
}

/// Local chat events `bot` hears, through its cells, as it would through its Near tier.
fn listen(bot: &Bot, at: (f32, f32)) -> Result<Arc<Mutex<Vec<LocalChat>>>, String> {
    for c in sim::cell::window_cells(sim::cell::cell_of(sim::cell::OPEN_WORLD, at.0, at.1), 1) {
        subscribe(bot, &[&format!("SELECT * FROM local_chat WHERE cell = {c}")])?;
    }
    let heard = Arc::new(Mutex::new(Vec::new()));
    let log = heard.clone();
    bot.conn.db.local_chat().on_insert(move |_, e| log.lock().unwrap().push(e.clone()));
    Ok(heard)
}

pub fn scenario(args: &Args) -> Check {
    let (x, z) = SCENE;
    let mut bots = Vec::new();
    for label in ["A", "B", "C"] {
        let bot = Bot::connect(args, label)?;
        bot.subscribe_self()?;
        subscribe(&bot, &["SELECT * FROM my_party", "SELECT * FROM my_party_invites", "SELECT * FROM my_chat", "SELECT * FROM game_clock"])?;
        bot.arrive()?;
        call(&bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
        bots.push(bot);
    }
    let [a, b, c] = <[Bot; 3]>::try_from(bots).map_err(|_| "three bots")?;
    let (pa, pb, pc) = (a.presence().ok_or("A left")?, b.presence().ok_or("B left")?, c.presence().ok_or("C left")?);
    let (na, nb, nc) = (name_of(&a, pa.char_id), name_of(&a, pb.char_id), name_of(&a, pc.char_id));
    a.teleport(x, z)?;
    b.teleport(x, z - 3.0)?;
    c.teleport(x + AWAY_M, z)?;
    let heard_b = listen(&b, (x, z))?;
    let heard_c = listen(&c, (x + AWAY_M, z))?;

    // Invitations: declined, lapsed, then taken.
    invite(&a, &nb)?;
    wait_for("B's invitation", || invitation(&b, pa.char_id).is_some())?;
    let id = invitation(&b, pa.char_id).ok_or("no invitation")?.scheduled_id;
    call(&b, |tx| b.conn.reducers.decline_party_invite_then(id, tx))?;
    wait_for("A to see it declined", || a.conn.db.my_party_invites().count() == 0)?;
    invite(&a, &nb)?;
    wait_for("B's second invitation", || invitation(&b, pa.char_id).is_some())?;
    let id = invitation(&b, pa.char_id).ok_or("no invitation")?.scheduled_id;
    advance(&a, i64::from(PARTY.invite_s) * 1_000_000)?;
    refused(&b, "a lapsed invitation", "lapsed", |tx| b.conn.reducers.accept_party_invite_then(id, tx))?;
    invite(&a, &nb)?;
    wait_for("B's third invitation", || invitation(&b, pa.char_id).is_some_and(|i| i.scheduled_id != id))?;
    let id = invitation(&b, pa.char_id).ok_or("no invitation")?.scheduled_id;
    call(&b, |tx| b.conn.reducers.accept_party_invite_then(id, tx))?;
    let two = vec![(na.clone(), true), (nb.clone(), false)];
    let sorted = |mut v: Vec<(String, bool)>| {
        v.sort();
        v
    };
    wait_for("a party of two", || roster(&a) == sorted(two.clone()) && roster(&b) == sorted(two.clone()))?;
    println!("B declined, let one lapse, then took A's invitation: a party of two led by A");

    refused(&b, "B's invitation", "only the party's leader", |tx| b.conn.reducers.invite_to_party_then(nc.clone(), tx))?;
    invite(&a, &nc)?;
    wait_for("C's invitation", || invitation(&c, pa.char_id).is_some())?;
    let id = invitation(&c, pa.char_id).ok_or("no invitation")?.scheduled_id;
    call(&c, |tx| c.conn.reducers.accept_party_invite_then(id, tx))?;
    wait_for("a party of three", || [&a, &b, &c].iter().all(|bot| roster(bot).len() == 3))?;
    println!("A brought in C, {AWAY_M} m away");

    // No harming party members.
    call(&a, |tx| a.conn.reducers.dev_set_level_then(level_max(), tx))?;
    call(&a, |tx| a.conn.reducers.pick_skillset_then(1, tx))?;
    let cut = skill_by_id("threefold_cut").ok_or("no skill")?.wire;
    refused(&a, "a blow at a party member", "party member", |tx| a.conn.reducers.use_skill_then(cut, pb.entity_id, tx))?;

    // Local chat: B, beside A, hears it; C, far off, doesn't.
    say(&a, Channel::Local, "well met")?;
    wait_for("B to hear A", || heard_b.lock().unwrap().iter().any(|e| e.text == "well met" && e.char_id == pa.char_id))?;
    let e = heard_b.lock().unwrap().iter().find(|e| e.text == "well met").cloned().ok_or("lost it")?;
    if !sim::chat::hears([e.x, e.y, e.z], [x, 0.0, z - 3.0]) {
        return Err("B heard Local chat from out of range".into());
    }
    std::thread::sleep(Duration::from_millis(500));
    if heard_c.lock().unwrap().iter().any(|e| e.text == "well met") {
        return Err("C heard Local chat 300 m away".into());
    }
    println!("A said \"well met\" aloud: B heard it and C didn't");

    // Party chat reaches everyone in it; a whisper only its two ends.
    say(&c, Channel::Party, "on my way")?;
    wait_for("party chat", || [&a, &b, &c].iter().all(|bot| got(bot, Channel::Party, "on my way")))?;
    call(&a, |tx| a.conn.reducers.whisper_then(nc.clone(), "psst".into(), tx))?;
    wait_for("C's whisper", || got(&c, Channel::Whisper, "psst") && got(&a, Channel::Whisper, "psst"))?;
    std::thread::sleep(Duration::from_millis(300));
    if got(&b, Channel::Whisper, "psst") {
        return Err("B saw a whisper to C".into());
    }
    println!("C's party message reached all three; A's whisper reached only C");

    // The rate limit: A has a little left of its burst, then is held up.
    let mut sent = 0;
    loop {
        match call(&a, |tx| a.conn.reducers.chat_then(Channel::Local as u8, format!("line {sent}"), tx)) {
            Ok(()) => sent += 1,
            Err(e) if e.contains("too fast") => break,
            Err(e) => return Err(format!("unexpected refusal {e:?}")),
        }
        if sent > CHAT.burst {
            return Err("no rate limit".into());
        }
    }
    println!("A was held up after {sent} more lines");

    // A new leader kicks; the last but one leaving breaks the party up.
    call(&a, |tx| a.conn.reducers.make_party_leader_then(pb.char_id, tx))?;
    wait_for("B leads", || roster(&c).iter().any(|m| m.0 == nb && m.1))?;
    call(&b, |tx| b.conn.reducers.kick_from_party_then(pc.char_id, tx))?;
    wait_for("C out", || c.conn.db.my_party().count() == 0 && got(&c, Channel::Party, "you were removed from the party"))?;
    call(&a, |tx| a.conn.reducers.leave_party_then(tx))?;
    wait_for("no party", || [&a, &b].iter().all(|bot| bot.conn.db.my_party().count() == 0))?;
    if !got(&b, Channel::Party, "the party has broken up") {
        return Err("B wasn't told the party broke up".into());
    }
    println!("A made B leader, B kicked C, and A's leaving broke the party up");

    for bot in [a, b, c] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}
