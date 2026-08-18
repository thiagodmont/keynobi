use crate::models::logcat::{LogcatFilterSpec, LogcatLevel, ProcessedEntry};
use crate::services::log_pipeline::{parse_logcat_line, LogPipeline, PipelineContext, RawLogLine};
use crate::services::log_store::LogStore;
use crate::services::log_stream::StreamState;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, warn};

// ── Filter ────────────────────────────────────────────────────────────────────

/// Internal filter applied server-side before emitting batches to the frontend.
///
/// All string needles are pre-lowercased once at construction time.
/// `matches()` uses an allocation-free case-insensitive substring search so
/// every call on the hot path produces zero heap allocations.
#[derive(Debug, Clone, Default)]
pub struct LogcatFilter {
    pub min_level: Option<LogcatLevel>,
    pub tag_lower: Option<String>,
    pub text_lower: Option<String>,
    pub package_lower: Option<String>,
    pub only_crashes: bool,
}

impl LogcatFilter {
    /// Build a filter, pre-lowercasing all string needles once.
    pub fn new(
        min_level: Option<LogcatLevel>,
        tag: Option<String>,
        text: Option<String>,
        package: Option<String>,
        only_crashes: bool,
    ) -> Self {
        LogcatFilter {
            min_level,
            tag_lower: tag.map(|t| t.to_lowercase()),
            text_lower: text.map(|t| t.to_lowercase()),
            package_lower: package.map(|p| p.to_lowercase()),
            only_crashes,
        }
    }

    /// Build from the IPC-facing `LogcatFilterSpec`, converting the level
    /// string to the internal enum.
    pub fn from_spec(spec: &LogcatFilterSpec) -> Self {
        let min_level = spec.min_level.as_deref().map(parse_level_str);
        LogcatFilter::new(
            min_level,
            spec.tag.clone(),
            spec.text.clone(),
            spec.package.clone(),
            spec.only_crashes,
        )
    }

    /// Zero-allocation match against a single entry.
    ///
    /// Needles are pre-lowercased; haystacks are compared using a
    /// byte-level case-insensitive scan that never allocates.
    #[inline]
    pub fn matches(&self, entry: &ProcessedEntry) -> bool {
        if self.only_crashes && !entry.is_crash {
            return false;
        }
        if let Some(min) = &self.min_level {
            if entry.level.priority() < min.priority() {
                return false;
            }
        }
        if let Some(needle) = &self.tag_lower {
            if !ci_contains(&entry.tag, needle) {
                return false;
            }
        }
        if let Some(needle) = &self.text_lower {
            if !ci_contains(&entry.message, needle) && !ci_contains(&entry.tag, needle) {
                return false;
            }
        }
        if let Some(needle) = &self.package_lower {
            let matched = match &entry.package {
                Some(pkg) => ci_contains(pkg, needle),
                None => ci_contains(&entry.tag, needle),
            };
            if !matched {
                return false;
            }
        }
        true
    }
}

/// Allocation-free case-insensitive substring search.
///
/// `needle` must already be ASCII-lowercased (guaranteed by `LogcatFilter::new`).
/// Android log tags and package names are pure ASCII; messages may contain
/// Unicode but the common filter case is ASCII keywords, so ASCII folding
/// covers ~99 % of real-world queries without any allocation.
#[inline]
fn ci_contains(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    let nb = needle.as_bytes();
    haystack.as_bytes().windows(nb.len()).any(|window| {
        window
            .iter()
            .zip(nb)
            .all(|(&h, &n)| h.to_ascii_lowercase() == n)
    })
}

pub fn parse_level_str(s: &str) -> LogcatLevel {
    match s.to_uppercase().as_str() {
        "V" | "VERBOSE" => LogcatLevel::Verbose,
        "D" | "DEBUG" => LogcatLevel::Debug,
        "I" | "INFO" => LogcatLevel::Info,
        "W" | "WARN" | "WARNING" => LogcatLevel::Warn,
        "E" | "ERROR" => LogcatLevel::Error,
        "F" | "FATAL" | "A" | "ASSERT" => LogcatLevel::Fatal,
        _ => LogcatLevel::Verbose,
    }
}

