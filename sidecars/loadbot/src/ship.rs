//! `loadbot ship`: spike 3, a ship and its moving deck (docs/design/ships.md).
//!
//! A dev admin (the dock) uploads a flat seabed far from the other scenarios and spawns two
//! clippers. A helmsman and four walkers swim to the first and climb aboard. The helmsman runs it
//! up to speed and then holds it in a full-rudder turn, easing the throttle now and then. The
//! walkers roam the deck, jump and swing at each other; one of them mans the starboard cannon and
//! shells the second ship, which sits near the centre of the turn. The dock watches like a client:
//! it extrapolates the ship from its 5 Hz transform with the shared `sim::ship::step`, which is
//! also how the gunner aims, and it measures how far off that prediction is.
//!
//! Pass: no honest bot is ever corrected, the ship turns at least 360°, every swing lands, at
//! least one shot hits after its flight time, and passengers are always published with the ship as
//! parent and in its cell. Then a ship left without a helmsman must slow down; a deck speed hack and
//! a boarding of the other ship from 50 m away must be corrected; and an out-of-reach shot must be
//! refused. Everyone steps off into the water, and the ships and seabed are removed.

use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sim::ship::{Hull, HullData, Input, Motion, Pose};
use spacetimedb_sdk::{DbContext, EventTable as _, Table};

use crate::director::Director;
use crate::module_bindings::*;
use crate::swarm::Rng;
use crate::terrain::{flatten, outcome};
use crate::{Args, Bot, Check, HEARTBEAT, RUN_SPEED, wait_for};

pub struct ShipArgs {
    /// How long the honest part runs.
    pub secs: u64,
}

impl Default for ShipArgs {
    fn default() -> Self {
        Self { secs: 60 }
    }
}

/// Open sea, well away from the other scenarios.
const ORIGIN: [f32; 2] = [3000.0, 3000.0];
const SEABED_M: f32 = -20.0;
/// Seabed cells each way from the origin's cell: ±256 m.
const SEABED_CELLS: u16 = 4;
/// Full ahead in a straight line this long, then the turn.
const RUN_UP: Duration = Duration::from_secs(8);
const THROTTLE_EVERY: Duration = Duration::from_secs(20);
const GUNNERY_FROM: Duration = Duration::from_secs(15);
/// A little over the clipper's reload.
const FIRE_EVERY: Duration = Duration::from_millis(5_500);
const SWING_EVERY: Duration = Duration::from_secs(3);
const DRIFT_FOR: Duration = Duration::from_secs(4);
const FRAME: Duration = Duration::from_millis(50);
const WALK: u8 = 0;
const RUN: u8 = 1;
const SWIM: u8 = 2;
const PORT: u8 = 0;
const STARBOARD: u8 = 1;
/// The dock watches the Near tier around ship A, ships and combat included.
const SHIP_TABLES: &[&str] = &["entity", "transform", "vehicle", "combat_event"];
/// Fire only with a solution this close to the target's centre.
const AIM_TOLERANCE_M: f32 = 4.0;
/// Walkers keep this far inside the deck's edge.
const INSET_M: f32 = 0.3;
/// Where the gunner stands: inboard of the starboard cannon at (2, 0).
const GUN_SPOT: [f32; 2] = [1.3, 0.4];
const JUMP_SPEED: f32 = 5.42;
const GRAVITY: f32 = 9.8;

fn clipper() -> &'static HullData {
    Hull::Clipper.data()
}

fn secs(d: Duration) -> f32 {
    d.as_secs_f32()
}

/// One published state of a ship, as a client receives it.
#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    m: Motion,
    input: Input,
}

/// Extrapolate like a client: the shared step at the tick rate from the last sample.
fn predict(s: &Sample, ahead_s: f32) -> Motion {
    let mut m = s.m;
    let mut left = ahead_s;
    while left > 1e-4 {
        let dt = left.min(0.1);
        m = sim::ship::step(m, clipper(), s.input, dt);
        left -= dt;
    }
    m
}

