//! `loadbot terrain`: spike 5, server-side terrain (docs/design/terrain.md).
//!
//! Procedural test terrain (hills, and mesas with cliff edges) goes to a `.mmoh` file, which is read
//! back and uploaded by a dev admin. Walkers then roam it with simple client-like physics: gravity,
//! 1.5 m jumps, Godot's 45° floor limit, walking off edges and sliding down what's too steep. Half
//! of them collide with one triangulation of the heightmap and half with the other. An honest walk
//! must never be corrected. A cheater's cliff teleport, fly hack and dive underground must each be
//! corrected, and a swing over a cliff edge must be refused for line of sight. The terrain is
//! cleared at the end, so the other scenarios run on bare ground.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sim::terrain::{Diagonal, HeightFile, quad};
use spacetimedb_sdk::DbContext;

use crate::module_bindings::*;
use crate::swarm::Rng;
use crate::{Args, Bot, Check, HEARTBEAT, RUN_SPEED, WAIT, wait_for};

pub struct TerrainArgs {
    /// Honest walkers.
    pub bots: usize,
    pub secs: u64,
    /// Where the height file goes; generated first if it doesn't exist.
    pub file: String,
}

impl Default for TerrainArgs {
    fn default() -> Self {
        Self { bots: 8, secs: 30, file: "target/terrain-test.mmoh".into() }
    }
}

/// The test terrain fills 16 × 16 cells in the map's south-west corner, away from the other scenarios.
const CELLS: u16 = 16;
const X0: f32 = -sim::cell::WORLD_HALF_M;
const Z0: f32 = -sim::cell::WORLD_HALF_M;
const SIZE_M: f32 = CELLS as f32 * sim::cell::CELL_SIZE_M;
/// Walkers turn back this far from the terrain's edge.
const MARGIN_M: f32 = 24.0;

const GRAVITY: f32 = 9.8;
/// Reaches 1.5 m.
const JUMP_SPEED: f32 = 5.42;
/// Godot's default `floor_max_angle`, 45°, as rise over run.
const WALKABLE: f32 = 1.0;
const PHYSICS_STEP: Duration = Duration::from_millis(50);
/// Turn events closer together than this wait for the next send.
const MIN_SEND_GAP: Duration = Duration::from_millis(100);
const UPLOAD_BATCH: usize = 32;

/// A flat-topped hill with a cliff all round: centre (from the corner), radius and height.
struct Mesa {
    x: f32,
    z: f32,
    r: f32,
    h: f32,
}

const MESAS: [Mesa; 5] = [
    Mesa { x: 250.0, z: 250.0, r: 60.0, h: 15.0 },
    Mesa { x: 700.0, z: 300.0, r: 45.0, h: 20.0 },
    Mesa { x: 400.0, z: 700.0, r: 70.0, h: 12.0 },
    Mesa { x: 800.0, z: 800.0, r: 50.0, h: 18.0 },
    Mesa { x: 150.0, z: 800.0, r: 40.0, h: 25.0 },
];

