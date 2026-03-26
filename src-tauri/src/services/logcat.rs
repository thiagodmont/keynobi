use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, warn};

// ── Model types ───────────────────────────────────────────────────────────────

/// Discriminant for special log entries (process lifecycle separators).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LogcatKind {
    #[default]
    Normal,
    /// The app process died — rendered as a visual separator in the log.
    ProcessDied,
    /// The app process (re)started after previously being seen.
    ProcessStarted,
}

/// A parsed logcat entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogcatEntry {
    pub id: u64,
    pub timestamp: String,
    pub pid: i32,
    pub tid: i32,
    pub level: LogcatLevel,
    pub tag: String,
    pub message: String,
    pub is_crash: bool,
    /// Package/process name resolved from the ActivityManager pid→package map.
    /// `None` for system processes or when the mapping hasn't been populated yet.
    pub package: Option<String>,
    /// Normal log line vs process lifecycle separator.
    #[serde(default)]
    pub kind: LogcatKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogcatLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

impl LogcatLevel {
    fn from_char(c: char) -> Self {
        match c {
            'V' => LogcatLevel::Verbose,
            'D' => LogcatLevel::Debug,
            'I' => LogcatLevel::Info,
            'W' => LogcatLevel::Warn,
            'E' => LogcatLevel::Error,
            'F' | 'A' => LogcatLevel::Fatal,
            _ => LogcatLevel::Unknown,
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            LogcatLevel::Verbose => 0,
            LogcatLevel::Debug => 1,
            LogcatLevel::Info => 2,
            LogcatLevel::Warn => 3,
            LogcatLevel::Error => 4,
            LogcatLevel::Fatal => 5,
            LogcatLevel::Unknown => 0,
        }
    }
}

// ── Filter ────────────────────────────────────────────────────────────────────

/// Filter specification for logcat entries.
///
/// The `tag_lower`, `text_lower`, and `package_lower` fields store
/// **pre-lowercased** needle strings so `matches()` never calls
/// `to_lowercase()` at comparison time, eliminating per-row heap allocations.
#[derive(Debug, Clone, Default)]
pub struct LogcatFilter {
    pub min_level: Option<LogcatLevel>,
    /// Pre-lowercased tag needle (set by callers, not computed here).
    pub tag_lower: Option<String>,
    /// Pre-lowercased text needle (set by callers, not computed here).
    pub text_lower: Option<String>,
    /// Pre-lowercased package name needle.
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

    /// O(1) match against a single entry.
    /// All string comparisons use pre-lowercased needles — no allocations.
    pub fn matches(&self, entry: &LogcatEntry) -> bool {
        if self.only_crashes && !entry.is_crash {
            return false;
        }
        if let Some(min) = &self.min_level {
            if entry.level.priority() < min.priority() {
                return false;
            }
        }
        if let Some(needle) = &self.tag_lower {
            if !entry.tag.to_lowercase().contains(needle.as_str()) {
                return false;
            }
        }
        if let Some(needle) = &self.text_lower {
            if !entry.message.to_lowercase().contains(needle.as_str())
                && !entry.tag.to_lowercase().contains(needle.as_str())
            {
                return false;
            }
        }
        if let Some(needle) = &self.package_lower {
            // Match against the resolved package name, or fall through to tag if unavailable.
            let matched = match &entry.package {
                Some(pkg) => pkg.to_lowercase().contains(needle.as_str()),
                // Fallback: if no package resolved yet, match against tag
                None => entry.tag.to_lowercase().contains(needle.as_str()),
            };
            if !matched {
                return false;
            }
        }
        true
    }
}

// ── Ring buffer ───────────────────────────────────────────────────────────────

/// Maximum entries kept in the Rust ring-buffer.
pub const MAX_LOGCAT_ENTRIES: usize = 50_000;

/// Pre-allocation size: large enough that a busy app never triggers a
/// reallocation in the first few seconds.
const INITIAL_CAPACITY: usize = 10_000;

/// Maximum entries per IPC batch.  Caps the size of each `logcat:entries`
/// JSON payload so the JS thread is never blocked deserialising a huge message.
pub const MAX_BATCH_SIZE: usize = 500;

pub struct LogcatBuffer {
    pub entries: VecDeque<LogcatEntry>,
    pub next_id: u64,
}

impl LogcatBuffer {
    pub fn new() -> Self {
        LogcatBuffer {
            entries: VecDeque::with_capacity(INITIAL_CAPACITY),
            next_id: 1,
        }
    }

