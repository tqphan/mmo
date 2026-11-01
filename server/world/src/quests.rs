//! Quests (docs/design/quests.md): givers placed in the world, quests taken from them, progress
//! counted as characters kill, gather, craft, harvest and sell, and turning in for experience and
//! items. Givers are public and static; each character's quest log is private and read through
//! `my_quests`.

use sim::content::QuestAct;
use sim::quest::{QUEST, QuestData, Refusal, giver, quest};
use spacetimedb::{ReducerContext, SpacetimeType, Table, Timestamp, ViewContext};

use crate::combat::fighter;
use crate::session::{Presence, character_private__view, presence, require_admin};
use crate::time::now;

/// Static and public: a quest giver standing in the world. Written only when admins place or
/// remove givers.
#[spacetimedb::table(accessor = quest_giver, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct QuestGiver {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    /// The 64 m cell it stands in, for the Static tier's subscriptions.
    #[index(btree)]
    pub cell: u64,
    /// The giver's wire (`content/quests.toml`).
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Facing, as a u16 fraction of a turn.
    pub yaw: u16,
}

/// Where an admin puts a giver.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct GiverPlacement {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: u16,
}

/// Private: a quest a character has taken, open until it's turned in, then kept as a record that
/// it was. Owners read theirs through `my_quests`.
#[spacetimedb::table(accessor = quest_log)]
#[derive(Clone, Debug, PartialEq)]
pub struct QuestLog {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub char_id: u64,
    /// The quest's wire.
    pub quest: u16,
    /// Each objective's count, in the quest's order.
    pub done: Vec<u32>,
    pub taken_at: Timestamp,
    /// When it was turned in; `None` while it's open.
    pub turned_in_at: Option<Timestamp>,
}

/// Every quest the caller's characters have open or have turned in.
#[spacetimedb::view(accessor = my_quests, public)]
pub(crate) fn my_quests(ctx: &ViewContext) -> Vec<QuestLog> {
    ctx.db.character_private().owner().filter(ctx.sender()).flat_map(|c| ctx.db.quest_log().char_id().filter(c.char_id)).collect()
}

fn me(ctx: &ReducerContext) -> Result<Presence, String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    Ok(p)
}

fn quest_of(wire: u16) -> Result<&'static QuestData, String> {
    quest(wire).ok_or_else(|| "no such quest".into())
}

fn name_of(kind: u8) -> &'static str {
    giver(kind).map_or("someone", |g| g.name)
}

/// The giver `giver_id`, if `p` stands close enough to talk to it.
fn talk_to(ctx: &ReducerContext, p: &Presence, giver_id: u64) -> Result<QuestGiver, String> {
    let g = ctx.db.quest_giver().id().find(giver_id).ok_or("nobody there")?;
    let at = crate::ship::world_position(ctx, p);
    let (dx, dz) = (at[0] - g.x, at[2] - g.z);
    if dx * dx + dz * dz > QUEST.reach_m * QUEST.reach_m || (at[1] - g.y).abs() > QUEST.reach_m {
        return Err(format!("too far from {}", name_of(g.kind)));
    }
    Ok(g)
}

fn entries(ctx: &ReducerContext, char_id: u64) -> Vec<QuestLog> {
    ctx.db.quest_log().char_id().filter(char_id).collect()
}

/// `char_id`'s open entry for quest `wire`.
fn open(ctx: &ReducerContext, char_id: u64, wire: u16) -> Option<QuestLog> {
    entries(ctx, char_id).into_iter().find(|e| e.quest == wire && e.turned_in_at.is_none())
}

fn level_of(ctx: &ReducerContext, char_id: u64) -> u8 {
    ctx.db.fighter().char_id().find(char_id).map_or(1, |f| f.level)
}

/// Take a quest from a giver in reach.
#[spacetimedb::reducer]
pub fn accept_quest(ctx: &ReducerContext, giver_id: u64, quest: u16) -> Result<(), String> {
    let p = me(ctx)?;
    let q = quest_of(quest)?;
    let g = talk_to(ctx, &p, giver_id)?;
    if g.kind != q.giver {
        return Err(format!("{} doesn't give that quest", name_of(g.kind)));
    }
    let log = entries(ctx, p.char_id);
    if log.iter().any(|e| e.quest == quest && e.turned_in_at.is_some()) {
        return Err("you've done that quest".into());
    }
    if log.iter().any(|e| e.quest == quest) {
        return Err("you're already on that quest".into());
    }
    let done: Vec<u16> = log.iter().filter(|e| e.turned_in_at.is_some()).map(|e| e.quest).collect();
    match sim::quest::may_take(q, level_of(ctx, p.char_id), &done) {
        Err(Refusal::Level(l)) => return Err(format!("you need to be level {l}")),
        Err(Refusal::After(a)) => return Err(format!("finish {} first", quest_of(a).map_or("another quest", |b| b.name))),
        Ok(()) => {}
    }
    if log.iter().filter(|e| e.turned_in_at.is_none()).count() >= sim::quest::log_size() {
        return Err("your quest log is full".into());
    }
    ctx.db.quest_log().insert(QuestLog { id: 0, char_id: p.char_id, quest, done: vec![0; q.objectives.len()], taken_at: now(ctx), turned_in_at: None });
    log::info!("char {} took quest {}", p.char_id, q.id);
    Ok(())
}

