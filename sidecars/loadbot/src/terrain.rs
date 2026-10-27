//! `loadbot terrain`: spike 5, server-side terrain (docs/design/terrain.md).
//!
//! Procedural test terrain (hills, and mesas with cliff edges) goes to a `.mmoh` file, which is read
//! back and uploaded by a dev admin. Walkers then roam it with simple client-like physics: gravity,
//! 1.5 m jumps, Godot's 45° floor limit, walking off edges and sliding down what's too steep. A
//! walker stuck sliding in a steep-sided dip tries new headings, then respawns on walkable ground.
//! Half of them collide with one triangulation of the heightmap and half with the other. An honest
//! walk must never be corrected. A cheater's cliff teleport, fly hack and dive underground must each
//! be corrected, and a swing over a cliff edge must be refused for line of sight. The terrain is
//! cleared at the end, so the other scenarios run on bare ground.
//!
//! `terrain-upload` and `world-upload` send a height file or the world's files (docs/design/world.md)
//! as an admin: the publisher, when `MMO_ADMIN_TOKEN` holds its token (`cargo xtask world` sets it),
//! or else a fresh identity made a GM with the dev-only `dev_set_gm`.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sim::terrain::{Diagonal, HeightFile, quad};
use spacetimedb_sdk::{DbContext, Table};

use crate::module_bindings::*;
use crate::swarm::Rng;
use crate::{Args, Bot, Check, HEARTBEAT, RUN_SPEED, WAIT, wait_for};

pub struct TerrainArgs {
    /// Honest walkers.
    pub bots: usize,
    pub secs: u64,
    /// Where the height file goes; generated first if it doesn't exist.
    pub file: String,
    /// The world's files, for `world-upload`.
    pub world: String,
}