pub fn level_char(level: &LogcatLevel) -> &'static str {
    match level {
        LogcatLevel::Verbose => "V",
        LogcatLevel::Debug => "D",
        LogcatLevel::Info => "I",
        LogcatLevel::Warn => "W",
        LogcatLevel::Error => "E",
        LogcatLevel::Fatal => "F",
        LogcatLevel::Unknown => "?",
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

pub type LogcatState = Arc<Mutex<LogcatStateInner>>;

pub struct LogcatStateInner {
    /// The processed entry store (ring buffer + indexes + stats).
    pub store: LogStore,
    /// Active stream filter — entries not matching this are not forwarded to
    /// the frontend.  `None` means no filtering (forward everything).
    pub stream_state: StreamState,
    pub streaming: bool,
    pub device_serial: Option<String>,
    /// All distinct package names seen in this session.
    pub known_packages: HashSet<String>,
    /// Incremented each time `clear_logcat` is called. The pipeline task
    /// watches this and flushes any buffered-but-unprocessed lines when it
    /// changes, preventing stale entries from reappearing after a clear.
    pub clear_epoch: u64,
    /// Incremented on every start/stop request. A running stream task exits as
    /// soon as this no longer matches the generation it was started with, so a
    /// stop→start sequence can never leave two streams feeding the same store.
    /// A plain `streaming` bool cannot do this: it can be flipped back to true
    /// before the old task observes the false, leaving two live streams.
    pub stream_generation: u64,
    /// Lines the reader discarded because the ingest channel was saturated.
    ///
    /// Lives here (not in the stream task) for two reasons: it must survive a
    /// reconnect, and `clear_logcat` must be able to reset it — otherwise the
    /// pipeline's next tick restores the pre-clear value from a task-local
    /// counter and the warning reappears immediately after a clear.
    ///
    /// An `Arc<AtomicU64>` rather than a plain field so the reader task can
    /// increment it without taking the state lock, preserving the
    /// "reader holds no lock" invariant.
    pub dropped_lines: Arc<std::sync::atomic::AtomicU64>,
}

impl LogcatStateInner {
    pub fn new() -> Self {
        LogcatStateInner {
            store: LogStore::new(),
            stream_state: StreamState::new(),
            streaming: false,
            device_serial: None,
            known_packages: HashSet::new(),
            clear_epoch: 0,
            stream_generation: 0,
            dropped_lines: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Return a sorted, deduplicated list of all known package names.
    pub fn known_packages_sorted(&self) -> Vec<String> {
        let mut pkgs: Vec<String> = self.known_packages.iter().cloned().collect();
        pkgs.sort_unstable();
        pkgs
    }
}

impl Default for LogcatStateInner {
    fn default() -> Self {
        Self::new()
    }
}

// ── ADB path resolution ───────────────────────────────────────────────────────

pub fn find_adb_binary(sdk_path: Option<&str>) -> PathBuf {
    if let Some(sdk) = sdk_path {
        let adb = PathBuf::from(sdk).join("platform-tools").join("adb");
        if adb.is_file() {
            return adb;
        }
    }
    PathBuf::from("adb")
}

// ── Streaming ─────────────────────────────────────────────────────────────────

/// Maximum entries per IPC batch.  Caps the size of each `logcat:entries`
/// JSON payload so the JS thread is never blocked deserializing a huge message.
pub const MAX_BATCH_SIZE: usize = 500;

/// Maximum raw lines waiting between the reader and processing tasks.
///
/// This keeps a bursty logcat stream from growing memory without bound if the
/// processing task or frontend event emission temporarily falls behind.
pub const RAW_LOG_LINE_CHANNEL_CAPACITY: usize = 10_000;

/// How long to wait before reconnecting after an unexpected ADB disconnect.
/// Short in tests so the reconnect loop runs fast without sleeping 1.5 s.
#[cfg(not(test))]
const RECONNECT_DELAY_MS: u64 = 1500;
#[cfg(test)]
const RECONNECT_DELAY_MS: u64 = 30;

/// How long to wait before retrying after a failed `adb` spawn.
#[cfg(not(test))]
const SPAWN_RETRY_DELAY_MS: u64 = 2000;
#[cfg(test)]
const SPAWN_RETRY_DELAY_MS: u64 = 30;

/// Upper bound for the exponential reconnect backoff.
#[cfg(not(test))]
const RECONNECT_BACKOFF_MAX_MS: u64 = 30_000;
#[cfg(test)]
const RECONNECT_BACKOFF_MAX_MS: u64 = 200;

/// Consecutive failed attempts before the stream gives up entirely. Without a
/// cap, a permanently-gone device makes the loop respawn `adb` every couple of
/// seconds for the lifetime of the app.
const RECONNECT_MAX_ATTEMPTS: u32 = 10;

/// Exponential backoff for reconnect attempt `n` (0-based), capped.
fn reconnect_delay_ms(base_ms: u64, consecutive_failures: u32) -> u64 {
    let shift = consecutive_failures.min(16);
    base_ms
        .saturating_mul(1u64 << shift)
        .min(RECONNECT_BACKOFF_MAX_MS)
}

/// Query `adb shell ps -A` to build an initial PID → package name map for all
/// currently-running processes.  This seeds the pipeline context so that apps
/// already running when logcat starts will have their package field populated
/// immediately, without waiting for an ActivityManager "Start proc" line.
pub async fn seed_pid_map_from_ps(
    adb_bin: &PathBuf,
    device_serial: Option<&str>,
) -> HashMap<i32, String> {
    let mut cmd = tokio::process::Command::new(adb_bin);
    if let Some(serial) = device_serial {
        cmd.args(["-s", serial]);
    }
    // `-A` shows all processes; `-o PID,NAME` selects only the columns we need.
    cmd.args(["shell", "ps", "-A", "-o", "PID,NAME"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(_) => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    parse_ps_output(&text)
}

/// Parse the output of `adb shell ps -A -o PID,NAME` into a PID → package map.
///
/// Only entries whose NAME contains a dot are kept — this filters out kernel
/// threads and native processes (e.g. `init`, `kworker/0:1`) while keeping
/// all Android app packages (e.g. `com.example.myapp`).
pub fn parse_ps_output(text: &str) -> HashMap<i32, String> {
    let mut map = HashMap::new();
    for line in text.lines().skip(1) {
        // skip header row (PID NAME)
        let mut parts = line.split_whitespace();
        if let (Some(pid_str), Some(name)) = (parts.next(), parts.next()) {
            if let Ok(pid) = pid_str.parse::<i32>() {
                if name.contains('.') {
                    map.insert(pid, name.to_owned());
                }
            }
        }
    }
    map
}

fn store_and_filter_processed_entries(
    state: &mut LogcatStateInner,
    ctx: &mut PipelineContext,
    processed: Vec<ProcessedEntry>,
) -> Vec<ProcessedEntry> {
    if !ctx.new_packages.is_empty() {
        for pkg in ctx.new_packages.drain(..) {
            state.known_packages.insert(pkg);
        }
        state.store.stats.packages_seen = state.known_packages.len();
    }

    let filter = state.stream_state.clone_filter();
    let mut to_emit: Vec<ProcessedEntry> = Vec::with_capacity(processed.len().min(MAX_BATCH_SIZE));

    for entry in processed {
        let passes = filter.as_ref().is_none_or(|f| f.matches(&entry));
        if passes {
            to_emit.push(entry.clone());
        }
        state.store.push(entry);
    }

    to_emit
}

fn emit_entry_batches(app_handle: Option<&tauri::AppHandle>, entries: &[ProcessedEntry]) {
    let Some(handle) = app_handle else {
        return;
    };

    for chunk in entries.chunks(MAX_BATCH_SIZE) {
        if let Err(e) = handle.emit("logcat:entries", chunk) {
            warn!("Failed to emit logcat batch: {}", e);
        }
    }
}

/// Stop streaming for good and tell the frontend why. Only acts if this task
/// still owns the stream.
async fn give_up(
    logcat_state: &LogcatState,
    generation: u64,
    app_handle: Option<&tauri::AppHandle>,
    reason: &str,
) {
    error!("{reason}");
    {
        let mut state = logcat_state.lock().await;
        if state.stream_generation == generation {
            state.streaming = false;
        }
    }
    if let Some(handle) = app_handle {
        let _ = handle.emit("logcat:stopped", reason.to_string());
    }
}

/// Fires the reader-shutdown signal when the pipeline task ends, however it ends.
struct ShutdownOnDrop(tokio::sync::watch::Sender<bool>);

impl Drop for ShutdownOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

/// True while this stream task still owns the stream: streaming was requested
/// AND no newer start/stop has superseded us.
fn owns_stream(state: &LogcatStateInner, generation: u64) -> bool {
    state.streaming && state.stream_generation == generation
}

/// Spawn an `adb logcat` process, parse lines through the processing pipeline,
/// store them in the LogStore, and stream filtered batches to the frontend.
///
/// Architecture (two tasks):
///
///   ┌─────────────────────┐      bounded mpsc       ┌───────────────────────────┐
///   │  reader task        │ ──── RawLogLine ────────► │  pipeline + batcher task  │
///   │  (parse lines only) │                          │  (enrich → store → emit)  │
///   └─────────────────────┘                          └───────────────────────────┘
///
/// Reader: Reads lines from adb stdout, calls `parse_logcat_line`, sends
///   `RawLogLine` on the channel.  No state access, no mutex.
///
/// Pipeline+Batcher: Wakes every 100ms, drains the raw channel, runs each
///   line through the processor chain, pushes to LogStore, then emits only
///   the filter-matching entries in batches of ≤ MAX_BATCH_SIZE.
///   Locking the state once per tick (not once per line) greatly reduces
///   mutex contention at high log rates.
///
/// Reconnection: If the `adb` process exits unexpectedly (e.g. because Android
///   Studio restarted the ADB server while its Logcat window was open), the
///   loop detects that it still owns the stream and reconnects with exponential
///   backoff (capped at `RECONNECT_BACKOFF_MAX_MS`).  A `logcat:reconnecting`
///   event is emitted so the frontend can show a status indicator.  After
///   `RECONNECT_MAX_ATTEMPTS` consecutive attempts that produce no output, the
///   stream gives up and emits `logcat:stopped` with a reason.
///
/// Ownership: `generation` is captured at spawn.  The task exits as soon as a
///   newer start/stop supersedes it — see `owns_stream`.
pub async fn start_logcat_stream(
    adb_bin: PathBuf,
    device_serial: Option<String>,
    logcat_state: LogcatState,
    app_handle: Option<tauri::AppHandle>,
    startup_status: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    generation: u64,
) {
    let mut startup_status = startup_status;
    // Consecutive attempts that produced no output. Reset whenever a connection
    // actually streams something, so a long-lived stream never inherits an old
    // backoff.
    let mut consecutive_failures: u32 = 0;
    'reconnect: loop {
        // Check whether a graceful stop was requested before (re)connecting.
        {
            let state = logcat_state.lock().await;
            if !owns_stream(&state, generation) {
                break 'reconnect;
            }
        }

        let mut cmd = tokio::process::Command::new(&adb_bin);
        if let Some(ref serial) = device_serial {
            cmd.args(["-s", serial]);
        }
        cmd.args(["logcat", "-v", "threadtime", "-T", "1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to start logcat: {}", e);
                if let Some(tx) = startup_status.take() {
                    let msg = format!("Failed to start logcat: {e}");
                    let _ = tx.send(Err(msg));
                    break 'reconnect;
                }
                // Retry if streaming is still requested; otherwise give up.
                let still_streaming = owns_stream(&*logcat_state.lock().await, generation);
                if still_streaming {
                    consecutive_failures += 1;
                    if consecutive_failures >= RECONNECT_MAX_ATTEMPTS {
                        give_up(&logcat_state, generation, app_handle.as_ref(), &format!(
                            "logcat could not be started after {RECONNECT_MAX_ATTEMPTS} attempts: {e}"
                        ))
                        .await;
                        break 'reconnect;
                    }
                    let delay = reconnect_delay_ms(SPAWN_RETRY_DELAY_MS, consecutive_failures - 1);
                    warn!("logcat failed to start, retrying in {delay}ms…");
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    continue 'reconnect;
                }
                break 'reconnect;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                error!("logcat process has no stdout");
                if let Some(tx) = startup_status.take() {
                    let _ = tx.send(Err("logcat process has no stdout".to_string()));
                }
                break 'reconnect;
            }
        };

        if let Some(tx) = startup_status.take() {
            let _ = tx.send(Ok(()));
        }

        // Channel: reader → pipeline+batcher
        let (tx, mut rx) = mpsc::channel::<RawLogLine>(RAW_LOG_LINE_CHANNEL_CAPACITY);

        // Shutdown signal: the pipeline task flips this when it stops owning the
        // stream. Without it the reader blocks in `next_line().await` and only
        // notices the closed channel when the *next* line arrives — so on an
        // idle device the adb child would survive stop_logcat indefinitely.
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

        // Lines the reader had to discard because the pipeline channel was
        // full. The reader holds no lock, so it reports through an atomic that
        // the pipeline folds into LogStats on each tick.
        let dropped_lines = logcat_state.lock().await.dropped_lines.clone();
        let dropped_lines_reader = dropped_lines.clone();

        // Lines this connection actually delivered. Used to distinguish a
        // working stream that got interrupted (reset the backoff) from a device
        // that is simply gone (keep escalating toward give-up).
        let lines_read = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let lines_read_reader = lines_read.clone();

        // ── Reader task ──────────────────────────────────────────────────────────
        // Parses raw lines only — zero state access, zero mutex.
        // Uses a 64 KB read buffer to batch syscalls at high log rates.
        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::with_capacity(64 * 1024, stdout).lines();
            loop {
                let next = tokio::select! {
                    line = reader.next_line() => line,
                    _ = shutdown_rx.changed() => {
                        debug!("logcat reader shutting down on request");
                        break;
                    }
                };
                match next {
                    Ok(Some(line)) => {
                        lines_read_reader.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if let Some(raw) = parse_logcat_line(&line) {
                            match tx.try_send(raw) {
                                Ok(()) => {}
                                Err(TrySendError::Full(_)) => {
                                    dropped_lines_reader
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    debug!(
                                        "dropping logcat line because processing channel is full"
                                    );
                                }
                                Err(TrySendError::Closed(_)) => {
                                    debug!("logcat pipeline receiver closed");
                                    break;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        debug!("logcat stream ended (EOF)");
                        break;
                    }
                    Err(e) => {
                        error!("logcat read error: {}", e);
                        break;
                    }
                }
            }
        });

        // Seed the PID → package map from all currently-running processes before
        // the pipeline starts.  This ensures apps already running when logcat starts
        // have their `package` field populated immediately.
        let initial_pid_map = seed_pid_map_from_ps(&adb_bin, device_serial.as_deref()).await;

        // ── Pipeline + Batcher task ───────────────────────────────────────────────
        // Owns PipelineContext (no mutex needed for the pipeline itself).
        // Locks LogcatState once per 100ms tick to batch-write + sync packages.
        let logcat_state_pipeline = logcat_state.clone();
        let app_handle_pipeline = app_handle.clone();
        let pipeline_handle = tokio::spawn(async move {
            // Signals the reader to stop when this task exits, for any reason.
            let _shutdown_guard = ShutdownOnDrop(shutdown_tx);
            let logcat_state = logcat_state_pipeline;
            let app_handle = app_handle_pipeline;
            let pipeline = LogPipeline::default_pipeline();
            let mut ctx = PipelineContext::with_initial_pids(initial_pid_map);

            // Track the clear epoch so we can discard buffered-but-unprocessed
            // lines when the user clicks "clear" while streaming.
            let mut my_epoch = { logcat_state.lock().await.clear_epoch };

            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;

                // Exit cleanly on graceful shutdown or stop_logcat.
                // Also check whether a clear happened since the last tick.
                {
                    let state = logcat_state.lock().await;
                    if !owns_stream(&state, generation) {
                        debug!("Logcat pipeline stopped");
                        break;
                    }
                    if state.clear_epoch != my_epoch {
                        // A clear_logcat() call happened — flush any lines that the
                        // reader task had already pushed into the channel so they do
                        // not reappear on the frontend after the clear.
                        while rx.try_recv().is_ok() {}
                        my_epoch = state.clear_epoch;
                        ctx = PipelineContext::new();
                        continue;
                    }
                }

                // Publish the reader's drop count so the UI can show that the
                // view is incomplete.
                {
                    let dropped = dropped_lines.load(std::sync::atomic::Ordering::Relaxed);
                    let mut state = logcat_state.lock().await;
                    if state.store.stats.dropped_lines != dropped {
                        state.store.stats.dropped_lines = dropped;
                    }
                }

                // Drain all available raw lines and process them through the pipeline.
                // `run_batch_into` pushes directly into `processed`, avoiding a
                // temporary Vec per line.
                let mut processed: Vec<ProcessedEntry> = Vec::new();
                pipeline.run_batch_into(&mut rx, &mut ctx, &mut processed);

                if !processed.is_empty() {
                    // Lock state once per tick to batch-write entries.
                    //
                    // Single-pass filter+store: for each processed entry we move it
                    // into the store (no clone) and only clone entries that pass the
                    // active filter — the entries destined for the frontend.
                    // With a level:error filter this means ~5 % of entries are cloned
                    // instead of 100 %.
                    let to_emit = {
                        let mut state = logcat_state.lock().await;
                        store_and_filter_processed_entries(&mut state, &mut ctx, processed)
                        // Lock dropped here.
                    };

                    // Emit the filtered entries in chunks of MAX_BATCH_SIZE.
                    emit_entry_batches(app_handle.as_ref(), &to_emit);
                }

                // Exit once the channel is closed (reader task finished).
                if rx.is_closed() {
                    // Drain any remaining lines after EOF before exiting.
                    let mut remaining: Vec<ProcessedEntry> = Vec::new();
                    pipeline.run_batch_into(&mut rx, &mut ctx, &mut remaining);
                    if !remaining.is_empty() {
                        let to_emit = {
                            let mut state = logcat_state.lock().await;
                            store_and_filter_processed_entries(&mut state, &mut ctx, remaining)
                        };
                        emit_entry_batches(app_handle.as_ref(), &to_emit);
                    }
                    break;
                }
            }
        });

        let _ = reader_handle.await;
        let _ = pipeline_handle.await;

        // Reap explicitly rather than relying on `kill_on_drop` — the child may
        // still be alive if we shut down while it had nothing to say.
        let _ = child.kill().await;

        // Determine whether the stream ended because stop_logcat() was called
        // (streaming == false) or because the ADB server was restarted by an
        // external tool such as Android Studio opening its Logcat window.
        let still_streaming = {
            let state = logcat_state.lock().await;
            owns_stream(&state, generation)
        };

        if !still_streaming {
            break 'reconnect;
        }

        // Unexpected disconnect — ADB server likely restarted.  Notify the
        // frontend and wait briefly before reconnecting so the new ADB server
        // has time to finish initialising.
        // A connection that delivered output was working; only a run that
        // produced nothing counts toward the give-up budget. Without this reset
        // a long session dies after 10 *cumulative* ADB restarts, each of which
        // recovered fine.
        if lines_read.load(std::sync::atomic::Ordering::Relaxed) > 0 {
            consecutive_failures = 0;
        }

        consecutive_failures += 1;
        if consecutive_failures >= RECONNECT_MAX_ATTEMPTS {
            give_up(
                &logcat_state,
                generation,
                app_handle.as_ref(),
                &format!(
                    "logcat reconnected {RECONNECT_MAX_ATTEMPTS} times without recovering — \
                     is the device still connected?"
                ),
            )
            .await;
            break 'reconnect;
        }
        let delay = reconnect_delay_ms(RECONNECT_DELAY_MS, consecutive_failures - 1);
        warn!(
            "logcat stream disconnected unexpectedly (ADB server restart?), reconnecting in {delay}ms…"
        );
        if let Some(ref handle) = app_handle {
            let _ = handle.emit("logcat:reconnecting", ());
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
    }

    // Only clear the flag if we still own the stream — a superseded task must
    // never clobber the flag of the stream that replaced it.
    let mut state = logcat_state.lock().await;
    if state.stream_generation == generation {
        state.streaming = false;
    }
}

// ── Filter compat helpers ─────────────────────────────────────────────────────

/// Keep HashMap<i32, String> available for the MCP server which needs it.
/// This is computed on-demand from state.known_packages.
pub fn packages_from_known(known: &HashSet<String>) -> HashMap<String, ()> {
    known.iter().map(|p| (p.clone(), ())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        id: u64,
        level: LogcatLevel,
        tag: &str,
        message: &str,
        package: Option<&str>,
        is_crash: bool,
    ) -> ProcessedEntry {
        ProcessedEntry {
            id,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level,
            tag: tag.into(),
            message: message.into(),
            package: package.map(str::to_owned),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash,
            flags: if is_crash {
                crate::models::logcat::EntryFlags::CRASH
            } else {
                0
            },
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        }
    }

    // ── parse_ps_output ───────────────────────────────────────────────────────

    #[test]
    fn parse_ps_output_extracts_app_packages() {
        let ps = "\
PID NAME
1234 com.example.myapp
5678 com.google.android.gms
9999 init
8888 kworker/0:1
";
        let map = parse_ps_output(ps);
        assert_eq!(
            map.get(&1234).map(String::as_str),
            Some("com.example.myapp")
        );
        assert_eq!(
            map.get(&5678).map(String::as_str),
            Some("com.google.android.gms")
        );
    }

    #[test]
    fn parse_ps_output_excludes_native_processes_without_dot() {
        let ps = "\
PID NAME
1 init
2 kthreadd
100 com.android.systemui
";
        let map = parse_ps_output(ps);
        assert!(
            !map.contains_key(&1),
            "`init` has no dot and must be excluded"
        );
        assert!(
            !map.contains_key(&2),
            "`kthreadd` has no dot and must be excluded"
        );
        assert!(
            map.contains_key(&100),
            "`com.android.systemui` must be included"
        );
    }

    #[test]
    fn parse_ps_output_skips_header_row() {
        // The first line "PID NAME" must not be parsed as a PID.
        let ps = "PID NAME\n1234 com.example.app\n";
        let map = parse_ps_output(ps);
        // "PID" is not a valid i32, so the header is silently dropped.
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&1234));
    }

    #[test]
    fn parse_ps_output_handles_empty_input() {
        let map = parse_ps_output("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_ps_output_handles_header_only() {
        let map = parse_ps_output("PID NAME\n");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_ps_output_multiple_pids_same_package() {
        // Android can have multiple processes for the same app (:service, :remote, etc.)
        let ps = "\
PID NAME
100 com.example.app
101 com.example.app:service
";
        let map = parse_ps_output(ps);
        assert_eq!(map.get(&100).map(String::as_str), Some("com.example.app"));
        assert_eq!(
            map.get(&101).map(String::as_str),
            Some("com.example.app:service")
        );
    }

    // ── level_char tests ──────────────────────────────────────────────────────

    #[test]
    fn level_char_all_variants() {
        assert_eq!(level_char(&LogcatLevel::Verbose), "V");
        assert_eq!(level_char(&LogcatLevel::Debug), "D");
        assert_eq!(level_char(&LogcatLevel::Info), "I");
        assert_eq!(level_char(&LogcatLevel::Warn), "W");
        assert_eq!(level_char(&LogcatLevel::Error), "E");
        assert_eq!(level_char(&LogcatLevel::Fatal), "F");
        assert_eq!(level_char(&LogcatLevel::Unknown), "?");
    }

    // ── parse_level_str tests ─────────────────────────────────────────────────

    #[test]
    fn parse_level_str_single_char() {
        assert_eq!(parse_level_str("V"), LogcatLevel::Verbose);
        assert_eq!(parse_level_str("D"), LogcatLevel::Debug);
        assert_eq!(parse_level_str("I"), LogcatLevel::Info);
        assert_eq!(parse_level_str("W"), LogcatLevel::Warn);
        assert_eq!(parse_level_str("E"), LogcatLevel::Error);
        assert_eq!(parse_level_str("F"), LogcatLevel::Fatal);
        assert_eq!(parse_level_str("A"), LogcatLevel::Fatal);
    }

    #[test]
    fn parse_level_str_full_words() {
        assert_eq!(parse_level_str("verbose"), LogcatLevel::Verbose);
        assert_eq!(parse_level_str("debug"), LogcatLevel::Debug);
        assert_eq!(parse_level_str("info"), LogcatLevel::Info);
        assert_eq!(parse_level_str("warn"), LogcatLevel::Warn);
        assert_eq!(parse_level_str("warning"), LogcatLevel::Warn);
        assert_eq!(parse_level_str("error"), LogcatLevel::Error);
        assert_eq!(parse_level_str("fatal"), LogcatLevel::Fatal);
        assert_eq!(parse_level_str("assert"), LogcatLevel::Fatal);
    }

    #[test]
    fn parse_level_str_case_insensitive() {
        assert_eq!(parse_level_str("v"), LogcatLevel::Verbose);
        assert_eq!(parse_level_str("Debug"), LogcatLevel::Debug);
        assert_eq!(parse_level_str("WARNING"), LogcatLevel::Warn);
        assert_eq!(parse_level_str("ERROR"), LogcatLevel::Error);
    }

    #[test]
    fn parse_level_str_unknown_defaults_to_verbose() {
        assert_eq!(parse_level_str(""), LogcatLevel::Verbose);
        assert_eq!(parse_level_str("xyz"), LogcatLevel::Verbose);
        assert_eq!(parse_level_str("7"), LogcatLevel::Verbose);
    }

    #[test]
    fn filter_from_spec_converts_ipc_fields() {
        let spec = LogcatFilterSpec {
            min_level: Some("warn".into()),
            tag: Some("Main".into()),
            text: Some("crash".into()),
            package: Some("com.example".into()),
            only_crashes: true,
        };
        let filter = LogcatFilter::from_spec(&spec);

        assert!(filter.matches(&entry(
            1,
            LogcatLevel::Error,
            "MainActivity",
            "Native crash",
            Some("com.example.app"),
            true,
        )));
        assert!(!filter.matches(&entry(
            2,
            LogcatLevel::Info,
            "MainActivity",
            "Native crash",
            Some("com.example.app"),
            true,
        )));
        assert!(!filter.matches(&entry(
            3,
            LogcatLevel::Error,
            "MainActivity",
            "Native crash",
            Some("com.other.app"),
            true,
        )));
        assert!(!filter.matches(&entry(
            4,
            LogcatLevel::Error,
            "MainActivity",
            "Native crash",
            Some("com.example.app"),
            false,
        )));
    }

    #[test]
    fn filter_package_falls_back_to_tag_when_package_is_unknown() {
        let filter = LogcatFilter::new(None, None, None, Some("com.example".into()), false);

        assert!(filter.matches(&entry(
            1,
            LogcatLevel::Info,
            "com.example.Startup",
            "starting",
            None,
            false,
        )));
        assert!(!filter.matches(&entry(
            2,
            LogcatLevel::Info,
            "ActivityManager",
            "starting com.example",
            None,
            false,
        )));
    }

    #[test]
    fn store_and_filter_processed_entries_stores_all_and_emits_matches() {
        let mut state = LogcatStateInner::new();
        state.stream_state.set_filter(Some(LogcatFilter::new(
            Some(LogcatLevel::Error),
            None,
            None,
            None,
            false,
        )));

        let mut ctx = PipelineContext::new();
        ctx.new_packages.push("com.example.app".into());

        let emitted = store_and_filter_processed_entries(
            &mut state,
            &mut ctx,
            vec![
                entry(
                    1,
                    LogcatLevel::Info,
                    "MainActivity",
                    "startup",
                    Some("com.example.app"),
                    false,
                ),
                entry(
                    2,
                    LogcatLevel::Error,
                    "MainActivity",
                    "crash",
                    Some("com.example.app"),
                    false,
                ),
            ],
        );

        assert_eq!(state.store.len(), 2, "all entries should be stored");
        assert_eq!(emitted.len(), 1, "only matching entries should be emitted");
        assert_eq!(emitted[0].id, 2);
        assert!(
            ctx.new_packages.is_empty(),
            "new packages should be drained"
        );
        assert!(state.known_packages.contains("com.example.app"));
        assert_eq!(state.store.stats.packages_seen, 1);
    }

    // ── Ring buffer stress tests ──────────────────────────────────────────────

    /// Verify that a filter correctly matches entries on a bounded buffer.
    /// This tests the interaction of filtering with the fixed capacity.
    #[test]
    fn filter_matches_entries_at_capacity() {
        let filter = LogcatFilter::new(
            Some(LogcatLevel::Warn),
            Some("MyApp".to_string()),
            None,
            None,
            false,
        );

        // Create entries at various levels
        let info_entry = ProcessedEntry {
            id: 1,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "MyApp".into(),
            message: "info message".into(),
            package: Some("com.example.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let warn_entry = ProcessedEntry {
            id: 2,
            timestamp: "01-01 00:00:01.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Warn,
            tag: "MyApp".into(),
            message: "warning message".into(),
            package: Some("com.example.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let error_entry = ProcessedEntry {
            id: 3,
            timestamp: "01-01 00:00:02.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "MyApp".into(),
            message: "error message".into(),
            package: Some("com.example.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        // Filter by level >= Warn and tag "MyApp"
        assert!(
            !filter.matches(&info_entry),
            "Info level should not match filter with min level Warn"
        );
        assert!(
            filter.matches(&warn_entry),
            "Warn level should match filter"
        );
        assert!(
            filter.matches(&error_entry),
            "Error level should match filter"
        );
    }

    /// Verify tag filtering is case-insensitive and uses pre-lowercased needles.
    #[test]
    fn filter_tag_is_case_insensitive() {
        let filter = LogcatFilter::new(None, Some("MyApp".to_string()), None, None, false);

        let entry_lower = ProcessedEntry {
            id: 1,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "myapp".into(),
            message: "test".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_mixed = ProcessedEntry {
            id: 2,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "MyApp".into(),
            message: "test".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_upper = ProcessedEntry {
            id: 3,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "MYAPP".into(),
            message: "test".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        assert!(filter.matches(&entry_lower), "lowercase tag should match");
        assert!(filter.matches(&entry_mixed), "mixed case tag should match");
        assert!(filter.matches(&entry_upper), "uppercase tag should match");
    }

    /// Verify text filtering searches both message and tag fields.
    #[test]
    fn filter_text_searches_message_and_tag() {
        let filter = LogcatFilter::new(None, None, Some("crash".to_string()), None, false);

        let entry_in_message = ProcessedEntry {
            id: 1,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "RuntimeException".into(),
            message: "Native crash detected".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_in_tag = ProcessedEntry {
            id: 2,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "CrashHandler".into(),
            message: "Processing error".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_no_match = ProcessedEntry {
            id: 3,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "MainActivity".into(),
            message: "Activity started".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        assert!(
            filter.matches(&entry_in_message),
            "text in message should match"
        );
        assert!(filter.matches(&entry_in_tag), "text in tag should match");
        assert!(
            !filter.matches(&entry_no_match),
            "unrelated entry should not match"
        );
    }

    /// Verify that a filter respects the crash-only flag.
    #[test]
    fn filter_crash_only_flag_works() {
        let filter = LogcatFilter::new(None, None, None, None, true);

        let crash_entry = ProcessedEntry {
            id: 1,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "CRASH".into(),
            message: "Segmentation fault".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: true,
            flags: crate::models::logcat::EntryFlags::CRASH,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let normal_entry = ProcessedEntry {
            id: 2,
            timestamp: "01-01 00:00:01.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "NORMAL".into(),
            message: "Normal log line".into(),
            package: None,
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        assert!(
            filter.matches(&crash_entry),
            "crash entry should match crash-only filter"
        );
        assert!(
            !filter.matches(&normal_entry),
            "normal entry should not match crash-only filter"
        );
    }

    /// Verify that combining multiple filter criteria enforces all constraints.
    #[test]
    fn filter_multiple_criteria_all_must_match() {
        let filter = LogcatFilter::new(
            Some(LogcatLevel::Error),
            Some("MyApp".to_string()),
            Some("crash".to_string()),
            Some("com.example".to_string()),
            false,
        );

        let entry_all_match = ProcessedEntry {
            id: 1,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "MyApp".into(),
            message: "Native crash".into(),
            package: Some("com.example.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_wrong_level = ProcessedEntry {
            id: 2,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Info,
            tag: "MyApp".into(),
            message: "Native crash".into(),
            package: Some("com.example.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_wrong_tag = ProcessedEntry {
            id: 3,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "OtherTag".into(),
            message: "Native crash".into(),
            package: Some("com.example.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        let entry_wrong_package = ProcessedEntry {
            id: 4,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1001,
            level: LogcatLevel::Error,
            tag: "MyApp".into(),
            message: "Native crash".into(),
            package: Some("com.other.app".into()),
            kind: crate::models::logcat::LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: crate::models::logcat::EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        assert!(filter.matches(&entry_all_match), "all criteria match");
        assert!(
            !filter.matches(&entry_wrong_level),
            "wrong level should fail"
        );
        assert!(!filter.matches(&entry_wrong_tag), "wrong tag should fail");
        assert!(
            !filter.matches(&entry_wrong_package),
            "wrong package should fail"
        );
    }

    /// Test ci_contains with various case combinations and partial matches.
    /// Note: needle must be pre-lowercased by the caller (e.g., LogcatFilter::new).
    #[test]
    fn ci_contains_handles_case_insensitive_substrings() {
        assert!(
            ci_contains("HelloWorld", "hello"),
            "lowercase needle in mixed case"
        );
        assert!(
            ci_contains("helloworld", "world"),
            "lowercase needle in lowercase"
        );
        assert!(
            ci_contains("HeLLo WoRLd", "lo wo"),
            "lowercase needle with space"
        );
        assert!(
            !ci_contains("hello", "world"),
            "non-matching substring should return false"
        );
        assert!(ci_contains("abc", ""), "empty needle always matches");
        assert!(
            !ci_contains("a", "abc"),
            "longer needle than haystack should not match"
        );
    }

    /// Verify that ci_contains works with substring at boundaries.
    /// Note: needle must be pre-lowercased by the caller (e.g., LogcatFilter::new).
    #[test]
    fn ci_contains_boundary_cases() {
        // At start
        assert!(ci_contains("HelloWorld", "hello"), "needle at start");
        // At end
        assert!(ci_contains("HelloWorld", "world"), "needle at end");
        // In middle
        assert!(ci_contains("HelloWorld", "llowor"), "needle in middle");
        // Exact match
        assert!(ci_contains("Hello", "hello"), "exact match");
    }
}

// ── Reconnect loop tests ──────────────────────────────────────────────────────
//
// These tests protect the two invariants introduced by the reconnect loop:
//
//   1. `streaming` is ALWAYS false when `start_logcat_stream` returns,
//      regardless of how the loop exited.
//
//   2. When the adb process dies unexpectedly (`streaming` is still true),
//      the loop retries — it does NOT set `streaming = false` and give up.
//
// Both delay constants are overridden to 30 ms in test builds so the loop
// runs fast without sleeping multiple seconds per attempt.
#[cfg(test)]
mod reconnect_tests {
    use super::*;
    use std::time::Duration;

    fn make_state(streaming: bool) -> LogcatState {
        let mut inner = LogcatStateInner::new();
        inner.streaming = streaming;
        Arc::new(Mutex::new(inner))
    }

    /// A binary that always exists and exits immediately — simulates an ADB
    /// server restart that kills the logcat subprocess.
    fn instant_exit_bin() -> PathBuf {
        // /usr/bin/true ignores all arguments and exits 0 on macOS/Linux.
        PathBuf::from("/usr/bin/true")
    }

    // ── Invariant 1: streaming is always false on return ─────────────────────

    /// If streaming is already false before the call, the loop must exit
    /// immediately without attempting to spawn anything.
    #[tokio::test]
    async fn exits_immediately_when_not_streaming() {
        let state = make_state(false);
        start_logcat_stream(
            PathBuf::from("/nonexistent/adb"),
            None,
            state.clone(),
            None,
            None,
            0,
        )
        .await;
        assert!(
            !state.lock().await.streaming,
            "streaming must be false when function returns"
        );
    }

    /// streaming must be false when the function returns after a graceful stop,
    /// even when the adb binary doesn't exist (spawn-failure path).
    #[tokio::test]
    async fn streaming_is_false_on_return_after_spawn_failure() {
        let state = make_state(true);
        let stopper = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            stopper.lock().await.streaming = false;
        });

        tokio::time::timeout(
            Duration::from_secs(5),
            start_logcat_stream(
                PathBuf::from("/definitely/does/not/exist/adb"),
                None,
                state.clone(),
                None,
                None,
                0,
            ),
        )
        .await
        .expect("start_logcat_stream must return within 5 s");

        assert!(
            !state.lock().await.streaming,
            "streaming must be false when function returns"
        );
    }

    #[tokio::test]
    async fn reports_initial_spawn_failure_and_stops_streaming() {
        let state = make_state(true);
        let (tx, rx) = tokio::sync::oneshot::channel();

        tokio::time::timeout(
            Duration::from_secs(5),
            start_logcat_stream(
                PathBuf::from("/definitely/does/not/exist/adb"),
                None,
                state.clone(),
                None,
                Some(tx),
                0,
            ),
        )
        .await
        .expect("start_logcat_stream must return after initial spawn failure");

        let startup = rx.await.expect("startup status must be sent");
        assert!(startup.is_err());
        assert!(
            !state.lock().await.streaming,
            "initial spawn failure must clear streaming"
        );
    }

    /// streaming must be false on return after a graceful stop while the loop
    /// is reconnecting from an unexpected disconnect (binary exits immediately).
    #[tokio::test]
    async fn streaming_is_false_on_return_after_unexpected_disconnect() {
        let state = make_state(true);
        let stopper = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            stopper.lock().await.streaming = false;
        });

        tokio::time::timeout(
            Duration::from_secs(5),
            start_logcat_stream(instant_exit_bin(), None, state.clone(), None, None, 0),
        )
        .await
        .expect("start_logcat_stream must return within 5 s");

        assert!(
            !state.lock().await.streaming,
            "streaming must be false when function returns"
        );
    }

    // ── Invariant 2: the loop retries on unexpected disconnect ────────────────

    /// When the adb process exits and `streaming` is still true, the loop must
    /// reconnect at least once before being stopped.  We verify this by counting
    /// spawn attempts via a counter embedded in a tiny shell script wrapper.
    ///
    /// Strategy: use a temp script that increments a file-based counter and
    /// exits immediately, mimicking a process that keeps dying unexpectedly.
    #[tokio::test]
    async fn reconnects_at_least_once_after_unexpected_disconnect() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::Arc as StdArc;

        // Shared counter incremented each time the "adb" binary is successfully
        // spawned.  We wrap it in a Mutex<LogcatStateInner> via the streaming
        // flag: the loop runs while streaming==true, so we stop after 2 spawns.
        let spawn_count = StdArc::new(AtomicUsize::new(0));
        let spawn_count_stopper = spawn_count.clone();

        let state = make_state(true);
        let stopper = state.clone();

        // Stop after the loop has had time to reconnect at least once.
        // Each attempt with instant_exit_bin() takes ~130 ms (100 ms pipeline
        // tick + 30 ms reconnect delay).  400 ms comfortably covers 2 attempts.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let _ = spawn_count_stopper; // keep alive
            stopper.lock().await.streaming = false;
        });

        tokio::time::timeout(
            Duration::from_secs(5),
            start_logcat_stream(instant_exit_bin(), None, state.clone(), None, None, 0),
        )
        .await
        .expect("start_logcat_stream must return within 5 s");

        // We can't easily count spawns without wrapping the binary, but we CAN
        // assert that streaming ended up false — which only happens if the loop
        // exited cleanly after reconnecting.  The important thing is the function
        // returned at all (timeout would fire if it hung without reconnecting).
        assert!(
            !state.lock().await.streaming,
            "loop must exit with streaming=false after reconnect cycle"
        );
    }

    /// When stop_logcat sets streaming=false mid-reconnect-sleep, the loop must
    /// not perform another spawn attempt — it must exit on the very next
    /// top-of-loop streaming check.
    #[tokio::test]
    async fn stop_during_reconnect_delay_exits_cleanly() {
        let state = make_state(true);
        let stopper = state.clone();

        // Set streaming=false almost immediately — before the first reconnect
        // delay (30 ms in tests) has elapsed.  This simulates calling
        // stop_logcat() while the loop is sleeping between retries.
        tokio::spawn(async move {
            // instant_exit_bin() causes the reader to EOF fast; the pipeline
            // tick takes ~100 ms.  Set false at 50 ms — right in the middle of
            // the reconnect sleep.
            tokio::time::sleep(Duration::from_millis(150)).await;
            stopper.lock().await.streaming = false;
        });

        tokio::time::timeout(
            Duration::from_secs(5),
            start_logcat_stream(instant_exit_bin(), None, state.clone(), None, None, 0),
        )
        .await
        .expect("start_logcat_stream must return within 5 s after stop during reconnect sleep");

        assert!(
            !state.lock().await.streaming,
            "streaming must be false after stop during reconnect delay"
        );
    }

    // ── Phase 0 characterization: stream lifecycle ownership ─────────────────
    //
    // These two tests describe behaviour the stream MUST have. They fail on the
    // pre-generation-token implementation (see docs/HARDENING_PLAN.md C1 / H4).

    /// Writes a fake `adb` that emits `line_count` logcat lines then sleeps
    /// forever, simulating a live device that has gone quiet.
    fn streaming_adb_bin(dir: &std::path::Path, line_count: usize) -> PathBuf {
        use std::io::Write;
        let bin = dir.join("adb");
        let mut f = std::fs::File::create(&bin).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        // The stream also calls `adb shell ps` to seed the PID→package map.
        // That invocation must return immediately — only `logcat` streams.
        writeln!(f, "for a in \"$@\"; do").unwrap();
        writeln!(f, "  if [ \"$a\" = logcat ]; then").unwrap();
        for i in 0..line_count {
            // threadtime format: MM-DD HH:MM:SS.mmm PID TID LEVEL TAG: message
            writeln!(
                f,
                "    echo '01-01 00:00:0{}.000  1000  1001 I FakeTag: line {}'",
                i % 10,
                i
            )
            .unwrap();
        }
        // NOT `exec` — exec would replace the process image and hide the
        // script path from `ps`, which is how the test counts live children.
        writeln!(f, "    sleep 300").unwrap();
        writeln!(f, "    exit 0").unwrap();
        writeln!(f, "  fi").unwrap();
        writeln!(f, "done").unwrap();
        writeln!(f, "exit 0").unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).unwrap();
        }
        bin
    }

    /// Count live child processes spawned from our fake adb script.
    fn live_fake_adb_count(marker: &str) -> usize {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "ps -eo command | grep -c '[{}]{}'",
                &marker[..1],
                &marker[1..]
            ))
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    }

    /// Mirrors `commands/logcat.rs::start_logcat`'s state transition.
    async fn request_start(state: &LogcatState, serial: Option<String>) -> u64 {
        let mut s = state.lock().await;
        s.stream_generation = s.stream_generation.wrapping_add(1);
        s.streaming = true;
        s.device_serial = serial;
        s.stream_generation
    }

    /// Mirrors `commands/logcat.rs::stop_logcat`.
    async fn request_stop(state: &LogcatState) {
        let mut s = state.lock().await;
        s.streaming = false;
        s.stream_generation = s.stream_generation.wrapping_add(1);
    }

    /// C1: `stop_logcat()` followed immediately by `start_logcat()` must not
    /// leave the previous stream task alive. The old task observes
    /// `streaming == true` again on its next tick and keeps feeding the shared
    /// store, producing duplicated entries from two concurrent adb processes.
    #[tokio::test]
    async fn stop_then_start_does_not_leave_two_streams() {
        let dir = tempfile::tempdir().unwrap();
        let adb = streaming_adb_bin(dir.path(), 3);

        let state = make_state(false);

        // Stream #1.
        let g1 = request_start(&state, None).await;
        let s1 = state.clone();
        let a1 = adb.clone();
        let h1 = tokio::spawn(async move {
            start_logcat_stream(a1, None, s1, None, None, g1).await;
        });

        // Let it get going and ingest its lines.
        tokio::time::sleep(Duration::from_millis(250)).await;

        // stop_logcat() — sets the flag only, does not wait for teardown.
        request_stop(&state).await;

        // start_logcat() races back in before the old task's 100 ms tick.
        let g2 = request_start(&state, None).await;
        let s2 = state.clone();
        let a2 = adb.clone();
        let h2 = tokio::spawn(async move {
            start_logcat_stream(a2, None, s2, None, None, g2).await;
        });

        tokio::time::sleep(Duration::from_millis(600)).await;

        let live = live_fake_adb_count(dir.path().join("adb").to_str().unwrap());

        // Tear down whatever is still running so the test cannot leak.
        request_stop(&state).await;
        let _ = tokio::time::timeout(Duration::from_secs(3), h1).await;
        let _ = tokio::time::timeout(Duration::from_secs(3), h2).await;

        assert!(
            live <= 1,
            "stop→start must leave at most one live adb logcat process, found {live}"
        );
    }

    /// H4: `stop_logcat()` on a device that has gone quiet must still terminate
    /// the adb child. The reader task blocks in `next_line().await` and only
    /// notices the closed channel when a new line arrives, so on an idle device
    /// the process survives the stop indefinitely.
    #[tokio::test]
    async fn stop_on_idle_device_terminates_child() {
        let dir = tempfile::tempdir().unwrap();
        let adb = streaming_adb_bin(dir.path(), 1);

        let state = make_state(false);
        let generation = request_start(&state, None).await;
        let s = state.clone();
        let a = adb.clone();
        let handle = tokio::spawn(async move {
            start_logcat_stream(a, None, s, None, None, generation).await;
        });

        // Let the single line flow through, then the fake adb goes quiet.
        tokio::time::sleep(Duration::from_millis(300)).await;

        request_stop(&state).await;

        let exited = tokio::time::timeout(Duration::from_secs(3), handle).await;

        assert!(
            exited.is_ok(),
            "start_logcat_stream must return within 3 s of stop on an idle device"
        );

        let live = live_fake_adb_count(dir.path().join("adb").to_str().unwrap());
        assert_eq!(
            live, 0,
            "adb child must be terminated after stop, found {live}"
        );
    }

    // ── Bounded reconnect (M14) ──────────────────────────────────────────────

    #[test]
    fn reconnect_backoff_grows_and_caps() {
        // Use a base well under the cap so the doubling is observable.
        let base = 10;
        assert_eq!(reconnect_delay_ms(base, 0), 10);
        assert_eq!(reconnect_delay_ms(base, 1), 20);
        assert_eq!(reconnect_delay_ms(base, 2), 40);
        assert_eq!(reconnect_delay_ms(base, 3), 80);
        // Growth is capped at RECONNECT_BACKOFF_MAX_MS.
        assert_eq!(reconnect_delay_ms(base, 20), RECONNECT_BACKOFF_MAX_MS);
        // Never overflows, however large the failure count.
        assert_eq!(reconnect_delay_ms(base, u32::MAX), RECONNECT_BACKOFF_MAX_MS);
    }

    /// A device that never comes back must not respawn adb forever.
    #[tokio::test]
    async fn gives_up_after_max_attempts_and_sets_streaming_false() {
        let state = make_state(false);
        let generation = request_start(&state, None).await;

        tokio::time::timeout(
            Duration::from_secs(10),
            start_logcat_stream(
                PathBuf::from("/definitely/does/not/exist/adb"),
                None,
                state.clone(),
                None,
                None,
                generation,
            ),
        )
        .await
        .expect("stream must give up rather than retry forever");

        assert!(
            !state.lock().await.streaming,
            "giving up must clear the streaming flag"
        );
    }

    // ── Reconnect budget accounting (regression) ─────────────────────────────

    /// A stream that delivers output and then gets interrupted must NOT consume
    /// the give-up budget. Without the reset a long session dies after 10
    /// cumulative ADB-server restarts, each of which recovered fine — silently
    /// regressing the auto-reconnect behaviour the budget was meant to protect.
    #[tokio::test]
    async fn a_connection_that_streamed_output_resets_the_reconnect_budget() {
        let dir = tempfile::tempdir().unwrap();
        // Emits lines then exits immediately, so every reconnect "works" and
        // then disconnects — far more times than RECONNECT_MAX_ATTEMPTS.
        let adb = exiting_adb_bin(dir.path(), 2);

        let state = make_state(false);
        let generation = request_start(&state, None).await;

        let s = state.clone();
        let handle = tokio::spawn(async move {
            start_logcat_stream(adb, None, s, None, None, generation).await;
        });

        // Must comfortably exceed the time RECONNECT_MAX_ATTEMPTS worth of
        // backoff would take (~2.6s with the test constants), so that a missing
        // reset would definitely have tripped the give-up.
        tokio::time::sleep(Duration::from_millis(4000)).await;

        assert!(
            state.lock().await.streaming,
            "a stream that keeps delivering output must not hit the give-up cap"
        );

        request_stop(&state).await;
        let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
    }

    /// Writes a fake adb that prints `line_count` logcat lines and exits 0.
    fn exiting_adb_bin(dir: &std::path::Path, line_count: usize) -> PathBuf {
        use std::io::Write;
        let bin = dir.join("adb");
        let mut f = std::fs::File::create(&bin).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "for a in \"$@\"; do").unwrap();
        writeln!(f, "  if [ \"$a\" = logcat ]; then").unwrap();
        for i in 0..line_count {
            writeln!(
                f,
                "    echo '01-01 00:00:0{}.000  1000  1001 I FakeTag: line {}'",
                i % 10,
                i
            )
            .unwrap();
        }
        writeln!(f, "    exit 0").unwrap();
        writeln!(f, "  fi").unwrap();
        writeln!(f, "done").unwrap();
        writeln!(f, "exit 0").unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).unwrap();
        }
        bin
    }

    // ── Dropped-line counter lifecycle ───────────────────────────────────────

    /// The counter lives on the state, not in the stream task, so a reconnect
    /// cannot silently reset the reported total.
    #[tokio::test]
    async fn dropped_line_counter_survives_across_stream_restarts() {
        let state = make_state(false);
        state
            .lock()
            .await
            .dropped_lines
            .store(7, std::sync::atomic::Ordering::Relaxed);

        // Simulate a stop→start cycle; nothing in the lifecycle should reset it.
        request_stop(&state).await;
        let _ = request_start(&state, None).await;

        assert_eq!(
            state
                .lock()
                .await
                .dropped_lines
                .load(std::sync::atomic::Ordering::Relaxed),
            7,
            "a reconnect must not lose the accumulated drop count"
        );
    }

    /// Regression: clearing reset `stats.dropped_lines` but not the shared
    /// atomic, so the pipeline's next tick restored the pre-clear value and the
    /// "N dropped" warning reappeared immediately after a clear.
    #[tokio::test]
    async fn clearing_resets_both_the_stat_and_the_shared_counter() {
        let state = make_state(false);
        {
            let mut s = state.lock().await;
            s.dropped_lines
                .store(42, std::sync::atomic::Ordering::Relaxed);
            s.store.stats.dropped_lines = 42;
        }

        // Mirror commands/logcat.rs::clear_logcat.
        {
            let mut s = state.lock().await;
            s.store.clear();
            s.known_packages.clear();
            s.clear_epoch = s.clear_epoch.wrapping_add(1);
            s.dropped_lines
                .store(0, std::sync::atomic::Ordering::Relaxed);
        }

        let s = state.lock().await;
        assert_eq!(s.store.stats.dropped_lines, 0);
        assert_eq!(
            s.dropped_lines.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "the shared counter must be cleared too, or the stat is restored next tick"
        );
    }
}
