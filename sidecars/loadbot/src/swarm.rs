//! `loadbot swarm`: many bots at once, for the throughput spike (docs/spikes/throughput.md).
//!
//! Every bot has its own connection, and all of them run as tasks on one tokio runtime. Each
//! teleports to a random spot in a square, then runs about with heartbeats, its Near tier kept
//! around it by a `Director`. A share of them are "in combat": they move twice as often and
//! swing at the nearest bot in reach once a second, using the dev-only stand-in for combat.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use spacetimedb_sdk::{Compression, DbContext, EventTable as _, Table};

use crate::module_bindings::*;
use crate::director::Director;
use crate::{Args, Check, RUN_SPEED, metrics, name_for};

pub struct SwarmArgs {
    pub bots: usize,
    /// Share of bots in combat, 0 to 1.
    pub combat: f32,
    /// Side of the square the bots spread over, in metres, centred on the origin.
    pub area: f32,
    /// How long to measure, after everyone is in and a warm-up.
    pub secs: u64,
    /// What the server should compress large messages with. Gzip, like the real client (ADR 0011);
    /// the others are there to compare.
    pub compression: Compression,
}

impl Default for SwarmArgs {
    fn default() -> Self {
        Self { bots: 100, combat: 0.3, area: 1000.0, secs: 60, compression: Compression::Gzip }
    }
}

const HEARTBEAT: Duration = Duration::from_millis(500);
const COMBAT_HEARTBEAT: Duration = Duration::from_millis(250);
const SWING_EVERY: Duration = Duration::from_secs(1);
/// A little short of the server's 25 m, since the target keeps moving.
const SWING_REACH_M: f32 = 20.0;
const WARMUP: Duration = Duration::from_secs(10);
/// Bots connect at this rate, so the server isn't hit by all of them in the same instant.
const CONNECTS_PER_SEC: u64 = 50;
const BOT_WAIT: Duration = Duration::from_secs(60);

/// Counters every bot writes to.
#[derive(Default)]
struct Shared {
    ready: AtomicUsize,
    failed: AtomicUsize,
    measuring: AtomicBool,
    stop: AtomicBool,
    /// Call to completion notice for reducers called while measuring, in µs.
    latencies_us: Mutex<Vec<u32>>,
    reducer_errors: AtomicU64,
    swings: AtomicU64,
    combat_events_seen: AtomicU64,
    resubscribes: AtomicU64,
}

/// A tiny xorshift generator, so each bot wanders differently without another dependency.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

async fn wait_until(what: &str, mut cond: impl FnMut() -> bool) -> Check {
    let start = Instant::now();
    while start.elapsed() < BOT_WAIT {
        if cond() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("timed out waiting for {what}"))
}

/// The Near tier's tables, including combat events (PLAN §4.3).
const NEAR_TABLES: &[&str] = &["entity", "transform", "combat_event"];

/// Records the call-to-completion time of a reducer called now, if we're measuring.
fn timer(shared: &Arc<Shared>) -> impl FnOnce(bool) + Send + 'static {
    let shared = shared.clone();
    let start = Instant::now();
    let measuring = shared.measuring.load(SeqCst);
    move |ok| {
        if !ok {
            shared.reducer_errors.fetch_add(1, SeqCst);
        }
        if measuring {
            shared.latencies_us.lock().unwrap().push(start.elapsed().as_micros().min(u32::MAX as u128) as u32);
        }
    }
}