impl Default for TerrainArgs {
    fn default() -> Self {
        Self { bots: 8, secs: 30, file: "target/terrain-test.mmoh".into(), world: worldtool::DEFAULT_DIR.into() }
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
/// A slide that gets less than `STUCK_M` anywhere in this long is stuck: try a random heading, and
/// after `STUCK_TURNS` of them, respawn somewhere walkable.
const STUCK_AFTER: Duration = Duration::from_millis(1500);
const STUCK_M: f32 = 1.0;
const STUCK_TURNS: u32 = 2;
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

/// The token's environment variable: the publisher's token, which makes its holder an admin.
const ADMIN_TOKEN: &str = "MMO_ADMIN_TOKEN";

/// Connect as an admin: the publisher if `MMO_ADMIN_TOKEN` is set, else a fresh identity that makes
/// itself a GM (dev builds only).
fn admin(args: &Args) -> Result<Bot, String> {
    if let Some(token) = std::env::var(ADMIN_TOKEN).ok().filter(|t| !t.is_empty()) {
        let bot = Bot::connect_as(args, "admin", Some(token))?;
        println!("admin: the publisher ({})", bot.identity().to_hex());
        return Ok(bot);
    }
    let bot = Bot::connect(args, "admin")?;
    outcome(|tx| bot.conn.reducers.dev_set_gm_then(3, move |_, r| drop(tx.send(flatten(r)))))
        .map_err(|e| format!("becoming a GM: {e} (a --prod build has no dev_set_gm; set {ADMIN_TOKEN} to the publisher's token)"))?;
    Ok(bot)
}

/// Upload a height file as `admin`.
fn upload(admin: &Bot, f: &HeightFile) -> Check {
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
    let admin = admin(args)?;
    upload(&admin, &read(&ta.file)?)?;
    admin.conn.disconnect().map_err(|e| e.to_string())
}

/// `loadbot world-upload [--world DIR]`: replace the server's terrain with the world's, then check
/// that the server's terrain hash is the manifest's; then replace its resource nodes with the
/// world's (docs/design/gathering.md), its crafting stations (docs/design/crafting.md) and its
/// trade posts (docs/design/trade.md), and check they all arrived.
pub fn upload_world(args: &Args, ta: &TerrainArgs) -> Check {
    let started = Instant::now();
    let world = worldtool::World::load(std::path::Path::new(&ta.world))?;
    let want = world.terrain_hash();
    println!("read {} in {:.1} s, terrain hash {want:016x}", ta.world, started.elapsed().as_secs_f32());
    let admin = admin(args)?;
    admin.subscribe_self()?;
    clear(&admin)?;
    upload(&admin, &world.heights)?;
    let got = || admin.server_terrain();
    wait_for("the server's terrain hash", || got().hash == want)
        .map_err(|_| format!("the server's terrain hash is {:016x} over {} chunks, expected {want:016x}", got().hash, got().chunks))?;
    println!("the server's terrain hash matches: {want:016x} over {} chunks", got().chunks);
    upload_land(&admin, &world)?;
    upload_nodes(&admin, &worldtool::nodes::load(std::path::Path::new(&ta.world))?)?;
    upload_stations(&admin, &worldtool::stations::load(std::path::Path::new(&ta.world))?)?;
    upload_posts(&admin, &worldtool::posts::load(std::path::Path::new(&ta.world))?)?;
    admin.conn.disconnect().map_err(|e| e.to_string())
}

/// Land chunks per `admin_put_land` call: 256 bytes each.
const LAND_BATCH: usize = 512;

/// Replace the server's land use with the world's (docs/design/land.md) and check its land hash.
fn upload_land(admin: &Bot, world: &worldtool::World) -> Check {
    let chunks = world.land_chunks();
    let want = chunks.iter().fold(0u64, |h, (cell, t)| h.wrapping_add(sim::land::land_chunk_hash(*cell, t)));
    outcome(|tx| admin.conn.reducers.admin_clear_land_then(move |_, r| drop(tx.send(flatten(r)))))?;
    let rows: Vec<LandChunk> = chunks.into_iter().map(|(cell, texels)| LandChunk { cell, texels }).collect();
    let n = rows.len();
    for batch in rows.chunks(LAND_BATCH) {
        let batch = batch.to_vec();
        outcome(|tx| admin.conn.reducers.admin_put_land_then(batch, move |_, r| drop(tx.send(flatten(r)))))?;
    }
    let got = || admin.server_terrain();
    wait_for("the server's land hash", || got().land_hash == want)
        .map_err(|_| format!("the server's land hash is {:016x} over {} chunks, expected {want:016x}", got().land_hash, got().land_chunks))?;
    println!("uploaded {n} land chunks; the server's land hash matches: {want:016x}");
    Ok(())
}

/// Nodes per `admin_put_nodes` call.
const NODE_BATCH: usize = 1000;

/// Replace every resource node on the server with `nodes`, and check the count.
fn upload_nodes(admin: &Bot, nodes: &[worldtool::nodes::Placement]) -> Check {
    let started = Instant::now();
    outcome(|tx| admin.conn.reducers.admin_clear_nodes_then(move |_, r| drop(tx.send(flatten(r)))))?;
    let rows: Vec<NodePlacement> = nodes
        .iter()
        .map(|n| NodePlacement { kind: n.kind.wire, x: n.x, y: n.y, z: n.z, yaw: sim::ship::yaw_to_u16(f32::from(n.yaw_deg).to_radians()) })
        .collect();
    for batch in rows.chunks(NODE_BATCH) {
        let batch = batch.to_vec();
        outcome(|tx| admin.conn.reducers.admin_put_nodes_then(batch, move |_, r| drop(tx.send(flatten(r)))))?;
    }
    let (tx, rx) = mpsc::channel();
    admin
        .conn
        .subscription_builder()
        .on_applied(move |_| {
            let _ = tx.send(());
        })
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(["SELECT * FROM resource_node"]);
    rx.recv_timeout(WAIT).map_err(|_| "no reply to the node subscription".to_string())?;
    let got = admin.conn.db.resource_node().count() as usize;
    if got != nodes.len() {
        return Err(format!("the server has {got} resource nodes, expected {}", nodes.len()));
    }
    println!("uploaded {got} resource nodes in {:.1} s", started.elapsed().as_secs_f32());
    Ok(())
}

/// Replace every crafting station on the server with `stations`, and check the count.
fn upload_stations(admin: &Bot, stations: &[worldtool::stations::Placement]) -> Check {
    outcome(|tx| admin.conn.reducers.admin_clear_stations_then(move |_, r| drop(tx.send(flatten(r)))))?;
    let rows: Vec<StationPlacement> = stations
        .iter()
        .map(|s| StationPlacement { kind: s.kind.wire, x: s.x, y: s.y, z: s.z, yaw: sim::ship::yaw_to_u16(f32::from(s.yaw_deg).to_radians()) })
        .collect();
    outcome(|tx| admin.conn.reducers.admin_put_stations_then(rows, move |_, r| drop(tx.send(flatten(r)))))?;
    let (tx, rx) = mpsc::channel();
    admin
        .conn
        .subscription_builder()
        .on_applied(move |_| {
            let _ = tx.send(());
        })
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(["SELECT * FROM station"]);
    rx.recv_timeout(WAIT).map_err(|_| "no reply to the station subscription".to_string())?;
    let got = admin.conn.db.station().count() as usize;
    if got != stations.len() {
        return Err(format!("the server has {got} crafting stations, expected {}", stations.len()));
    }
    println!("uploaded {got} crafting stations");
    Ok(())
}

/// Replace every trade post on the server with `posts`, and check the count.
fn upload_posts(admin: &Bot, posts: &[worldtool::posts::Placement]) -> Check {
    outcome(|tx| admin.conn.reducers.admin_clear_posts_then(move |_, r| drop(tx.send(flatten(r)))))?;
    let rows: Vec<PostPlacement> = posts
        .iter()
        .map(|s| PostPlacement { kind: s.kind.wire, x: s.x, y: s.y, z: s.z, yaw: sim::ship::yaw_to_u16(f32::from(s.yaw_deg).to_radians()) })
        .collect();
    outcome(|tx| admin.conn.reducers.admin_put_posts_then(rows, move |_, r| drop(tx.send(flatten(r)))))?;
    let (tx, rx) = mpsc::channel();
    admin
        .conn
        .subscription_builder()
        .on_applied(move |_| {
            let _ = tx.send(());
        })
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(["SELECT * FROM trade_post"]);
    rx.recv_timeout(WAIT).map_err(|_| "no reply to the post subscription".to_string())?;
    let got = admin.conn.db.trade_post().count() as usize;
    if got != posts.len() {
        return Err(format!("the server has {got} trade posts, expected {}", posts.len()));
    }
    println!("uploaded {got} trade posts");
    Ok(())
}

/// What the walkers did, added up.
#[derive(Default, Debug)]
struct WalkStats {
    events: u64,
    metres: f32,
    jumps: u32,
    /// Walked off an edge steeper than the floor limit.
    drops: u32,
    /// Landed on ground too steep to stand on (once per fall).
    slides: u32,
    /// Stuck sliding in a steep-sided dip and moved elsewhere.
    respawns: u32,
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
        self.respawns += o.respawns;
        self.blocked += o.blocked;
        self.biggest_fall_m = self.biggest_fall_m.max(o.biggest_fall_m);
        self.corrections += o.corrections;
    }
}

/// An honest client: predicts its own movement on its own collision triangles and reports it.
struct Walker {
    bot: Bot,
    body: Body,
    last_send: Instant,
    corrections_seen: u32,
}

/// A walker's physics, apart from its connection.
struct Body {
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
    /// Set while sliding on ground too steep to stand on.
    slide: Option<Slide>,
    stats: WalkStats,
}

/// Where and when a slide last made progress, and the random headings tried since.
struct Slide {
    since: Instant,
    at: [f32; 2],
    turns: u32,
}

#[derive(Debug, PartialEq)]
enum Send {
    No,
    /// Turned, blocked or slid: send soon.
    Soon,
    /// Jumped or landed: send now.
    Now,
    /// Stuck sliding: teleport somewhere walkable.
    Respawn,
}

impl Body {
    fn new(map: HashMap<u64, Vec<u16>>, diag: Diagonal, rng: Rng, x: f32, z: f32, now: Instant) -> Option<Self> {
        let mut b = Body {
            map,
            diag,
            rng,
            pos: [x, 0.0, z],
            vy: 0.0,
            grounded: true,
            took_off_at: 0.0,
            heading: 0.0,
            next_jump: now,
            next_turn: now,
            slide: None,
            stats: WalkStats::default(),
        };
        b.pos[1] = b.surface(x, z)?.0;
        b.turn_randomly(now);
        Some(b)
    }

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

