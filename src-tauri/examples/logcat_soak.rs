//! Logcat soak measurement.
//!
//! Feeds synthetic logcat through the real stream (`request_start` → reader →
//! pipeline → store) for a fixed time and prints one JSON report: RSS over
//! time, entries ingested and dropped, backlog, and pipeline batch latency
//! percentiles. No device is used: this binary re-runs itself as a fake `adb`
//! that answers `shell ps` and streams generated `logcat` lines at a fixed rate.
//!
//! ```text
//! cargo run --release --example logcat_soak -- [--rate 1000] [--duration-secs 600]
//!     [--sample-secs 10] [--giant-line-mb 10]
//! ```
//!
//! `npm run perf:soak` builds it, runs it, and adds build provenance.

use keynobi_lib::services::logcat::{
    request_start, request_stop, LogcatState, LogcatStateInner, PIPELINE_BATCH_TRACE_TARGET,
};
use keynobi_lib::services::settings_manager;
use serde_json::json;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{filter::Targets, Layer};

const RATE_ENV: &str = "KEYNOBI_SOAK_RATE";
const GIANT_LINE_ENV: &str = "KEYNOBI_SOAK_GIANT_LINE_BYTES";
const GIANT_TAG: &str = "SoakGiant";
/// Apps the fake device runs; one of them dies and restarts every second.
const APP_COUNT: usize = 20;
const SYSTEM_SERVER_PID: u32 = 1500;
const FIRST_APP_PID: u32 = 10_000;
const PACE: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
struct Config {
    rate: u64,
    duration: Duration,
    sample_every: Duration,
    giant_line_bytes: usize,
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config {
        rate: 1_000,
        duration: Duration::from_secs(600),
        sample_every: Duration::from_secs(10),
        giant_line_bytes: 10 * 1024 * 1024,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number: u64 = value
            .parse()
            .map_err(|_| format!("{flag}: not a number: {value}"))?;
        match flag.as_str() {
            "--rate" => config.rate = number.max(1),
            "--duration-secs" => config.duration = Duration::from_secs(number.max(1)),
            "--sample-secs" => config.sample_every = Duration::from_secs(number.max(1)),
            "--giant-line-mb" => config.giant_line_bytes = (number as usize) * 1024 * 1024,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    Ok(config)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "shell") {
        fake_ps();
        return;
    }
    if args.iter().any(|a| a == "logcat") {
        fake_logcat();
        return;
    }

    let config = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("logcat_soak: {e}");
            std::process::exit(2);
        }
    };
    // The fake adb child reads its settings from the environment. Set before
    // any thread starts.
    std::env::set_var(RATE_ENV, config.rate.to_string());
    std::env::set_var(GIANT_LINE_ENV, config.giant_line_bytes.to_string());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let report = runtime.block_on(soak(config));
    println!("{report:#}");
}

// ── Fake adb ──────────────────────────────────────────────────────────────────

fn app_package(index: usize) -> String {
    format!("com.soak.app{index}")
}

fn fake_ps() {
    println!("PID NAME");
    for i in 0..APP_COUNT {
        println!("{} {}", FIRST_APP_PID + i as u32, app_package(i));
    }
}

/// Small deterministic generator; the soak must be repeatable run to run.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct Generator {
    rng: XorShift,
    seq: u64,
    app_pids: Vec<u32>,
    next_pid: u32,
    next_restart: usize,
}

impl Generator {
    fn new() -> Self {
        Generator {
            rng: XorShift(0x9E37_79B9_7F4A_7C15),
            seq: 0,
            app_pids: (0..APP_COUNT).map(|i| FIRST_APP_PID + i as u32).collect(),
            next_pid: FIRST_APP_PID + APP_COUNT as u32,
            next_restart: 0,
        }
    }

    fn timestamp() -> String {
        chrono::Local::now()
            .format("%m-%d %H:%M:%S%.3f")
            .to_string()
    }

    fn line(pid: u32, level: char, tag: &str, message: &str) -> String {
        format!(
            "{} {pid:5} {pid:5} {level} {tag}: {message}\n",
            Self::timestamp()
        )
    }

    /// One app dies and starts again on a new PID, as after a crash or an
    /// install. Without eviction the PID map would grow by one per call.
    fn restart_one(&mut self, out: &mut String) {
        let index = self.next_restart % APP_COUNT;
        self.next_restart += 1;
        let package = app_package(index);
        let old_pid = self.app_pids[index];
        let new_pid = self.next_pid;
        self.next_pid += 1;
        self.app_pids[index] = new_pid;
        out.push_str(&Self::line(
            SYSTEM_SERVER_PID,
            'I',
            "ActivityManager",
            &format!("Process {package} (pid {old_pid}) has died: cch CRE"),
        ));
        out.push_str(&Self::line(
            SYSTEM_SERVER_PID,
            'I',
            "ActivityManager",
            &format!("Start proc {new_pid}:{package}/u0a{index} for activity {{{package}/.Main}}"),
        ));
    }