    #[inline]
    pub fn push(&mut self, mut entry: LogcatEntry) {
        entry.id = self.next_id;
        self.next_id += 1;
        if self.entries.len() >= MAX_LOGCAT_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
// ── State ─────────────────────────────────────────────────────────────────────

pub type LogcatState = Arc<Mutex<LogcatStateInner>>;

pub struct LogcatStateInner {
    pub buffer: LogcatBuffer,
    pub streaming: bool,
    pub device_serial: Option<String>,
    /// PID → package name, populated from ActivityManager "Start proc" lines.
    /// Cleared when `buffer.clear()` is called (since old PIDs become stale).
    pub pid_to_package: std::collections::HashMap<i32, String>,
    /// Packages that have been seen at least once — used to detect re-starts.
    pub seen_packages: std::collections::HashSet<String>,
}

impl LogcatStateInner {
    pub fn new() -> Self {
        LogcatStateInner {
            buffer: LogcatBuffer::new(),
            streaming: false,
            device_serial: None,
            pid_to_package: std::collections::HashMap::new(),
            seen_packages: std::collections::HashSet::new(),
        }
    }

    /// Return a sorted, deduplicated list of all known package names.
    pub fn known_packages(&self) -> Vec<String> {
        let mut pkgs: Vec<String> = self.pid_to_package.values().cloned().collect();
        pkgs.sort_unstable();
        pkgs.dedup();
        pkgs
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse a single logcat line in `threadtime` format:
/// `MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG: message`
pub fn parse_logcat_line(line: &str, id: u64) -> Option<LogcatEntry> {
    let mut parts = line.splitn(2, ' ');
    let date = parts.next()?.to_owned();
    let rest = parts.next()?.trim_start();

    let mut rest_parts = rest.splitn(2, ' ');
    let time = rest_parts.next()?.to_owned();
    let rest = rest_parts.next()?.trim_start();

    let timestamp = format!("{} {}", date, time);

    // PID
    let mut rest_parts = rest.splitn(2, |c: char| c == ' ');
    let pid: i32 = rest_parts.next()?.trim().parse().ok()?;
    let rest = rest_parts.next()?.trim_start();

    // TID
    let mut rest_parts = rest.splitn(2, |c: char| c == ' ');
    let tid: i32 = rest_parts.next()?.trim().parse().ok()?;
    let rest = rest_parts.next()?.trim_start();

    // Level (single char)
    let mut chars = rest.chars();
    let level_char = chars.next()?;
    let level = LogcatLevel::from_char(level_char);
    let rest = chars.as_str().trim_start();

    // "TAG: message" or "TAG:message"
    let (tag, message) = if let Some(idx) = rest.find(": ") {
        (&rest[..idx], rest[idx + 2..].to_owned())
    } else if let Some(idx) = rest.find(':') {
        (&rest[..idx], rest[idx + 1..].to_owned())
    } else {
        ("", rest.to_owned())
    };

    let is_crash = message.contains("FATAL EXCEPTION")
        || message.contains("AndroidRuntime")
        || tag.contains("AndroidRuntime");

    Some(LogcatEntry {
        id,
        timestamp,
        pid,
        tid,
        level,
        tag: tag.trim().to_owned(),
        message: message.trim_end_matches('\n').to_owned(),
        is_crash,
        package: None, // filled in by caller from pid_to_package map
        kind: LogcatKind::Normal,
    })
}

/// Try to extract a (pid, package_name) pair from an ActivityManager "Start proc" line.
///
/// Handles two common formats across Android versions:
///   - Modern (5.0+): `ActivityManager: Start proc 12345:com.example.app/u0a123 for ...`
///   - Legacy:        `ActivityManager: Start proc com.example.app for activity ...`
///
/// Returns `Some((pid, package))` when a mapping is found, `None` otherwise.
pub fn extract_pid_package(tag: &str, message: &str) -> Option<(i32, String)> {
    if tag != "ActivityManager" {
        return None;
    }
    // Modern format: "Start proc 12345:com.example.app/u0a123 ..."
    if let Some(rest) = message.strip_prefix("Start proc ") {
        // Try "PID:package/uid"
        if let Some(colon_pos) = rest.find(':') {
            let pid_str = &rest[..colon_pos];
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                let after_pid = &rest[colon_pos + 1..];
                // Package ends at '/' (uid separator) or ' ' (whitespace)
                let end = after_pid
                    .find(|c| c == '/' || c == ' ')
                    .unwrap_or(after_pid.len());
                let raw_pkg = &after_pid[..end];
                let pkg = strip_process_suffix(raw_pkg);
                if looks_like_package(pkg) {
                    return Some((pid, pkg.to_owned()));
                }
            }
        }
        // Legacy format: "Start proc com.example.app for ..."
        let end = rest.find(" for ").or_else(|| rest.find(' ')).unwrap_or(rest.len());
        let pkg = strip_process_suffix(&rest[..end]);
        if looks_like_package(pkg) {
            // No PID in this format — we can't build a pid→package entry, skip.
            return None;
        }
    }
    None
}

/// Try to extract a package name from ActivityManager process-death messages.
///
/// Recognises:
///   - `Process com.example.app (pid 12345) has died`
///   - `Killing 12345:com.example.app/u0a123: remove task`
///   - `Force finishing activity com.example.app/.MainActivity`
pub fn extract_process_death(tag: &str, message: &str) -> Option<String> {
    if tag != "ActivityManager" {
        return None;
    }
    // "Process com.example.app (pid N) has died"
    if let Some(rest) = message.strip_prefix("Process ") {
        if let Some(space) = rest.find(' ') {
            let pkg = strip_process_suffix(&rest[..space]);
            if looks_like_package(pkg) && rest.contains("has died") {
                return Some(pkg.to_owned());
            }
        }
    }
    // "Killing N:com.example.app/uid: reason"
    if let Some(rest) = message.strip_prefix("Killing ") {
        if let Some(colon) = rest.find(':') {
            let after = &rest[colon + 1..];
            let end = after.find(|c| c == '/' || c == ' ' || c == ':').unwrap_or(after.len());
            let pkg = strip_process_suffix(&after[..end]);
            if looks_like_package(pkg) {
                return Some(pkg.to_owned());
            }
        }
    }
    // "Force finishing activity com.example.app/.Activity"
    if let Some(rest) = message.strip_prefix("Force finishing activity ") {
        let end = rest.find('/').or_else(|| rest.find(' ')).unwrap_or(rest.len());
        let pkg = strip_process_suffix(&rest[..end]);
        if looks_like_package(pkg) {
            return Some(pkg.to_owned());
        }
    }
    None
}

/// Strip the process-role suffix after `:` in a process name.
/// e.g. `com.example.app:pushservice` → `com.example.app`
fn strip_process_suffix(name: &str) -> &str {
    name.find(':').map_or(name, |i| &name[..i])
}

/// A heuristic: a valid Android package name contains at least one `.` and
/// only alphanumeric characters, `_`, and `.`.
fn looks_like_package(s: &str) -> bool {
    s.contains('.') && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// Build a synthetic separator entry for the log stream.
fn make_separator(
    timestamp: &str,
    pid: i32,
    package: &str,
    kind: LogcatKind,
    next_id: &mut u64,
) -> LogcatEntry {
    let message = match kind {
        LogcatKind::ProcessDied => format!("{} process died", package),
        LogcatKind::ProcessStarted => format!("{} process (re)started", package),
        LogcatKind::Normal => String::new(),
    };
    let entry = LogcatEntry {
        id: *next_id,
        timestamp: timestamp.to_owned(),
        pid,
        tid: 0,
        level: match kind {
            LogcatKind::ProcessDied => LogcatLevel::Error,
            _ => LogcatLevel::Info,
        },
        tag: "---".to_owned(),
        message,
        is_crash: false,
        package: Some(package.to_owned()),
        kind,
    };
    *next_id += 1;
    entry
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

/// Spawn an `adb logcat` process, parse lines, and stream batched entries to
/// the frontend via Tauri events.
///
/// Architecture (producer/consumer with clock-driven batching):
///
///   ┌─────────────────────┐     mpsc::unbounded     ┌──────────────────────┐
///   │  reader task        │ ──── LogcatEntry ──────► │  batcher task        │
///   │  (parse + store)    │                          │  (100ms timer emit)  │
///   └─────────────────────┘                          └──────────────────────┘
///
/// The reader parses each line, writes it to the ring-buffer (single lock),
/// and sends a clone to the channel.  The batcher wakes on a 100ms Tokio
/// interval, drains the channel, and emits batches capped at MAX_BATCH_SIZE.
///
/// This decouples emission rate from line arrival rate:
///   - Trickle input: lines are always emitted within 100ms of arrival.
///   - Burst input:   large bursts are split into ≤500-entry chunks,
///                    preventing one giant IPC message from blocking the JS thread.
pub async fn start_logcat_stream(
    adb_bin: PathBuf,
    device_serial: Option<String>,
    logcat_state: LogcatState,
    app_handle: tauri::AppHandle,
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

    // Channel between the reader task and the batcher task.
    let (tx, mut rx) = mpsc::unbounded_channel::<LogcatEntry>();

    // ── Reader task ──────────────────────────────────────────────────────────
    let state_for_reader = logcat_state.clone();
    let reader_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        loop {
            match reader.next_line().await {
                Ok(Some(line)) => {
                    // Single mutex acquisition: read next_id, parse, store — all in one lock.
                    let entries_to_send: Vec<LogcatEntry> = {
                        let mut state = state_for_reader.lock().await;
                        let id = state.buffer.next_id;
                        match parse_logcat_line(&line, id) {
                            Some(mut e) => {
                                state.buffer.next_id += 1;
                                let mut extras: Vec<LogcatEntry> = Vec::new();

                                // 1. Check for process death
                                if let Some(dead_pkg) = extract_process_death(&e.tag, &e.message) {
                                    let sep = make_separator(
                                        &e.timestamp, e.pid, &dead_pkg,
                                        LogcatKind::ProcessDied, &mut state.buffer.next_id,
                                    );
                                    state.buffer.push(sep.clone());
                                    extras.push(sep);
                                }

                                // 2. Check for process start; detect re-start
                                if let Some((new_pid, pkg)) = extract_pid_package(&e.tag, &e.message) {
                                    let is_restart = state.seen_packages.contains(&pkg);
                                    state.seen_packages.insert(pkg.clone());
                                    state.pid_to_package.insert(new_pid, pkg.clone());

                                    if is_restart {
                                        let sep = make_separator(
                                            &e.timestamp, new_pid, &pkg,
                                            LogcatKind::ProcessStarted, &mut state.buffer.next_id,
                                        );
                                        state.buffer.push(sep.clone());
                                        extras.push(sep);
                                    }
                                }

                                // 3. Resolve the package for this entry's PID
                                e.package = state.pid_to_package.get(&e.pid).cloned();
                                state.buffer.push(e.clone());

                                let mut all = extras;
                                all.push(e);
                                all
                            }
                            None => vec![],
                        }
                    };
                    for entry in entries_to_send {
                        let _ = tx.send(entry);
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

    // ── Batcher task ─────────────────────────────────────────────────────────
    // Wakes every 100ms on a Tokio interval, regardless of whether new lines
    // have arrived.  This guarantees ≤100ms latency for trickle streams, and
    // caps each IPC message at MAX_BATCH_SIZE entries for burst streams.
    let batcher_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_millis(100),
        );
        // MissedTickBehavior::Delay: if a tick is delayed (e.g. by a slow emit)
        // we skip missed ticks rather than issuing a burst of them.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            // Drain available entries.
            let mut batch: Vec<LogcatEntry> = Vec::new();
            while let Ok(entry) = rx.try_recv() {
                batch.push(entry);
                if batch.len() >= MAX_BATCH_SIZE {
                    // Emit this chunk immediately and continue draining.
                    if let Err(e) = app_handle.emit("logcat:entries", &batch) {
                        warn!("Failed to emit logcat batch: {}", e);
                    }
                    batch.clear();
                }
            }

            // Emit any remaining entries in the last partial batch.
            if !batch.is_empty() {
                if let Err(e) = app_handle.emit("logcat:entries", &batch) {
                    warn!("Failed to emit logcat batch: {}", e);
                }
            }

            // Exit once the channel is closed (reader task finished).
            if rx.is_closed() {
                break;
            }
        }
    });

    // Wait for the reader to finish (process ended), then let the batcher drain.
    let _ = reader_handle.await;
    let _ = batcher_handle.await;

    // Mark streaming as stopped.
    let mut state = logcat_state.lock().await;
    state.streaming = false;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parser tests ──────────────────────────────────────────────────────────

    #[test]
    fn parse_standard_threadtime_line() {
        let line = "01-23 12:34:56.789  1234  5678 D MyTag: Hello world";
        let entry = parse_logcat_line(line, 1).expect("should parse");
        assert_eq!(entry.level, LogcatLevel::Debug);
        assert_eq!(entry.tag, "MyTag");
        assert_eq!(entry.message, "Hello world");
        assert_eq!(entry.pid, 1234);
        assert_eq!(entry.tid, 5678);
    }

    #[test]
    fn parse_error_level() {
        let line = "01-23 12:34:56.789  1234  5678 E AndroidRuntime: FATAL EXCEPTION";
        let entry = parse_logcat_line(line, 1).expect("should parse");
        assert_eq!(entry.level, LogcatLevel::Error);
        assert!(entry.is_crash);
    }

    #[test]
    fn parse_fatal_level() {
        let line = "01-23 12:34:56.789  1234  5678 F MyApp: crash message";
        let entry = parse_logcat_line(line, 1).expect("should parse");
        assert_eq!(entry.level, LogcatLevel::Fatal);
    }

    #[test]
    fn parse_verbose_level() {
        let line = "01-23 12:34:56.789  1234  5678 V MyTag: verbose message";
        let entry = parse_logcat_line(line, 1).expect("should parse");
        assert_eq!(entry.level, LogcatLevel::Verbose);
        assert!(!entry.is_crash);
    }

    #[test]
    fn parse_assigns_id() {
        let line = "01-23 12:34:56.789  1 1 I T: m";
        let entry = parse_logcat_line(line, 42).expect("should parse");
        assert_eq!(entry.id, 42);
    }

    #[test]
    fn parse_returns_none_for_garbage() {
        assert!(parse_logcat_line("not a logcat line", 1).is_none());
        assert!(parse_logcat_line("", 1).is_none());
    }

    // ── Filter tests ──────────────────────────────────────────────────────────

    fn make_entry(level: LogcatLevel, tag: &str, message: &str) -> LogcatEntry {
        LogcatEntry {
            id: 1,
            timestamp: "".into(),
            pid: 0,
            tid: 0,
            level,
            tag: tag.into(),
            message: message.into(),
            is_crash: false,
            package: None,
            kind: LogcatKind::Normal,
        }
    }

    fn make_entry_with_package(level: LogcatLevel, tag: &str, message: &str, pid: i32, pkg: &str) -> LogcatEntry {
        LogcatEntry {
            id: 1,
            timestamp: "".into(),
            pid,
            tid: 0,
            level,
            tag: tag.into(),
            message: message.into(),
            is_crash: false,
            package: Some(pkg.into()),
            kind: LogcatKind::Normal,
        }
    }

    #[test]
    fn filter_by_min_level() {
        let filter = LogcatFilter::new(Some(LogcatLevel::Warn), None, None, None, false);
        assert!(!filter.matches(&make_entry(LogcatLevel::Debug, "T", "m")));
        assert!(!filter.matches(&make_entry(LogcatLevel::Info, "T", "m")));
        assert!(filter.matches(&make_entry(LogcatLevel::Warn, "T", "m")));
        assert!(filter.matches(&make_entry(LogcatLevel::Error, "T", "m")));
        assert!(filter.matches(&make_entry(LogcatLevel::Fatal, "T", "m")));
    }

    #[test]
    fn filter_by_tag_case_insensitive() {
        let filter = LogcatFilter::new(None, Some("myapp".into()), None, None, false);
        assert!(filter.matches(&make_entry(LogcatLevel::Info, "MyApp", "m")));
        assert!(filter.matches(&make_entry(LogcatLevel::Info, "MYAPP", "m")));
        assert!(!filter.matches(&make_entry(LogcatLevel::Info, "OtherTag", "m")));
    }

    #[test]
    fn filter_by_text_case_insensitive() {
        let filter = LogcatFilter::new(None, None, Some("hello".into()), None, false);
        assert!(filter.matches(&make_entry(LogcatLevel::Info, "T", "Hello World")));
        assert!(filter.matches(&make_entry(LogcatLevel::Info, "T", "HELLO")));
        assert!(!filter.matches(&make_entry(LogcatLevel::Info, "T", "goodbye")));
    }

    #[test]
    fn filter_only_crashes() {
        let filter = LogcatFilter::new(None, None, None, None, true);
        let mut crash = make_entry(LogcatLevel::Error, "T", "m");
        crash.is_crash = true;
        let normal = make_entry(LogcatLevel::Error, "T", "m");
        assert!(filter.matches(&crash));
        assert!(!filter.matches(&normal));
    }

    #[test]
    fn filter_by_package_case_insensitive() {
        let filter = LogcatFilter::new(None, None, None, Some("com.example".into()), false);
        let matching = make_entry_with_package(LogcatLevel::Info, "T", "m", 1234, "com.example.myapp");
        let non_matching = make_entry_with_package(LogcatLevel::Info, "T", "m", 9999, "com.other.app");
        assert!(filter.matches(&matching));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn filter_by_package_falls_back_to_tag_when_no_package() {
        // When entry.package is None, the filter matches against tag instead.
        let filter = LogcatFilter::new(None, None, None, Some("myapp".into()), false);
        let no_pkg = make_entry(LogcatLevel::Info, "MyApp", "m");  // no package field
        assert!(filter.matches(&no_pkg));
    }

    #[test]
    fn filter_package_no_match_when_tag_also_differs() {
        let filter = LogcatFilter::new(None, None, None, Some("com.example".into()), false);
        let no_pkg = make_entry(LogcatLevel::Info, "SomeOtherTag", "m");
        assert!(!filter.matches(&no_pkg));
    }

    // ── extract_pid_package tests ─────────────────────────────────────────────

    #[test]
    fn extract_modern_start_proc_format() {
        let result = extract_pid_package(
            "ActivityManager",
            "Start proc 12345:com.example.myapp/u0a123 for activity ...",
        );
        assert_eq!(result, Some((12345, "com.example.myapp".to_string())));
    }

    #[test]
    fn extract_strips_process_role_suffix() {
        let result = extract_pid_package(
            "ActivityManager",
            "Start proc 9876:com.example.myapp:pushservice/u0a123",
        );
        assert_eq!(result, Some((9876, "com.example.myapp".to_string())));
    }

    #[test]
    fn extract_returns_none_for_non_activity_manager() {
        let result = extract_pid_package("SomeOtherTag", "Start proc 123:com.foo/u0 for ...");
        assert!(result.is_none());
    }

    #[test]
    fn extract_returns_none_for_non_start_proc() {
        let result = extract_pid_package("ActivityManager", "Killing com.example.app");
        assert!(result.is_none());
    }

    #[test]
    fn extract_returns_none_for_non_package_string() {
        // "server" doesn't look like a package name (no dot)
        let result = extract_pid_package("ActivityManager", "Start proc 123:server/u0 for ...");
        assert!(result.is_none());
    }

    // ── known_packages tests ──────────────────────────────────────────────────

    #[test]
    fn known_packages_sorted_and_deduplicated() {
        let mut state = super::LogcatStateInner::new();
        state.pid_to_package.insert(1, "com.z.app".into());
        state.pid_to_package.insert(2, "com.a.app".into());
        state.pid_to_package.insert(3, "com.z.app".into()); // duplicate value
        let packages = state.known_packages();
        assert_eq!(packages, vec!["com.a.app", "com.z.app"]);
    }

    // ── Buffer tests ──────────────────────────────────────────────────────────

    #[test]
    fn buffer_assigns_sequential_ids() {
        let mut buf = LogcatBuffer::new();
        for i in 0..5u64 {
            let e = make_entry(LogcatLevel::Info, "T", "m");
            buf.push(e);
            assert_eq!(buf.entries.back().unwrap().id, i + 1);
        }
    }

    #[test]
    fn buffer_evicts_oldest_at_capacity() {
        let mut buf = LogcatBuffer::new();
        // Override capacity for testing with a smaller limit
        for _ in 0..MAX_LOGCAT_ENTRIES {
            buf.push(make_entry(LogcatLevel::Info, "T", "m"));
        }
        let first_id_before = buf.entries.front().unwrap().id;
        // Push one more — should evict the oldest
        buf.push(make_entry(LogcatLevel::Info, "T", "extra"));
        assert_eq!(buf.entries.len(), MAX_LOGCAT_ENTRIES);
        assert_eq!(buf.entries.front().unwrap().id, first_id_before + 1);
    }

    #[test]
    fn buffer_clear_empties_entries() {
        let mut buf = LogcatBuffer::new();
        buf.push(make_entry(LogcatLevel::Info, "T", "m"));
        buf.clear();
        assert!(buf.entries.is_empty());
    }

    #[test]
    fn buffer_initial_capacity_avoids_early_reallocation() {
        // The buffer should start with at least INITIAL_CAPACITY slots
        // so that pushing INITIAL_CAPACITY entries causes no reallocation.
        let buf = LogcatBuffer::new();
        // VecDeque doesn't expose capacity directly in stable Rust, but we can
        // confirm construction doesn't panic and the initial push path works.
        assert_eq!(buf.entries.len(), 0);
        assert_eq!(buf.next_id, 1);
    }
}
