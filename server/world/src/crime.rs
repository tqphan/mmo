//! Crime (docs/design/crime.md): harm to characters marks the attacker and leaves bloodstains,
//! killing leaves a blood pool and theft leaves footprints. Evidence is public but says only what
//! and where; who did it is private, and comes out only as crime points when someone reports it.
//! Evidence fades by a one-shot timer, and nothing else is timed.

use spacetimedb::{Identity, ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp, ViewContext};
use sim::crime::{CRIME, Evidence as Kind};

use crate::combat::{Vitals, vitals};
use crate::session::{character_private, presence};
use crate::time::now;

/// Near tier, public: a piece of evidence, where it lies in the world and when it was left.
#[spacetimedb::table(accessor = evidence, public)]
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub cell: u64,
    /// A `sim::crime::Evidence`.
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub made_at: Timestamp,
}

/// Private, with no view: who left a piece of evidence, against whom, and what reporting it adds.
#[spacetimedb::table(accessor = evidence_secret)]
#[derive(Clone, Debug, PartialEq)]
pub struct EvidenceSecret {
    #[primary_key]
    pub evidence_id: u64,
    #[index(btree)]
    pub offender: u64,
    pub victim: u64,
    pub points: u32,
}

/// Private: a piece of evidence fades at its fading time. One per piece.
#[spacetimedb::table(accessor = evidence_fade, scheduled(fade_evidence))]
pub struct EvidenceFade {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    #[unique]
    pub evidence_id: u64,
}

/// Private: a character's crimes as reported. No row is a clean record. The owner reads it through
/// `my_crime`.
#[spacetimedb::table(accessor = crime_record)]
#[derive(Clone, Debug, PartialEq)]
pub struct CrimeRecord {
    #[primary_key]
    pub char_id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub crime_points: u32,
    /// Never falls (the original's infamy).
    pub notoriety: u32,
    /// How many reports there have been.
    pub reports: u32,
}

/// The crime records of the caller's characters.
#[spacetimedb::view(accessor = my_crime, public)]
pub(crate) fn my_crime(ctx: &ViewContext) -> Vec<CrimeRecord> {
    ctx.db.crime_record().owner().filter(ctx.sender()).collect()
}

fn micros(t: Timestamp) -> i64 {
    t.to_micros_since_unix_epoch()
}

/// Whether `v` carries the attacker's mark at `at`, which makes it fair game.
pub fn marked(v: &Vitals, at: Timestamp) -> bool {
    v.statuses.iter().any(|s| s.status == CRIME.mark && s.until > at)
}

/// Whether `char_id` is Sought, for its vitals on entering the world.
pub fn is_sought(ctx: &ReducerContext, char_id: u64) -> bool {
    ctx.db.crime_record().char_id().find(char_id).is_some_and(|r| sim::crime::sought(r.crime_points))
}

/// Called before harm from `source` lands on `victim` (a skill's damage, damage over time or a
/// debuff), with `kills` when it takes the last HP. Harm by one character to another that isn't
/// marked is assault: the attacker is marked, and the first harm of a bout leaves a Bloodstain.
/// A kill leaves a Blood Pool.
pub fn harmed(ctx: &ReducerContext, source: u64, victim: &Vitals, kills: bool) {
    if source == victim.entity_id {
        return;
    }
    let Some(attacker) = ctx.db.presence().entity_id().find(source) else { return };
    let Some(target) = ctx.db.presence().entity_id().find(victim.entity_id) else { return };
    let at = now(ctx);
    if marked(victim, at) {
        return;
    }
    let bout = ctx.db.vitals().entity_id().find(source).is_some_and(|v| marked(&v, at));
    crate::combat::mark(ctx, source, CRIME.mark, Timestamp::from_micros_since_unix_epoch(sim::crime::mark_until(micros(at))));
    let spot = crate::ship::world_position(ctx, &target);
    if !bout {
        leave(ctx, Kind::Bloodstain, spot, attacker.char_id, target.char_id);
    }
    if kills {
        leave(ctx, Kind::BloodPool, spot, attacker.char_id, target.char_id);
    }
}

/// Called when `thief` takes what belongs to another account's character `victim` at `at` (world):
/// it leaves Footprints.
pub fn theft(ctx: &ReducerContext, thief: u64, victim: u64, at: [f32; 3]) {
    leave(ctx, Kind::Footprints, at, thief, victim);
}