    /// Append one ordinary line (occasionally a JSON body or a crash burst).
    fn ordinary(&mut self, out: &mut String) {
        self.seq += 1;
        let seq = self.seq;
        let pid = self.app_pids[self.rng.below(APP_COUNT as u64) as usize];
        if seq.is_multiple_of(10_000) {
            out.push_str(&Self::line(
                pid,
                'E',
                "AndroidRuntime",
                "FATAL EXCEPTION: main",
            ));
            out.push_str(&Self::line(
                pid,
                'E',
                "AndroidRuntime",
                "java.lang.IllegalStateException: soak crash",
            ));
            for frame in 0..5 {
                out.push_str(&Self::line(
                    pid,
                    'E',
                    "AndroidRuntime",
                    &format!("\tat com.soak.Frame{frame}.run(Frame.kt:{frame})"),
                ));
            }
            return;
        }
        if self.rng.below(50) == 0 {
            out.push_str(&Self::line(
                pid,
                'D',
                "OkHttp",
                &format!(r#"Response body: {{"seq":{seq},"items":[1,2,3],"ok":true}}"#),
            ));
            return;
        }
        const TAGS: [&str; 6] = [
            "MainActivity",
            "Choreographer",
            "RecyclerView",
            "OkHttp",
            "art",
            "SoakTag",
        ];
        const LEVELS: [char; 8] = ['V', 'D', 'D', 'D', 'I', 'I', 'W', 'E'];
        let tag = TAGS[self.rng.below(TAGS.len() as u64) as usize];
        let level = LEVELS[self.rng.below(LEVELS.len() as u64) as usize];
        let padding = "p".repeat(20 + self.rng.below(220) as usize);
        out.push_str(&Self::line(
            pid,
            level,
            tag,
            &format!("event seq={seq} {padding}"),
        ));
    }
}

fn fake_logcat() {
    let rate: u64 = std::env::var(RATE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000);
    let giant: usize = std::env::var(GIANT_LINE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::with_capacity(64 * 1024, stdout.lock());

    if giant > 0 {
        let head = format!(
            "{} {FIRST_APP_PID:5} {FIRST_APP_PID:5} I {GIANT_TAG}: ",
            Generator::timestamp()
        );
        let chunk = vec![b'x'; 64 * 1024];
        let mut written = 0;
        let mut ok = out.write_all(head.as_bytes()).is_ok();
        while ok && written < giant {
            let n = chunk.len().min(giant - written);
            ok = out.write_all(&chunk[..n]).is_ok();
            written += n;
        }
        if !ok || out.write_all(b"\n").is_err() {
            return;
        }
    }

    let mut generator = Generator::new();
    let started = Instant::now();
    let mut emitted: u64 = 0;
    let mut restarts: u64 = 0;
    let mut buf = String::new();
    loop {
        let elapsed = started.elapsed();
        let due = (elapsed.as_secs_f64() * rate as f64) as u64;
        while emitted < due {
            generator.ordinary(&mut buf);
            emitted += 1;
        }
        while restarts < elapsed.as_secs() {
            generator.restart_one(&mut buf);
            restarts += 1;
        }
        if out.write_all(buf.as_bytes()).is_err() || out.flush().is_err() {
            return;
        }
        buf.clear();
        let next = PACE.saturating_sub(started.elapsed().saturating_sub(elapsed));
        std::thread::sleep(next);
    }
}

// ── Measurement ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct BatchStats {
    latencies_us: Vec<u64>,
    rows_max: u64,
    rows_total: u64,
    backlog_max: u64,
    tracked_pids_max: u64,
    tracked_pids_last: u64,
}

#[derive(Default)]
struct BatchFields {
    rows: u64,
    elapsed_us: u64,
    backlog: u64,
    tracked_pids: u64,
}

impl Visit for BatchFields {
    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "rows" => self.rows = value,
            "elapsed_us" => self.elapsed_us = value,
            "backlog" => self.backlog = value,
            "tracked_pids" => self.tracked_pids = value,
            _ => {}
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

struct BatchRecorder(Arc<Mutex<BatchStats>>);

impl<S: Subscriber> Layer<S> for BatchRecorder {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = BatchFields::default();
        event.record(&mut fields);
        let Ok(mut stats) = self.0.lock() else {
            return;
        };
        stats.latencies_us.push(fields.elapsed_us);
        stats.rows_max = stats.rows_max.max(fields.rows);
        stats.rows_total += fields.rows;
        stats.backlog_max = stats.backlog_max.max(fields.backlog);
        stats.tracked_pids_max = stats.tracked_pids_max.max(fields.tracked_pids);
        stats.tracked_pids_last = fields.tracked_pids;
    }
}

fn rss_bytes(system: &mut sysinfo::System) -> u64 {
    let pid = sysinfo::Pid::from(std::process::id() as usize);
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), false);
    system.process(pid).map(|p| p.memory()).unwrap_or(0)
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

