use crate::models::build::{BuildError, BuildErrorSeverity, BuildRecord, BuildResult, BuildStatus};
use crate::services::build_parser;
use crate::services::process_manager::{self, ProcessId, ProcessManager};
use crate::services::settings_manager::{data_dir, unique_tmp_path, with_data_lock_in};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;

// Re-export parsing functions for backward compatibility.
pub use build_parser::{parse_build_duration, parse_build_line};

/// Maximum number of build records kept in history (bounded collection).
pub const MAX_HISTORY: usize = 10;

const BUILD_HISTORY_FILE: &str = "build-history.json";
/// Maximum number of build records to persist across sessions.
const MAX_PERSISTED_HISTORY: usize = 20;

/// Persist the most recent build summaries to ~/.keynobi/build-history.json.
/// Uses atomic write (temp + rename) so a crash mid-save can't corrupt the file.
/// Callers hold the data lock: other processes append to the same file.
fn save_build_history_to(dir: &Path, history: &VecDeque<BuildRecord>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create data directory: {e}"))?;
    let path = dir.join(BUILD_HISTORY_FILE);
    let recent: Vec<&BuildRecord> = history.iter().rev().take(MAX_PERSISTED_HISTORY).collect();
    let json = serde_json::to_string_pretty(&recent)
        .map_err(|e| format!("Failed to serialize build history: {e}"))?;
    let tmp = unique_tmp_path(&path);
    std::fs::write(&tmp, &json).map_err(|e| format!("Failed to write build history: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to save build history: {e}")
    })
}

/// Build IDs name the log files, so a new ID must be above every ID in the
/// history and every log still on disk, whichever process wrote them.
fn next_build_id(history: &VecDeque<BuildRecord>, build_log_dir: &Path) -> u32 {
    let max_logged = std::fs::read_dir(build_log_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| build_log_id(&entry.path()))
        .max()
        .unwrap_or(0);
    let max_recorded = history.iter().map(|r| r.id).max().unwrap_or(0);
    max_logged.max(max_recorded).saturating_add(1)
}

fn build_log_id(path: &Path) -> Option<u32> {
    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return None;
    }
    path.file_stem()?
        .to_str()?
        .strip_prefix("build-")?
        .parse()
        .ok()
}

/// Append `record` to the persisted history, save its log, and rotate logs, all
/// under the data lock. The history is re-read first so records appended by
/// another process are kept, and the record's ID is allocated here. Returns the
/// ID and the history as persisted.
fn persist_build_record_in(
    dir: &Path,
    mut record: BuildRecord,
    raw_lines: &VecDeque<String>,
    retention_days: u32,
    max_folder_mb: u32,
) -> Result<(u32, VecDeque<BuildRecord>), String> {
    with_data_lock_in(dir, || {
        let build_log_dir = dir.join("build-logs");
        let mut history = load_build_history_from(dir);
        let id = next_build_id(&history, &build_log_dir);
        record.id = id;
        history.push_back(record);
        while history.len() > MAX_HISTORY {
            history.pop_front();
        }
        save_build_history_to(dir, &history)?;
        save_build_log_to(id, raw_lines, &build_log_dir);
        rotate_build_logs(&build_log_dir, retention_days, max_folder_mb, &history);
        Ok((id, history))
    })?
}

/// Rotate build logs against the persisted history, under the data lock so a
/// build another process is recording keeps its log.
pub fn rotate_persisted_build_logs(retention_days: u32, max_folder_mb: u32) -> Result<(), String> {
    let dir = data_dir();
    with_data_lock_in(&dir, || {
        let history = load_build_history_from(&dir);
        rotate_build_logs(
            &dir.join("build-logs"),
            retention_days,
            max_folder_mb,
            &history,
        );
    })
}

/// Records from both sides, by ID, oldest first, capped at `MAX_HISTORY`.
/// IDs are allocated in order under the data lock, so ID order is the order
/// builds were recorded.
fn merge_history(
    memory: &VecDeque<BuildRecord>,
    persisted: VecDeque<BuildRecord>,
) -> VecDeque<BuildRecord> {
    let mut by_id: std::collections::BTreeMap<u32, BuildRecord> =
        memory.iter().map(|r| (r.id, r.clone())).collect();
    for record in persisted {
        by_id.insert(record.id, record);
    }
    let skip = by_id.len().saturating_sub(MAX_HISTORY);
    by_id.into_values().skip(skip).collect()
}

/// Load build history from disk. Returns empty VecDeque if file is missing or corrupt.
///
/// The file is written newest-first (see `save_build_history_to`), so we reverse
/// the loaded entries to restore oldest-first order — matching the invariant that
/// `push_back` adds the newest record and `pop_front` evicts the oldest.
pub fn load_build_history() -> VecDeque<BuildRecord> {
    load_build_history_from(&data_dir())
}

fn load_build_history_from(dir: &Path) -> VecDeque<BuildRecord> {
    let path = dir.join(BUILD_HISTORY_FILE);
    if !path.exists() {
        return VecDeque::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<Vec<BuildRecord>>(&content)
            .map(|v| {
                // Reverse: file is newest-first, VecDeque must be oldest-first.
                let mut deque: VecDeque<BuildRecord> = v.into_iter().rev().collect();
                // Trim to MAX_HISTORY so the in-memory cap is enforced immediately.
                while deque.len() > MAX_HISTORY {
                    deque.pop_front();
                }
                deque
            })
            .unwrap_or_default(),
        Err(_) => VecDeque::new(),
    }
}

/// Maximum number of raw build output lines retained for MCP `get_build_log`.
pub const MAX_BUILD_LOG: usize = 5_000;

/// Maximum number of structured errors/warnings retained per build. Verbose
/// Gradle runs can emit hundreds of thousands of warning lines (lint,
/// deprecation, per-class); the raw log is capped at `MAX_BUILD_LOG`, so this
/// list is capped too — it is cloned into every history record and serialized
/// into the on-disk build history. Once the cap is reached, one extra slot is
/// used for a truncation notice.
pub const MAX_BUILD_ERRORS: usize = 1_000;

const TRUNCATION_NOTICE_PREFIX: &str = "Older diagnostics truncated";

fn truncation_notice() -> BuildError {
    BuildError {
        message: format!(
            "{TRUNCATION_NOTICE_PREFIX}: more than {MAX_BUILD_ERRORS} errors/warnings were emitted."
        ),
        file: None,
        line: None,
        col: None,
        severity: BuildErrorSeverity::Warning,
    }
}

/// Push a parsed diagnostic into the build's error accumulator.
///
/// The buffer keeps the MOST RECENT diagnostics (like `push_build_log` keeps
/// the newest raw lines): once full, the oldest entry is evicted so late
/// root-cause errors are not lost to early lint/deprecation noise. The first
/// time an entry is evicted, a truncation notice is inserted at the front.
pub fn push_build_error(buf: &mut Vec<BuildError>, error: BuildError) {
    if buf.len() < MAX_BUILD_ERRORS {
        buf.push(error);
        return;
    }

    // Length invariant: the buffer holds MAX_BUILD_ERRORS entries while only
    // real diagnostics have been seen, and exactly MAX_BUILD_ERRORS + 1 once
    // the truncation notice has been inserted. Detecting the notice by length
    // avoids false positives from user content (e.g. a diagnostic that happens
    // to start with the notice text).
    let has_notice = buf.len() > MAX_BUILD_ERRORS;
    if !has_notice {
        buf.insert(0, truncation_notice());
    }
    // Evict the oldest diagnostic (right after the notice) to make room.
    buf.remove(1);
    buf.push(error);
}

pub struct BuildStateInner {
    /// Process ID of the currently running Gradle process, if any.
    pub current_build: Option<ProcessId>,
    /// True after a build request reserves the slot and before the process ID is known.
    pub starting: bool,
    /// The most recently started run. A run that finishes after a newer one has
    /// started (for example a cancelled build still shutting down) must not
    /// touch the shared status, errors, or cancellable process of that newer run.
    pub latest_run: Option<ProcessId>,
    /// Current build status.
    pub status: BuildStatus,
    /// Ring-buffer of past build records.
    pub history: VecDeque<BuildRecord>,
    /// Errors accumulated from the current (or last) build.
    pub current_errors: Vec<BuildError>,
}

impl Default for BuildStateInner {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildStateInner {
    pub fn new() -> Self {
        let history = load_build_history();
        Self {
            current_build: None,
            starting: false,
            latest_run: None,
            status: BuildStatus::Idle,
            history,
            current_errors: vec![],
        }
    }
}

/// Synchronously-accessible ring-buffer of raw build output lines.
///
/// Uses a `std::sync::Mutex` (not tokio) so the `on_line` process callback can
/// push lines without `await`. Capped at `MAX_BUILD_LOG` entries.
pub type BuildLog = Arc<std::sync::Mutex<VecDeque<String>>>;

/// Points at the log of the most recently started run.
///
/// Each run writes to its own [`BuildLog`], so a cancelled run that is still
/// printing while it shuts down cannot write into the next run's log, and its
/// own history entry keeps its own lines.
#[derive(Clone, Default)]
pub struct BuildLogSlot(Arc<StdMutex<BuildLog>>);

impl BuildLogSlot {
    /// Install an empty log for a new run and return it.
    pub fn start_run(&self) -> BuildLog {
        let log = BuildLog::default();
        match self.0.lock() {
            Ok(mut current) => *current = log.clone(),
            Err(poisoned) => *poisoned.into_inner() = log.clone(),
        }
        log
    }