/// Leave evidence of `kind` at `at`, and its secret, and schedule its fading.
fn leave(ctx: &ReducerContext, kind: Kind, at: [f32; 3], offender: u64, victim: u64) {
    let e = ctx.db.evidence().insert(Evidence {
        id: 0,
        cell: sim::cell::cell_of(sim::cell::OPEN_WORLD, at[0], at[2]),
        kind: kind as u8,
        x: at[0],
        y: at[1],
        z: at[2],
        made_at: now(ctx),
    });
    ctx.db.evidence_secret().insert(EvidenceSecret { evidence_id: e.id, offender, victim, points: kind.points() });
    schedule(ctx, &e);
    log::info!("crime: char {offender} left {} {} against char {victim} at ({}, {})", kind.name(), e.id, at[0], at[2]);
}

/// Schedule `e`'s fading for its fading time, replacing any earlier schedule. Game time may run
/// ahead of the host's (the dev clock), so the delay is measured in game time.
fn schedule(ctx: &ReducerContext, e: &Evidence) {
    ctx.db.evidence_fade().evidence_id().delete(e.id);
    let wait = (sim::crime::fades_at(micros(e.made_at)) - micros(now(ctx))).max(0);
    let at = ctx.timestamp + TimeDuration::from_micros(wait);
    ctx.db.evidence_fade().insert(EvidenceFade { scheduled_id: 0, scheduled_at: at.into(), evidence_id: e.id });
}

/// Called when the dev clock moves: every fading is rescheduled against the new game time.
#[cfg(feature = "dev")]
pub fn clock_moved(ctx: &ReducerContext) {
    for e in ctx.db.evidence().iter().collect::<Vec<_>>() {
        schedule(ctx, &e);
    }
}

fn remove(ctx: &ReducerContext, id: u64) {
    ctx.db.evidence().id().delete(id);
    ctx.db.evidence_secret().evidence_id().delete(id);
    ctx.db.evidence_fade().evidence_id().delete(id);
}

/// A piece of evidence's fading time has come by the host's clock. If game time says it hasn't,
/// it's rescheduled; otherwise it goes, unreported.
#[spacetimedb::reducer]
pub fn fade_evidence(ctx: &ReducerContext, job: EvidenceFade) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("fade_evidence runs on the server's schedule only".into());
    }
    ctx.db.evidence_fade().scheduled_id().delete(job.scheduled_id);
    let Some(e) = ctx.db.evidence().id().find(job.evidence_id) else { return Ok(()) };
    if sim::crime::faded(micros(e.made_at), micros(now(ctx))) {
        remove(ctx, e.id);
    } else {
        schedule(ctx, &e);
    }
    Ok(())
}

/// Report evidence in reach: it goes, and its points are added to the offender's crime points and
/// Notoriety. Nobody learns who the offender is, or who reported.
#[spacetimedb::reducer]
pub fn report_evidence(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let p = ctx.db.presence().identity().find(ctx.sender()).ok_or("not in the world")?;
    crate::combat::require_alive(ctx, &p)?;
    let e = ctx.db.evidence().id().find(id).ok_or("no such evidence")?;
    if sim::crime::faded(micros(e.made_at), micros(now(ctx))) {
        return Err("this evidence has faded".into());
    }
    if !sim::crime::in_reach([e.x, e.y, e.z], crate::ship::world_position(ctx, &p)) {
        return Err("out of reach".into());
    }
    let secret = ctx.db.evidence_secret().evidence_id().find(id).ok_or("no such evidence")?;
    let owner = ctx.db.character_private().char_id().find(secret.offender).map(|c| c.owner);
    if owner == Some(ctx.sender()) {
        return Err("you can't report your own crime".into());
    }
    remove(ctx, id);
    if let Some(owner) = owner {
        let r = ctx.db.crime_record().char_id().find(secret.offender);
        let (points, notoriety, reports) = r.as_ref().map_or((0, 0, 0), |r| (r.crime_points, r.notoriety, r.reports));
        record(ctx, CrimeRecord {
            char_id: secret.offender,
            owner,
            crime_points: points.saturating_add(secret.points),
            notoriety: notoriety.saturating_add(secret.points),
            reports: reports + 1,
        });
    }
    log::info!("evidence {id} reported");
    Ok(())
}

/// Store `r`, and show whether its character is Sought if it's in the world.
pub(crate) fn record(ctx: &ReducerContext, r: CrimeRecord) {
    let sought = sim::crime::sought(r.crime_points);
    let char_id = r.char_id;
    if ctx.db.crime_record().char_id().find(char_id).is_some() {
        ctx.db.crime_record().char_id().update(r);
    } else {
        ctx.db.crime_record().insert(r);
    }
    if let Some(p) = ctx.db.presence().char_id().find(char_id)
        && let Some(v) = ctx.db.vitals().entity_id().find(p.entity_id).filter(|v| v.sought != sought)
    {
        ctx.db.vitals().entity_id().update(Vitals { sought, ..v });
    }
}
