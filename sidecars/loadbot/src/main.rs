//! Headless bots that play scripted scenarios against a running server and check what they see.
//!
//! `cargo run -p loadbot -- [--uri URI] [--db NAME] <scenario>`; scenarios are listed by `help`.
//! `swarm` also takes `--bots N --combat SHARE --area METRES --secs S [--compression none|gzip|brotli]`;
//! `terrain` takes `--bots N --secs S --file F`, `ship` takes `--secs S`, and `terrain-gen` and `terrain-upload` take `--file F`.
//! Exits non-zero when a check fails.

#[allow(clippy::all, unused)]
mod module_bindings;
mod director;
mod metrics;
mod ship;
mod swarm;
mod terrain;

use std::time::{Duration, Instant};

use director::Director;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table};

const RUN_SPEED: f32 = 5.4;
const HEARTBEAT: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_secs(5);
/// The Near tier's tables (PLAN §4.3), 5×5 cells.
const NEAR_TABLES: &[&str] = &["entity", "transform"];

struct Args {
    uri: String,
    db: String,
    scenario: String,
}

fn parse_args() -> (Args, swarm::SwarmArgs, terrain::TerrainArgs, ship::ShipArgs) {
    let mut a = Args { uri: "http://127.0.0.1:3000".into(), db: "mmo-world".into(), scenario: "help".into() };
    let mut sw = swarm::SwarmArgs::default();
    let mut ta = terrain::TerrainArgs::default();
    let mut sh = ship::ShipArgs::default();
    let mut it = std::env::args().skip(1);
    fn num<T: std::str::FromStr>(flag: &str, v: Option<String>) -> T {
        v.and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("{flag} needs a number"))
    }
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--uri" => a.uri = it.next().expect("--uri needs a value"),
            "--db" => a.db = it.next().expect("--db needs a value"),
            "--bots" => {
                sw.bots = num("--bots", it.next());
                ta.bots = sw.bots;
            }
            "--combat" => sw.combat = num("--combat", it.next()),
            "--area" => sw.area = num("--area", it.next()),
            "--secs" => {
                sw.secs = num("--secs", it.next());
                ta.secs = sw.secs;
                sh.secs = sw.secs;
            }
            "--file" => ta.file = it.next().expect("--file needs a path"),
            "--compression" => {
                sw.compression = match it.next().as_deref() {
                    Some("none") => spacetimedb_sdk::Compression::None,
                    Some("gzip") => spacetimedb_sdk::Compression::Gzip,
                    Some("brotli") => spacetimedb_sdk::Compression::Brotli,
                    _ => panic!("--compression is none, gzip or brotli"),
                }
            }
            s => a.scenario = s.into(),
        }
    }
    (a, sw, ta, sh)
}

fn main() {
    let (args, sw, ta, sh) = parse_args();
    let scenario = args.scenario.clone();
    let result = match scenario.as_str() {
        "see-each-other" => see_each_other(&args),
        "tick" => tick(&args),
        "cross-cells" => cross_cells(&args),
        "metrics" => metrics::read(&args.uri, &args.db).map(|r| print!("{r}")),
        "swarm" => swarm::swarm(args, sw),
        "terrain" => terrain::scenario(&args, &ta),
        "terrain-gen" => terrain::write(&ta.file),
        "terrain-upload" => terrain::upload_file(&args, &ta),
        "ship" => ship::scenario(&args, &sh),
        _ => {
            eprintln!(
                "scenarios:\n  \
                 see-each-other   two bots enter, see each other move, get corrected, leave\n  \
                 tick             the world tick runs at 10 Hz and its timings can be read\n  \
                 cross-cells      a bot walks 300 m through cell borders and never loses sight of anyone\n  \
                 metrics          print the server's tick timings and reducer counts\n  \
                 swarm            the throughput spike: many bots moving and fighting, measured\n  \
                 terrain          walkers roam uploaded test terrain with no false corrections; cheats are caught\n  \
                 terrain-gen      write the procedural test terrain to a .mmoh file\n  \
                 terrain-upload   upload a .mmoh height file as a dev admin\n  \
                 ship             bots walk the deck of a turning ship, fire its cannon and try to cheat"
            );
            std::process::exit(2);
        }
    };
    match result {
        Ok(()) => println!("PASS {scenario}"),
        Err(e) => {
            eprintln!("FAIL {scenario}: {e}");
            std::process::exit(1);
        }
    }
}