    /// The log of the most recently started run.
    pub fn current(&self) -> BuildLog {
        match self.0.lock() {
            Ok(current) => current.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

pub struct BuildState {
    pub inner: Arc<Mutex<BuildStateInner>>,
    /// Raw output of the most recent run — accessible from both sync callbacks and async MCP tools.
    pub build_log: BuildLogSlot,
    /// Set synchronously in the same task tick immediately after `spawn` returns (no `.await`
    /// before this), so `cancel_build` can always resolve the `ProcessId` even if it runs
    /// before `inner.current_build` is updated (otherwise cancel saw `None` and did not kill Gradle).
    pub active_process_id: Arc<StdMutex<Option<ProcessId>>>,
}

impl BuildState {
    pub fn new() -> Self {
        BuildState {
            inner: Arc::new(Mutex::new(BuildStateInner::new())),
            build_log: BuildLogSlot::default(),
            active_process_id: Arc::new(StdMutex::new(None)),
        }
    }

    pub fn take_active_process_id(&self) -> Option<ProcessId> {
        match self.active_process_id.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }

    /// Clear the cancellable process only if it is still `run`, so a finishing
    /// run cannot make a newer run uncancellable.
    pub fn release_active_process_id(&self, run: ProcessId) {
        let mut guard = match self.active_process_id.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *guard == Some(run) {
            *guard = None;
        }
    }

    pub fn set_active_process_id(&self, pid: Option<ProcessId>) {
        match self.active_process_id.lock() {
            Ok(mut guard) => *guard = pid,
            Err(poisoned) => *poisoned.into_inner() = pid,
        }
    }
}

impl Clone for BuildState {
    fn clone(&self) -> Self {
        BuildState {
            inner: self.inner.clone(),
            build_log: self.build_log.clone(),
            active_process_id: self.active_process_id.clone(),
        }
    }
}

impl Default for BuildState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn push_build_log(build_log: &BuildLog, line: String) {
    if let Ok(mut log) = build_log.lock() {
        if log.len() >= MAX_BUILD_LOG {
            log.pop_front();
        }
        log.push_back(line);
    }
}

/// Core of save_build_log — accepts a target directory for testability.
pub fn save_build_log_to(id: u32, raw_lines: &VecDeque<String>, build_log_dir: &Path) {
    if std::fs::create_dir_all(build_log_dir).is_err() {
        return;
    }
    let path = build_log_dir.join(format!("build-{id}.jsonl"));
    let tmp = unique_tmp_path(&path);

    let mut content = String::new();
    for raw in raw_lines.iter().take(MAX_BUILD_LOG) {
        let line = parse_build_line(raw);
        if let Ok(json) = serde_json::to_string(&line) {
            content.push_str(&json);
            content.push('\n');
        }
    }

    if std::fs::write(&tmp, &content).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Persist the structured build log for a completed build to ~/.keynobi/build-logs/build-{id}.jsonl.
/// Re-parses each raw line into a BuildLine and writes as JSON Lines. Best-effort — failures are silent.
pub fn save_build_log(id: u32, raw_lines: &VecDeque<String>) {
    save_build_log_to(id, raw_lines, &data_dir().join("build-logs"));
}

/// Rotate the build-logs directory:
/// 1. Age — delete .jsonl files older than retention_days.
/// 2. Orphans — delete build-{id}.jsonl whose ID is not in history.
/// 3. Size cap — if total folder size > max_folder_mb, delete oldest by mtime until under cap.
///
/// All operations are best-effort; individual failures are silently ignored.
pub fn rotate_build_logs(
    build_log_dir: &Path,
    retention_days: u32,
    max_folder_mb: u32,
    history: &VecDeque<BuildRecord>,
) {
    if !build_log_dir.is_dir() {
        return;
    }

    let now = std::time::SystemTime::now();
    // retention_days = 0 means disabled (no age-based deletion).
    let retention_secs = u64::from(retention_days).checked_mul(86_400);
    let valid_ids: std::collections::HashSet<u32> = history.iter().map(|r| r.id).collect();

    // Collect all .jsonl files with their metadata.
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(build_log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            files.push((path, mtime));
        }
    }

    // Pass 1+2: Age and orphans — combined to avoid double-deleting files that match both.
    for (path, mtime) in &files {
        let aged = retention_secs.is_some_and(|limit| {
            now.duration_since(*mtime)
                .map(|d| d.as_secs() > limit)
                .unwrap_or(false)
        });
        let is_orphan = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| stem.strip_prefix("build-"))
            .and_then(|id_str| id_str.parse::<u32>().ok())
            .is_some_and(|id| !valid_ids.contains(&id));
        if aged || is_orphan {
            let _ = std::fs::remove_file(path);
        }
    }

    // Re-collect surviving files for size-cap pass.
    let mut surviving: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(build_log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let meta = entry.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            surviving.push((path, size, mtime));
        }
    }

    // Pass 3: Size cap.
    let max_bytes = u64::from(max_folder_mb) * 1024 * 1024;
    let total: u64 = surviving.iter().map(|(_, size, _)| *size).sum();
    if total > max_bytes {
        surviving.sort_by_key(|(_, _, mtime)| *mtime); // oldest first
        let mut running = total;
        for (path, size, _) in &surviving {
            if running <= max_bytes {
                break;
            }
            if std::fs::remove_file(path).is_ok() {
                running = running.saturating_sub(*size);
            }
        }
    }
}

/// Locate the `gradlew` wrapper relative to `gradle_root`.
pub fn find_gradlew(gradle_root: &Path) -> Option<PathBuf> {
    let gradlew = gradle_root.join("gradlew");
    if gradlew.is_file() {
        Some(gradlew)
    } else {
        None
    }
}

/// Walk a directory up to `max_depth` levels, returning all matching files.
fn walk_dir_for_apk(base: &Path, max_depth: u32) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if max_depth == 0 || !base.is_dir() {
        return results;
    }
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walk_dir_for_apk(&path, max_depth - 1));
            } else {
                results.push(path);
            }
        }
    }
    results
}

/// One installable APK under `app/build/outputs/apk`, and the variant it belongs to.
struct ApkCandidate {
    path: PathBuf,
    /// From `output-metadata.json` when present, else the directory segments
    /// below `apk/` joined (`paid/debug` → `paiddebug`), lowercased.
    variant: String,
}

/// Directory holding the APK outputs of the `app` module.
fn apk_outputs_dir(gradle_root: &Path) -> PathBuf {
    gradle_root
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
}

/// The AGP `output-metadata.json` written next to a variant's APKs.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputMetadata {
    #[serde(default)]
    application_id: Option<String>,
    variant_name: String,
    #[serde(default)]
    elements: Vec<OutputMetadataElement>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputMetadataElement {
    output_file: String,
}

