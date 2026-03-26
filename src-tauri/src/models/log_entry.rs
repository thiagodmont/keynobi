use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use ts_rs::TS;

static LOG_ENTRY_ID: AtomicU32 = AtomicU32::new(0);

/// A single structured log entry emitted by any log source (LSP server,
/// build system, Logcat, etc.).  The `source` field identifies the origin
/// so multiple sources can share the same [`LogViewer`] component.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Monotonically-increasing counter used as a stable React/SolidJS key.
    pub id: u32,
    /// RFC 3339 timestamp of when the entry was received by the host process.
    pub timestamp: String,
    pub level: LogLevel,
    /// Identifies the log source, e.g. `"lsp:server"`, `"lsp:stderr"`,
    /// `"logcat:MyTag"`, `"build"`.
    pub source: String,
    pub message: String,
}

impl LogEntry {
    pub fn new(level: LogLevel, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: LOG_ENTRY_ID.fetch_add(1, Ordering::Relaxed),
            timestamp: Utc::now().to_rfc3339(),
            level,
            source: source.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
