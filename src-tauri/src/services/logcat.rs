use crate::models::logcat::{LogcatFilterSpec, LogcatLevel, ProcessedEntry};
use crate::services::log_pipeline::{parse_logcat_line, LogPipeline, PipelineContext};
use crate::services::log_store::LogStore;
use crate::services::log_stream::StreamState;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
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

fn parse_level_str(s: &str) -> LogcatLevel {
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
}

impl LogcatStateInner {
    pub fn new() -> Self {
        LogcatStateInner {
            store: LogStore::new(),
            stream_state: StreamState::new(),
            streaming: false,
            device_serial: None,
            known_packages: HashSet::new(),
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

/// Spawn an `adb logcat` process, parse lines through the processing pipeline,
/// store them in the LogStore, and stream filtered batches to the frontend.
///
/// Architecture (two tasks):
///
///   ┌─────────────────────┐     mpsc::unbounded     ┌───────────────────────────┐
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
pub async fn start_logcat_stream(
    adb_bin: PathBuf,
    device_serial: Option<String>,
    logcat_state: LogcatState,
    app_handle: Option<tauri::AppHandle>,
) {
    use tauri::Emitter;

    let mut cmd = tokio::process::Command::new(&adb_bin);
    if let Some(ref serial) = device_serial {
        cmd.args(["-s", serial]);
    }
    cmd.args(["logcat", "-v", "threadtime"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to start logcat: {}", e);
            let mut state = logcat_state.lock().await;
            state.streaming = false;
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            error!("logcat process has no stdout");
            let mut state = logcat_state.lock().await;
            state.streaming = false;
            return;
        }
    };

    // Channel: reader → pipeline+batcher
    let (tx, mut rx) = mpsc::unbounded_channel::<crate::services::log_pipeline::RawLogLine>();

    // ── Reader task ──────────────────────────────────────────────────────────
    // Parses raw lines only — zero state access, zero mutex.
    // Uses a 64 KB read buffer to batch syscalls at high log rates.
    let reader_handle = tokio::spawn(async move {
        let mut reader = BufReader::with_capacity(64 * 1024, stdout).lines();
        loop {
            match reader.next_line().await {
                Ok(Some(line)) => {
                    if let Some(raw) = parse_logcat_line(&line) {
                        let _ = tx.send(raw);
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

    // ── Pipeline + Batcher task ───────────────────────────────────────────────
    // Owns PipelineContext (no mutex needed for the pipeline itself).
    // Locks LogcatState once per 100ms tick to batch-write + sync packages.
    let logcat_state_pipeline = logcat_state.clone();
    let pipeline_handle = tokio::spawn(async move {
        let logcat_state = logcat_state_pipeline;
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

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

                    // Sync newly discovered packages into state (drain buffer).
                    if !ctx.new_packages.is_empty() {
                        for pkg in ctx.new_packages.drain(..) {
                            state.known_packages.insert(pkg);
                        }
                        state.store.stats.packages_seen = state.known_packages.len();
                    }

                    let filter = state.stream_state.clone_filter();
                    let mut to_emit: Vec<ProcessedEntry> =
                        Vec::with_capacity(processed.len().min(MAX_BATCH_SIZE));

                    for entry in processed {
                        // Clone only if this entry will be emitted.
                        let passes = filter.as_ref().map_or(true, |f| f.matches(&entry));
                        if passes {
                            to_emit.push(entry.clone());
                        }
                        state.store.push(entry); // move into store — zero clone
                    }

                    to_emit
                    // Lock dropped here.
                };

                // Emit the filtered entries in chunks of MAX_BATCH_SIZE.
                for chunk in to_emit.chunks(MAX_BATCH_SIZE) {
                    if let Some(ref handle) = app_handle {
                        if let Err(e) = handle.emit("logcat:entries", chunk) {
                            warn!("Failed to emit logcat batch: {}", e);
                        }
                    }
                }
            }

            // Exit once the channel is closed (reader task finished).
            if rx.is_closed() {
                // Drain any remaining lines after EOF before exiting.
                let mut remaining: Vec<ProcessedEntry> = Vec::new();
                pipeline.run_batch_into(&mut rx, &mut ctx, &mut remaining);
                if !remaining.is_empty() {
                    let to_emit = {
                        let mut state = logcat_state.lock().await;
                        if !ctx.new_packages.is_empty() {
                            for pkg in ctx.new_packages.drain(..) {
                                state.known_packages.insert(pkg);
                            }
                            state.store.stats.packages_seen = state.known_packages.len();
                        }
                        let filter = state.stream_state.clone_filter();
                        let mut to_emit = Vec::with_capacity(remaining.len());
                        for entry in remaining {
                            let passes = filter.as_ref().map_or(true, |f| f.matches(&entry));
                            if passes {
                                to_emit.push(entry.clone());
                            }
                            state.store.push(entry);
                        }
                        to_emit
                    };
                    for chunk in to_emit.chunks(MAX_BATCH_SIZE) {
                        if let Some(ref handle) = app_handle {
                            if let Err(e) = handle.emit("logcat:entries", chunk) {
                                warn!("Failed to emit final logcat batch: {}", e);
                            }
                        }
                    }
                }
                break;
            }
        }
    });

    let _ = reader_handle.await;
    let _ = pipeline_handle.await;

    let mut state = logcat_state.lock().await;
    state.streaming = false;
}

// ── Filter compat helpers ─────────────────────────────────────────────────────

/// Keep HashMap<i32, String> available for the MCP server which needs it.
/// This is computed on-demand from state.known_packages.
pub fn packages_from_known(known: &HashSet<String>) -> HashMap<String, ()> {
    known.iter().map(|p| (p.clone(), ())).collect()
}