fn distance(a: &Motion, b: &Motion) -> f32 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// The dock's view of ship A.
#[derive(Default)]
struct Track {
    samples: Vec<Sample>,
    turned_rad: f32,
}

impl Track {
    fn observe(&mut self, conn: &DbConnection, ship: u64) {
        let (Some(t), Some(v)) = (conn.db.transform().entity_id().find(&ship), conn.db.vehicle().entity_id().find(&ship)) else {
            return;
        };
        let m = Motion { x: t.x, z: t.z, yaw: sim::ship::yaw_from_u16(t.yaw), speed: f32::from(t.speed) / 100.0 };
        let input = Input { throttle: v.throttle, rudder: v.rudder };
        if let Some(last) = self.samples.last_mut() {
            if last.m == m {
                last.input = input;
                return;
            }
            let d = (m.yaw - last.m.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            self.turned_rad += d.abs();
        }
        self.samples.push(Sample { at: Instant::now(), m, input });
    }

    fn now(&self) -> Option<Motion> {
        self.samples.last().map(|s| predict(s, secs(s.at.elapsed())))
    }

    /// Mean and worst error of extrapolating `n` publishes (0.2 s each) ahead, with the shared step
    /// and in a straight line, over stretches where the helm didn't change.
    fn prediction_errors(&self, n: usize) -> Option<([f32; 2], [f32; 2])> {
        let (mut shared, mut straight) = (Vec::new(), Vec::new());
        for w in self.samples.windows(n + 1) {
            let (a, b) = (&w[0], &w[n]);
            if w.iter().any(|s| s.input != a.input) || a.m.speed < 1.0 {
                continue;
            }
            let ahead = 0.2 * n as f32;
            shared.push(distance(&predict(a, ahead), &b.m));
            let [fx, fz] = sim::ship::facing(a.m.yaw);
            let line = Motion { x: a.m.x + fx * a.m.speed * ahead, z: a.m.z + fz * a.m.speed * ahead, ..a.m };
            straight.push(distance(&line, &b.m));
        }
        let stats = |v: &[f32]| [v.iter().sum::<f32>() / v.len() as f32, v.iter().copied().fold(0.0, f32::max)];
        (!shared.is_empty()).then(|| (stats(&shared), stats(&straight)))
    }
}

/// A walker on deck, moving like a client: toward a random spot, sending a move every heartbeat,
/// now and then jumping and swinging at someone.
struct Walker {
    bot: Bot,
    entity: u64,
    rng: Rng,
    /// Ship-local.
    pos: [f32; 3],
    vy: f32,
    goal: [f32; 2],
    mode: u8,
    /// The mode of a stretch that just ended, to be sent before the next one starts.
    ended: Option<u8>,
    gunner: bool,
    last_send: Instant,
    sent: [f32; 3],
    next_swing: Instant,
    corrections_seen: u32,
    moves: u32,
    jumps: u32,
    metres: f32,
}

impl Walker {
    fn pick_goal(&mut self) {
        let (w, l) = (clipper().deck_half_width_m - INSET_M, clipper().deck_half_length_m - INSET_M);
        self.goal = [(self.rng.unit() * 2.0 - 1.0) * w, (self.rng.unit() * 2.0 - 1.0) * l];
        self.mode = if self.rng.unit() < 0.25 { WALK } else { RUN };
    }

    fn speed(mode: u8) -> f32 {
        0.9 * if mode == WALK { 2.0 } else { RUN_SPEED }
    }