    fn turn_randomly(&mut self, now: Instant) {
        self.heading = self.rng.unit() * std::f32::consts::TAU;
        self.next_turn = now + self.secs(2.0, 6.0);
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

    /// Put down at `pos` on the ground, as after a correction or a teleport.
    fn settle(&mut self, pos: [f32; 3]) {
        self.pos = pos;
        self.grounded = true;
        self.vy = 0.0;
        self.slide = None;
    }

    /// One physics step of `dt` seconds, ending at `now`.
    fn step(&mut self, now: Instant, dt: f32) -> Send {
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
                self.turn_randomly(now);
                return Send::Soon;
            }
            let rise = ng - y;
            if rise > 0.0 && (rise > run * WALKABLE || nslope > WALKABLE) {
                self.stats.blocked += 1;
                self.turn_randomly(now);
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
            self.slide = None;
            return Send::Now;
        }
        let at = [mx, mz];
        let Some(s) = &mut self.slide else {
            self.stats.slides += 1;
            self.slide = Some(Slide { since: now, at, turns: 0 });
            self.face_downhill();
            return Send::Soon;
        };
        if (at[0] - s.at[0]).hypot(at[1] - s.at[1]) > STUCK_M {
            *s = Slide { since: now, at, turns: 0 };
        } else if now - s.since >= STUCK_AFTER {
            if s.turns == STUCK_TURNS {
                return Send::Respawn;
            }
            s.turns += 1;
            s.since = now;
            self.heading = self.rng.unit() * std::f32::consts::TAU;
            return Send::Soon;
        }
        // Downhill, unless trying a way out.
        if s.turns == 0 {
            self.face_downhill();
        }
        Send::Soon
    }