fn read_output_metadata(dir: &Path) -> Option<OutputMetadata> {
    let text = std::fs::read_to_string(dir.join("output-metadata.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The application ID AGP recorded for the variant that produced `apk`,
/// including any `applicationIdSuffix`.
pub fn application_id_from_output_metadata(apk: &Path) -> Option<String> {
    read_output_metadata(apk.parent()?)?.application_id
}

/// Max built variants read when collecting application IDs from the build outputs.
const MAX_BUILT_APPLICATION_IDS: usize = 64;

/// The application IDs AGP recorded for every variant built into the `app`
/// module's APK outputs, including any `applicationIdSuffix`.
pub fn built_application_ids(gradle_root: &Path) -> Vec<String> {
    let mut dirs: Vec<PathBuf> = walk_dir_for_apk(&apk_outputs_dir(gradle_root), 6)
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("output-metadata.json"))
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    let mut ids: Vec<String> = dirs
        .iter()
        .take(MAX_BUILT_APPLICATION_IDS)
        .filter_map(|dir| read_output_metadata(dir)?.application_id)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn collect_apk_candidates(base: &Path) -> Vec<ApkCandidate> {
    let is_installable = |p: &Path| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        p.extension().and_then(|e| e.to_str()) == Some("apk") && !name.ends_with("-unaligned.apk")
    };
    let mut dirs: Vec<PathBuf> = walk_dir_for_apk(base, 6)
        .into_iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();

    let mut candidates = Vec::new();
    for dir in dirs {
        if let Some(meta) = read_output_metadata(&dir) {
            for element in meta.elements {
                let path = dir.join(&element.output_file);
                if path.is_file() && is_installable(&path) {
                    candidates.push(ApkCandidate {
                        path,
                        variant: meta.variant_name.to_lowercase(),
                    });
                }
            }
            continue;
        }
        let variant: String = dir
            .strip_prefix(base)
            .map(|rel| {
                rel.components()
                    .filter_map(|c| c.as_os_str().to_str())
                    .collect::<String>()
                    .to_lowercase()
            })
            .unwrap_or_default();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for path in entries.flatten().map(|e| e.path()) {
                if path.is_file() && is_installable(&path) {
                    candidates.push(ApkCandidate {
                        path,
                        variant: variant.clone(),
                    });
                }
            }
        }
    }
    candidates
}

/// Resolve the APK that building `variant_name` produced.
///
/// Standard AGP layout:
///   `{gradle_root}/app/build/outputs/apk/{buildType}/app-{buildType}.apk`
///   or with flavors:
///   `{gradle_root}/app/build/outputs/apk/{flavor}/{buildType}/app-{flavor}-{buildType}.apk`
///
/// A directory's `output-metadata.json` (written by AGP) decides the variant
/// and file; without it the directory segments below `apk/` must spell the
/// variant (`paid/debug` for `paidDebug`). Only that variant's APKs are
/// considered: installing a stale output of another flavor or build type is
/// worse than failing. Within the variant a signed APK wins over an
/// `-unsigned` one; `-unaligned` APKs are never installable.
///
/// An empty `variant_name` accepts the single APK present, if there is exactly one.
///
/// # Errors
/// An actionable message when there are no outputs, no APK for the variant,
/// or more than one candidate (for example split APKs).
pub fn find_output_apk(gradle_root: &Path, variant_name: &str) -> Result<PathBuf, String> {
    let base = apk_outputs_dir(gradle_root);
    let candidates = collect_apk_candidates(&base);
    if candidates.is_empty() {
        return Err(format!(
            "No APK found under {}. Build the variant first (for example assembleDebug).",
            base.display()
        ));
    }

    let wanted = variant_name.to_lowercase();
    let matching: Vec<&ApkCandidate> = candidates
        .iter()
        .filter(|c| wanted.is_empty() || c.variant == wanted)
        .collect();
    if matching.is_empty() {
        let mut available: Vec<&str> = candidates.iter().map(|c| c.variant.as_str()).collect();
        available.sort_unstable();
        available.dedup();
        return Err(format!(
            "No APK for variant '{variant_name}' under {}. Found outputs for: {}. \
             Build that variant first.",
            base.display(),
            available.join(", ")
        ));
    }

    let is_signed = |c: &&&ApkCandidate| {
        !c.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .contains("-unsigned")
    };
    let signed: Vec<&&ApkCandidate> = matching.iter().filter(is_signed).collect();
    let preferred: Vec<&ApkCandidate> = if signed.is_empty() {
        matching
    } else {
        signed.into_iter().copied().collect()
    };
    match preferred.as_slice() {
        [only] => Ok(only.path.clone()),
        many => Err(format!(
            "More than one APK matches variant '{variant_name}': {}. \
             Split or multi-output APKs are not supported yet; install one with install_apk.",
            many.iter()
                .map(|c| c.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

// ── Build completion ──────────────────────────────────────────────────────────

/// Finalize a build and, when running with a GUI attached, notify the frontend.
///
/// Headless MCP runs pass `None` and simply record history. With a handle, the
/// UI learns about builds an AI agent started — previously the MCP path recorded
/// state but emitted nothing, so the Build panel silently went stale.
pub async fn emit_build_complete(
    build_state: &BuildState,
    app_handle: Option<&tauri::AppHandle>,
    finalization: BuildFinalization,
) -> BuildCompleteEvent {
    let event = finalize_completed_build(build_state, finalization).await;
    if let Some(handle) = app_handle {
        use tauri::Emitter;
        let _ = handle.emit("build:complete", event.clone());
    }
    event
}

/// Payload of the `build:complete` event.
///
/// Emitted by every path that runs a build — the Tauri command layer and the
/// MCP server — so the UI reflects builds an AI agent started too.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCompleteEvent {
    /// The run this event belongs to (the Gradle process ID the build started with).
    pub run_id: ProcessId,
    pub success: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub task: String,
}

pub struct BuildFinalization {
    /// The Gradle process ID of the run being finalized.
    pub run_id: ProcessId,
    /// That run's own output, from [`BuildLogSlot::start_run`].
    pub log: BuildLog,
    pub task: String,
    pub started_at: String,
    pub project_root: Option<String>,
    pub success: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub errors: Vec<BuildError>,
}

pub async fn finalize_completed_build(
    build_state: &BuildState,
    finalization: BuildFinalization,
) -> BuildCompleteEvent {
    let error_count = finalization
        .errors
        .iter()
        .filter(|e| e.severity == BuildErrorSeverity::Error)
        .count() as u32;
    let warn_count = finalization
        .errors
        .iter()
        .filter(|e| e.severity == BuildErrorSeverity::Warning)
        .count() as u32;
    let result = BuildResult {
        success: finalization.success,
        duration_ms: finalization.duration_ms,
        error_count,
        warning_count: warn_count,
    };

    record_build_result(
        build_state,
        finalization.run_id,
        &finalization.log,
        finalization.task.clone(),
        finalization.started_at,
        result,
        finalization.cancelled,
        finalization.errors,
        finalization.project_root,
    )
    .await;

    BuildCompleteEvent {
        run_id: finalization.run_id,
        success: finalization.success,
        cancelled: finalization.cancelled,
        duration_ms: finalization.duration_ms,
        error_count,
        warning_count: warn_count,
        task: finalization.task,
    }
}

/// Release the build slot after a failed spawn.
///
/// MUST be called on every early return between `try_reserve_build_slot` and the
/// point where finalization takes over — otherwise `starting` stays true and
/// every subsequent build, from either front door, is refused for the lifetime
/// of the process.
pub fn mark_build_spawn_failed(bs: &mut BuildStateInner) {
    bs.starting = false;
    bs.current_build = None;
    if !matches!(bs.status, BuildStatus::Cancelled) {
        bs.status = BuildStatus::Failed(BuildResult {
            success: false,
            duration_ms: 0,
            error_count: 1,
            warning_count: 0,
        });
    }
}

/// Reserve the single build slot, marking the build as starting.
///
/// Every code path that spawns Gradle MUST call this first. Previously only the
/// Tauri command layer guarded, so an MCP client could start a second Gradle
/// process against the same project — which also overwrote `active_process_id`
/// and left the first build uncancellable.
///
/// # Errors
/// Returns an error when a build is already starting or running.
pub async fn try_reserve_build_slot(
    build_state: &BuildState,
    task: &str,
    started_at: &str,
) -> Result<(), String> {
    let mut bs = build_state.inner.lock().await;
    if bs.starting || bs.current_build.is_some() || matches!(bs.status, BuildStatus::Running { .. })
    {
        return Err("A Gradle build is already running".to_string());
    }
    bs.starting = true;
    bs.status = BuildStatus::Running {
        task: task.to_owned(),
        started_at: started_at.to_owned(),
    };
    bs.current_errors.clear();
    Ok(())
}

/// Cancel the currently running build. Returns `true` if a build was running, `false` otherwise.
pub async fn cancel_build(build_state: &BuildState, process_manager: &ProcessManager) -> bool {
    let (id, was_running) = {
        let from_sync = build_state.take_active_process_id();
        if let Some(id) = from_sync {
            let mut bs = build_state.inner.lock().await;
            if bs.current_build == Some(id) {
                bs.current_build = None;
            }
            bs.starting = false;
            bs.status = BuildStatus::Cancelled;
            (Some(id), true)
        } else {
            let mut bs = build_state.inner.lock().await;
            let pid = bs.current_build.take();
            let was_running =
                pid.is_some() || bs.starting || matches!(bs.status, BuildStatus::Running { .. });
            if was_running {
                bs.starting = false;
                bs.status = BuildStatus::Cancelled;
            }
            (pid, was_running)
        }
    };
    if let Some(id) = id {
        process_manager::cancel(&process_manager.0, id).await;
    }
    was_running
}

/// Clear all build history from memory and disk. The in-memory clear always
/// happens; a failure to clear the file is returned.
pub async fn clear_history(build_state: &BuildState) -> Result<(), String> {
    build_state.inner.lock().await.history.clear();
    tokio::task::spawn_blocking(|| {
        let dir = data_dir();
        with_data_lock_in(&dir, || save_build_history_to(&dir, &VecDeque::new()))?
    })
    .await
    .map_err(|e| format!("Failed to clear build history: {e}"))?
}

/// Record the completed build result and push it to history.
///
/// Every run gets a history entry, but only the latest run updates the shared
/// status, errors, and cancellable process. A run cancelled and replaced by a
/// newer one can finish seconds later; letting it write those would show its
/// outcome as the newer build's, make the newer build uncancellable, and free
/// the build slot while the newer Gradle is still running.
#[allow(clippy::too_many_arguments)]
pub async fn record_build_result(
    build_state: &BuildState,
    run_id: ProcessId,
    log: &BuildLog,
    task: String,
    started_at: String,
    result: BuildResult,
    cancelled: bool,
    errors: Vec<BuildError>,
    project_root: Option<String>,
) {
    // Snapshot the run's log before taking the inner lock so we don't hold two
    // locks simultaneously.
    let raw_lines: VecDeque<String> = log.lock().map(|g| g.clone()).unwrap_or_default();

    build_state.release_active_process_id(run_id);

    let status = if cancelled {
        BuildStatus::Cancelled
    } else if result.success {
        BuildStatus::Success(result.clone())
    } else {
        BuildStatus::Failed(result.clone())
    };

    {
        let mut bs = build_state.inner.lock().await;
        if !bs.starting && bs.latest_run == Some(run_id) {
            bs.status = status.clone();
            bs.current_errors = errors.clone();
            bs.current_build = None;
        }
    }

    // The ID is allocated when the record is persisted.
    let record = BuildRecord {
        id: 0,
        task,
        status,
        errors,
        started_at,
        project_root,
    };

    // Disk I/O runs off the async runtime and outside the build-state lock.
    let record_for_io = record.clone();
    let persisted = tokio::task::spawn_blocking(move || {
        let (settings, _) = crate::services::settings_manager::load_settings();
        persist_build_record_in(
            &data_dir(),
            record_for_io,
            &raw_lines,
            settings.build.build_log_retention_days,
            settings.build.build_log_max_folder_mb,
        )
    })
    .await
    .map_err(|e| format!("Build persistence task failed: {e}"))
    .and_then(|result| result);

    let mut bs = build_state.inner.lock().await;
    match persisted {
        Ok((_, persisted_history)) => {
            bs.history = merge_history(&bs.history, persisted_history);
        }
        Err(e) => {
            // Keep the build visible in this session even though it was not
            // saved. A failure here used to be silent.
            tracing::warn!("Failed to persist build history: {e}");
            let mut record = record;
            record.id = bs.history.iter().map(|r| r.id).max().unwrap_or(0) + 1;
            bs.history.push_back(record);
            while bs.history.len() > MAX_HISTORY {
                bs.history.pop_front();
            }
        }
    }
}

/// Build environment variables for a Gradle process, and ensure `gradlew` is executable.
pub fn build_env_vars(
    settings: &crate::models::settings::AppSettings,
    gradle_root: &Path,
) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(java_home) = crate::services::jdk::java_home_for_gradle(settings, Some(gradle_root))
    {
        env.push(("JAVA_HOME".into(), java_home));
    }
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        env.push(("ANDROID_HOME".into(), sdk.into()));
        env.push(("ANDROID_SDK_ROOT".into(), sdk.into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let gradlew = gradle_root.join("gradlew");
        if let Ok(meta) = std::fs::metadata(&gradlew) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(&gradlew, perms);
        }
    }
    #[cfg(not(unix))]
    let _ = gradle_root;
    env
}

#[derive(Debug)]
pub struct GradleTaskResult {
    pub success: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub errors: Vec<crate::models::build::BuildError>,
}

/// Format `BuildError` structs into human-readable strings for display.
///
/// Each error is formatted as `[severity] location — message` or `[severity] message`
/// if no location is available. Severity is derived from the error's severity field.
pub fn format_build_issues(errors: &[crate::models::build::BuildError]) -> Vec<String> {
    errors
        .iter()
        .map(|e| {
            let loc = match (&e.file, e.line) {
                (Some(f), Some(l)) => format!("{}:{}", f, l),
                (Some(f), None) => f.clone(),
                _ => String::new(),
            };
            let sev = format!("{:?}", e.severity).to_lowercase();
            if loc.is_empty() {
                format!("[{sev}] {}", e.message)
            } else {
                format!("[{sev}] {loc} — {}", e.message)
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub async fn run_task(
    task: &str,
    extra_args: &[&str],
    gradle_root: &std::path::Path,
    gradlew: &std::path::Path,
    timeout_sec: u64,
    env: Vec<(String, String)>,
    project_root_for_history: Option<String>,
    build_state: &BuildState,
    process_manager: &crate::services::process_manager::ProcessManager,
    app_handle: Option<&tauri::AppHandle>,
) -> Result<GradleTaskResult, String> {
    use crate::models::build::{BuildError, BuildErrorSeverity, BuildLineKind};
    use crate::services::process_manager::{self as pm, ProcessTermination, SpawnOptions};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    let started_at = chrono::Utc::now().to_rfc3339();

    try_reserve_build_slot(build_state, task, &started_at).await?;

    let build_log = build_state.build_log.start_run();

    let mut args = vec![task, "--console=plain"];
    args.extend_from_slice(extra_args);

    let errors_buf = Arc::new(std::sync::Mutex::new(Vec::<BuildError>::new()));
    let success_flag = Arc::new(AtomicBool::new(false));
    let duration_buf = Arc::new(AtomicU64::new(0));
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<ProcessTermination>();
    let done_tx = Arc::new(StdMutex::new(Some(done_tx)));

    let pid = pm::spawn(
        &process_manager.0,
        gradlew.to_str().unwrap_or("./gradlew"),
        &args,
        gradle_root.to_path_buf(),
        env,
        SpawnOptions {
            on_line: Box::new({
                let build_log = build_log.clone();
                let errors_buf = errors_buf.clone();
                let success_flag = success_flag.clone();
                let duration_buf = duration_buf.clone();
                move |proc_line| {
                    push_build_log(&build_log, proc_line.text.clone());
                    let line = parse_build_line(&proc_line.text);
                    if matches!(line.kind, BuildLineKind::Error | BuildLineKind::Warning) {
                        if let Ok(mut e) = errors_buf.lock() {
                            push_build_error(
                                &mut e,
                                BuildError {
                                    message: line.content.clone(),
                                    file: line.file.clone(),
                                    line: line.line,
                                    col: line.col,
                                    severity: if line.kind == BuildLineKind::Error {
                                        BuildErrorSeverity::Error
                                    } else {
                                        BuildErrorSeverity::Warning
                                    },
                                },
                            );
                        }
                    }
                    if line.kind == BuildLineKind::Summary {
                        let dur = parse_build_duration(&line.content);
                        duration_buf.store(dur, Ordering::Relaxed);
                        if line.content.contains("BUILD SUCCESSFUL") {
                            success_flag.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }),
            on_exit: Box::new(move |_pid, termination| {
                if let Ok(mut g) = done_tx.lock() {
                    if let Some(tx) = g.take() {
                        let _ = tx.send(termination);
                    }
                }
            }),
        },
    )
    .await;

    let pid = match pid {
        Ok(pid) => pid,
        Err(e) => {
            // Release the slot we reserved above; without this `starting` stays
            // true and every later build is refused until the app restarts.
            mark_build_spawn_failed(&mut *build_state.inner.lock().await);
            return Err(format!("Failed to spawn Gradle: {e}"));
        }
    };

    build_state.set_active_process_id(Some(pid));
    let cancelled_during_spawn = {
        let mut bs = build_state.inner.lock().await;
        bs.latest_run = Some(pid);
        if matches!(bs.status, BuildStatus::Cancelled) {
            bs.starting = false;
            true
        } else {
            bs.starting = false;
            bs.current_build = Some(pid);
            false
        }
    };
    if cancelled_during_spawn {
        // The user cancelled between spawn and this lock. Kill the process we
        // just started — returning here without it orphaned a live Gradle.
        let _ = build_state.take_active_process_id();
        process_manager::cancel(&process_manager.0, pid).await;
        return Ok(GradleTaskResult {
            success: false,
            timed_out: false,
            duration_ms: 0,
            errors: Vec::new(),
        });
    }

    let termination =
        tokio::time::timeout(std::time::Duration::from_secs(timeout_sec), done_rx).await;
    let timed_out = termination.is_err();

    if timed_out {
        cancel_build(build_state, process_manager).await;
        // A timeout is a build failure, not a user cancellation: record it so
        // it appears in history with a reason instead of vanishing.
        let timeout_err = BuildError {
            message: format!("Build timed out after {timeout_sec}s and was cancelled"),
            file: None,
            line: None,
            col: None,
            severity: BuildErrorSeverity::Error,
        };
        let errors = vec![timeout_err];
        emit_build_complete(
            build_state,
            app_handle,
            BuildFinalization {
                run_id: pid,
                log: build_log.clone(),
                task: task.to_owned(),
                started_at,
                project_root: project_root_for_history.clone(),
                success: false,
                cancelled: false,
                duration_ms: 0,
                errors: errors.clone(),
            },
        )
        .await;
        return Ok(GradleTaskResult {
            success: false,
            timed_out: true,
            duration_ms: 0,
            errors,
        });
    }

    // Exit code is authoritative. The summary line alone is not enough: stray
    // "BUILD SUCCESSFUL" text in the output must not override a non-zero exit.
    let exit_ok = matches!(termination, Ok(Ok(ProcessTermination::ExitCode(0))));
    let success = exit_ok && success_flag.load(Ordering::Acquire);
    let errors = errors_buf.lock().map(|g| g.clone()).unwrap_or_default();
    let duration_ms = duration_buf.load(Ordering::Relaxed);
    // Counts are derived inside finalize_completed_build — single source of truth.
    let cancelled = matches!(termination, Ok(Ok(ProcessTermination::Cancelled)));
    emit_build_complete(
        build_state,
        app_handle,
        BuildFinalization {
            run_id: pid,
            log: build_log,
            task: task.to_owned(),
            started_at,
            project_root: project_root_for_history,
            success,
            cancelled,
            duration_ms,
            errors: errors.clone(),
        },
    )
    .await;

    Ok(GradleTaskResult {
        success,
        timed_out: false,
        duration_ms,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::build::BuildLineKind;

    // ── build_env_vars tests ───────────────────────────────────────────────────

    fn java_home_env(env: &[(String, String)]) -> Option<&str> {
        env.iter()
            .find(|(k, _)| k == "JAVA_HOME")
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn build_env_uses_the_gradle_properties_jdk_over_the_setting() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("gradle.properties"),
            "org.gradle.java.home=/opt/jdk-21\n",
        )
        .unwrap();
        let mut settings = crate::models::settings::AppSettings::default();
        settings.java.home = Some("/opt/jdk-11".into());

        let env = build_env_vars(&settings, project.path());

        assert_eq!(java_home_env(&env), Some("/opt/jdk-21"));
    }

    #[test]
    fn build_env_expands_a_tilde_in_the_java_home_setting() {
        let project = tempfile::tempdir().unwrap();
        let mut settings = crate::models::settings::AppSettings::default();
        settings.java.home = Some("~/jdks/17".into());

        let env = build_env_vars(&settings, project.path());

        let expected = dirs::home_dir().unwrap().join("jdks/17");
        assert_eq!(java_home_env(&env), Some(expected.to_str().unwrap()));
    }

    #[test]
    fn build_env_leaves_java_home_unset_when_no_jdk_resolves() {
        let project = tempfile::tempdir().unwrap();
        let env = build_env_vars(
            &crate::models::settings::AppSettings::default(),
            project.path(),
        );
        assert_eq!(java_home_env(&env), None);
    }

    // ── push_build_error tests ─────────────────────────────────────────────────

    fn make_numbered_error(n: usize) -> BuildError {
        BuildError {
            message: format!("error {n}"),
            file: Some(format!("src/Main{n}.kt")),
            line: Some(n as u32),
            col: None,
            severity: BuildErrorSeverity::Error,
        }
    }

    #[test]
    fn push_build_error_accepts_entries_below_cap() {
        let mut buf = Vec::new();
        for n in 0..MAX_BUILD_ERRORS {
            push_build_error(&mut buf, make_numbered_error(n));
        }
        assert_eq!(buf.len(), MAX_BUILD_ERRORS);
        assert_eq!(buf[0].message, "error 0");
    }

    #[test]
    fn push_build_error_keeps_newest_and_prepends_notice_once_at_cap() {
        let mut buf: Vec<BuildError> = (0..MAX_BUILD_ERRORS).map(make_numbered_error).collect();
        push_build_error(&mut buf, make_numbered_error(MAX_BUILD_ERRORS));

        // One truncation notice at the front + the newest diagnostic kept.
        assert_eq!(buf.len(), MAX_BUILD_ERRORS + 1);
        let notice = &buf[0];
        assert_eq!(notice.severity, BuildErrorSeverity::Warning);
        assert!(notice.message.starts_with(TRUNCATION_NOTICE_PREFIX));
        assert!(notice.file.is_none());
        assert_eq!(buf[1].message, "error 1", "oldest diagnostic evicted");
        assert_eq!(
            buf.last().unwrap().message,
            format!("error {}", MAX_BUILD_ERRORS),
            "newest diagnostic must survive"
        );
    }

    #[test]
    fn push_build_error_stays_capped_across_many_overflows() {
        let mut buf: Vec<BuildError> = (0..MAX_BUILD_ERRORS).map(make_numbered_error).collect();
        for n in 0..50 {
            push_build_error(&mut buf, make_numbered_error(MAX_BUILD_ERRORS + n));
        }
        assert_eq!(
            buf.len(),
            MAX_BUILD_ERRORS + 1,
            "buffer must stay capped at the cap plus one truncation notice"
        );
        assert!(buf[0].message.starts_with(TRUNCATION_NOTICE_PREFIX));
        assert_eq!(
            buf.last().unwrap().message,
            format!("error {}", MAX_BUILD_ERRORS + 49),
            "the final root-cause error must be retained"
        );
    }

    #[test]
    fn push_build_error_notice_detection_is_not_fooled_by_user_content() {
        // A real diagnostic whose message happens to start with the notice
        // prefix must not suppress the truncation notice (detected by length,
        // not by message content).
        let mut buf: Vec<BuildError> = (0..MAX_BUILD_ERRORS).map(make_numbered_error).collect();
        buf[0].message = format!("{TRUNCATION_NOTICE_PREFIX}: error 0");

        push_build_error(&mut buf, make_numbered_error(MAX_BUILD_ERRORS));

        assert_eq!(buf.len(), MAX_BUILD_ERRORS + 1);
        assert!(
            buf[0].message.starts_with(TRUNCATION_NOTICE_PREFIX),
            "a synthetic notice must still be inserted at the front"
        );
    }

    // ── parse_build_line tests ─────────────────────────────────────────────────

    #[test]
    fn parses_kotlin_error_with_file_uri() {
        let line = parse_build_line(
            "e: file:///Users/dev/app/src/main/java/com/example/Main.kt:42:13: Unresolved reference: foo",
        );
        assert_eq!(line.kind, BuildLineKind::Error);
        assert!(line.file.unwrap().contains("Main.kt"));
        assert_eq!(line.line, Some(42));
        assert_eq!(line.col, Some(13));
        assert!(line.content.contains("Unresolved reference"));
    }

    #[test]
    fn parses_kotlin_error_without_file_uri() {
        let line =
            parse_build_line("e: /Users/dev/app/src/Main.kt:5:1: Expecting member declaration");
        assert_eq!(line.kind, BuildLineKind::Error);
        assert_eq!(line.line, Some(5));
    }

    #[test]
    fn parses_kotlin_warning() {
        let line = parse_build_line("w: file:///src/Foo.kt:10:3: Parameter 'x' is never used");
        assert_eq!(line.kind, BuildLineKind::Warning);
        assert_eq!(line.line, Some(10));
    }

    #[test]
    fn parses_gradle_task_start() {
        let line = parse_build_line("> Task :app:compileDebugKotlin");
        assert_eq!(line.kind, BuildLineKind::TaskStart);
        assert_eq!(line.content, ":app:compileDebugKotlin");
    }

    #[test]
    fn parses_gradle_task_failed() {
        let line = parse_build_line("> Task :app:compileDebugKotlin FAILED");
        assert_eq!(line.kind, BuildLineKind::TaskEnd);
        assert!(line.content.contains("FAILED"));
    }

    #[test]
    fn parses_build_successful() {
        let line = parse_build_line("BUILD SUCCESSFUL in 1m 23s");
        assert_eq!(line.kind, BuildLineKind::Summary);
    }

    #[test]
    fn parses_build_failed() {
        let line = parse_build_line("BUILD FAILED in 45s");
        assert_eq!(line.kind, BuildLineKind::Summary);
    }

    #[test]
    fn plain_output_has_output_kind() {
        let line = parse_build_line("Note: some informational line");
        assert_eq!(line.kind, BuildLineKind::Output);
    }

    #[test]
    fn parses_java_compiler_error() {
        let line =
            parse_build_line("src/main/java/com/example/Foo.java:23: error: cannot find symbol");
        assert_eq!(line.kind, BuildLineKind::Error);
        assert_eq!(line.line, Some(23));
        assert!(line.file.as_deref().unwrap().contains("Foo.java"));
    }

    #[test]
    fn parses_aapt_file_error() {
        let line = parse_build_line(
            "app/src/main/res/layout/activity_main.xml:10: error: attribute missing",
        );
        assert_eq!(line.kind, BuildLineKind::Error);
        assert_eq!(line.line, Some(10));
    }

    #[test]
    fn parses_aapt_bare_error() {
        let line = parse_build_line("AAPT: error: failed to compile resources");
        assert_eq!(line.kind, BuildLineKind::Error);
        assert!(line.content.contains("failed to compile resources"));
        assert!(line.file.is_none());
    }

    #[test]
    fn parses_gradle_failure_header() {
        let line = parse_build_line("FAILURE: Build failed with an exception.");
        assert_eq!(line.kind, BuildLineKind::Error);
        assert!(line.file.is_none());
    }

    #[test]
    fn parses_could_not_resolve() {
        let line = parse_build_line("> Could not resolve com.example:library:1.0.0");
        assert_eq!(line.kind, BuildLineKind::Error);
        assert!(line.file.is_none());
    }

    #[test]
    fn parses_download_as_info() {
        let line = parse_build_line("Download https://repo.example.com/file.jar");
        assert_eq!(line.kind, BuildLineKind::Info);
    }

    // ── find_output_apk tests ──────────────────────────────────────────────────

    #[test]
    fn finds_signed_apk_in_variant_dir() {
        let tmp = std::env::temp_dir().join("apk_test_signed");
        let apk_dir = tmp
            .join("app")
            .join("build")
            .join("outputs")
            .join("apk")
            .join("release");
        std::fs::create_dir_all(&apk_dir).unwrap();
        let apk = apk_dir.join("app-release.apk");
        std::fs::write(&apk, b"").unwrap();

        let found = find_output_apk(&tmp, "release");
        assert_eq!(found.unwrap(), apk);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn finds_unsigned_apk_when_no_signed_exists() {
        let tmp = std::env::temp_dir().join("apk_test_unsigned");
        let apk_dir = tmp
            .join("app")
            .join("build")
            .join("outputs")
            .join("apk")
            .join("release");
        std::fs::create_dir_all(&apk_dir).unwrap();
        let apk = apk_dir.join("app-release-unsigned.apk");
        std::fs::write(&apk, b"").unwrap();

        let found = find_output_apk(&tmp, "release");
        assert_eq!(found.unwrap(), apk, "should find unsigned APK");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn prefers_signed_over_unsigned_in_same_dir() {
        let tmp = std::env::temp_dir().join("apk_test_prefer_signed");
        let apk_dir = tmp
            .join("app")
            .join("build")
            .join("outputs")
            .join("apk")
            .join("release");
        std::fs::create_dir_all(&apk_dir).unwrap();
        let unsigned = apk_dir.join("app-release-unsigned.apk");
        let signed = apk_dir.join("app-release.apk");
        std::fs::write(&unsigned, b"").unwrap();
        std::fs::write(&signed, b"").unwrap();

        let found = find_output_apk(&tmp, "release");
        assert_eq!(found.unwrap(), signed, "signed should take priority");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn excludes_unaligned_apk() {
        let tmp = std::env::temp_dir().join("apk_test_unaligned");
        let apk_dir = tmp
            .join("app")
            .join("build")
            .join("outputs")
            .join("apk")
            .join("release");
        std::fs::create_dir_all(&apk_dir).unwrap();
        // Only file present is unaligned — should NOT be returned.
        let unaligned = apk_dir.join("app-release-unaligned.apk");
        std::fs::write(&unaligned, b"").unwrap();

        let found = find_output_apk(&tmp, "release");
        assert!(found.is_err(), "unaligned APK must be excluded");
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Create `app/build/outputs/apk/<rel>` under `root` as an empty file.
    fn apk_at(root: &Path, rel: &str) -> PathBuf {
        let path = apk_outputs_dir(root).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn flavored_variant_matches_its_flavor_and_build_type_dirs() {
        let root = tempfile::tempdir().unwrap();
        let paid = apk_at(root.path(), "paid/debug/app-paid-debug.apk");
        apk_at(root.path(), "free/debug/app-free-debug.apk");

        assert_eq!(find_output_apk(root.path(), "paidDebug").unwrap(), paid);
    }

    /// With only a stale output of another flavor present, the old fallback
    /// passes returned it and Run installed the wrong app without a word.
    #[test]
    fn never_falls_back_to_another_variants_apk() {
        let root = tempfile::tempdir().unwrap();
        apk_at(root.path(), "free/debug/app-free-debug.apk");
        apk_at(root.path(), "release/app-release.apk");

        let err = find_output_apk(root.path(), "paidDebug").unwrap_err();
        assert!(err.contains("paidDebug"), "{err}");
        assert!(
            err.contains("freedebug") && err.contains("release"),
            "should list the variants that do have outputs: {err}"
        );
    }

    #[test]
    fn output_metadata_decides_the_variant_and_file() {
        let root = tempfile::tempdir().unwrap();
        let apk = apk_at(root.path(), "demo/app-renamed.apk");
        std::fs::write(
            apk.parent().unwrap().join("output-metadata.json"),
            r#"{"version":3,"applicationId":"com.example.app.debug","variantName":"demoDebug",
                "elements":[{"type":"SINGLE","outputFile":"app-renamed.apk"}]}"#,
        )
        .unwrap();

        assert_eq!(find_output_apk(root.path(), "demoDebug").unwrap(), apk);
        assert!(find_output_apk(root.path(), "demo").is_err());
        assert_eq!(
            application_id_from_output_metadata(&apk).as_deref(),
            Some("com.example.app.debug"),
            "the suffixed application ID comes from the metadata"
        );
    }

    #[test]
    fn built_application_ids_lists_each_built_variant_once() {
        let root = tempfile::tempdir().unwrap();
        for (dir, id) in [
            ("debug", "com.example.app.debug"),
            ("paid/debug", "com.example.app.paid.debug"),
            ("release", "com.example.app"),
        ] {
            let apk = apk_at(root.path(), &format!("{dir}/app.apk"));
            std::fs::write(
                apk.parent().unwrap().join("output-metadata.json"),
                format!(r#"{{"applicationId":"{id}","variantName":"v","elements":[]}}"#),
            )
            .unwrap();
        }
        apk_at(root.path(), "free/app.apk");

        assert_eq!(
            built_application_ids(root.path()),
            vec![
                "com.example.app",
                "com.example.app.debug",
                "com.example.app.paid.debug"
            ]
        );
    }

    #[test]
    fn more_than_one_candidate_is_an_error_not_a_guess() {
        let root = tempfile::tempdir().unwrap();
        apk_at(root.path(), "debug/app-arm64-v8a-debug.apk");
        apk_at(root.path(), "debug/app-x86_64-debug.apk");

        let err = find_output_apk(root.path(), "debug").unwrap_err();
        assert!(err.contains("More than one APK"), "{err}");
    }

    #[test]
    fn empty_variant_accepts_only_a_single_apk() {
        let root = tempfile::tempdir().unwrap();
        let debug = apk_at(root.path(), "debug/app-debug.apk");
        assert_eq!(find_output_apk(root.path(), "").unwrap(), debug);

        apk_at(root.path(), "release/app-release.apk");
        assert!(find_output_apk(root.path(), "").is_err());
    }

    #[test]
    fn missing_outputs_dir_is_an_actionable_error() {
        let root = tempfile::tempdir().unwrap();
        let err = find_output_apk(root.path(), "debug").unwrap_err();
        assert!(err.contains("Build the variant first"), "{err}");
    }

    // ── parse_build_duration tests ─────────────────────────────────────────────

    #[test]
    fn parses_seconds_only() {
        assert_eq!(parse_build_duration("BUILD SUCCESSFUL in 45s"), 45_000);
    }

    #[test]
    fn parses_minutes_and_seconds() {
        assert_eq!(parse_build_duration("BUILD FAILED in 1m 30s"), 90_000);
    }

    #[test]
    fn parses_fractional_seconds() {
        // "2.5s" -> 2500ms
        assert_eq!(parse_build_duration("BUILD SUCCESSFUL in 2.5s"), 2_500);
    }

    #[test]
    fn returns_zero_for_no_match() {
        assert_eq!(parse_build_duration("BUILD SUCCESSFUL"), 0);
    }

    // ── cancel_build ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_build_returns_true_when_only_active_process_id_set() {
        // Simulates the window after `spawn` returns but before `inner.current_build` is updated.
        let state = BuildState::new();
        let pm = ProcessManager::new();
        *state.active_process_id.lock().unwrap() = Some(99998);
        let was_running = cancel_build(&state, &pm).await;
        assert!(
            was_running,
            "cancel must see active_process_id even when inner.current_build is still None"
        );
        assert!(state.active_process_id.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn cancel_build_returns_false_when_idle() {
        let state = BuildState::new();
        let pm = ProcessManager::new();
        let was_running = cancel_build(&state, &pm).await;
        assert!(
            !was_running,
            "cancel_build should return false when no build is running"
        );
    }

    #[tokio::test]
    async fn cancel_build_returns_true_while_build_is_starting() {
        let state = BuildState::new();
        let pm = ProcessManager::new();

        {
            let mut inner = state.inner.lock().await;
            inner.starting = true;
            inner.status = BuildStatus::Running {
                task: "assembleDebug".into(),
                started_at: "2024-01-01T00:00:00Z".into(),
            };
        }

        let was_running = cancel_build(&state, &pm).await;
        let inner = state.inner.lock().await;

        assert!(was_running, "starting builds should be cancellable");
        assert!(!inner.starting, "starting flag must be cleared on cancel");
        assert!(matches!(inner.status, BuildStatus::Cancelled));
    }

    #[tokio::test]
    async fn cancel_build_does_not_change_status_when_idle() {
        let state = BuildState::new();
        let pm = ProcessManager::new();
        cancel_build(&state, &pm).await;
        let inner = state.inner.lock().await;
        assert!(
            matches!(inner.status, BuildStatus::Idle),
            "status must remain Idle when there was no build to cancel"
        );
    }

    #[tokio::test]
    async fn cancel_build_returns_true_when_build_was_running() {
        let state = BuildState::new();
        let pm = ProcessManager::new();

        // Simulate a running build by injecting a fake PID and Running status.
        {
            let mut inner = state.inner.lock().await;
            inner.current_build = Some(99999);
            inner.status = BuildStatus::Running {
                task: "assembleDebug".into(),
                started_at: "2024-01-01T00:00:00Z".into(),
            };
        }

        let was_running = cancel_build(&state, &pm).await;
        assert!(
            was_running,
            "cancel_build should return true when a build was running"
        );
    }

    #[tokio::test]
    async fn cancel_build_sets_cancelled_status_when_build_was_running() {
        let state = BuildState::new();
        let pm = ProcessManager::new();

        {
            let mut inner = state.inner.lock().await;
            inner.current_build = Some(99999);
            inner.status = BuildStatus::Running {
                task: "assembleDebug".into(),
                started_at: "2024-01-01T00:00:00Z".into(),
            };
        }

        cancel_build(&state, &pm).await;

        let inner = state.inner.lock().await;
        assert!(
            matches!(inner.status, BuildStatus::Cancelled),
            "status must be Cancelled after cancelling a running build"
        );
    }

    #[tokio::test]
    async fn cancel_build_clears_current_build_pid() {
        let state = BuildState::new();
        let pm = ProcessManager::new();

        {
            let mut inner = state.inner.lock().await;
            inner.current_build = Some(99999);
            inner.status = BuildStatus::Running {
                task: "assembleDebug".into(),
                started_at: "2024-01-01T00:00:00Z".into(),
            };
        }

        cancel_build(&state, &pm).await;

        let inner = state.inner.lock().await;
        assert!(
            inner.current_build.is_none(),
            "current_build PID must be cleared after cancel"
        );
    }

    // ── format_build_issues tests ─────────────────────────────────────────────

    fn make_error(
        msg: &str,
        file: Option<&str>,
        line: Option<u32>,
        severity: crate::models::build::BuildErrorSeverity,
    ) -> crate::models::build::BuildError {
        crate::models::build::BuildError {
            message: msg.to_string(),
            file: file.map(str::to_string),
            line,
            col: None,
            severity,
        }
    }

    #[test]
    fn format_error_with_file_and_line() {
        use crate::models::build::BuildErrorSeverity;
        let errors = vec![make_error(
            "Unresolved reference: foo",
            Some("Main.kt"),
            Some(42),
            BuildErrorSeverity::Error,
        )];
        let lines = format_build_issues(&errors);
        assert_eq!(
            lines,
            vec!["[error] Main.kt:42 — Unresolved reference: foo"]
        );
    }

    #[test]
    fn format_error_with_file_only() {
        use crate::models::build::BuildErrorSeverity;
        let errors = vec![make_error(
            "Syntax error",
            Some("build.gradle"),
            None,
            BuildErrorSeverity::Error,
        )];
        let lines = format_build_issues(&errors);
        assert_eq!(lines, vec!["[error] build.gradle — Syntax error"]);
    }

    #[test]
    fn format_error_with_message_only() {
        use crate::models::build::BuildErrorSeverity;
        let errors = vec![make_error(
            "Task :app:compileDebugKotlin FAILED",
            None,
            None,
            BuildErrorSeverity::Error,
        )];
        let lines = format_build_issues(&errors);
        assert_eq!(lines, vec!["[error] Task :app:compileDebugKotlin FAILED"]);
    }

    #[test]
    fn format_warning_severity() {
        use crate::models::build::BuildErrorSeverity;
        let errors = vec![make_error(
            "Deprecated API",
            Some("Foo.kt"),
            Some(10),
            BuildErrorSeverity::Warning,
        )];
        let lines = format_build_issues(&errors);
        assert_eq!(lines, vec!["[warning] Foo.kt:10 — Deprecated API"]);
    }

    #[test]
    fn format_empty_errors_returns_empty_vec() {
        assert!(format_build_issues(&[]).is_empty());
    }

    #[test]
    fn build_history_serializes_round_trip() {
        use crate::models::build::{BuildRecord, BuildResult, BuildStatus};
        let record = BuildRecord {
            id: 1,
            task: "assembleDebug".into(),
            status: BuildStatus::Success(BuildResult {
                success: true,
                duration_ms: 5000,
                error_count: 0,
                warning_count: 0,
            }),
            errors: vec![],
            started_at: "2026-04-06T12:00:00Z".into(),
            project_root: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: BuildRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task, "assembleDebug");
        assert_eq!(parsed.id, 1);
    }

    #[test]
    fn next_id_starts_after_max_history_id() {
        use std::collections::VecDeque;
        let records: VecDeque<BuildRecord> = (1u32..=5)
            .map(|i| BuildRecord {
                id: i,
                task: format!("task_{i}"),
                status: BuildStatus::Idle,
                errors: vec![],
                started_at: "2026-01-01T00:00:00Z".into(),
                project_root: None,
            })
            .collect();
        // This is the formula that BuildStateInner::new() must use.
        let next_id = records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        assert_eq!(next_id, 6, "next_id must continue from max existing id");
    }

    #[test]
    fn next_id_is_one_when_history_empty() {
        use std::collections::VecDeque;
        let records: VecDeque<BuildRecord> = VecDeque::new();
        let next_id = records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        assert_eq!(next_id, 1);
    }

    #[tokio::test]
    async fn clear_history_empties_the_deque() {
        let state = BuildState::new();
        // Inject 3 records directly into the state.
        {
            let mut bs = state.inner.lock().await;
            for i in 1u32..=3 {
                bs.history.push_back(BuildRecord {
                    id: i,
                    task: format!("task_{i}"),
                    status: BuildStatus::Idle,
                    errors: vec![],
                    started_at: "2026-01-01T00:00:00Z".into(),
                    project_root: None,
                });
            }
        }
        clear_history(&state).await.unwrap();
        let bs = state.inner.lock().await;
        assert!(
            bs.history.is_empty(),
            "history must be empty after clear_history"
        );
    }

    #[test]
    fn save_and_load_history_round_trip() {
        use crate::models::build::{BuildRecord, BuildStatus};

        // We can't easily override data_dir() in tests, but we can test
        // the serialization/deserialization logic directly.
        let records: Vec<BuildRecord> = (1..=5u32)
            .map(|i| BuildRecord {
                id: i,
                task: format!("task_{i}"),
                status: BuildStatus::Idle,
                errors: vec![],
                started_at: "2026-04-06T12:00:00Z".into(),
                project_root: None,
            })
            .collect();

        let json = serde_json::to_string_pretty(&records).unwrap();
        let loaded: Vec<BuildRecord> = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.len(), 5);
        assert_eq!(loaded[0].task, "task_1");
        assert_eq!(loaded[4].task, "task_5");
    }

    // ── save_build_log_to tests ────────────────────────────────────────────────

    #[test]
    fn save_build_log_to_writes_jsonl_file() {
        use std::io::BufRead;
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let mut raw: VecDeque<String> = VecDeque::new();
        raw.push_back("e: /src/Foo.kt:1:1: Unresolved reference: bar".into());
        raw.push_back("> Task :app:compileDebugKotlin".into());

        save_build_log_to(42, &raw, dir_path);

        let path = dir_path.join("build-42.jsonl");
        assert!(path.exists(), "jsonl file must be created");

        let file = std::fs::File::open(&path).unwrap();
        let lines: Vec<String> = std::io::BufReader::new(file)
            .lines()
            .map(|l| l.unwrap())
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(lines.len(), 2);

        let first: crate::models::build::BuildLine = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(first.kind, BuildLineKind::Error);
        assert!(first.content.contains("Unresolved reference"));
    }

    // ── History shared between processes ──────────────────────────────────────

    fn record_named(task: &str) -> BuildRecord {
        BuildRecord {
            id: 0,
            task: task.into(),
            status: BuildStatus::Idle,
            errors: vec![],
            started_at: "2026-01-01T00:00:00Z".into(),
            project_root: None,
        }
    }

    fn persist(dir: &Path, task: &str) -> u32 {
        let lines = VecDeque::from([format!("output of {task}")]);
        persist_build_record_in(dir, record_named(task), &lines, 7, 100)
            .unwrap()
            .0
    }

    #[test]
    fn a_build_recorded_by_another_process_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        // The headless server records a build this process never saw.
        persist(dir.path(), "mcp build");
        persist(dir.path(), "gui build");

        let tasks: Vec<String> = load_build_history_from(dir.path())
            .into_iter()
            .map(|r| r.task)
            .collect();
        assert_eq!(tasks, ["mcp build", "gui build"]);
    }

    #[test]
    fn build_ids_never_repeat_even_after_history_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let first = persist(dir.path(), "first");
        let second = persist(dir.path(), "second");
        assert!(second > first);

        // Cleared history, but the earlier logs are still on disk.
        save_build_history_to(dir.path(), &VecDeque::new()).unwrap();
        let third = persist(dir.path(), "third");
        assert!(third > second, "{third} reuses the ID of a retained log");
    }

    #[test]
    fn rotation_keeps_the_log_of_a_build_another_process_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let other = persist(dir.path(), "mcp build");
        persist(dir.path(), "gui build");

        assert!(dir
            .path()
            .join("build-logs")
            .join(format!("build-{other}.jsonl"))
            .is_file());
    }

    #[test]
    fn merged_history_is_the_union_by_id_capped_to_the_newest() {
        let with_ids = |ids: &[u32]| -> VecDeque<BuildRecord> {
            ids.iter()
                .map(|&id| BuildRecord {
                    id,
                    ..record_named(&format!("build {id}"))
                })
                .collect()
        };
        // Two builds of this process finished together, and the older
        // snapshot is applied after the newer one.
        let memory = with_ids(&(5..=14).collect::<Vec<_>>());
        let older_snapshot = with_ids(&(3..=12).collect::<Vec<_>>());

        let ids: Vec<u32> = merge_history(&memory, older_snapshot)
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, (5..=14).collect::<Vec<_>>());
    }

    // ── rotate_build_logs tests ────────────────────────────────────────────────

    #[test]
    fn rotate_build_logs_removes_orphan() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // Write two log files: id=42 (in history) and id=99 (orphan)
        std::fs::write(dir_path.join("build-42.jsonl"), "{}").unwrap();
        std::fs::write(dir_path.join("build-99.jsonl"), "{}").unwrap();

        let mut history: VecDeque<crate::models::build::BuildRecord> = VecDeque::new();
        history.push_back(crate::models::build::BuildRecord {
            id: 42,
            task: "assembleDebug".into(),
            status: BuildStatus::Success(BuildResult {
                success: true,
                duration_ms: 1000,
                error_count: 0,
                warning_count: 0,
            }),
            errors: vec![],
            started_at: "2026-04-09T00:00:00Z".into(),
            project_root: None,
        });

        rotate_build_logs(dir_path, 365, 1000, &history);

        assert!(
            dir_path.join("build-42.jsonl").exists(),
            "id=42 (in history) must survive"
        );
        assert!(
            !dir_path.join("build-99.jsonl").exists(),
            "id=99 (orphan) must be deleted"
        );
    }

    #[test]
    fn production_code_does_not_unwrap_active_process_mutex() {
        let source = include_str!("build_runner.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("production section");

        assert!(
            !production_source.contains("active_process_id.lock().unwrap()"),
            "active_process_id mutex must handle poisoning without unwrap()"
        );
    }

    // ── Build slot reservation (H2) ──────────────────────────────────────────

    #[tokio::test]
    async fn try_reserve_build_slot_succeeds_when_idle() {
        let bs = BuildState::new();
        try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .expect("idle state must grant the slot");

        let inner = bs.inner.lock().await;
        assert!(inner.starting);
        assert!(matches!(inner.status, BuildStatus::Running { .. }));
    }

    #[tokio::test]
    async fn try_reserve_build_slot_rejects_when_starting() {
        let bs = BuildState::new();
        try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .unwrap();

        // A second caller — e.g. the MCP server while the UI build is starting.
        let err = try_reserve_build_slot(&bs, "assembleRelease", "2026-01-01T00:00:01Z")
            .await
            .expect_err("second reservation must be refused");
        assert!(err.contains("already running"));
    }

    #[tokio::test]
    async fn try_reserve_build_slot_rejects_when_a_process_is_tracked() {
        let bs = BuildState::new();
        {
            let mut inner = bs.inner.lock().await;
            inner.current_build = Some(42);
        }
        assert!(
            try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:00:00Z")
                .await
                .is_err(),
            "a tracked process must block a new build"
        );
    }

    /// What both front doors do once Gradle has spawned as `pid`: the run
    /// becomes the latest one and the cancellable process. Returns its log.
    /// Unit tests share one data directory, so tests that record builds and
    /// then read the history hold this to keep other tests' builds out of it.
    static PERSISTED_HISTORY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn start_run(bs: &BuildState, pid: ProcessId) -> BuildLog {
        let log = bs.build_log.start_run();
        bs.set_active_process_id(Some(pid));
        let mut inner = bs.inner.lock().await;
        inner.latest_run = Some(pid);
        inner.starting = false;
        inner.current_build = Some(pid);
        log
    }

    fn finalization(run_id: ProcessId, log: BuildLog, task: &str) -> BuildFinalization {
        BuildFinalization {
            run_id,
            log,
            task: task.to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            project_root: None,
            success: false,
            cancelled: false,
            duration_ms: 0,
            errors: vec![],
        }
    }

    #[tokio::test]
    async fn build_slot_is_released_after_finalization() {
        let _history = PERSISTED_HISTORY.lock().await;
        let bs = BuildState::new();
        try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .unwrap();
        let log = start_run(&bs, 7).await;

        emit_build_complete(
            &bs,
            None,
            BuildFinalization {
                run_id: 7,
                log,
                task: "assembleDebug".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                project_root: None,
                success: true,
                cancelled: false,
                duration_ms: 10,
                errors: vec![],
            },
        )
        .await;

        // The next build must be able to start.
        try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:01:00Z")
            .await
            .expect("slot must be free after finalization");
    }

    // ── Finalization is shared by both front doors (H3) ──────────────────────

    #[tokio::test]
    async fn emit_build_complete_records_history_without_an_app_handle() {
        let _history = PERSISTED_HISTORY.lock().await;
        let bs = BuildState::new();
        let log = start_run(&bs, 7).await;
        let event = emit_build_complete(
            &bs,
            None,
            BuildFinalization {
                run_id: 7,
                log,
                task: "assembleDebug".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                project_root: Some("/tmp/p".to_string()),
                success: false,
                cancelled: false,
                duration_ms: 1234,
                errors: vec![BuildError {
                    message: "boom".to_string(),
                    file: None,
                    line: None,
                    col: None,
                    severity: BuildErrorSeverity::Error,
                }],
            },
        )
        .await;

        assert!(!event.success);
        assert_eq!(event.error_count, 1);
        assert_eq!(event.warning_count, 0);

        let inner = bs.inner.lock().await;
        assert_eq!(
            inner.history.back().map(|r| r.task.as_str()),
            Some("assembleDebug"),
            "headless MCP runs must still record history"
        );
        assert_eq!(
            inner.history.back().and_then(|r| r.project_root.as_deref()),
            Some("/tmp/p"),
            "the recorded root must match get_build_history's project filter"
        );
    }

    #[tokio::test]
    async fn finalization_counts_warnings_separately_from_errors() {
        let _history = PERSISTED_HISTORY.lock().await;
        let bs = BuildState::new();
        let log = start_run(&bs, 7).await;
        let mk = |sev| BuildError {
            message: "m".to_string(),
            file: None,
            line: None,
            col: None,
            severity: sev,
        };
        let event = emit_build_complete(
            &bs,
            None,
            BuildFinalization {
                run_id: 7,
                log,
                task: "assembleDebug".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                project_root: None,
                success: true,
                cancelled: false,
                duration_ms: 1,
                errors: vec![
                    mk(BuildErrorSeverity::Warning),
                    mk(BuildErrorSeverity::Warning),
                    mk(BuildErrorSeverity::Error),
                ],
            },
        )
        .await;

        assert_eq!(event.error_count, 1);
        assert_eq!(event.warning_count, 2);
        assert!(event.success, "warnings alone must not fail a build");
    }

    #[tokio::test]
    async fn cancelled_build_is_finalized_as_cancelled() {
        let _history = PERSISTED_HISTORY.lock().await;
        let bs = BuildState::new();
        let log = start_run(&bs, 7).await;
        let event = emit_build_complete(
            &bs,
            None,
            BuildFinalization {
                run_id: 7,
                log,
                task: "assembleDebug".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                project_root: None,
                success: false,
                cancelled: true,
                duration_ms: 0,
                errors: vec![],
            },
        )
        .await;

        assert!(event.cancelled);
        assert!(!event.success);
        let inner = bs.inner.lock().await;
        assert!(matches!(inner.status, BuildStatus::Cancelled));
        assert!(
            matches!(
                inner.history.back().map(|r| &r.status),
                Some(BuildStatus::Cancelled)
            ),
            "history must tell a cancel from a failure"
        );
    }

    /// Cancel A, start B, then A finishes (Gradle takes seconds to shut down).
    /// A used to take B's process ID, clear the slot, and write its own
    /// outcome as the current status: B became uncancellable, a third build
    /// could start next to it, and B's panel showed A's result.
    #[tokio::test]
    async fn late_finalization_of_a_replaced_run_leaves_the_newer_run_alone() {
        let _history = PERSISTED_HISTORY.lock().await;
        let bs = BuildState::new();
        let pm = ProcessManager::new();
        try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .unwrap();
        let log_a = start_run(&bs, 100).await;
        push_build_log(&log_a, "line from A".into());
        cancel_build(&bs, &pm).await;

        try_reserve_build_slot(&bs, "assembleRelease", "2026-01-01T00:00:05Z")
            .await
            .expect("cancelling A frees the slot for B");
        let log_b = start_run(&bs, 200).await;
        push_build_log(&log_a, "A still shutting down".into());
        push_build_log(&log_b, "line from B".into());

        let mut late = finalization(100, log_a, "assembleDebug");
        late.cancelled = true;
        late.errors = vec![BuildError {
            message: "error from A".into(),
            file: None,
            line: None,
            col: None,
            severity: BuildErrorSeverity::Error,
        }];
        emit_build_complete(&bs, None, late).await;

        let (record_id, record_status) = {
            let inner = bs.inner.lock().await;
            assert_eq!(inner.current_build, Some(200));
            assert!(
                matches!(&inner.status, BuildStatus::Running { task, .. } if task == "assembleRelease"),
                "B's status was replaced: {:?}",
                inner.status
            );
            assert!(inner.current_errors.is_empty(), "A's errors leaked into B");
            let record = inner.history.back().expect("A is recorded");
            (record.id, record.status.clone())
        };
        assert!(matches!(record_status, BuildStatus::Cancelled));
        assert_eq!(
            *bs.active_process_id.lock().unwrap(),
            Some(200),
            "B must stay cancellable"
        );
        assert!(
            try_reserve_build_slot(&bs, "check", "2026-01-01T00:00:10Z")
                .await
                .is_err(),
            "B still holds the build slot"
        );

        let saved = std::fs::read_to_string(
            data_dir()
                .join("build-logs")
                .join(format!("build-{record_id}.jsonl")),
        )
        .unwrap();
        assert!(saved.contains("line from A") && saved.contains("A still shutting down"));
        assert!(
            !saved.contains("line from B"),
            "A's saved log holds B's lines"
        );
        let current: Vec<String> = bs
            .build_log
            .current()
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        assert_eq!(current, vec!["line from B".to_string()]);
    }

    // ── Slot release on failure paths ────────────────────────────────────────

    /// Regression: run_task reserved the slot and then returned early via `?`
    /// when the spawn failed, leaving `starting = true` forever. Every later
    /// build from either front door was refused until the app restarted.
    #[tokio::test]
    async fn run_task_releases_the_slot_when_the_spawn_fails() {
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let missing_gradlew = dir.path().join("gradlew-does-not-exist");

        let result = run_task(
            "assembleDebug",
            &[],
            dir.path(),
            &missing_gradlew,
            30,
            vec![],
            Some(dir.path().to_string_lossy().into_owned()),
            &build_state,
            &pm,
            None,
        )
        .await;

        assert!(result.is_err(), "a missing gradlew must fail the run");

        {
            let bs = build_state.inner.lock().await;
            assert!(
                !bs.starting,
                "the slot must not stay reserved after a failed spawn"
            );
            assert!(bs.current_build.is_none());
        }

        // The decisive assertion: a subsequent build can still start.
        try_reserve_build_slot(&build_state, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .expect("slot must be free after a failed spawn");
    }

    #[test]
    fn mark_build_spawn_failed_preserves_a_cancelled_status() {
        let mut state = BuildStateInner::new();
        state.starting = true;
        state.status = BuildStatus::Cancelled;

        mark_build_spawn_failed(&mut state);

        assert!(!state.starting);
        assert!(
            matches!(state.status, BuildStatus::Cancelled),
            "an explicit cancellation must not be relabelled as a failure"
        );
    }

    #[tokio::test]
    async fn mark_build_spawn_failed_frees_the_slot_for_the_next_build() {
        let build_state = BuildState::new();
        try_reserve_build_slot(&build_state, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .unwrap();

        {
            let mut bs = build_state.inner.lock().await;
            mark_build_spawn_failed(&mut bs);
        }

        try_reserve_build_slot(&build_state, "assembleDebug", "2026-01-01T00:00:01Z")
            .await
            .expect("slot must be reusable");
    }
}