async fn soak(config: Config) -> serde_json::Value {
    // The stream never touches persisted data today; keep it that way even if
    // that changes.
    let data_dir = std::env::temp_dir().join(format!("keynobi-soak-{}", std::process::id()));
    settings_manager::set_data_dir_override(data_dir);

    let batches = Arc::new(Mutex::new(BatchStats::default()));
    let recorder = BatchRecorder(batches.clone()).with_filter(
        Targets::new().with_target(PIPELINE_BATCH_TRACE_TARGET, tracing::Level::TRACE),
    );
    let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(recorder));

    let mut system = sysinfo::System::new();
    let rss_start = rss_bytes(&mut system);
    let state: LogcatState = Arc::new(tokio::sync::Mutex::new(LogcatStateInner::new()));
    let adb = std::env::current_exe().expect("path of this binary");
    if let Err(e) = request_start(&state, adb, None, None).await {
        return json!({ "error": format!("logcat did not start: {e}") });
    }

    let started = Instant::now();
    let mut samples = Vec::new();
    let mut rss_peak = rss_start;
    let mut giant_line = serde_json::Value::Null;
    let mut next_sample = Duration::ZERO;
    while started.elapsed() < config.duration {
        if giant_line.is_null() && config.giant_line_bytes > 0 {
            let s = state.lock().await;
            if let Some(entry) = s.store.iter().find(|e| e.tag == GIANT_TAG) {
                giant_line = json!({
                    "sentBytes": config.giant_line_bytes,
                    "storedBytes": entry.message.len(),
                    "truncatedVisibly": entry.message.contains("… [truncated "),
                    "suffix": entry.message.chars().rev().take(40).collect::<String>()
                        .chars().rev().collect::<String>(),
                });
            }
        }
        if started.elapsed() >= next_sample {
            let rss = rss_bytes(&mut system);
            rss_peak = rss_peak.max(rss);
            let s = state.lock().await;
            samples.push(json!({
                "tSecs": started.elapsed().as_secs(),
                "rssBytes": rss,
                "ingested": s.store.stats.total_ingested,
                "dropped": s.store.stats.dropped_lines,
                "backlog": s.store.stats.backlog_lines,
                "stored": s.store.len(),
            }));
            next_sample += config.sample_every;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let elapsed = started.elapsed();
    let rss_end = rss_bytes(&mut system);
    rss_peak = rss_peak.max(rss_end);
    let (ingested, dropped, stored, packages_seen) = {
        let s = state.lock().await;
        (
            s.store.stats.total_ingested,
            s.store.stats.dropped_lines,
            s.store.len(),
            s.store.stats.packages_seen,
        )
    };
    samples.push(json!({
        "tSecs": elapsed.as_secs(),
        "rssBytes": rss_end,
        "ingested": ingested,
        "dropped": dropped,
        "stored": stored,
    }));
    request_stop(&state).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let stats = match batches.lock() {
        Ok(mut stats) => std::mem::take(&mut *stats),
        Err(_) => BatchStats::default(),
    };
    let mut latencies = stats.latencies_us;
    latencies.sort_unstable();

    json!({
        "config": {
            "rateLinesPerSec": config.rate,
            "durationSecs": config.duration.as_secs(),
            "sampleSecs": config.sample_every.as_secs(),
            "giantLineBytes": config.giant_line_bytes,
        },
        "elapsedSecs": elapsed.as_secs_f64(),
        "rss": {
            "startBytes": rss_start,
            "peakBytes": rss_peak,
            "endBytes": rss_end,
            "samples": samples,
        },
        "entries": {
            "ingested": ingested,
            "ingestedPerSec": ingested as f64 / elapsed.as_secs_f64(),
            "dropped": dropped,
            "storedAtEnd": stored,
            "packagesSeen": packages_seen,
        },
        "batches": {
            "count": latencies.len(),
            "rowsTotal": stats.rows_total,
            "rowsMax": stats.rows_max,
            "backlogMax": stats.backlog_max,
            "latencyUs": {
                "p50": percentile(&latencies, 50.0),
                "p90": percentile(&latencies, 90.0),
                "p99": percentile(&latencies, 99.0),
                "p999": percentile(&latencies, 99.9),
                "max": latencies.last().copied().unwrap_or(0),
            },
        },
        "trackedPids": {
            "max": stats.tracked_pids_max,
            "end": stats.tracked_pids_last,
        },
        "giantLine": giant_line,
    })
}