/// Returns how many corrections the bot got.
async fn run_bot(i: usize, args: Arc<Args>, sw: Arc<SwarmArgs>, shared: Arc<Shared>) -> Result<u32, String> {
    let fighter = (i as f32) < sw.bots as f32 * sw.combat;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (i as u64 + 1).wrapping_mul(0x2545_f491_4f6c_dd1d));
    let conn = Arc::new(
        DbConnection::builder()
            .with_uri(args.uri.as_str())
            .with_database_name(args.db.as_str())
            .with_compression(sw.compression)
            .build()
            .map_err(|e| format!("bot {i}: connect failed: {e}"))?,
    );
    let driver = conn.clone();
    tokio::spawn(async move {
        let _ = driver.run_async().await;
    });
    wait_until("identity", || conn.try_identity().is_some()).await?;

    // Arrive, with only our own rows subscribed for now.
    let applied = Arc::new(AtomicBool::new(false));
    let flag = applied.clone();
    conn.subscription_builder()
        .on_applied(move |_| flag.store(true, SeqCst))
        .subscribe(["SELECT * FROM my_characters", "SELECT * FROM my_presence", "SELECT * FROM character_public", "SELECT * FROM terrain_info"]);
    wait_until("own subscription", || applied.load(SeqCst)).await?;
    conn.reducers.create_character(name_for(conn.identity())).map_err(|e| e.to_string())?;
    wait_until("character", || conn.db.my_characters().count() > 0).await?;
    let char_id = conn.db.my_characters().iter().next().unwrap().char_id;
    let terrain = conn.db.terrain_info().iter().next().map_or(0, |t| t.hash);
    conn.reducers.enter_world(char_id, sim::content::CONTENT_HASH, terrain).map_err(|e| e.to_string())?;
    wait_until("presence", || conn.db.my_presence().count() > 0).await?;
    let me = conn.db.my_presence().iter().next().unwrap().entity_id;

    let half = sw.area / 2.0;
    let (mut x, mut z) = ((rng.unit() * 2.0 - 1.0) * half, (rng.unit() * 2.0 - 1.0) * half);
    let teleported = Arc::new(AtomicBool::new(false));
    let flag = teleported.clone();
    conn.reducers
        .dev_teleport_then(x, z, move |_, r| flag.store(matches!(r, Ok(Ok(()))), SeqCst))
        .map_err(|e| e.to_string())?;
    wait_until("teleport", || teleported.load(SeqCst)).await?;

    let near = Director::new(NEAR_TABLES, 2);
    near.update(&conn, x, z);
    wait_until("Near tier", || near.ready()).await?;
    {
        let shared = shared.clone();
        conn.db.combat_event().on_insert(move |_, _| {
            shared.combat_events_seen.fetch_add(1, SeqCst);
        });
    }
    shared.ready.fetch_add(1, SeqCst);

    let every = if fighter { COMBAT_HEARTBEAT } else { HEARTBEAT };
    let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut heading = rng.unit() * std::f32::consts::TAU;
    let mut last_swing = Instant::now();
    let mut last_step = Instant::now();
    let mut seq = 0u32;
    let mut corrections = 0;
    while !shared.stop.load(SeqCst) {
        ticker.tick().await;
        // Like a real client: when the server overrules us, snap to where it says we are.
        if let Some(p) = conn.db.my_presence().iter().next()
            && p.corrections != corrections
        {
            corrections = p.corrections;
            (x, z) = (p.x, p.z);
        }
        // Wander: turn a little each step, and head back in when outside the square.
        heading += (rng.unit() - 0.5) * 0.6;
        if x.abs() > half || z.abs() > half {
            heading = (-z).atan2(-x);
        }
        // Stride by the time that really passed, as a client predicting at frame rate would.
        let stride = RUN_SPEED * 0.95 * last_step.elapsed().as_secs_f32().min(1.0);
        last_step = Instant::now();
        x += heading.cos() * stride;
        z += heading.sin() * stride;
        seq += 1;
        let yaw = ((heading.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU) * 65535.0) as u16;
        let done = timer(&shared);
        conn.reducers
            .move_event_then(seq, 0, x, 0.0, z, yaw, (RUN_SPEED * 100.0) as i16, 1, move |_, r| done(matches!(r, Ok(Ok(())))))
            .map_err(|e| e.to_string())?;

        if near.update(&conn, x, z) {
            shared.resubscribes.fetch_add(1, SeqCst);
        }

        if fighter && last_swing.elapsed() >= SWING_EVERY {
            let target = conn
                .db
                .transform()
                .iter()
                .filter(|t| t.entity_id != me)
                .map(|t| (t.entity_id, (t.x - x).powi(2) + (t.z - z).powi(2)))
                .filter(|&(_, d2)| d2 <= SWING_REACH_M * SWING_REACH_M)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((target, _)) = target {
                last_swing = Instant::now();
                shared.swings.fetch_add(1, SeqCst);
                let done = timer(&shared);
                conn.reducers
                    .dev_swing_then(target, move |_, r| done(matches!(r, Ok(Ok(())))))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let corrections = conn.db.my_presence().iter().next().map_or(0, |p| p.corrections);
    let _ = conn.disconnect();
    Ok(corrections)
}

/// One reading of everything measured from outside the bots.
struct Snapshot {
    at: Instant,
    host: metrics::Report,
    /// Bytes received on each of our connections, by local port.
    sockets: HashMap<u16, u64>,
    server: Option<ProcStats>,
    us: Option<ProcStats>,
}

fn snapshot(args: &Args) -> Result<Snapshot, String> {
    Ok(Snapshot {
        at: Instant::now(),
        host: metrics::read(&args.uri, &args.db)?,
        sockets: socket_bytes(std::process::id()),
        server: server_pid().and_then(proc_stats),
        us: proc_stats(std::process::id()),
    })
}

pub fn swarm(args: Args, sw: SwarmArgs) -> Check {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| e.to_string())?;
    let args = Arc::new(args);
    let sw = Arc::new(sw);
    let shared = Arc::new(Shared::default());
    let idle = server_pid().and_then(proc_stats);
    let fighters = (sw.bots as f32 * sw.combat).ceil() as usize;
    println!("swarm: {} bots ({fighters} in combat) over {} m, measuring {} s", sw.bots, sw.area, sw.secs);

    let started = Instant::now();
    let bots: Vec<_> = (0..sw.bots)
        .map(|i| {
            std::thread::sleep(Duration::from_millis(1000 / CONNECTS_PER_SEC));
            let (args, sw, shared) = (args.clone(), sw.clone(), shared.clone());
            rt.spawn(async move {
                let r = run_bot(i, args, sw, shared.clone()).await;
                if let Err(e) = &r {
                    shared.failed.fetch_add(1, SeqCst);
                    eprintln!("bot {i}: {e}");
                }
                r
            })
        })
        .collect();
    while shared.ready.load(SeqCst) + shared.failed.load(SeqCst) < sw.bots {
        if started.elapsed() > BOT_WAIT * 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let ready = shared.ready.load(SeqCst);
    println!("{ready} bots in the world after {:.0} s; warming up", started.elapsed().as_secs_f32());
    if ready < sw.bots * 99 / 100 {
        shared.stop.store(true, SeqCst);
        return Err(format!("only {ready} of {} bots got in", sw.bots));
    }
    std::thread::sleep(WARMUP);

    let a = snapshot(&args)?;
    shared.measuring.store(true, SeqCst);
    std::thread::sleep(Duration::from_secs(sw.secs));
    shared.measuring.store(false, SeqCst);
    let b = snapshot(&args)?;

    shared.stop.store(true, SeqCst);
    let corrections: u32 = rt.block_on(async {
        let mut sum = 0;
        for h in bots {
            if let Ok(Ok(c)) = h.await {
                sum += c;
            }
        }
        sum
    });

    let secs = (b.at - a.at).as_secs_f64();
    let host = b.host.since(&a.host);
    print!("{host}");
    let busy: Vec<String> =
        host.txn_cpu.iter().filter(|(_, t)| **t > 0.0).map(|(k, t)| format!("{k} {:.1}%", t / secs * 100.0)).collect();
    println!(
        "database busy {:.1}% (CPU of all transactions per second: {})",
        host.txn_cpu.values().sum::<f64>() / secs * 100.0,
        busy.join(", ")
    );

    let mut lat = std::mem::take(&mut *shared.latencies_us.lock().unwrap());
    lat.sort_unstable();
    let pct = |v: &[u32], q: f64| v.get(((v.len() as f64 * q) as usize).min(v.len().saturating_sub(1))).copied();
    let ms = |us: Option<u32>| us.map_or("n/a".into(), |u| format!("{:.1} ms", u as f64 / 1000.0));
    println!(
        "reducer latency (call to completion notice, {} calls): p50 {}, p99 {}, max {}",
        lat.len(),
        ms(pct(&lat, 0.5)),
        ms(pct(&lat, 0.99)),
        ms(lat.last().copied())
    );

    let mut rates: Vec<f64> = b
        .sockets
        .iter()
        .filter_map(|(port, after)| a.sockets.get(port).map(|before| (after - before) as f64 / secs))
        .collect();
    rates.sort_by(f64::total_cmp);
    let kb = |r: Option<&f64>| r.map_or("n/a".into(), |r| format!("{:.1} KB/s", r / 1000.0));
    if rates.is_empty() {
        println!("bytes per client: n/a (needs `ss` on Linux)");
    } else {
        let mean = rates.iter().sum::<f64>() / rates.len() as f64;
        let at = |q: f64| rates.get(((rates.len() as f64 * q) as usize).min(rates.len() - 1));
        println!(
            "received per client ({} connections): mean {}, p50 {}, p99 {}, max {}",
            rates.len(),
            kb(Some(&mean)),
            kb(at(0.5)),
            kb(at(0.99)),
            kb(rates.last())
        );
    }

    let cpu = |a: &Option<ProcStats>, b: &Option<ProcStats>| match (a, b) {
        (Some(a), Some(b)) => format!("{:.2} cores", (b.cpu_secs - a.cpu_secs) / secs),
        _ => "n/a".into(),
    };
    let mb = |p: &Option<ProcStats>| p.as_ref().map_or("n/a".into(), |p| format!("{:.0} MB", p.rss_kb as f64 / 1024.0));
    println!("server CPU {}, RSS {} (idle {})", cpu(&a.server, &b.server), mb(&b.server), mb(&idle));
    if let (Some(idle), Some(now)) = (&idle, &b.server) {
        println!("server RSS per bot ≈ {:.2} MB", (now.rss_kb as f64 - idle.rss_kb as f64) / 1024.0 / ready as f64);
    }
    println!("loadbot CPU {}, RSS {}", cpu(&a.us, &b.us), mb(&b.us));
    println!(
        "swings {}, combat events seen {}, window moves {}, reducer errors {}, corrections {corrections}",
        shared.swings.load(SeqCst),
        shared.combat_events_seen.load(SeqCst),
        shared.resubscribes.load(SeqCst),
        shared.reducer_errors.load(SeqCst),
    );

    let tick_p99 = host.tick.quantile_bound(0.99);
    let lat_p99 = pct(&lat, 0.99).map_or(f64::INFINITY, |u| u as f64 / 1e6);
    let worst_rate = rates.last().copied().unwrap_or(0.0);
    let verdict = |ok: bool| if ok { "go" } else { "NO-GO" };
    println!(
        "verdict: tick p99 ≤ 20 ms: {} · reducer p99 ≤ 100 ms: {} · busiest client ≤ 40 KB/s: {}",
        verdict(tick_p99 <= 0.02),
        verdict(lat_p99 <= 0.1),
        verdict(worst_rate <= 40_000.0),
    );
    rt.shutdown_timeout(Duration::from_secs(5));
    Ok(())
}

// ── Linux process and socket probes; elsewhere they report nothing ──

struct ProcStats {
    cpu_secs: f64,
    rss_kb: u64,
}

/// The local SpacetimeDB server (its process name is cut to 15 characters).
fn server_pid() -> Option<u32> {
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|d| {
        let pid: u32 = d.file_name().to_str()?.parse().ok()?;
        let comm = std::fs::read_to_string(d.path().join("comm")).ok()?;
        comm.trim().starts_with("spacetimedb-sta").then_some(pid)
    })
}

fn proc_stats(pid: u32) -> Option<ProcStats> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the parenthesised name; utime and stime are fields 14 and 15 overall.
    let rest: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let ticks: f64 = rest.get(11)?.parse::<f64>().ok()? + rest.get(12)?.parse::<f64>().ok()?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kb = status.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse().ok()?;
    // USER_HZ is 100 on every mainstream Linux.
    Some(ProcStats { cpu_secs: ticks / 100.0, rss_kb })
}

/// Bytes received so far on each of `pid`'s TCP connections, by local port, from `ss -tinp`.
fn socket_bytes(pid: u32) -> HashMap<u16, u64> {
    let Ok(out) = std::process::Command::new("ss").args(["-tinpH"]).output() else { return HashMap::new() };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut found = HashMap::new();
    let mut port = None;
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            // A socket line: state, queues, local address, peer address, users.
            let ours = line.contains(&format!("pid={pid},"));
            port = line
                .split_whitespace()
                .nth(3)
                .and_then(|local| local.rsplit_once(':'))
                .and_then(|(_, p)| p.parse::<u16>().ok())
                .filter(|_| ours);
        } else if let Some(p) = port.take()
            && let Some(n) = line.split_whitespace().find_map(|f| f.strip_prefix("bytes_received:"))
        {
            found.insert(p, n.parse().unwrap_or(0));
        }
    }
    found
}