    fn update(&mut self, dt: f32, gunnery: bool) {
        if gunnery && self.gunner {
            self.goal = GUN_SPOT;
            self.mode = RUN;
        }
        let (dx, dz) = (self.goal[0] - self.pos[0], self.goal[1] - self.pos[2]);
        let d = (dx * dx + dz * dz).sqrt();
        let stride = Self::speed(self.mode) * dt;
        if d <= stride {
            self.pos[0] = self.goal[0];
            self.pos[2] = self.goal[1];
            self.metres += d;
            if !(gunnery && self.gunner) {
                let was = self.mode;
                self.pick_goal();
                if self.mode != was {
                    self.ended = Some(was);
                }
            }
        } else {
            self.pos[0] += dx / d * stride;
            self.pos[2] += dz / d * stride;
            self.metres += stride;
        }
        if self.pos[1] > 0.0 || self.vy > 0.0 {
            self.vy -= GRAVITY * dt;
            self.pos[1] = (self.pos[1] + self.vy * dt).max(0.0);
            if self.pos[1] == 0.0 {
                self.vy = 0.0;
            }
        }
    }

    /// A move every heartbeat while moving, and one whenever the mode changes, as a client sends;
    /// sometimes jump first.
    fn heartbeat(&mut self, ship: u64) -> Check {
        if let Some(mode) = self.ended.take() {
            return self.send(ship, self.pos, mode);
        }
        if self.last_send.elapsed() < HEARTBEAT || (self.pos == self.sent && self.vy == 0.0) {
            return Ok(());
        }
        if self.pos[1] == 0.0 && !self.gunner && self.rng.unit() < 0.1 {
            self.vy = JUMP_SPEED;
            self.jumps += 1;
        }
        self.send(ship, self.pos, self.mode)
    }

    fn send(&mut self, parent: u64, at: [f32; 3], mode: u8) -> Check {
        self.bot.seq += 1;
        self.last_send = Instant::now();
        self.sent = at;
        self.moves += 1;
        let speed = (Self::speed(mode) * 100.0) as i16;
        self.bot.conn.reducers.move_event(self.bot.seq, parent, at[0], at[1], at[2], 0, speed, mode).map_err(|e| e.to_string())
    }

