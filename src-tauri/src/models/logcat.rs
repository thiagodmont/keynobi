use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Level ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
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
    pub fn from_char(c: char) -> Self {
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

// ── Kind ──────────────────────────────────────────────────────────────────────

/// Discriminant for special log entries (process lifecycle separators).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum LogcatKind {
    #[default]
    Normal,
    /// The app process died — rendered as a visual separator in the log.
    ProcessDied,
    /// The app process (re)started after previously being seen.
    ProcessStarted,
}

// ── Category ──────────────────────────────────────────────────────────────────

/// High-level classification of a log entry inferred from its tag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum EntryCategory {
    #[default]
    General,
    Network,
    Lifecycle,
    Performance,
    Gc,
    Database,
    Security,
}

// ── Flags ─────────────────────────────────────────────────────────────────────

/// Bitfield constants for `ProcessedEntry.flags`.
/// These are checked on the frontend as `(entry.flags & EntryFlags.CRASH) !== 0`.
pub struct EntryFlags;

impl EntryFlags {
    pub const CRASH: u32 = 1 << 0;
    pub const ANR: u32 = 1 << 1;
    pub const JSON_BODY: u32 = 1 << 2;
    pub const NATIVE_CRASH: u32 = 1 << 3;
}

// ── ProcessedEntry ────────────────────────────────────────────────────────────

/// A fully-processed logcat entry, suitable for IPC transfer to the frontend.
///
/// This replaces the old `LogcatEntry`. All enrichment from the pipeline
/// (package resolution, crash grouping, JSON detection, category) is embedded
/// directly in this struct to minimise IPC overhead.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ProcessedEntry {
    pub id: u64,
    pub timestamp: String,
    pub pid: i32,
    pub tid: i32,
    pub level: LogcatLevel,
    pub tag: String,
    pub message: String,
    pub package: Option<String>,
    pub kind: LogcatKind,

    // ── Backward-compat (derived from flags & EntryFlags::CRASH) ──────────
    pub is_crash: bool,

    // ── Pipeline-enriched fields ───────────────────────────────────────────
    /// Bitfield of `EntryFlags` constants. Check with `(flags & EntryFlags::CRASH) !== 0`.
    pub flags: u32,
    /// High-level category inferred from the tag (e.g. Network, Lifecycle).
    pub category: EntryCategory,
    /// Groups consecutive lines belonging to the same crash/ANR stack trace.
    /// All lines in a single crash share the same `crash_group_id`.
    pub crash_group_id: Option<u64>,
    /// Raw JSON string extracted from the message, if the message contains
    /// valid JSON. The frontend parses this on-demand (only when the user
    /// expands the row) to avoid IPC bloat.
    pub json_body: Option<String>,
}

// ── LogStats ──────────────────────────────────────────────────────────────────

/// Running statistics for the current logcat session.
/// Maintained in O(1) per entry by the LogStore.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LogStats {
    pub total_ingested: u64,
    /// Counts indexed by `LogcatLevel::priority()` (0 = Verbose … 5 = Fatal, 6 = Unknown).
    pub counts_by_level: [u64; 7],
    pub crash_count: u64,
    pub json_count: u64,
    pub packages_seen: usize,
    /// Percentage of the ring buffer currently in use (0.0 – 100.0).
    /// Computed as `(current_len / MAX_LOGCAT_ENTRIES) * 100`.
    pub buffer_usage_pct: f32,
}

// ── LogcatFilterSpec ──────────────────────────────────────────────────────────

/// IPC-serialisable version of a logcat filter, sent from frontend → Rust.
/// The service layer converts this to the internal `LogcatFilter` for matching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LogcatFilterSpec {
    pub min_level: Option<String>,
    pub tag: Option<String>,
    pub text: Option<String>,
    pub package: Option<String>,
    pub only_crashes: bool,
}
