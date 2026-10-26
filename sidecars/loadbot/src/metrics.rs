//! Server timings, read from the host's Prometheus endpoint (`/v1/metrics`). Reducers have no
//! clock, so SpacetimeDB's own per-reducer histograms are the source of truth (ADR 0010).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// A plain HTTP/1.0 GET, so the reply is never chunked. Only meant for the local dev server.
fn http_get(uri: &str, path: &str) -> Result<String, String> {
    let host = uri.strip_prefix("http://").ok_or("metrics need an http:// server URI")?.trim_end_matches('/');
    let mut s = TcpStream::connect(host).map_err(|e| format!("{host}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    write!(s, "GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    s.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    let (head, body) = reply.split_once("\r\n\r\n").ok_or("malformed HTTP reply")?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("GET {path}: {status}"));
    }
    Ok(body.to_string())
}

/// The database's identity in hex, as the metrics label it.
fn database_identity(uri: &str, db: &str) -> Result<String, String> {
    let body = http_get(uri, &format!("/v1/database/{db}"))?;
    let key = "\"database_identity\":{\"__identity__\":\"0x";
    let at = body.find(key).ok_or_else(|| format!("no database called {db}"))? + key.len();
    body.get(at..at + 64).map(str::to_string).ok_or_else(|| "short identity".into())
}

/// One Prometheus sample: `name{k="v",...} value`.
struct Sample {
    name: String,
    labels: BTreeMap<String, String>,
    value: f64,
}

fn parse(text: &str) -> Vec<Sample> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let (series, value) = l.rsplit_once(' ')?;
            let value = value.parse().ok()?;
            let (name, labels) = match series.split_once('{') {
                Some((n, rest)) => (n, rest.strip_suffix('}')?),
                None => (series, ""),
            };
            let labels = labels
                .split("\",")
                .filter_map(|kv| {
                    let (k, v) = kv.split_once("=\"")?;
                    Some((k.to_string(), v.trim_end_matches('"').to_string()))
                })
                .collect();
            Some(Sample { name: name.to_string(), labels, value })
        })
        .collect()
}

/// A Prometheus histogram: cumulative counts at each upper bound, plus the sum and count.
#[derive(Default)]
pub struct Histogram {
    buckets: Vec<(f64, f64)>,
    pub sum: f64,
    pub count: f64,
}

impl Histogram {
    pub fn mean(&self) -> f64 {
        if self.count > 0.0 { self.sum / self.count } else { 0.0 }
    }

    /// The upper bound of the bucket holding quantile `q`: the true value is at most this.
    /// Infinite if it falls past the last finite bound.
    pub fn quantile_bound(&self, q: f64) -> f64 {
        let want = q * self.count;
        self.buckets.iter().find(|(_, cum)| *cum >= want).map_or(f64::INFINITY, |(le, _)| *le)
    }
}

/// Timings and call counts for one database, as of when they were read.
pub struct Report {
    /// The tick's own CPU time.
    pub tick: Histogram,
    /// CPU time spent after each tick working out what subscribers should receive.
    pub tick_update: Histogram,
    /// How late the scheduler started each tick.
    pub tick_delay: Histogram,
    /// Reducer name → (calls, mean CPU time in seconds).
    pub reducers: BTreeMap<String, (f64, f64)>,
}

