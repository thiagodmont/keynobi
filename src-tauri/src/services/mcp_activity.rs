/// MCP Activity Logger
///
/// Writes a JSONL log of every tool call, resource read, prompt request, and
/// lifecycle event to `~/.keynobi/mcp-activity.jsonl` so the companion GUI
/// can display a live activity feed for sessions attached to the app and for
/// standalone servers alike.
///
/// The app and every standalone server append to the same file, so appends,
/// rotation, and clearing all run under `settings_manager::with_data_lock`.
use crate::services::settings_manager;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use ts_rs::TS;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Rotate the log once it grows past this size.
const ROTATE_THRESHOLD_BYTES: u64 = 256 * 1024;
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
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    /// `"ok"` or `"error"`.
    pub status: String,
    /// Brief human-readable summary of the result or error.
    pub summary: Option<String>,
}

// ── File paths ────────────────────────────────────────────────────────────────

fn activity_log_path() -> PathBuf {
    settings_manager::data_dir().join("mcp-activity.jsonl")
}

// ── Activity log ──────────────────────────────────────────────────────────────

/// When the log is rotated and how much of it is kept.
#[derive(Debug, Clone, Copy)]
struct RotationLimits {
    threshold_bytes: u64,
    keep: usize,
}

const ROTATION: RotationLimits = RotationLimits {
    threshold_bytes: ROTATE_THRESHOLD_BYTES,
    keep: ROTATE_KEEP,
};

/// Append one activity entry to the JSONL log file, rotating it when it has
/// grown past [`ROTATE_THRESHOLD_BYTES`].
///
/// Non-fatal: silently returns on any I/O error so a broken log never
/// disrupts the MCP server itself. Must not be called while holding the data
/// lock (it takes it).
pub fn log_activity(entry: &McpActivityEntry) {
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    append_at(&activity_log_path(), &line, ROTATION);
}

fn append_at(path: &Path, line: &str, limits: RotationLimits) {
    let _ = settings_manager::with_data_lock(|| {
        append_line(path, line);
        rotate_if_needed_locked(path, limits);
    });
}

fn rotate_if_needed_locked(path: &Path, limits: RotationLimits) {
    let too_big = std::fs::metadata(path)
        .map(|m| m.len() > limits.threshold_bytes)
        .unwrap_or(false);
    if too_big {
        rotate_locked(path, limits.keep);
    }
}

fn append_line(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(format!("{line}\n").as_bytes());
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

/// Trim the activity log to the last [`ROTATE_KEEP`] entries if it has grown
/// past [`ROTATE_THRESHOLD_BYTES`]. Called when a server starts; appends
/// rotate on their own.
pub fn rotate_activity_log() {
    let path = activity_log_path();
    let _ = settings_manager::with_data_lock(|| rotate_if_needed_locked(&path, ROTATION));
}

/// Keep the last `keep` entries by writing them to a temporary file and
/// renaming it over the log. The caller holds the data lock, and every
/// appender takes it too, so no append can land in the file being replaced.
fn rotate_locked(path: &Path, keep: usize) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let kept = &lines[lines.len().saturating_sub(keep)..];
    let mut new_content = kept.join("\n");
    new_content.push('\n');
    let tmp = settings_manager::unique_tmp_path(path);
    if std::fs::write(&tmp, new_content).is_err() || std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("Failed to rotate the MCP activity log");
    }
}

/// Truncate the activity log (called from the UI "Clear Log" action).
pub fn clear_activity_log() {
    let path = activity_log_path();
    let _ = settings_manager::with_data_lock(|| std::fs::write(&path, ""));
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

    pub fn prompt(name: impl Into<String>, duration_ms: u64, status: impl Into<String>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize) -> String {
        serde_json::to_string(&McpActivityEntry::lifecycle(format!("entry {n}"))).unwrap()
    }

    #[test]
    fn rotation_keeps_the_newest_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-activity.jsonl");
        let limits = RotationLimits {
            threshold_bytes: 1,
            keep: 3,
        };
        for n in 0..10 {
            append_at(&path, &line(n), limits);
        }
        let content = std::fs::read_to_string(&path).unwrap();
        let names: Vec<String> = content
            .lines()
            .map(|l| serde_json::from_str::<McpActivityEntry>(l).unwrap().name)
            .collect();
        assert_eq!(names, ["entry 7", "entry 8", "entry 9"]);
    }

    /// Rotation used to read the log and rewrite it in place while other
    /// processes kept appending, so a line appended between the read and the
    /// write was lost. With a keep limit larger than the log, rotation must
    /// not drop anything, however it interleaves with appends.
    #[test]
    fn appends_are_not_lost_while_the_log_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-activity.jsonl");
        let limits = RotationLimits {
            threshold_bytes: 1,
            keep: usize::MAX,
        };
        const WRITERS: usize = 4;
        const PER_WRITER: usize = 100;
        std::thread::scope(|scope| {
            for w in 0..WRITERS {
                let path = &path;
                scope.spawn(move || {
                    for n in 0..PER_WRITER {
                        append_at(path, &line(w * PER_WRITER + n), limits);
                    }
                });
            }
            let path = &path;
            scope.spawn(move || {
                for _ in 0..200 {
                    let _ = settings_manager::with_data_lock(|| rotate_locked(path, usize::MAX));
                }
            });
        });
        let content = std::fs::read_to_string(&path).unwrap();
        let mut names: Vec<String> = content
            .lines()
            .map(|l| serde_json::from_str::<McpActivityEntry>(l).unwrap().name)
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), WRITERS * PER_WRITER);
    }
}
