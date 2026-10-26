//! Headless bots that play scripted scenarios against a running server and check what they see.
//!
//! `cargo run -p loadbot -- [--uri URI] [--db NAME] <scenario>`; scenarios are listed by `help`.
//! `swarm` also takes `--bots N --combat SHARE --area METRES --secs S [--compression none|gzip|brotli]`.
//! Exits non-zero when a check fails.

#[allow(clippy::all, unused)]
mod module_bindings;
mod metrics;
mod swarm;

use std::time::{Duration, Instant};

use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table};

const RUN_SPEED: f32 = 5.4;
const HEARTBEAT: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_secs(5);

struct Args {
    uri: String,
    db: String,
    scenario: String,
}

fn parse_args() -> (Args, swarm::SwarmArgs) {
    let mut a = Args { uri: "http://127.0.0.1:3000".into(), db: "mmo-world".into(), scenario: "help".into() };
    let mut sw = swarm::SwarmArgs::default();
    let mut it = std::env::args().skip(1);
    fn num<T: std::str::FromStr>(flag: &str, v: Option<String>) -> T {
        v.and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("{flag} needs a number"))
    }
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--uri" => a.uri = it.next().expect("--uri needs a value"),
            "--db" => a.db = it.next().expect("--db needs a value"),
            "--bots" => sw.bots = num("--bots", it.next()),
            "--combat" => sw.combat = num("--combat", it.next()),
            "--area" => sw.area = num("--area", it.next()),
            "--secs" => sw.secs = num("--secs", it.next()),
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
    (a, sw)
}

fn main() {
    let (args, sw) = parse_args();
    let scenario = args.scenario.clone();
    let result = match scenario.as_str() {
        "see-each-other" => see_each_other(&args),
        "tick" => tick(&args),
        "metrics" => metrics::read(&args.uri, &args.db).map(|r| print!("{r}")),
        "swarm" => swarm::swarm(args, sw),
        _ => {
            eprintln!(
                "scenarios:\n  \
                 see-each-other   two bots enter, see each other move, get corrected, leave\n  \
                 tick             the world tick runs at 10 Hz and its timings can be read\n  \
                 metrics          print the server's tick timings and reducer counts\n  \
                 swarm            the throughput spike: many bots moving and fighting, measured"
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
        Ok(Self { label, conn, seq: 0 })
    }

    fn identity(&self) -> Identity {
        self.conn.identity()
    }

    /// Subscribe to our private views plus the Near tier around `cell` (PLAN §4.3).
    fn subscribe_near(&self, cell: u64) -> Check {
        let mut queries = vec![
            "SELECT * FROM my_characters".to_string(),
            "SELECT * FROM my_presence".to_string(),
            "SELECT * FROM character_public".to_string(),
        ];
        for c in sim::cell::window_cells(cell, 2) {
            queries.push(format!("SELECT * FROM entity WHERE cell = {c}"));
            queries.push(format!("SELECT * FROM transform WHERE cell = {c}"));
        }
        let applied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = applied.clone();
        self.conn
            .subscription_builder()
            .on_applied(move |_| flag.store(true, std::sync::atomic::Ordering::SeqCst))
            .on_error(|_, e| eprintln!("subscription error: {e}"))
            .subscribe(queries);
        wait_for("subscription", || applied.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Create a character named after our identity and enter the world with it.
    fn arrive(&self) -> Result<Presence, String> {
        let name = name_for(self.identity());
        self.conn.reducers.create_character(name.clone()).map_err(|e| e.to_string())?;
        wait_for("character", || self.conn.db.my_characters().count() > 0)?;
        let char_id = self.conn.db.my_characters().iter().next().unwrap().char_id;
        self.conn.reducers.enter_world(char_id).map_err(|e| e.to_string())?;
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
            .move_event(self.seq, x, 0.0, z, 0, (RUN_SPEED * 100.0) as i16, 1)
            .map_err(|e| e.to_string())
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
    let spawn = sim::cell::cell_of(sim::cell::OPEN_WORLD, 0.0, 0.0);
    let mut a = Bot::connect(args, "A")?;
    let b = Bot::connect(args, "B")?;
    a.subscribe_near(spawn)?;
    b.subscribe_near(spawn)?;
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