/// Turn a quest in to its giver (or whoever it's to) in reach, once it's far enough along: its
/// share of the experience, and the items in full.
#[spacetimedb::reducer]
pub fn turn_in_quest(ctx: &ReducerContext, giver_id: u64, quest: u16) -> Result<(), String> {
    let p = me(ctx)?;
    let q = quest_of(quest)?;
    let e = open(ctx, p.char_id, quest).ok_or("you're not on that quest")?;
    let g = talk_to(ctx, &p, giver_id)?;
    if g.kind != q.to {
        return Err(format!("turn that quest in to {}", name_of(q.to)));
    }
    let score = sim::quest::score(q.objectives, &e.done);
    if !sim::quest::ready(score) {
        return Err("that quest isn't far enough along".into());
    }
    // A reward that doesn't fit fails the whole call, which rolls back the others.
    for r in q.rewards {
        crate::items::grant(ctx, p.char_id, r.item, r.count).map_err(|_| "your bag is too full for the reward".to_string())?;
    }
    crate::combat::grant_xp(ctx, &p, sim::quest::xp_for(q.xp, score));
    ctx.db.quest_log().id().update(QuestLog { turned_in_at: Some(now(ctx)), ..e });
    log::info!("char {} turned in quest {} at {score}%", p.char_id, q.id);
    Ok(())
}

/// Drop an open quest and what's been done towards it. It may be taken again.
#[spacetimedb::reducer]
pub fn abandon_quest(ctx: &ReducerContext, quest: u16) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    let e = open(ctx, p.char_id, quest).ok_or("you're not on that quest")?;
    ctx.db.quest_log().id().delete(e.id);
    Ok(())
}

/// `char_id` did `act` on `target` `count` times (at post kind `post`, for a sale): every open
/// quest with a matching objective counts it, up to the objective's cap.
pub fn count(ctx: &ReducerContext, char_id: u64, act: QuestAct, target: u16, post: Option<u8>, count: u32) {
    for e in entries(ctx, char_id) {
        if e.turned_in_at.is_some() {
            continue;
        }
        let Some(q) = quest(e.quest) else { continue };
        let mut done = e.done.clone();
        done.resize(q.objectives.len(), 0);
        for (k, o) in q.objectives.iter().enumerate() {
            if sim::quest::counts(o, act, target, post) {
                done[k] = sim::quest::tally(done[k], o.count, count);
            }
        }
        if done != e.done {
            ctx.db.quest_log().id().update(QuestLog { done, ..e });
        }
    }
}

/// A kill of NPC `victim` of kind `kind` earned by `earner`: it counts for everyone who shares the
/// kill's experience, the earner and its party members nearby (docs/design/social.md).
pub fn killed(ctx: &ReducerContext, earner: u64, victim: u64, kind: u8) {
    let Some(p) = ctx.db.presence().entity_id().find(earner) else { return };
    for s in crate::party::sharers(ctx, &p, crate::combat::world_at(ctx, victim)) {
        count(ctx, s.char_id, QuestAct::Kill, u16::from(kind), None, 1);
    }
}

/// Add givers.
#[spacetimedb::reducer]
pub fn admin_put_givers(ctx: &ReducerContext, givers: Vec<GiverPlacement>) -> Result<(), String> {
    require_admin(ctx)?;
    if let Some(bad) = givers.iter().find(|g| giver(g.kind).is_none() || ![g.x, g.y, g.z].iter().all(|v| v.is_finite())) {
        return Err(format!("bad giver {bad:?}: unknown kind or a non-finite position"));
    }
    let half = sim::cell::WORLD_HALF_M;
    if let Some(bad) = givers.iter().find(|g| g.x.abs() >= half || g.z.abs() >= half) {
        return Err(format!("bad giver {bad:?}: off the map"));
    }
    for g in &givers {
        let cell = sim::cell::cell_of(sim::cell::OPEN_WORLD, g.x, g.z);
        ctx.db.quest_giver().insert(QuestGiver { id: 0, cell, kind: g.kind, x: g.x, y: g.y, z: g.z, yaw: g.yaw });
    }
    log::info!("{} quest givers placed", givers.len());
    Ok(())
}

/// Remove givers by id. Unknown ids are ignored.
#[spacetimedb::reducer]
pub fn admin_remove_givers(ctx: &ReducerContext, ids: Vec<u64>) -> Result<(), String> {
    require_admin(ctx)?;
    for id in ids {
        ctx.db.quest_giver().id().delete(id);
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn admin_clear_givers(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    let ids: Vec<u64> = ctx.db.quest_giver().iter().map(|g| g.id).collect();
    for &id in &ids {
        ctx.db.quest_giver().id().delete(id);
    }
    log::info!("{} quest givers cleared", ids.len());
    Ok(())
}

/// Dev builds only: drop every quest of the caller's character, open or turned in, so a test can
/// run the chain again.
#[cfg(feature = "dev")]
#[spacetimedb::reducer]
pub fn dev_reset_quests(ctx: &ReducerContext) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    for e in entries(ctx, p.char_id) {
        ctx.db.quest_log().id().delete(e.id);
    }
    Ok(())
}