fn smoothstep(edge0: f32, edge1: f32, v: f32) -> f32 {
    let t = ((v - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rolling hills with bumps up to about 45°, plus the mesas, whose 3 m edges rise 4–8 m per metre.
pub fn test_height(x: f32, z: f32) -> f32 {
    let hills = 40.0
        + 18.0 * (x / 97.0 + 1.3).sin() * (z / 83.0 + 0.4).sin()
        + 9.0 * (x / 41.0 + z / 57.0).sin()
        + 3.0 * (x / 13.1).sin() * (z / 11.7).cos()
        + (x / 4.3 + z / 5.1).sin();
    let mesas: f32 = MESAS
        .iter()
        .map(|m| {
            let d = ((x - X0 - m.x).powi(2) + (z - Z0 - m.z).powi(2)).sqrt();
            m.h * smoothstep(m.r + 1.5, m.r - 1.5, d)
        })
        .sum();
    hills + mesas
}

fn generate() -> HeightFile {
    HeightFile::generate(0, 0, CELLS, CELLS, test_height)
}

/// Write the test terrain to `file`.
pub fn write(file: &str) -> Check {
    let f = generate();
    if let Some(dir) = std::path::Path::new(file).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, f.to_bytes()).map_err(|e| format!("{file}: {e}"))?;
    println!("wrote {} cells of test terrain to {file}", f.chunks().len());
    Ok(())
}

fn read(file: &str) -> Result<HeightFile, String> {
    HeightFile::from_bytes(&std::fs::read(file).map_err(|e| format!("{file}: {e}"))?).map_err(|e| format!("{file}: {e}"))
}

/// Run a reducer through its `_then` form and wait for the outcome.
pub(crate) fn outcome(send: impl FnOnce(mpsc::Sender<Result<(), String>>) -> spacetimedb_sdk::Result<()>) -> Check {
    let (tx, rx) = mpsc::channel();
    send(tx).map_err(|e| e.to_string())?;
    rx.recv_timeout(WAIT).map_err(|_| "no reply from the server".to_string())?
}

pub(crate) fn flatten<E: std::fmt::Display>(r: Result<Result<(), String>, E>) -> Result<(), String> {
    r.map_err(|e| e.to_string())?
}

/// Upload a height file as `admin`, which becomes a GM first (dev builds only).
fn upload(admin: &Bot, f: &HeightFile) -> Check {
    outcome(|tx| admin.conn.reducers.dev_set_gm_then(3, move |_, r| drop(tx.send(flatten(r)))))?;
    let chunks: Vec<TerrainChunk> = f.chunks().into_iter().map(|(cell, samples)| TerrainChunk { cell, samples }).collect();
    let started = Instant::now();
    for batch in chunks.chunks(UPLOAD_BATCH) {
        let batch = batch.to_vec();
        outcome(|tx| admin.conn.reducers.admin_put_terrain_then(batch, move |_, r| drop(tx.send(flatten(r)))))?;
    }
    println!("uploaded {} chunks in {:.1} s", chunks.len(), started.elapsed().as_secs_f32());
    Ok(())
}

fn clear(admin: &Bot) -> Check {
    outcome(|tx| admin.conn.reducers.admin_clear_terrain_then(move |_, r| drop(tx.send(flatten(r)))))
}

/// `loadbot terrain-upload --file F`.
pub fn upload_file(args: &Args, ta: &TerrainArgs) -> Check {
    let admin = Bot::connect(args, "admin")?;
    upload(&admin, &read(&ta.file)?)?;
    admin.conn.disconnect().map_err(|e| e.to_string())
}

/// What the walkers did, added up.
#[derive(Default, Debug)]
struct WalkStats {
    events: u64,
    metres: f32,
    jumps: u32,
    /// Walked off an edge steeper than the floor limit.
    drops: u32,
    /// Landed on ground too steep to stand on.
    slides: u32,
    blocked: u32,
    /// The highest drop from leaving the ground to landing.
    biggest_fall_m: f32,
    corrections: u32,
}

impl WalkStats {
    fn add(&mut self, o: &WalkStats) {
        self.events += o.events;
        self.metres += o.metres;
        self.jumps += o.jumps;
        self.drops += o.drops;
        self.slides += o.slides;
        self.blocked += o.blocked;
        self.biggest_fall_m = self.biggest_fall_m.max(o.biggest_fall_m);
        self.corrections += o.corrections;
    }
}

/// An honest client: predicts its own movement on its own collision triangles and reports it.
struct Walker {
    bot: Bot,
    map: HashMap<u64, Vec<u16>>,
    diag: Diagonal,
    rng: Rng,
    pos: [f32; 3],
    vy: f32,
    grounded: bool,
    /// Height where it last left the ground.
    took_off_at: f32,
    heading: f32,
    next_jump: Instant,
    next_turn: Instant,
    last_send: Instant,
    corrections_seen: u32,
    stats: WalkStats,
}

enum Send {
    No,
    /// Turned, blocked or slid: send soon.
    Soon,
    /// Jumped or landed: send now.
    Now,
}

impl Walker {
    /// Height and slope of this walker's own collision triangle at `(x, z)`.
    fn surface(&mut self, x: f32, z: f32) -> Option<(f32, f32)> {
        quad(&mut self.map, x, z).map(|q| q.triangle(self.diag))
    }

    fn inside(x: f32, z: f32) -> bool {
        (X0 + MARGIN_M..X0 + SIZE_M - MARGIN_M).contains(&x) && (Z0 + MARGIN_M..Z0 + SIZE_M - MARGIN_M).contains(&z)
    }

    fn secs(&mut self, lo: f32, hi: f32) -> Duration {
        Duration::from_secs_f32(lo + (hi - lo) * self.rng.unit())
    }

    fn turn_randomly(&mut self) {
        self.heading = self.rng.unit() * std::f32::consts::TAU;
        self.next_turn = Instant::now() + self.secs(2.0, 6.0);
    }

    /// Head down the slope at the current position.
    fn face_downhill(&mut self) {
        let [x, _, z] = self.pos;
        let h = |w: &mut Self, dx: f32, dz: f32| w.surface(x + dx, z + dz).map_or(0.0, |s| s.0);
        let gx = h(self, 0.5, 0.0) - h(self, -0.5, 0.0);
        let gz = h(self, 0.0, 0.5) - h(self, 0.0, -0.5);
        self.heading = (-gz).atan2(-gx);
    }

    fn land(&mut self, y: f32) {
        self.stats.biggest_fall_m = self.stats.biggest_fall_m.max(self.took_off_at - y);
        self.pos[1] = y;
        self.vy = 0.0;
    }

    /// One physics step of `dt` seconds.
    fn step(&mut self, dt: f32) -> Send {
        let now = Instant::now();
        let [x, y, z] = self.pos;
        let run = RUN_SPEED * dt;
        let (nx, nz) = (x + self.heading.cos() * run, z + self.heading.sin() * run);
        if !Self::inside(nx, nz) {
            // Back towards the middle.
            let (cx, cz) = (X0 + SIZE_M / 2.0, Z0 + SIZE_M / 2.0);
            self.heading = (cz - z).atan2(cx - x) + (self.rng.unit() - 0.5);
            return Send::Soon;
        }
        let (ng, nslope) = self.surface(nx, nz).expect("inside the terrain");
        if self.grounded {
            if now >= self.next_jump {
                self.next_jump = now + self.secs(3.0, 8.0);
                self.vy = JUMP_SPEED;
                self.grounded = false;
                self.took_off_at = y;
                self.stats.jumps += 1;
                return Send::Now; // at take-off, before moving
            }
            if now >= self.next_turn {
                self.turn_randomly();
                return Send::Soon;
            }
            let rise = ng - y;
            if rise > 0.0 && (rise > run * WALKABLE || nslope > WALKABLE) {
                self.stats.blocked += 1;
                self.turn_randomly();
                return Send::Soon;
            }
            self.stats.metres += run;
            self.pos = [nx, y, nz];
            if nslope > WALKABLE || rise < -run * WALKABLE {
                // Off an edge, or onto ground too steep to stand on: fall.
                self.grounded = false;
                self.vy = 0.0;
                self.took_off_at = y;
                self.stats.drops += 1;
            } else {
                self.pos[1] = ng;
            }
            return Send::No;
        }
        self.vy -= GRAVITY * dt;
        let ny = y + self.vy * dt;
        // Into rising ground mid-air: stop and drop straight down.
        let (mx, mz, mg, mslope) = if ng > y {
            let (g, s) = self.surface(x, z).expect("inside the terrain");
            (x, z, g, s)
        } else {
            self.stats.metres += run;
            (nx, nz, ng, nslope)
        };
        self.pos = [mx, ny, mz];
        if ny > mg {
            return Send::No;
        }
        self.land(mg);
        if mslope <= WALKABLE {
            self.grounded = true;
            Send::Now
        } else {
            self.stats.slides += 1;
            self.face_downhill();
            Send::Soon
        }
    }

    fn send(&mut self) -> Check {
        self.bot.seq += 1;
        self.last_send = Instant::now();
        self.stats.events += 1;
        let [x, y, z] = self.pos;
        let yaw = (self.heading.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 65536.0) as u16;
        self.bot.conn.reducers.move_event(self.bot.seq, 0, x, y, z, yaw, (RUN_SPEED * 100.0) as i16, 1).map_err(|e| e.to_string())
    }

    /// Like the client: when the server's correction count changes, snap to its position.
    fn follow_corrections(&mut self) {
        let Some(p) = self.bot.presence() else { return };
        if p.corrections != self.corrections_seen {
            println!(
                "{}: corrected at ({:.1}, {:.2}, {:.1}) -> ({:.1}, {:.2}, {:.1}), {}",
                self.bot.label,
                self.pos[0],
                self.pos[1],
                self.pos[2],
                p.x,
                p.y,
                p.z,
                if self.grounded { "grounded" } else { "airborne" }
            );
            self.stats.corrections += p.corrections - self.corrections_seen;
            self.corrections_seen = p.corrections;
            self.pos = [p.x, p.y, p.z];
            self.grounded = true;
            self.vy = 0.0;
        }
    }

    fn walk(mut self, until: Instant) -> Result<WalkStats, String> {
        let mut last = Instant::now();
        while Instant::now() < until {
            std::thread::sleep(PHYSICS_STEP);
            let now = Instant::now();
            let dt = (now - last).as_secs_f32().min(0.1);
            last = now;
            self.follow_corrections();
            let due = match self.step(dt) {
                Send::Now => true,
                Send::Soon => now - self.last_send >= MIN_SEND_GAP,
                Send::No => false,
            };
            if due || now - self.last_send >= HEARTBEAT {
                self.send()?;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        self.follow_corrections();
        self.bot.conn.disconnect().map_err(|e| e.to_string())?;
        Ok(self.stats)
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn spawn_walker(args: &Args, f: &HeightFile, i: usize) -> Result<Walker, String> {
    let bot = Bot::connect(args, leak(format!("walker{i}")))?;
    bot.subscribe_self()?;
    bot.arrive()?;
    let mut rng = Rng(0x2545_f491_4f6c_dd1d ^ (i as u64 + 7).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    // A third start anywhere; the rest at a mesa's edge, on top heading out or at the foot heading
    // in, so even a short run meets cliffs.
    let m = &MESAS[(i / 3) % MESAS.len()];
    let angle = rng.unit() * std::f32::consts::TAU;
    let (x, z, heading) = match i % 3 {
        0 => {
            let span = SIZE_M - 2.0 * MARGIN_M;
            (X0 + MARGIN_M + span * rng.unit(), Z0 + MARGIN_M + span * rng.unit(), None)
        }
        k => {
            let d = if k == 1 { m.r - 6.0 } else { m.r + 6.0 };
            let out = if k == 1 { angle } else { angle + std::f32::consts::PI };
            (X0 + m.x + d * angle.cos(), Z0 + m.z + d * angle.sin(), Some(out))
        }
    };
    bot.teleport(x, z)?;
    let now = Instant::now();
    let mut w = Walker {
        bot,
        map: f.chunks().into_iter().collect(),
        diag: if i.is_multiple_of(2) { Diagonal::Main } else { Diagonal::Anti },
        rng,
        pos: [x, 0.0, z],
        vy: 0.0,
        grounded: true,
        took_off_at: 0.0,
        heading: 0.0,
        next_jump: now,
        next_turn: now,
        last_send: now,
        corrections_seen: 0,
        stats: WalkStats::default(),
    };
    w.pos[1] = w.surface(x, z).ok_or("spawned off the terrain")?.0;
    w.turn_randomly();
    if let Some(h) = heading {
        w.heading = h;
        w.next_turn = now + Duration::from_secs(4);
    }
    w.next_jump = now + w.secs(1.0, 5.0);
    Ok(w)
}

fn move_to(bot: &mut Bot, x: f32, y: f32, z: f32) -> Check {
    bot.seq += 1;
    bot.conn.reducers.move_event(bot.seq, 0, x, y, z, 0, 0, 1).map_err(|e| e.to_string())
}

fn expect_corrections(bot: &Bot, n: u32, what: &str) -> Check {
    wait_for(what, || bot.presence().is_some_and(|p| p.corrections == n))?;
    println!("cheater: {what} corrected");
    Ok(())
}

/// The cheats the spike must catch, at the first mesa's west edge, and a swing across that edge.
fn cheats_and_sight(args: &Args, f: &HeightFile) -> Check {
    let mut map: HashMap<u64, Vec<u16>> = f.chunks().into_iter().collect();
    let mut ground = |x: f32, z: f32| sim::terrain::ground(&mut map, x, z).map(|g| g.hi).ok_or("off the terrain");
    let m = &MESAS[0];
    let (mx, mz) = (X0 + m.x, Z0 + m.z);

    let mut c = Bot::connect(args, "cheater")?;
    c.subscribe_self()?;
    c.arrive()?;
    c.teleport(mx - m.r - 6.0, mz)?;

    // Up the cliff in one step: 9 m across in 2 s is a legal distance, 15 m up is not.
    std::thread::sleep(Duration::from_secs(2));
    let top = mx - m.r + 3.0;
    move_to(&mut c, top, ground(top, mz)?, mz)?;
    expect_corrections(&c, 1, "cliff teleport")?;

    // Straight up 10 m.
    std::thread::sleep(HEARTBEAT);
    let p = c.presence().ok_or("cheater left")?;
    move_to(&mut c, p.x, p.y + 10.0, p.z)?;
    expect_corrections(&c, 2, "fly hack")?;

    // Under the ground.
    std::thread::sleep(HEARTBEAT);
    let p = c.presence().ok_or("cheater left")?;
    move_to(&mut c, p.x + 1.0, ground(p.x + 1.0, p.z)? - 5.0, p.z)?;
    expect_corrections(&c, 3, "dive underground")?;

    // A spotter on the mesa 16 m from the cheater below: the cliff edge is in the way.
    let s = Bot::connect(args, "spotter")?;
    s.subscribe_self()?;
    let sp = s.arrive()?;
    c.teleport(mx - m.r - 8.0, mz)?;
    s.teleport(mx - m.r + 8.0, mz)?;
    let swing = |c: &Bot| outcome(|tx| c.conn.reducers.dev_swing_then(sp.entity_id, move |_, r| drop(tx.send(flatten(r)))));
    match swing(&c) {
        Err(e) if e.contains("line of sight") => println!("cheater: swing up the cliff refused: {e}"),
        other => return Err(format!("swing up the cliff: expected no line of sight, got {other:?}")),
    }
    // Beside it on open ground, it lands.
    s.teleport(mx - m.r - 8.0, mz + 4.0)?;
    swing(&c).map_err(|e| format!("swing on open ground: {e}"))?;
    println!("cheater: swing on open ground landed");
    for b in [&c, &s] {
        b.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn scenario(args: &Args, ta: &TerrainArgs) -> Check {
    if !std::path::Path::new(&ta.file).exists() {
        write(&ta.file)?;
    }
    let f = read(&ta.file)?;
    let admin = Bot::connect(args, "admin")?;
    admin.subscribe_self()?;
    upload(&admin, &f)?;
    let result = run(args, ta, &f);
    clear(&admin)?;
    println!("terrain cleared");
    admin.conn.disconnect().map_err(|e| e.to_string())?;
    result
}

fn run(args: &Args, ta: &TerrainArgs, f: &HeightFile) -> Check {
    let walkers = (0..ta.bots).map(|i| spawn_walker(args, f, i)).collect::<Result<Vec<_>, _>>()?;
    let until = Instant::now() + Duration::from_secs(ta.secs);
    println!("{} walkers out for {} s", walkers.len(), ta.secs);
    let handles: Vec<_> = walkers.into_iter().map(|w| std::thread::spawn(move || w.walk(until))).collect();

    let cheats = cheats_and_sight(args, f);

    let mut total = WalkStats::default();
    for h in handles {
        total.add(&h.join().map_err(|_| "a walker panicked")??);
    }
    println!(
        "walked {:.0} m: {} events, {} jumps, {} drops off edges, {} slides, {} times blocked, biggest fall {:.1} m",
        total.metres, total.events, total.jumps, total.drops, total.slides, total.blocked, total.biggest_fall_m
    );
    cheats?;
    if total.corrections > 0 {
        return Err(format!("{} corrections on honest walks", total.corrections));
    }
    if total.drops == 0 || total.blocked == 0 || total.jumps == 0 {
        return Err("the walk never met a cliff, so it proves little".into());
    }
    println!("no false corrections");
    Ok(())
}
