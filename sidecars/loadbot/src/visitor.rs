//! `loadbot visitor`: company for client playtests and captures (docs/design/client.md).
//!
//! One bot enters, teleports onto a circle near spawn and runs round it for `--secs`, sending a
//! move with its heading every 150 ms, about as often as a client running that circle sends turns. Every step it lists the characters in
//! its Near tier and prints each one the first time it sees it. With `--expect N` it fails unless
//! it saw at least N other characters; it always fails if it's corrected.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use spacetimedb_sdk::{DbContext, Table};

use crate::module_bindings::*;
use crate::{Args, Bot, Check, RUN_SPEED};

pub struct VisitorArgs {
    pub secs: u64,
    /// Fail unless at least this many other characters were seen.
    pub expect: usize,
}

impl Default for VisitorArgs {
    fn default() -> Self {
        Self { secs: 60, expect: 0 }
    }
}

/// North-east of spawn, in view of a client standing there and looking north.
const CENTER: [f32; 2] = [6.0, -14.0];
const RADIUS: f32 = 4.0;
const STEP: Duration = Duration::from_millis(150);

pub fn scenario(args: &Args, va: &VisitorArgs) -> Check {
    let mut bot = Bot::connect(args, "V")?;
    bot.subscribe_self()?;
    let me = bot.arrive()?;
    let start = [CENTER[0] + RADIUS, CENTER[1]];
    bot.teleport(start[0], start[1])?;
    bot.watch_near(start[0], start[1])?;

    let mut seen = BTreeSet::new();
    let began = Instant::now();
    // Anticlockwise seen from above: the angle about the centre, with z pointing south, falls.
    let mut angle = 0.0f32;
    while began.elapsed() < Duration::from_secs(va.secs) {
        std::thread::sleep(STEP);
        let step = RUN_SPEED * 0.9 * STEP.as_secs_f32();
        angle -= step / RADIUS;
        let (x, z) = (CENTER[0] + RADIUS * angle.cos(), CENTER[1] + RADIUS * angle.sin());
        // Facing along the circle: the tangent of a falling angle is (sin, -cos).
        let heading = (angle.sin(), -(angle.cos()));
        let yaw = sim::ship::yaw_to_u16((-heading.0).atan2(-heading.1));
        bot.seq += 1;
        bot.conn
            .reducers
            .move_event(bot.seq, 0, x, 0.0, z, yaw, (RUN_SPEED * 0.9 * 100.0) as i16, 1)
            .map_err(|e| e.to_string())?;
        bot.near.update(&bot.conn, x, z);

        for t in bot.conn.db.transform().iter().filter(|t| t.entity_id != me.entity_id) {
            let Some(e) = bot.conn.db.entity().id().find(&t.entity_id) else { continue };
            let name = bot.conn.db.character_public().char_id().find(&e.ref_id).map(|c| c.name);
            if let Some(name) = name.filter(|_| e.kind == 1)
                && seen.insert(name.clone())
            {
                println!("V: sees {name} (entity {}) at ({:.1}, {:.1})", t.entity_id, t.x, t.z);
            }
        }
        if let Some(p) = bot.presence()
            && p.corrections > 0
        {
            return Err(format!("corrected at ({:.1}, {:.1})", p.x, p.z));
        }
    }
    println!("V: saw {} characters: {}", seen.len(), seen.iter().cloned().collect::<Vec<_>>().join(", "));
    bot.conn.disconnect().map_err(|e| e.to_string())?;
    if seen.len() < va.expect {
        return Err(format!("expected to see {} characters, saw {}", va.expect, seen.len()));
    }
    Ok(())
}
