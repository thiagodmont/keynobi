use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

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

/// Filter specification for logcat entries.
#[derive(Debug, Clone, Default)]
pub struct LogcatFilter {
    pub min_level: Option<LogcatLevel>,
    pub tag: Option<String>,
    pub tag_regex: bool,
    pub package: Option<String>,
    pub text: Option<String>,
    pub only_crashes: bool,
}

impl LogcatFilter {
    pub fn matches(&self, entry: &LogcatEntry) -> bool {
        if self.only_crashes && !entry.is_crash {
            return false;
        }
        if let Some(min) = &self.min_level {
            if entry.level.priority() < min.priority() {
                return false;
            }
        }
        if let Some(tag) = &self.tag {
            if !entry.tag.to_lowercase().contains(&tag.to_lowercase()) {
                return false;
            }
        }
        if let Some(text) = &self.text {
            if !entry.message.to_lowercase().contains(&text.to_lowercase())
                && !entry.tag.to_lowercase().contains(&text.to_lowercase())
            {
                return false;
            }
        }
        true
    }
}

/// Ring-buffer holding recent logcat entries (bounded at MAX_ENTRIES).
pub const MAX_LOGCAT_ENTRIES: usize = 50_000;

pub struct LogcatBuffer {
    pub entries: VecDeque<LogcatEntry>,
    pub next_id: u64,
}

impl LogcatBuffer {
    pub fn new() -> Self {
        LogcatBuffer {
            entries: VecDeque::with_capacity(1000),
            next_id: 1,
        }
    }

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

pub type LogcatState = Arc<Mutex<LogcatStateInner>>;

pub struct LogcatStateInner {
    pub buffer: LogcatBuffer,
    pub streaming: bool,
    pub device_serial: Option<String>,
}

impl LogcatStateInner {
    pub fn new() -> Self {
        LogcatStateInner {
            buffer: LogcatBuffer::new(),
            streaming: false,
            device_serial: None,
        }
    }
}

/// Parse a single logcat line in `threadtime` format:
/// `MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG: message`
pub fn parse_logcat_line(line: &str, id: u64) -> Option<LogcatEntry> {
    // Threadtime format: "01-23 12:34:56.789  1234  5678 D MyTag: message here"
    // Fields are separated by whitespace; PID/TID may have extra leading spaces.
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

    // Skip the space after level char
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
    })
}

/// Find the adb binary path from settings.
pub fn find_adb_binary(sdk_path: Option<&str>) -> PathBuf {
    if let Some(sdk) = sdk_path {
        let adb = PathBuf::from(sdk).join("platform-tools").join("adb");
        if adb.is_file() {
            return adb;
        }
    }
    PathBuf::from("adb")
}

/// Spawn an `adb logcat` process and stream entries into the logcat state.
/// Emits `logcat:entries` Tauri events with batches of new entries.
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
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            error!("logcat process has no stdout");
            return;
        }
    };

    let mut reader = BufReader::new(stdout).lines();
    let mut batch: Vec<LogcatEntry> = Vec::new();
    let mut last_emit = std::time::Instant::now();
    const BATCH_INTERVAL_MS: u64 = 100;

    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                let id = {
                    let state = logcat_state.lock().await;
                    state.buffer.next_id
                };
                if let Some(entry) = parse_logcat_line(&line, id) {
                    {
                        let mut state = logcat_state.lock().await;
                        state.buffer.push(entry.clone());
                    }
                    batch.push(entry);
                }

                // Emit in batches every ~100ms to avoid flooding the frontend.
                if last_emit.elapsed().as_millis() >= BATCH_INTERVAL_MS as u128 && !batch.is_empty() {
                    if let Err(e) = app_handle.emit("logcat:entries", &batch) {
                        warn!("Failed to emit logcat entries: {}", e);
                    }
                    batch.clear();
                    last_emit = std::time::Instant::now();
                }
            }
            Ok(None) => {
                debug!("logcat stream ended");
                break;
            }
            Err(e) => {
                error!("logcat read error: {}", e);
                break;
            }
        }
    }

    // Emit any remaining buffered entries.
    if !batch.is_empty() {
        let _ = app_handle.emit("logcat:entries", &batch);
    }

    // Mark streaming as stopped.
    let mut state = logcat_state.lock().await;
    state.streaming = false;
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn filter_by_min_level() {
        let filter = LogcatFilter {
            min_level: Some(LogcatLevel::Warn),
            ..Default::default()
        };
        let entry_debug = LogcatEntry {
            id: 1, timestamp: "".into(), pid: 0, tid: 0,
            level: LogcatLevel::Debug, tag: "T".into(), message: "m".into(), is_crash: false,
        };
        let entry_error = LogcatEntry {
            id: 2, timestamp: "".into(), pid: 0, tid: 0,
            level: LogcatLevel::Error, tag: "T".into(), message: "m".into(), is_crash: false,
        };
        assert!(!filter.matches(&entry_debug));
        assert!(filter.matches(&entry_error));
    }

    #[test]
    fn filter_by_tag() {
        let filter = LogcatFilter {
            tag: Some("myapp".into()),
            ..Default::default()
        };
        let entry_match = LogcatEntry {
            id: 1, timestamp: "".into(), pid: 0, tid: 0,
            level: LogcatLevel::Info, tag: "MyApp".into(), message: "m".into(), is_crash: false,
        };
        let entry_no_match = LogcatEntry {
            id: 2, timestamp: "".into(), pid: 0, tid: 0,
            level: LogcatLevel::Info, tag: "OtherTag".into(), message: "m".into(), is_crash: false,
        };
        assert!(filter.matches(&entry_match));
        assert!(!filter.matches(&entry_no_match));
    }
}