    /// A random place inside the terrain that isn't too steep to stand on.
    fn walkable_spot(&mut self) -> Result<(f32, f32), String> {
        let span = SIZE_M - 2.0 * MARGIN_M;
        for _ in 0..100 {
            let (x, z) = (X0 + MARGIN_M + span * self.rng.unit(), Z0 + MARGIN_M + span * self.rng.unit());
            if self.surface(x, z).is_some_and(|(_, slope)| slope <= WALKABLE) {
                return Ok((x, z));
            }
        }
        Err("no walkable ground found".into())
    }
}

impl Walker {
    fn respawn(&mut self) -> Check {
        let (x, z) = self.body.walkable_spot()?;
        self.bot.teleport(x, z)?;
        let p = self.bot.presence().ok_or("left the world")?;
        self.body.settle([p.x, p.y, p.z]);
        self.body.stats.respawns += 1;
        self.body.turn_randomly(Instant::now());
        Ok(())
    }

    fn send(&mut self) -> Check {
        self.bot.seq += 1;
        self.last_send = Instant::now();
        self.body.stats.events += 1;
        let [x, y, z] = self.body.pos;
        let yaw = (self.body.heading.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 65536.0) as u16;
        self.bot.conn.reducers.move_event(self.bot.seq, 0, x, y, z, yaw, (RUN_SPEED * 100.0) as i16, 1).map_err(|e| e.to_string())
    }