pub fn read(uri: &str, db: &str) -> Result<Report, String> {
    let id = database_identity(uri, db)?;
    let samples = parse(&http_get(uri, "/v1/metrics")?);
    let ours = |s: &&Sample| s.labels.get("db") == Some(&id);
    let histogram = |base: &str, keep: &dyn Fn(&Sample) -> bool| {
        let mut h = Histogram::default();
        for s in samples.iter().filter(ours).filter(|s| keep(s)) {
            match s.name.strip_prefix(base) {
                Some("_bucket") => {
                    let le = s.labels.get("le").map_or(f64::INFINITY, |v| v.parse().unwrap_or(f64::INFINITY));
                    h.buckets.push((le, s.value));
                }
                Some("_sum") => h.sum += s.value,
                Some("_count") => h.count += s.value,
                _ => {}
            }
        }
        h.buckets.sort_by(|a, b| a.0.total_cmp(&b.0));
        h
    };
    let label = |s: &Sample, k: &str, v: &str| s.labels.get(k).is_some_and(|x| x == v);
    let cpu = "spacetime_txn_cpu_time_sec";
    let tick = histogram(cpu, &|s| label(s, "reducer", "world_tick") && label(s, "txn_type", "Reducer"));
    let tick_update = histogram(cpu, &|s| label(s, "reducer", "world_tick") && label(s, "txn_type", "Update"));
    let tick_delay = histogram("spacetime_scheduled_function_delay_seconds", &|s| label(s, "function", "world_tick"));

    let mut reducers = BTreeMap::new();
    for s in samples.iter().filter(ours) {
        if s.name == format!("{cpu}_count") && label(s, "txn_type", "Reducer") {
            let name = s.labels.get("reducer").cloned().unwrap_or_default();
            let sum = samples
                .iter()
                .filter(ours)
                .find(|x| x.name == format!("{cpu}_sum") && x.labels == s.labels)
                .map_or(0.0, |x| x.value);
            if s.value > 0.0 && !name.is_empty() {
                reducers.insert(name, (s.value, sum / s.value));
            }
        }
    }
    Ok(Report { tick, tick_update, tick_delay, reducers })
}

fn fmt_secs(s: f64) -> String {
    if s.is_infinite() {
        "over the last bucket".into()
    } else if s < 1e-3 {
        format!("{:.0} µs", s * 1e6)
    } else {
        format!("{:.1} ms", s * 1e3)
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = &self.tick;
        writeln!(
            f,
            "world_tick: {} ticks, CPU mean {} (+{} subscription update), p50 ≤ {}, p99 ≤ {}",
            t.count,
            fmt_secs(t.mean()),
            fmt_secs(self.tick_update.mean()),
            fmt_secs(t.quantile_bound(0.5)),
            fmt_secs(t.quantile_bound(0.99)),
        )?;
        writeln!(
            f,
            "scheduler delay: mean {}, p99 ≤ {}",
            fmt_secs(self.tick_delay.mean()),
            fmt_secs(self.tick_delay.quantile_bound(0.99))
        )?;
        writeln!(f, "{:<24} {:>10} {:>12}", "reducer", "calls", "mean CPU")?;
        for (name, (calls, mean)) in &self.reducers {
            writeln!(f, "{name:<24} {calls:>10} {:>12}", fmt_secs(*mean))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"# HELP x
spacetime_txn_cpu_time_sec_bucket{db="ab",reducer="world_tick",txn_type="Reducer",le="0.0001"} 90
spacetime_txn_cpu_time_sec_bucket{db="ab",reducer="world_tick",txn_type="Reducer",le="0.001"} 99
spacetime_txn_cpu_time_sec_bucket{db="ab",reducer="world_tick",txn_type="Reducer",le="+Inf"} 100
spacetime_txn_cpu_time_sec_sum{db="ab",reducer="world_tick",txn_type="Reducer"} 0.02
spacetime_txn_cpu_time_sec_count{db="ab",reducer="world_tick",txn_type="Reducer"} 100
"#;

    fn tick_histogram() -> Histogram {
        let mut h = Histogram::default();
        for s in parse(TEXT) {
            match s.name.as_str() {
                "spacetime_txn_cpu_time_sec_bucket" => {
                    h.buckets.push((s.labels["le"].parse().unwrap_or(f64::INFINITY), s.value))
                }
                "spacetime_txn_cpu_time_sec_sum" => h.sum = s.value,
                "spacetime_txn_cpu_time_sec_count" => h.count = s.value,
                _ => {}
            }
        }
        h
    }

    #[test]
    fn parses_labels_and_values() {
        let s = parse(TEXT);
        assert_eq!(s.len(), 5);
        assert_eq!(s[0].labels["reducer"], "world_tick");
        assert_eq!(s[0].labels["le"], "0.0001");
        assert_eq!(s[2].labels["le"], "+Inf");
        assert_eq!(s[4].value, 100.0);
    }

    #[test]
    fn quantiles_report_the_bucket_bound() {
        let h = tick_histogram();
        assert_eq!(h.quantile_bound(0.5), 0.0001);
        assert_eq!(h.quantile_bound(0.99), 0.001);
        assert_eq!(h.quantile_bound(1.0), f64::INFINITY);
        assert!((h.mean() - 0.0002).abs() < 1e-12);
    }
}
