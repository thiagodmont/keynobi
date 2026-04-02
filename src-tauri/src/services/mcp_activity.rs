/// MCP Activity Logger
///
/// Writes a JSONL log of every tool call, resource read, prompt request, and
/// lifecycle event to `~/.keynobi/mcp-activity.jsonl` so the companion GUI
/// can display a live activity feed regardless of whether the server is running
/// in GUI or headless mode.
///
/// Also manages a PID file (`~/.keynobi/mcp-server.pid`) so the GUI can
/// check whether a headless MCP process is still alive.
use crate::services::settings_manager;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use ts_rs::TS;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Rotate the log when it exceeds this many entries.
const ROTATE_THRESHOLD: usize = 1_000;
/// Keep this many entries after rotation.
const ROTATE_KEEP: usize = 500;

// ── Types ─────────────────────────────────────────────────────────────────────

/// One entry in the MCP activity log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpActivityEntry {
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// Event category: `"tool_call"`, `"resource_read"`, `"prompt"`, or `"lifecycle"`.
    pub kind: String,
    /// Tool name, resource URI, prompt name, or lifecycle event description.
    pub name: String,
    /// Wall-clock duration in milliseconds (present for tool/resource/prompt events).
    pub duration_ms: Option<u64>,
    /// `"ok"` or `"error"`.
    pub status: String,
    /// Brief human-readable summary of the result or error.
    pub summary: Option<String>,
}

/// Live status of the headless MCP server process.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpServerStatus {
    /// Whether the MCP server process appears to be running.
    pub alive: bool,
    /// PID of the server process, if a PID file exists.
    pub pid: Option<u32>,
}

// ── File paths ────────────────────────────────────────────────────────────────

fn activity_log_path() -> PathBuf {
    settings_manager::data_dir().join("mcp-activity.jsonl")
}

fn pid_file_path() -> PathBuf {
    settings_manager::data_dir().join("mcp-server.pid")
}

// ── Activity log ──────────────────────────────────────────────────────────────

/// Append one activity entry to the JSONL log file.
///
/// Opens the file in append mode so concurrent writes from multiple processes
/// each complete as a single atomic line.  Non-fatal: silently returns on any
/// I/O error so a broken log never disrupts the MCP server itself.
pub fn log_activity(entry: &McpActivityEntry) {
    let path = activity_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        if let Ok(line) = serde_json::to_string(entry) {
            let _ = writeln!(file, "{}", line);
        }
    }
}

/// Read the last `limit` entries from the activity log, oldest first.
pub fn read_activity(limit: usize) -> Vec<McpActivityEntry> {
    let path = activity_log_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<McpActivityEntry>(line).ok())
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Trim the activity log to the last `ROTATE_KEEP` entries if it exceeds
/// `ROTATE_THRESHOLD`.  Called once on server startup.
pub fn rotate_activity_log() {
    let path = activity_log_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();

    if lines.len() <= ROTATE_THRESHOLD {
        return;
    }

    let keep = &lines[lines.len() - ROTATE_KEEP..];
    let new_content = keep.join("\n") + "\n";
    let _ = std::fs::write(&path, new_content);
}

/// Truncate the activity log (called from the UI "Clear Log" action).
pub fn clear_activity_log() {
    let path = activity_log_path();
    let _ = std::fs::write(&path, "");
}

// ── PID file ──────────────────────────────────────────────────────────────────

/// Write the current process PID to the PID file.
pub fn write_pid_file() {
    let path = pid_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pid = std::process::id();
    let _ = std::fs::write(&path, pid.to_string());
}

/// Remove the PID file on clean shutdown.
pub fn remove_pid_file() {
    let _ = std::fs::remove_file(pid_file_path());
}

/// Read the PID from the PID file, returning `None` if the file is absent or
/// contains non-numeric data.
pub fn read_pid_file() -> Option<u32> {
    let content = std::fs::read_to_string(pid_file_path()).ok()?;
    content.trim().parse::<u32>().ok()
}

/// Return `true` if the process recorded in the PID file is still alive.
///
/// Uses `kill(pid, 0)` on Unix — this sends no signal but checks whether the
/// process exists and the caller has permission to signal it.
pub fn is_mcp_server_alive() -> bool {
    let Some(pid) = read_pid_file() else {
        return false;
    };

    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) never sends a signal; it just probes existence.
        let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
        ret == 0
    }

    #[cfg(not(unix))]
    {
        // Fallback for non-Unix: assume alive if PID file exists.
        let _ = pid;
        true
    }
}

// ── Convenience constructors ──────────────────────────────────────────────────

impl McpActivityEntry {
    pub fn lifecycle(event: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            kind: "lifecycle".into(),
            name: event.into(),
            duration_ms: None,
            status: "ok".into(),
            summary: None,
        }
    }

    pub fn tool_call(
        name: impl Into<String>,
        duration_ms: u64,
        status: impl Into<String>,
        summary: Option<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            kind: "tool_call".into(),
            name: name.into(),
            duration_ms: Some(duration_ms),
            status: status.into(),
            summary,
        }
    }

    pub fn resource_read(
        uri: impl Into<String>,
        duration_ms: u64,
        status: impl Into<String>,
        summary: Option<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            kind: "resource_read".into(),
            name: uri.into(),
            duration_ms: Some(duration_ms),
            status: status.into(),
            summary,
        }
    }

    pub fn prompt(
        name: impl Into<String>,
        duration_ms: u64,
        status: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            kind: "prompt".into(),
            name: name.into(),
            duration_ms: Some(duration_ms),
            status: status.into(),
            summary: None,
        }
    }
}
