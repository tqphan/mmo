//! Chat (docs/design/social.md).

use super::party::{group, place};
use super::*;
use crate::chat::*;
use sim::chat::{CHAT, Channel};

/// Say `text` in `channel` and return the Local chat events it made: event rows only exist inside
/// their transaction.
fn say(t: &TestContext, player: u128, channel: Channel, text: &str) -> Result<Vec<LocalChat>, String> {
    call(t, player, |ctx| {
        chat(ctx, channel as u8, text.into())?;
        Ok(ctx.db.local_chat().iter().collect())
    })
}

/// `char_id`'s inbox as (channel, from, to, text), the oldest first.
fn inbox(t: &TestContext, char_id: u64) -> Vec<(Channel, String, String, String)> {
    read(t, |ctx| {
        let mut all: Vec<ChatMessage> = ctx.db.chat_message().char_id().filter(char_id).collect();
        all.sort_by_key(|m| m.id);
        all.into_iter().map(|m| (Channel::from_u8(m.channel).unwrap(), m.from_name, m.to_name, m.text)).collect()
    })
}

/// Messages from characters only, without the party's notices.
fn said(t: &TestContext, char_id: u64) -> Vec<(Channel, String, String, String)> {
    inbox(t, char_id).into_iter().filter(|m| !m.1.is_empty()).collect()
}

/// Wait out the rate limit.
fn breathe(t: &TestContext) {
    advance_ms(t, i64::from(CHAT.burst) * i64::from(CHAT.refill_ms));
}

#[test]
fn local_chat_is_an_event_where_it_was_said() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    place(&t, 1, 100.0, -70.0);
    let seen = say(&t, 1, Channel::Local, "  well met  ").unwrap();
    assert_eq!(seen.len(), 1);
    let e = &seen[0];
    assert_eq!((e.entity_id, e.char_id, e.name.as_str(), e.text.as_str()), (a.entity_id, a.char_id, "Mira", "well met"));
    assert_eq!((e.x, e.z), (100.0, -70.0));
    assert_eq!(e.cell, sim::cell::cell_of(sim::cell::OPEN_WORLD, 100.0, -70.0));
    // Nothing kept: an event is gone once its transaction is.
    assert_eq!(read(&t, |ctx| ctx.db.local_chat().count()), 0);
    assert!(read(&t, |ctx| ctx.db.chat_message().count()) == 0);
}

#[test]
fn messages_are_checked() {
    let t = world();
    arrive(&t, 1, "Mira");
    assert_eq!(say(&t, 1, Channel::Local, "   ").unwrap_err(), "say something");
    assert_eq!(say(&t, 1, Channel::Local, "a\nb").unwrap_err(), "messages are one line of text");
    assert_eq!(say(&t, 1, Channel::Local, &"a".repeat(CHAT.max_chars as usize + 1)).unwrap_err(), "that message is too long");
    assert_eq!(call(&t, 1, |ctx| chat(ctx, 7, "hi".into())).unwrap_err(), "no such channel");
    assert_eq!(call(&t, 1, |ctx| chat(ctx, Channel::Whisper as u8, "hi".into())).unwrap_err(), "no such channel");
    assert_eq!(say(&t, 1, Channel::Party, "hi").unwrap_err(), "you're not in a party");
    call(&t, 2, |ctx| {
        on_connect(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(say(&t, 2, Channel::Local, "hi").unwrap_err(), "not in the world");
}

#[test]
fn a_burst_then_one_per_refill_per_account() {
    let t = world();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    for i in 0..CHAT.burst {
        say(&t, 1, Channel::Local, &format!("{i}")).unwrap();
    }
    assert_eq!(say(&t, 1, Channel::Local, "more").unwrap_err(), "you're sending messages too fast");
    assert_eq!(call(&t, 1, |ctx| whisper(ctx, "Tobin".into(), "more".into())).unwrap_err(), "you're sending messages too fast");
    // Refusals spend nothing, and others aren't held up.
    say(&t, 2, Channel::Local, "hello").unwrap();
    advance_ms(&t, i64::from(CHAT.refill_ms));
    say(&t, 1, Channel::Local, "one more").unwrap();
    assert!(say(&t, 1, Channel::Local, "and another").is_err());
}

#[test]
fn party_chat_reaches_every_member_wherever_they_are() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    let c = arrive(&t, 3, "Wren");
    group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
    place(&t, 2, 5000.0, 5000.0);
    assert!(say(&t, 2, Channel::Party, "on my way").unwrap().is_empty(), "no Local event");
    let msg = (Channel::Party, "Tobin".to_string(), String::new(), "on my way".to_string());
    assert_eq!(said(&t, a.char_id), vec![msg.clone()]);
    assert_eq!(said(&t, b.char_id), vec![msg]);
    assert!(said(&t, c.char_id).is_empty());
    // Members out of the world get it too, for when they're back.
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    say(&t, 2, Channel::Party, "still there?").unwrap();
    assert_eq!(said(&t, a.char_id).len(), 2);
}

#[test]
fn whispers_go_to_one_character_with_a_copy_for_the_sender() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    let b = arrive(&t, 2, "Tobin");
    let c = arrive(&t, 3, "Wren");
    call(&t, 1, |ctx| whisper(ctx, "wren".into(), "psst".into())).unwrap();
    let msg = (Channel::Whisper, "Mira".to_string(), "Wren".to_string(), "psst".to_string());
    assert_eq!(said(&t, c.char_id), vec![msg.clone()]);
    assert_eq!(said(&t, a.char_id), vec![msg]);
    assert!(said(&t, b.char_id).is_empty());
    assert_eq!(call(&t, 1, |ctx| whisper(ctx, "Mira".into(), "me".into())).unwrap_err(), "you can't whisper to yourself");
    assert_eq!(call(&t, 1, |ctx| whisper(ctx, "Nobody".into(), "hi".into())).unwrap_err(), "no one called Nobody is in the world");
    call(&t, 3, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(call(&t, 1, |ctx| whisper(ctx, "Wren".into(), "hi".into())).unwrap_err(), "no one called Wren is in the world");
}

#[test]
fn an_inbox_keeps_the_latest_messages() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    let keep = CHAT.inbox as usize;
    for i in 0..keep + 3 {
        if (i as u32).is_multiple_of(CHAT.burst.unsigned_abs()) {
            breathe(&t);
        }
        call(&t, 2, |ctx| whisper(ctx, "Mira".into(), format!("{i}"))).unwrap();
    }
    let texts: Vec<String> = inbox(&t, a.char_id).into_iter().map(|m| m.3).collect();
    assert_eq!(texts.len(), keep);
    assert_eq!(texts.first().unwrap(), "3");
    assert_eq!(texts.last().unwrap(), &format!("{}", keep + 2));
}

#[test]
fn my_chat_shows_only_our_own() {
    let t = world();
    arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    arrive(&t, 3, "Wren");
    call(&t, 1, |ctx| whisper(ctx, "Tobin".into(), "psst".into())).unwrap();
    let texts = |player: u128| my_chat(&t.view_context(auth(player))).into_iter().map(|m| m.text).collect::<Vec<_>>();
    assert_eq!(texts(1), vec!["psst"]);
    assert_eq!(texts(2), vec!["psst"]);
    assert!(texts(3).is_empty());
}