    /// Like the client: when the server's correction count changes, snap to its position.
    fn follow_corrections(&mut self) {
        let Some(p) = self.bot.presence() else { return };
        if p.corrections != self.corrections_seen {
            let b = &self.body;
            println!(
                "{}: corrected at ({:.1}, {:.2}, {:.1}) -> ({:.1}, {:.2}, {:.1}), {}",
                self.bot.label,
                b.pos[0],
                b.pos[1],
                b.pos[2],
                p.x,
                p.y,
                p.z,
                if b.grounded { "grounded" } else { "airborne" }
            );
            self.body.stats.corrections += p.corrections - self.corrections_seen;
            self.corrections_seen = p.corrections;
            self.body.settle([p.x, p.y, p.z]);
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
            let due = match self.body.step(now, dt) {
                Send::Now => true,
                Send::Soon => now - self.last_send >= MIN_SEND_GAP,
                Send::No => false,
                Send::Respawn => {
                    self.respawn()?;
                    continue;
                }
            };
            if due || now - self.last_send >= HEARTBEAT {
                self.send()?;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        self.follow_corrections();
        self.bot.conn.disconnect().map_err(|e| e.to_string())?;
        Ok(self.body.stats)
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
    let diag = if i.is_multiple_of(2) { Diagonal::Main } else { Diagonal::Anti };
    let mut body = Body::new(f.chunks().into_iter().collect(), diag, rng, x, z, now).ok_or("spawned off the terrain")?;
    if let Some(h) = heading {
        body.heading = h;
        body.next_turn = now + Duration::from_secs(4);
    }
    body.next_jump = now + body.secs(1.0, 5.0);
    Ok(Walker { bot, body, last_send: now, corrections_seen: 0 })
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
    let admin = admin(args)?;
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
        "walked {:.0} m: {} events, {} jumps, {} drops off edges, {} slides, {} respawns from dips, {} times blocked, biggest fall {:.1} m",
        total.metres,
        total.events,
        total.jumps,
        total.drops,
        total.slides,
        total.respawns,
        total.blocked,
        total.biggest_fall_m
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The test terrain's middle, on a sample.
    const CX: f32 = X0 + SIZE_M / 2.0;
    const CZ: f32 = Z0 + SIZE_M / 2.0;

    /// A body dropped just above `(x, z)` on flat ground at 40 m reshaped by `height`, with no jumps due.
    fn falling_body(height: impl Fn(f32, f32) -> f32, x: f32, z: f32, now: Instant) -> Body {
        let f = HeightFile::generate(0, 0, CELLS, CELLS, height);
        let mut b = Body::new(f.chunks().into_iter().collect(), Diagonal::Main, Rng(0x1234_5678), x, z, now).unwrap();
        b.next_jump = now + Duration::from_secs(3600);
        b.grounded = false;
        b.pos[1] += 0.5;
        b
    }

    /// Steps of `PHYSICS_STEP` until `done` or `secs` pass; the outcome and the time taken.
    fn run(b: &mut Body, mut now: Instant, secs: f32, done: impl Fn(&Body, &Send) -> bool) -> (Send, f32) {
        let dt = PHYSICS_STEP.as_secs_f32();
        let mut t = 0.0;
        while t < secs {
            now += PHYSICS_STEP;
            t += dt;
            let s = b.step(now, dt);
            if done(b, &s) {
                return (s, t);
            }
        }
        (Send::No, t)
    }

    #[test]
    fn stuck_in_a_steep_pit_tries_headings_then_respawns() {
        // A cone 6 m across falling 3 m per metre: no way out on foot.
        let pit = |x: f32, z: f32| 40.0 - 3.0 * (6.0 - (x - CX).hypot(z - CZ)).max(0.0);
        let now = Instant::now();
        let mut b = falling_body(pit, CX, CZ, now);
        let (s, t) = run(&mut b, now, 30.0, |_, s| *s == Send::Respawn);
        assert_eq!(s, Send::Respawn);
        let expected = STUCK_AFTER.as_secs_f32() * (STUCK_TURNS + 1) as f32;
        assert!((expected..expected + 1.0).contains(&t), "respawned after {t} s");
        assert_eq!(b.stats.slides, 1, "one slide, counted once");
        // Somewhere walkable to go instead.
        let (x, z) = b.walkable_spot().unwrap();
        assert!(b.surface(x, z).unwrap().1 <= WALKABLE);
    }

    #[test]
    fn a_slide_down_a_steep_bank_lands_once_and_walks_on() {
        // A bank rising 2 m per metre east of the middle, for 10 m.
        let bank = |x: f32, _: f32| 40.0 + 2.0 * (x - CX).clamp(0.0, 10.0);
        let now = Instant::now();
        let mut b = falling_body(bank, CX + 5.0, CZ, now);
        let (s, _) = run(&mut b, now, 10.0, |b, s| b.grounded || *s == Send::Respawn);
        assert_ne!(s, Send::Respawn);
        assert!(b.grounded && b.pos[0] < CX, "slid to the foot of the bank: {:?}", b.pos);
        assert_eq!(b.stats.slides, 1);
        assert!(b.slide.is_none());
    }
}
