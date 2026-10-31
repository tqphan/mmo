//! Chat (docs/design/social.md): the channels, what a message may hold, how far Local carries, and
//! the rate limit. The module delivers messages and keeps the inboxes.

pub use crate::content::{CHAT, ChatRules};

const MICROS_PER_MS: i64 = 1_000;

/// Where a message goes. The discriminant is the wire value in the message rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Channel {
    /// Everyone near the speaker.
    Local = 0,
    /// The speaker's party.
    Party = 1,
    /// One character, by name.
    Whisper = 2,
}

impl Channel {
    pub fn from_u8(v: u8) -> Option<Self> {
        [Self::Local, Self::Party, Self::Whisper].get(usize::from(v)).copied()
    }

    /// What players see.
    pub fn name(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Party => "Party",
            Self::Whisper => "Whisper",
        }
    }
}

/// Why a message is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Empty,
    TooLong,
    /// A control character, such as a newline.
    Control,
    TooFast,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "say something",
            Self::TooLong => "that message is too long",
            Self::Control => "messages are one line of text",
            Self::TooFast => "you're sending messages too fast",
        }
    }
}

/// `text` as it will be sent: trimmed, and refused if it's empty, too long or has control
/// characters in it.
pub fn clean(text: &str) -> Result<&str, Refusal> {
    let t = text.trim();
    if t.is_empty() {
        return Err(Refusal::Empty);
    }
    if t.chars().any(char::is_control) {
        return Err(Refusal::Control);
    }
    if t.chars().count() > CHAT.max_chars.unsigned_abs() as usize {
        return Err(Refusal::TooLong);
    }
    Ok(t)
}

/// The rate limit, a token bucket kept as one timestamp, `due`: the time at which the bucket will
/// be full again (or earlier: it's full). A message at `now` is allowed if the bucket isn't empty,
/// that is if `due` is less than a burst ahead of `now`; it returns the new `due`. µs since the
/// Unix epoch.
pub fn send(due: i64, now: i64) -> Result<i64, Refusal> {
    let refill = i64::from(CHAT.refill_ms) * MICROS_PER_MS;
    let burst = i64::from(CHAT.burst) * refill;
    let due = due.max(now);
    if due + refill > now + burst {
        return Err(Refusal::TooFast);
    }
    Ok(due + refill)
}

/// Whether someone at `at` hears Local chat said at `from`: horizontally within reach.
pub fn hears(from: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - from[0], at[2] - from[2]);
    dx * dx + dz * dz <= CHAT.local_m * CHAT.local_m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_round_trip() {
        for (wire, c) in [(0, Channel::Local), (1, Channel::Party), (2, Channel::Whisper)] {
            assert_eq!(Channel::from_u8(wire), Some(c));
            assert_eq!(c as u8, wire);
        }
        assert_eq!(Channel::from_u8(3), None);
        assert_eq!(Channel::Whisper.name(), "Whisper");
    }

    #[test]
    fn messages_are_trimmed_and_checked() {
        assert_eq!(clean("  hello there \t"), Ok("hello there"));
        assert_eq!(clean("   "), Err(Refusal::Empty));
        assert_eq!(clean("two\nlines"), Err(Refusal::Control));
        let most = "é".repeat(200);
        assert_eq!(clean(&most), Ok(most.as_str()));
        assert_eq!(clean(&"a".repeat(201)), Err(Refusal::TooLong));
    }

    #[test]
    fn a_burst_then_one_per_refill() {
        let t0 = 1_000_000_000;
        let refill = 2_000_000;
        let mut due = 0;
        for _ in 0..5 {
            due = send(due, t0).unwrap();
        }
        assert_eq!(due, t0 + 5 * refill);
        assert_eq!(send(due, t0), Err(Refusal::TooFast));
        assert_eq!(send(due, t0 + refill - 1), Err(Refusal::TooFast));
        due = send(due, t0 + refill).unwrap();
        assert_eq!(send(due, t0 + refill), Err(Refusal::TooFast));
        // Quiet for long enough, the bucket is full again.
        let later = t0 + 60 * refill;
        assert_eq!(send(due, later), Ok(later + refill));
    }

    #[test]
    fn local_carries_horizontally() {
        let from = [10.0, 0.0, 10.0];
        assert!(hears(from, [50.0, 0.0, 10.0]));
        assert!(!hears(from, [39.0, 0.0, 39.0]));
        assert!(hears(from, [10.0, 300.0, 10.0]));
    }
}