type Check = Result<(), String>;

/// Poll `cond` until it holds or `WAIT` runs out.
fn wait_for(what: &str, mut cond: impl FnMut() -> bool) -> Check {
    let start = Instant::now();
    while start.elapsed() < WAIT {
        if cond() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(format!("timed out waiting for {what}"))
}

/// One connected bot with its own fresh identity.
struct Bot {
    label: &'static str,
    conn: DbConnection,
    seq: u32,
    near: Director,
}

impl Bot {
    fn connect(args: &Args, label: &'static str) -> Result<Self, String> {
        let conn = DbConnection::builder()
            .with_uri(args.uri.as_str())
            .with_database_name(args.db.as_str())
            .with_compression(spacetimedb_sdk::Compression::Gzip)
            .build()
            .map_err(|e| format!("{label}: connect failed: {e}"))?;
        conn.run_threaded();
        wait_for("identity", || conn.try_identity().is_some())?;
        Ok(Self { label, conn, seq: 0, near: Director::new(NEAR_TABLES, 2) })
    }

    fn identity(&self) -> Identity {
        self.conn.identity()
    }

    /// Subscribe to our private views and everyone's names.
    fn subscribe_self(&self) -> Check {
        let applied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = applied.clone();
        self.conn
            .subscription_builder()
            .on_applied(move |_| flag.store(true, std::sync::atomic::Ordering::SeqCst))
            .on_error(|_, e| eprintln!("subscription error: {e}"))
            .subscribe(["SELECT * FROM my_characters", "SELECT * FROM my_presence", "SELECT * FROM character_public"]);
        wait_for("subscription", || applied.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Start the Near tier around `(x, z)`; `step` keeps it following us from then on.
    fn watch_near(&self, x: f32, z: f32) -> Check {
        self.near.update(&self.conn, x, z);
        wait_for("Near tier", || self.near.ready())
    }

    /// Move anywhere at once with the dev-only GM teleport.
    fn teleport(&self, x: f32, z: f32) -> Check {
        self.conn.reducers.dev_teleport(x, z).map_err(|e| e.to_string())?;
        wait_for("teleport", || self.presence().is_some_and(|p| p.x == x && p.z == z))
    }

    /// Create a character named after our identity and enter the world with it.
    fn arrive(&self) -> Result<Presence, String> {
        let name = name_for(self.identity());
        self.conn.reducers.create_character(name.clone()).map_err(|e| e.to_string())?;
        wait_for("character", || self.conn.db.my_characters().count() > 0)?;
        let char_id = self.conn.db.my_characters().iter().next().unwrap().char_id;
        self.conn.reducers.enter_world(char_id, sim::content::CONTENT_HASH).map_err(|e| e.to_string())?;
        wait_for("presence", || self.presence().is_some())?;
        println!("{}: entered as {name} (char {char_id})", self.label);
        Ok(self.presence().unwrap())
    }

    fn presence(&self) -> Option<Presence> {
        self.conn.db.my_presence().iter().next()
    }

    fn step(&mut self, x: f32, z: f32) -> Check {
        self.seq += 1;
        self.conn
            .reducers
            .move_event(self.seq, 0, x, 0.0, z, 0, (RUN_SPEED * 100.0) as i16, 1)
            .map_err(|e| e.to_string())?;
        if self.near.center().is_some() {
            self.near.update(&self.conn, x, z);
        }
        Ok(())
    }

    fn sees(&self, entity_id: u64) -> Option<Transform> {
        self.conn.db.transform().entity_id().find(&entity_id)
    }
}

/// A unique letters-only name, since names allow A–Z only.
fn name_for(id: Identity) -> String {
    let hex = id.to_hex().to_string();
    let tail: String = hex.chars().rev().take(12).map(|c| (b'a' + c.to_digit(16).unwrap() as u8) as char).collect();
    format!("Bot{tail}")
}

fn see_each_other(args: &Args) -> Check {
    let mut a = Bot::connect(args, "A")?;
    let b = Bot::connect(args, "B")?;
    for bot in [&a, &b] {
        bot.subscribe_self()?;
        bot.watch_near(0.0, 0.0)?;
    }
    let pa = a.arrive()?;
    let pb = b.arrive()?;

    wait_for("A to see B", || a.sees(pb.entity_id).is_some())?;
    wait_for("B to see A", || b.sees(pa.entity_id).is_some())?;
    println!("both bots see each other");

    // A runs east for 3 s with heartbeats; B should see it arrive.
    let start = a.sees(pa.entity_id).unwrap();
    let mut x = start.x;
    for _ in 0..6 {
        std::thread::sleep(HEARTBEAT);
        x += RUN_SPEED * HEARTBEAT.as_secs_f32() * 0.95;
        a.step(x, start.z)?;
    }
    wait_for("B to see A's run", || b.sees(pa.entity_id).is_some_and(|t| (t.x - x).abs() < 0.01))?;
    println!("B saw A run to x = {x:.1}");

    // A tries to jump 50 m in half a second; the server keeps it in place and A is told to snap back.
    std::thread::sleep(HEARTBEAT);
    a.step(x + 50.0, start.z)?;
    wait_for("A's correction", || a.presence().is_some_and(|p| p.corrections == 1))?;
    let seen = b.sees(pa.entity_id).ok_or("B lost A")?;
    if (seen.x - x).abs() > 0.01 {
        return Err(format!("B saw the illegal jump: x = {}", seen.x));
    }
    println!("speed hack corrected; B still sees A at x = {:.1}", seen.x);

    a.conn.disconnect().map_err(|e| e.to_string())?;
    wait_for("A to despawn for B", || b.sees(pa.entity_id).is_none())?;
    println!("A left and B saw it go");
    b.conn.disconnect().map_err(|e| e.to_string())?;
    Ok(())
}

/// The world tick runs at about 10 Hz with a sane `dt`, and its timings can be read from the host.
fn tick(args: &Args) -> Check {
    // Connecting starts the tick on a database published before it existed.
    let bot = Bot::connect(args, "T")?;
    let applied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = applied.clone();
    bot.conn
        .subscription_builder()
        .on_applied(move |_| flag.store(true, std::sync::atomic::Ordering::SeqCst))
        .on_error(|_, e| eprintln!("subscription error: {e}"))
        .subscribe(["SELECT * FROM dev_tick"]);
    wait_for("subscription", || applied.load(std::sync::atomic::Ordering::SeqCst))?;
    let ticks = || bot.conn.db.dev_tick().iter().next().map(|s| (s.ticks, s.last_dt));
    wait_for("the first tick", || ticks().is_some())?;

    let (start, _) = ticks().unwrap();
    let begun = Instant::now();
    std::thread::sleep(Duration::from_secs(3));
    let (end, last_dt) = ticks().unwrap();
    let hz = (end - start) as f64 / begun.elapsed().as_secs_f64();
    println!("tick rate {hz:.1} Hz, last dt {last_dt:.3} s");
    if !(8.0..=12.0).contains(&hz) {
        return Err(format!("tick rate {hz:.1} Hz, expected about 10"));
    }
    if !(0.05..=sim::tick::MAX_DT).contains(&last_dt) {
        return Err(format!("last dt {last_dt} s, expected about 0.1"));
    }

    let report = metrics::read(&args.uri, &args.db)?;
    print!("{report}");
    if report.tick.count < 1.0 {
        return Err("the host reports no world_tick timings".into());
    }
    bot.conn.disconnect().map_err(|e| e.to_string())
}

/// One bot walks 290 m east through five cell borders, zigzagging across a row border all the way,
/// then paces back and forth over the last column border. Bots standing along the route must stay
/// in view the whole time, one standing out of reach must never appear, and the window must move
/// exactly once per border crossed (docs/design/interest.md).
fn cross_cells(args: &Args) -> Check {
    const START: (f32, f32) = (-150.0, 0.0);
    const END_X: f32 = 140.0;
    /// The window reaches at least 2 cells less the margin past the player: 120 m. The rest is
    /// slack for subscription latency.
    const MUST_SEE_M: f32 = 110.0;
    // Bots along the route, then one beyond the window's reach in z.
    let spots: [(&'static str, f32, f32); 6] = [
        ("W1", -150.0, 40.0),
        ("W2", -60.0, -40.0),
        ("W3", 20.0, 60.0),
        ("W4", 100.0, -50.0),
        ("W5", 150.0, 30.0),
        ("FAR", 0.0, 250.0),
    ];
    let mut standing = Vec::new();
    for (label, x, z) in spots {
        let bot = Bot::connect(args, label)?;
        bot.subscribe_self()?;
        let p = bot.arrive()?;
        bot.teleport(x, z)?;
        standing.push((bot, p.entity_id, x, z));
    }
    let far = standing.last().unwrap().1;

    let mut w = Bot::connect(args, "walker")?;
    w.subscribe_self()?;
    w.arrive()?;
    w.teleport(START.0, START.1)?;
    w.watch_near(START.0, START.1)?;

    // Zigzag ±4 m across z = 0 every 32 m, so the row border is crossed again and again inside the
    // margin; then pace over x = 128 to 6 m past it and back, twice.
    let mut route: Vec<(f32, f32)> = (1..=9).map(|i| (START.0 + 32.0 * i as f32, if i % 2 == 1 { 4.0 } else { -4.0 })).collect();
    route.push((END_X, 0.0));
    for _ in 0..2 {
        route.extend([(122.0, 0.0), (134.0, 0.0)]);
    }

    let stride = RUN_SPEED * HEARTBEAT.as_secs_f32() * 0.95;
    let (mut x, mut z) = START;
    let mut checks = 0u32;
    for (tx, tz) in route {
        loop {
            let (dx, dz) = (tx - x, tz - z);
            let d = (dx * dx + dz * dz).sqrt();
            if d < 1e-3 {
                break;
            }
            // Look the whole time between heartbeats, not just at them.
            for _ in 0..10 {
                std::thread::sleep(HEARTBEAT / 10);
                for (bot, id, sx, sz) in &standing {
                    let seen = w.sees(*id).is_some();
                    let near = (sx - x).abs().max((sz - z).abs()) <= MUST_SEE_M;
                    if *id == far && seen {
                        return Err(format!("saw {} from ({x:.1}, {z:.1}), out of reach", bot.label));
                    }
                    if *id != far && near && !seen {
                        return Err(format!(
                            "lost sight of {} from ({x:.1}, {z:.1}), window on {:?}",
                            bot.label,
                            w.near.center().map(sim::cell::unpack)
                        ));
                    }
                    checks += 1;
                }
            }
            let k = (stride / d).min(1.0);
            (x, z) = (x + dx * k, z + dz * k);
            w.step(x, z)?;
        }
    }
    println!("walked to ({x:.1}, {z:.1}) with {checks} sightline checks and no gap");

    // Borders at x = -128, -64, 0, 64 and 128; none of the z = 0 or pacing crossings count.
    let recenters = w.near.recenters();
    if recenters != 5 {
        return Err(format!("the window moved {recenters} times, expected 5"));
    }
    wait_for("the old cells to be dropped", || w.near.settled())?;
    let w1 = standing[0].1;
    wait_for("W1 to drop out of view", || w.sees(w1).is_none())?;
    let window = sim::cell::window_cells(w.near.center().unwrap(), 2);
    if let Some(t) = w.conn.db.transform().iter().find(|t| !window.contains(&t.cell)) {
        return Err(format!("still holding entity {} outside the window", t.entity_id));
    }
    let corrections = w.presence().map_or(0, |p| p.corrections);
    if corrections != 0 {
        return Err(format!("{corrections} corrections on an honest walk"));
    }
    println!("the window moved {recenters} times, once per border; W1 dropped out behind");

    for bot in std::iter::once(&w).chain(standing.iter().map(|s| &s.0)) {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}