    /// Like the client: when the server's correction count changes, snap to its position.
    fn follow_corrections(&mut self) -> u32 {
        let Some(p) = self.bot.presence() else { return 0 };
        let new = p.corrections - self.corrections_seen;
        if new > 0 {
            println!("{}: corrected to ({:.1}, {:.2}, {:.1}) on {}", self.bot.label, p.x, p.y, p.z, p.parent);
            self.corrections_seen = p.corrections;
            self.pos = [p.x, p.y, p.z];
            self.vy = 0.0;
        }
        new
    }
}

/// Counters shared with reducer callbacks.
#[derive(Default)]
struct Tally {
    swings_ok: AtomicU32,
    swings_failed: AtomicU32,
    fire_refused: AtomicU32,
}

/// A shot the gunner fired: when, and the flight time its aim predicted.
struct Shot {
    fired: Instant,
    flight_s: f32,
}

/// The best aim of the starboard cannon at `target` from `ship`: `(yaw, pitch, flight)`.
fn aim(ship: &Motion, target: [f32; 2]) -> Option<(f32, f32, sim::ship::Flight, f32)> {
    let h = clipper();
    let c = &h.cannon;
    let pose = ship.pose(h);
    let mount = &h.mounts[usize::from(STARBOARD)];
    let mut best: Option<(f32, f32, sim::ship::Flight, f32)> = None;
    let mut yaw_deg = -c.arc_deg;
    while yaw_deg <= c.arc_deg {
        let mut pitch_deg = c.min_pitch_deg;
        while pitch_deg <= c.max_pitch_deg {
            let (yaw, pitch) = (yaw_deg.to_radians(), pitch_deg.to_radians());
            let (from, world_yaw) = sim::ship::muzzle(h, &pose, mount, yaw);
            if let Some(f) = sim::ship::cannon_flight(from, world_yaw, pitch, c.muzzle_speed_mps) {
                let miss = ((f.landing[0] - target[0]).powi(2) + (f.landing[2] - target[1]).powi(2)).sqrt();
                if best.is_none_or(|b| miss < b.3) {
                    best = Some((yaw, pitch, f, miss));
                }
            }
            pitch_deg += 0.25;
        }
        yaw_deg += 0.5;
    }
    best.filter(|b| b.3 < AIM_TOLERANCE_M)
}

fn seabed() -> sim::terrain::HeightFile {
    let (cx, cz) = (sim::cell::axis_index(ORIGIN[0]), sim::cell::axis_index(ORIGIN[1]));
    let n = 2 * SEABED_CELLS + 1;
    sim::terrain::HeightFile::generate(cx - SEABED_CELLS, cz - SEABED_CELLS, n, n, |_, _| SEABED_M)
}

/// Where the turn will centre: A's position after the run-up, plus the turning radius to starboard.
fn turn_centre() -> [f32; 2] {
    let mut m = Motion { x: ORIGIN[0], z: ORIGIN[1], yaw: 0.0, speed: 0.0 };
    for _ in 0..(secs(RUN_UP) * 10.0) as usize {
        m = sim::ship::step(m, clipper(), Input { throttle: 100, rudder: 0 }, 0.1);
    }
    [m.x + sim::ship::turning_radius(clipper()), m.z]
}

pub fn scenario(args: &Args, sa: &ShipArgs) -> Check {
    let mut dock = Bot::connect(args, "dock")?;
    dock.near = Director::new(SHIP_TABLES, 2);
    dock.subscribe_self()?;
    outcome(|tx| dock.conn.reducers.dev_set_gm_then(3, move |_, r| drop(tx.send(flatten(r)))))?;
    let file = seabed();
    let chunks: Vec<TerrainChunk> = file.chunks().into_iter().map(|(cell, samples)| TerrainChunk { cell, samples }).collect();
    for batch in chunks.chunks(32) {
        let batch = batch.to_vec();
        outcome(|tx| dock.conn.reducers.admin_put_terrain_then(batch, move |_, r| drop(tx.send(flatten(r)))))?;
    }
    println!("seabed of {} cells at {SEABED_M} m", chunks.len());
    dock.watch_near(ORIGIN[0], ORIGIN[1])?;

    let centre = turn_centre();
    let mut ships = Vec::new();
    for at in [ORIGIN, centre] {
        outcome(|tx| dock.conn.reducers.dev_spawn_ship_then(Hull::Clipper as u8, at[0], at[1], 0.0, move |_, r| drop(tx.send(flatten(r)))))?;
        let find = || dock.conn.db.transform().iter().find(|t| t.x == at[0] && t.z == at[1] && t.y > 0.0).map(|t| t.entity_id);
        wait_for("the ship to appear", || find().is_some())?;
        ships.push(find().unwrap());
    }
    let (a, b) = (ships[0], ships[1]);
    println!("clipper {a} at ({:.0}, {:.0}), clipper {b} at ({:.1}, {:.1})", ORIGIN[0], ORIGIN[1], centre[0], centre[1]);

    let result = run(args, sa, &mut dock, a, b);

    for ship in [a, b] {
        let _ = outcome(|tx| dock.conn.reducers.dev_remove_ship_then(ship, move |_, r| drop(tx.send(flatten(r)))));
    }
    outcome(|tx| dock.conn.reducers.admin_clear_terrain_then(move |_, r| drop(tx.send(flatten(r)))))?;
    println!("ships and seabed removed");
    dock.conn.disconnect().map_err(|e| e.to_string())?;
    result
}

/// Join the world and climb aboard ship `a` (at rest at `ORIGIN`) at deck spot `(x, z)`.
fn embark(args: &Args, label: &'static str, a: u64, x: f32, z: f32) -> Result<(Bot, u64), String> {
    let mut bot = Bot::connect(args, label)?;
    bot.subscribe_self()?;
    let p = bot.arrive()?;
    let at = Pose { x: ORIGIN[0], y: clipper().deck_height_m, z: ORIGIN[1], yaw: 0.0 }.to_world([x, 0.0, z]);
    bot.teleport(at[0], at[2])?;
    // Up from the seabed to swim at the surface, then over the side.
    for (parent, pos, mode) in [(0, [at[0], 0.0, at[2]], SWIM), (a, [x, 0.0, z], RUN)] {
        bot.seq += 1;
        bot.conn.reducers.move_event(bot.seq, parent, pos[0], pos[1], pos[2], 0, 0, mode).map_err(|e| e.to_string())?;
    }
    wait_for("boarding", || bot.presence().is_some_and(|p| p.parent == a && p.last_seq == bot.seq))?;
    Ok((bot, p.entity_id))
}

fn run(args: &Args, sa: &ShipArgs, dock: &mut Bot, a: u64, b: u64) -> Check {
    let h = clipper();
    let (helmsman, h_entity) = embark(args, "helmsman", a, h.helm_x, h.helm_z)?;
    outcome(|tx| helmsman.conn.reducers.take_helm_then(move |_, r| drop(tx.send(flatten(r)))))?;
    let spots = [(GUN_SPOT[0], GUN_SPOT[1]), (-1.5, -6.0), (1.5, -6.0), (-1.5, 4.0)];
    let labels = ["W1-gunner", "W2", "W3", "W4"];
    let mut walkers = Vec::new();
    for (i, (&(x, z), label)) in spots.iter().zip(labels).enumerate() {
        let (bot, entity) = embark(args, label, a, x, z)?;
        let now = Instant::now();
        let mut w = Walker {
            bot,
            entity,
            rng: Rng(0x5eed_0000 + i as u64 * 7919),
            pos: [x, 0.0, z],
            vy: 0.0,
            goal: [x, z],
            mode: RUN,
            ended: None,
            gunner: i == 0,
            last_send: now,
            sent: [x, 0.0, z],
            next_swing: now + SWING_EVERY + Duration::from_millis(700 * i as u64),
            corrections_seen: 0,
            moves: 0,
            jumps: 0,
            metres: 0.0,
        };
        w.pick_goal();
        walkers.push(w);
    }
    let entities: Vec<u64> = walkers.iter().map(|w| w.entity).collect();
    println!("a helmsman and {} walkers aboard clipper {a}", walkers.len());

    let tally = Arc::new(Tally::default());
    let events: Arc<Mutex<Vec<(Instant, CombatEvent)>>> = Arc::default();
    let sink = events.clone();
    dock.conn.db.combat_event().on_insert(move |_, e| {
        if e.skill_id == 2 {
            sink.lock().unwrap().push((Instant::now(), e.clone()));
        }
    });
    let helm = |throttle: i8, rudder: i8| outcome(|tx| helmsman.conn.reducers.helm_then(throttle, rudder, move |_, r| drop(tx.send(flatten(r)))));

    let mut track = Track::default();
    let mut shots: Vec<Shot> = Vec::new();
    let mut misaimed = 0u32;
    let mut publish_checks = 0u32;
    let mut publish_mismatches = Vec::new();
    let start = Instant::now();
    let until = start + Duration::from_secs(sa.secs);
    let mut last_frame = start;
    let mut last_publish_check = start;
    let mut turning = false;
    let mut throttle: i8 = 100;
    let mut next_throttle = start + RUN_UP + THROTTLE_EVERY;
    helm(100, 0)?;
    while Instant::now() < until {
        std::thread::sleep(FRAME);
        let now = Instant::now();
        let dt = secs(now - last_frame);
        last_frame = now;
        track.observe(&dock.conn, a);
        if let Some(m) = track.now() {
            dock.near.update(&dock.conn, m.x, m.z);
        }
        if !turning && now >= start + RUN_UP {
            helm(throttle, 100)?;
            turning = true;
        }
        if now >= next_throttle {
            throttle = if throttle == 100 { 80 } else { 100 };
            helm(throttle, 100)?;
            next_throttle += THROTTLE_EVERY;
        }
        let gunnery = now >= start + GUNNERY_FROM;
        for w in walkers.iter_mut() {
            w.follow_corrections();
            w.update(dt, gunnery);
            w.heartbeat(a)?;
            if now >= w.next_swing && !w.gunner {
                w.next_swing = now + SWING_EVERY;
                let others: Vec<u64> = entities.iter().copied().filter(|&e| e != w.entity).collect();
                let target = others[(w.rng.unit() * others.len() as f32) as usize % others.len()];
                let t = tally.clone();
                let label = w.bot.label;
                w.bot
                    .conn
                    .reducers
                    .dev_swing_then(target, move |_, r| match flatten(r) {
                        Ok(()) => drop(t.swings_ok.fetch_add(1, SeqCst)),
                        Err(e) => {
                            eprintln!("{label}: swing at {target} refused: {e}");
                            t.swings_failed.fetch_add(1, SeqCst);
                        }
                    })
                    .map_err(|e| e.to_string())?;
            }
        }
        // The gunner fires once at the cannon and loaded, with a solution on the target.
        let g = &walkers[0];
        let at_gun = g.pos[0] == GUN_SPOT[0] && g.pos[2] == GUN_SPOT[1] && g.sent == g.pos;
        let loaded = shots.last().is_none_or(|s| s.fired.elapsed() >= FIRE_EVERY);
        if gunnery
            && at_gun
            && loaded
            && let (Some(m), Some(tb)) = (track.now(), dock.conn.db.transform().entity_id().find(&b))
        {
            match aim(&m, [tb.x, tb.z]) {
                Some((yaw, pitch, flight, _)) => {
                    let t = tally.clone();
                    g.bot
                        .conn
                        .reducers
                        .fire_then(STARBOARD, yaw, pitch, move |_, r| {
                            if let Err(e) = flatten(r) {
                                eprintln!("shot refused: {e}");
                                t.fire_refused.fetch_add(1, SeqCst);
                            }
                        })
                        .map_err(|e| e.to_string())?;
                    shots.push(Shot { fired: Instant::now(), flight_s: flight.secs });
                }
                None => misaimed += 1,
            }
        }
        if now - last_publish_check >= Duration::from_secs(1) && now - start >= Duration::from_secs(1) {
            last_publish_check = now;
            let Some(ta) = dock.conn.db.transform().entity_id().find(&a) else { return Err("lost sight of the ship".into()) };
            for &e in entities.iter().chain([&h_entity]) {
                publish_checks += 1;
                match dock.conn.db.transform().entity_id().find(&e) {
                    Some(t) if t.parent == a && t.cell == ta.cell => {}
                    other => publish_mismatches.push(format!("{e}: {:?}", other.map(|t| (t.parent, t.cell)))),
                }
            }
        }
    }
    std::thread::sleep(Duration::from_secs(3)); // the last shot lands

    // The honest part.
    let mut failures = Vec::new();
    for w in walkers.iter_mut() {
        w.follow_corrections();
    }
    let corrected: Vec<String> = walkers
        .iter()
        .map(|w| (w.bot.label, w.corrections_seen))
        .chain([("helmsman", helmsman.presence().map_or(0, |p| p.corrections))])
        .filter(|&(_, n)| n > 0)
        .map(|(l, n)| format!("{l} {n}"))
        .collect();
    let (metres, moves, jumps) = walkers.iter().fold((0.0, 0, 0), |t, w| (t.0 + w.metres, t.1 + w.moves, t.2 + w.jumps));
    println!("walkers covered {metres:.0} m of deck in {moves} moves with {jumps} jumps");
    if !corrected.is_empty() {
        failures.push(format!("corrections on honest bots: {}", corrected.join(", ")));
    }
    let turned = track.turned_rad.to_degrees();
    let top = track.samples.iter().map(|s| s.m.speed).fold(0.0, f32::max);
    println!("the ship turned {turned:.0}° at up to {top:.1} m/s ({} published states)", track.samples.len());
    if turned < 360.0 {
        failures.push(format!("the ship turned only {turned:.0}°"));
    }
    let (ok, refused) = (tally.swings_ok.load(SeqCst), tally.swings_failed.load(SeqCst));
    println!("swings: {ok} landed, {refused} refused");
    if refused > 0 || ok == 0 {
        failures.push(format!("{refused} swings refused, {ok} landed"));
    }
    if !publish_mismatches.is_empty() {
        failures.push(format!("passengers published wrongly {} of {publish_checks} times: {:?}", publish_mismatches.len(), &publish_mismatches[..publish_mismatches.len().min(5)]));
    } else {
        println!("passengers published aboard and in the ship's cell in all {publish_checks} checks");
    }
    let target_hp = dock.conn.db.vehicle().entity_id().find(&b).map(|v| v.hp);
    failures.extend(gunnery_report(&shots, &events.lock().unwrap(), &tally, misaimed, entities[0], b, target_hp));
    for (n, what) in [(1, "0.2 s"), (5, "1 s")] {
        if let Some(([mean, max], [line_mean, line_max])) = track.prediction_errors(n) {
            println!(
                "extrapolating {what} ahead: shared step {mean:.2} m mean, {max:.2} m worst; straight line {line_mean:.2} m mean, {line_max:.2} m worst"
            );
        }
    }

    // Nobody at the helm: the ship drifts and slows.
    let before = track.now().map_or(0.0, |m| m.speed);
    outcome(|tx| helmsman.conn.reducers.leave_helm_then(move |_, r| drop(tx.send(flatten(r)))))?;
    let drift_end = Instant::now() + DRIFT_FOR;
    while Instant::now() < drift_end {
        std::thread::sleep(FRAME);
        track.observe(&dock.conn, a);
    }
    let after = track.samples.last().map_or(0.0, |s| s.m.speed);
    println!("left without a helmsman, the ship slowed from {before:.1} to {after:.1} m/s in {} s", DRIFT_FOR.as_secs());
    if before - after < 2.0 {
        failures.push(format!("the ship only slowed from {before:.1} to {after:.1} m/s"));
    }

    failures.extend(cheats(&mut walkers, &helmsman, a, b)?);

    // Everyone over the side, a metre past the rail, where the client thinks the ship is.
    for w in walkers.iter_mut() {
        track.observe(&dock.conn, a);
        let m = track.now().ok_or("lost track of the ship")?;
        let side = if w.pos[0] < 0.0 { -1.0 } else { 1.0 };
        let at = m.pose(clipper()).to_world([side * (clipper().deck_half_width_m + 1.0), 0.0, w.pos[2]]);
        let before = w.corrections_seen;
        w.send(0, [at[0], 0.0, at[2]], SWIM)?;
        let seq = w.bot.seq;
        wait_for("stepping off", || w.bot.presence().is_some_and(|p| p.last_seq == seq))?;
        let p = w.bot.presence().unwrap();
        if p.corrections != before || p.parent != 0 {
            failures.push(format!("{} was corrected stepping off (parent, corrections): {:?}", w.bot.label, (p.parent, p.corrections - before)));
        }
    }
    println!("all four walkers stepped off into the sea");

    for bot in walkers.iter().map(|w| &w.bot).chain([&helmsman]) {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    if failures.is_empty() { Ok(()) } else { Err(failures.join("; ")) }
}

/// Match each shot with the impact that followed it, and check the timing and the damage.
fn gunnery_report(
    shots: &[Shot],
    events: &[(Instant, CombatEvent)],
    tally: &Tally,
    misaimed: u32,
    gunner: u64,
    target: u64,
    target_hp: Option<i32>,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mine: Vec<&(Instant, CombatEvent)> = events.iter().filter(|(_, e)| e.source == gunner).collect();
    let hits = mine.iter().filter(|(_, e)| e.flags & 1 == 1 && e.target == target).count() as i32;
    let refused = tally.fire_refused.load(SeqCst);
    println!(
        "gunnery: {} shots, {} impacts, {hits} hits, {refused} refused, {misaimed} frames without a solution",
        shots.len(),
        mine.len()
    );
    if refused > 0 {
        failures.push(format!("{refused} shots refused"));
    }
    if mine.len() != shots.len() - refused as usize {
        failures.push(format!("{} shots but {} impacts", shots.len(), mine.len()));
    }
    let mut early = 0;
    for (shot, (at, _)) in shots.iter().zip(&mine) {
        let delay = secs(at.duration_since(shot.fired));
        if delay < shot.flight_s - 0.05 {
            early += 1;
            eprintln!("impact after {delay:.2} s, flight {:.2} s", shot.flight_s);
        }
    }
    if let (Some(first), Some((at, _))) = (shots.first(), mine.first()) {
        println!("first shot: {:.2} s flight, impact seen after {:.2} s", first.flight_s, secs(at.duration_since(first.fired)));
    }
    if early > 0 {
        failures.push(format!("{early} impacts came before their flight time"));
    }
    if hits == 0 {
        failures.push("no shot hit the target".into());
    }
    let expected = (clipper().hp - hits * clipper().cannon.damage).max(0);
    if target_hp != Some(expected) {
        failures.push(format!("target hp {target_hp:?}, expected {expected} after {hits} hits"));
    }
    failures
}

/// A deck speed hack and a long-range boarding are corrected; a shot from out of reach is refused.
fn cheats(walkers: &mut [Walker], helmsman: &Bot, a: u64, b: u64) -> Result<Vec<String>, String> {
    let mut failures = Vec::new();
    {
        // A fresh move on the deck first (it may have stopped mid-jump), so the dash can't hide in
        // the time spent standing still.
        let w = &mut walkers[1];
        w.pos[1] = 0.0;
        w.vy = 0.0;
        w.send(a, w.pos, RUN)?;
        let seq = w.bot.seq;
        wait_for("W2's move", || w.bot.presence().is_some_and(|p| p.last_seq == seq))?;
        if w.follow_corrections() > 0 {
            failures.push("W2 was corrected landing on the deck".into());
        }
        let z = if w.pos[2] > 0.0 { w.pos[2] - 12.0 } else { w.pos[2] + 12.0 };
        let before = w.corrections_seen;
        w.send(a, [w.pos[0], 0.0, z], RUN)?;
        if wait_for("the deck speed hack's correction", || w.bot.presence().is_some_and(|p| p.corrections > before)).is_err() {
            failures.push("a 12 m dash on deck wasn't corrected".into());
        }
        w.follow_corrections();
    }
    {
        let w = &mut walkers[2];
        let before = w.corrections_seen;
        w.send(b, [0.0, 0.0, 0.0], RUN)?;
        let corrected = wait_for("the long boarding's correction", || w.bot.presence().is_some_and(|p| p.corrections > before));
        if corrected.is_err() || w.bot.presence().is_none_or(|p| p.parent != a) {
            failures.push("boarding the other ship from 50 m away wasn't corrected".into());
        }
        w.follow_corrections();
    }
    match outcome(|tx| helmsman.conn.reducers.fire_then(PORT, 0.0, 0.0, move |_, r| drop(tx.send(flatten(r))))) {
        Err(e) if e.contains("too far") => {}
        other => failures.push(format!("a shot from the helm was {other:?}")),
    }
    if failures.is_empty() {
        println!("deck speed hack and long boarding corrected; out-of-reach shot refused");
    }
    Ok(failures)
}
