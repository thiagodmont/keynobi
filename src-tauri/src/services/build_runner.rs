use crate::models::build::{
    BuildError, BuildErrorSeverity, BuildLine, BuildLineKind, BuildLinesEvent, BuildRecord,
    BuildResult, BuildStartedEvent, BuildStatus, LaunchTiming,
};
use crate::models::error::AppError;
use crate::services::build_lock::{self, BuildLock};
use crate::services::build_parser;
use crate::services::gradle_modules::{self, GradleModule};
use crate::services::mapping_snapshots::{self, MappingSource, PreparedMapping};
use crate::services::process_manager::{self, ProcessId, ProcessManager, ProcessTermination};
use crate::services::settings_manager::{data_dir, unique_tmp_path, with_data_lock_in};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::sync::{Arc, Weak};
use tokio::sync::{watch, Mutex};

pub use crate::models::build::{AgentActor, BuildActor, BuildCompleteEvent};

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
/// another process are kept, and the record's ID is allocated here. The
/// build's prepared R8 mappings are published and linked to the record, and
/// snapshots the kept history no longer references are pruned, in the same
/// critical section. Returns the ID and the history as persisted.
fn persist_build_record_in(
    dir: &Path,
    mut record: BuildRecord,
    raw_lines: &VecDeque<String>,
    retention_days: u32,
    max_folder_mb: u32,
    mappings: Vec<PreparedMapping>,
) -> Result<(u32, VecDeque<BuildRecord>), String> {
    with_data_lock_in(dir, || {
        let build_log_dir = dir.join("build-logs");
        let mut history = load_build_history_from(dir);
        let id = next_build_id(&history, &build_log_dir);
        record.id = id;
        record.mappings = mapping_snapshots::publish_snapshots(dir, mappings);
        history.push_back(record);
        while history.len() > MAX_HISTORY {
            history.pop_front();
        }
        save_build_history_to(dir, &history)?;
        save_build_log_to(id, raw_lines, &build_log_dir);
        rotate_build_logs(&build_log_dir, retention_days, max_folder_mb, &history);
        mapping_snapshots::prune_snapshots(dir, &mapping_snapshots::mappings_to_keep(&history));
        Ok((id, history))
    })?
}

/// Rotate build logs and R8 mapping snapshots against the persisted history,
/// under the data lock so a build another process is recording keeps both.
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
        mapping_snapshots::prune_snapshots(&dir, &mapping_snapshots::mappings_to_keep(&history));
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
    /// Who started the build `status` describes.
    pub status_origin: Option<BuildActor>,
    /// Who cancelled the build `status` describes, when it was cancelled.
    pub status_cancelled_by: Option<BuildActor>,
    /// Counts slot reservations, so a cancel that lands while a build is
    /// still starting is matched to that build and no other.
    reservation: u64,
    /// Why the cancellable run was stopped, until its finalization reads it.
    stop: Option<(RunKey, StopReason)>,
}

/// The run a stop request is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunKey {
    /// A build still starting, by its reservation.
    Starting(u64),
    Run(ProcessId),
}

/// Why a run was stopped before Gradle finished on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Cancelled(BuildActor),
    /// The MCP caller's wait ran out; recorded as a failure, not a cancel.
    TimedOut {
        after_sec: u64,
    },
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
            status_origin: None,
            status_cancelled_by: None,
            reservation: 0,
            stop: None,
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
    /// Runs started and not yet finalized.
    runs_in_flight: Arc<watch::Sender<usize>>,
    /// The cross-process lock this process's runs hold, shared by a run still
    /// shutting down and its replacement on the same project.
    project_lock: Arc<StdMutex<Option<HeldProjectLock>>>,
}

/// A Gradle root and the lock this process's runs hold on it.
type HeldProjectLock = (PathBuf, Weak<BuildLock>);

impl BuildState {
    pub fn new() -> Self {
        BuildState {
            inner: Arc::new(Mutex::new(BuildStateInner::new())),
            build_log: BuildLogSlot::default(),
            active_process_id: Arc::new(StdMutex::new(None)),
            runs_in_flight: Arc::new(watch::channel(0).0),
            project_lock: Arc::default(),
        }
    }

    pub fn take_active_process_id(&self) -> Option<ProcessId> {
        match self.active_process_id.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }

    fn active_process_id(&self) -> Option<ProcessId> {
        match self.active_process_id.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    /// Wait until every run this state started has been recorded, or
    /// `timeout` passes. Returns whether none is left.
    pub async fn wait_for_runs(&self, timeout: std::time::Duration) -> bool {
        let mut runs = self.runs_in_flight.subscribe();
        tokio::time::timeout(timeout, runs.wait_for(|n| *n == 0))
            .await
            .is_ok_and(|r| r.is_ok())
    }

    /// The cross-process lock for `gradle_root`, reusing the one a run of
    /// this process still holds. `Ok(None)` when the lock file is unusable:
    /// builds then proceed without cross-process exclusion.
    fn acquire_project_lock(&self, gradle_root: &Path) -> Result<Option<Arc<BuildLock>>, String> {
        let mut slot = self
            .project_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((root, held)) = slot.as_ref() {
            if root == gradle_root {
                if let Some(lock) = held.upgrade() {
                    return Ok(Some(lock));
                }
            }
        }
        match build_lock::try_acquire(&data_dir(), gradle_root) {
            Ok(lock) => {
                let lock = Arc::new(lock);
                *slot = Some((gradle_root.to_path_buf(), Arc::downgrade(&lock)));
                Ok(Some(lock))
            }
            Err(build_lock::LockError::Held { pid }) => Err(format!(
                "{BUILD_ALREADY_RUNNING} for this project in another Keynobi process{}",
                pid.map(|p| format!(" (pid {p})")).unwrap_or_default()
            )),
            Err(build_lock::LockError::Io(e)) => {
                tracing::warn!("Building without the cross-process build lock: {e}");
                Ok(None)
            }
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
            runs_in_flight: self.runs_in_flight.clone(),
            project_lock: self.project_lock.clone(),
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

/// Most lines returned for one saved build log.
pub const MAX_BUILD_LOG_ENTRIES: usize = 10_000;

/// Read the saved log of build `id`.
///
/// Every recorded build saves a log file, empty when Gradle printed nothing, so
/// a missing file means rotation removed it: that is `NotFound`, not an empty log.
pub async fn read_build_log_in(build_log_dir: &Path, id: u32) -> Result<Vec<BuildLine>, AppError> {
    let path = build_log_dir.join(format!("build-{id}.jsonl"));
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::NotFound(format!(
                "The log of build #{id} is no longer on disk"
            )));
        }
        Err(e) => return Err(AppError::io(path.display(), e)),
    };
    Ok(content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .take(MAX_BUILD_LOG_ENTRIES)
        .collect())
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

/// One installable APK under `<module>/build/outputs/apk`, and the variant it belongs to.
struct ApkCandidate {
    path: PathBuf,
    /// From `output-metadata.json` when present, else the directory segments
    /// below `apk/` joined (`paid/debug` → `paiddebug`), lowercased.
    variant: String,
}

/// Directory holding a module's APK outputs.
fn apk_outputs_dir(module_dir: &Path) -> PathBuf {
    module_dir.join("build").join("outputs").join("apk")
}

/// [`apk_outputs_dir`] of `module`, unless a symlinked module, `build`,
/// `outputs`, or `apk` directory moves it outside the project. A missing
/// directory is fine.
fn apk_outputs_dir_within(gradle_root: &Path, module: &GradleModule) -> Result<PathBuf, String> {
    let base = apk_outputs_dir(&module.dir);
    let relative = format!("{}/build/outputs/apk", module.relative_dir(gradle_root));
    match crate::utils::path::validate_within_root(gradle_root, &relative) {
        Err(AppError::PermissionDenied(_)) => Err(format!(
            "{} resolves outside the project; its APKs are not used.",
            base.display()
        )),
        _ => Ok(base),
    }
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

/// The application IDs AGP recorded for every variant built into the APK
/// outputs of the project's application modules, including any
/// `applicationIdSuffix`.
pub fn built_application_ids(gradle_root: &Path) -> Vec<String> {
    let mut dirs: Vec<PathBuf> = gradle_modules::application_modules(gradle_root)
        .iter()
        .filter_map(|module| apk_outputs_dir_within(gradle_root, module).ok())
        .flat_map(|base| walk_dir_for_apk(&base, 6))
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

/// Resolve the APK that building `variant_name` produced in the application
/// module (see [`gradle_modules::resolve_application_module`]: `module` names
/// it, or a task in it, and is required when the project has several).
///
/// Standard AGP layout:
///   `{module}/build/outputs/apk/{buildType}/{module}-{buildType}.apk`
///   or with flavors:
///   `{module}/build/outputs/apk/{flavor}/{buildType}/{module}-{flavor}-{buildType}.apk`
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
/// An actionable message when the application module cannot be determined,
/// there are no outputs, no APK for the variant, or more than one candidate
/// (for example split APKs).
pub fn find_output_apk(
    gradle_root: &Path,
    module: Option<&str>,
    variant_name: &str,
) -> Result<PathBuf, String> {
    let module = gradle_modules::resolve_application_module(gradle_root, module)?;
    let base = apk_outputs_dir_within(gradle_root, &module)?;
    // An `outputFile` in the metadata or a symlink can point anywhere; only
    // APKs that resolve inside the build outputs are this project's.
    let candidates: Vec<ApkCandidate> = collect_apk_candidates(&base)
        .into_iter()
        .filter(|c| {
            crate::utils::path::validate_apk_within_build_outputs(gradle_root, &c.path).is_ok()
        })
        .collect();
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
/// UI learns about builds whoever started them.
pub async fn emit_build_complete(
    build_state: &BuildState,
    app_handle: Option<&tauri::AppHandle>,
    finalization: BuildFinalization,
) -> BuildCompleteEvent {
    let event = finalize_completed_build(build_state, finalization).await;
    if let Some(handle) = app_handle {
        use tauri::Emitter;
        let _ = handle.emit(BUILD_COMPLETE_EVENT, event.clone());
    }
    event
}

/// Emitted once per run after it spawned, before any of its output.
pub const BUILD_STARTED_EVENT: &str = "build:started";
/// Batched output of a run.
pub const BUILD_LINES_EVENT: &str = "build:lines";
/// Emitted once per run after its history entry is recorded.
pub const BUILD_COMPLETE_EVENT: &str = "build:complete";

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
    pub origin: Option<BuildActor>,
    pub cancelled_by: Option<BuildActor>,
    /// Where to look for the R8 mappings the build wrote. Only a successful
    /// build's are saved.
    pub mappings: Option<MappingSource>,
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
    let status = if finalization.cancelled {
        BuildStatus::Cancelled
    } else if result.success {
        BuildStatus::Success(result)
    } else {
        BuildStatus::Failed(result)
    };
    let mapping_source = finalization
        .mappings
        .filter(|_| finalization.success && !finalization.cancelled);

    let record_id = record_run(
        build_state,
        finalization.run_id,
        &finalization.log,
        BuildRecord {
            id: 0,
            task: finalization.task.clone(),
            status,
            errors: finalization.errors,
            started_at: finalization.started_at,
            project_root: finalization.project_root,
            origin: finalization.origin.clone(),
            cancelled_by: finalization.cancelled_by.clone(),
            launch: None,
            mappings: Vec::new(),
        },
        mapping_source,
    )
    .await;

    BuildCompleteEvent {
        run_id: finalization.run_id,
        record_id,
        success: finalization.success,
        cancelled: finalization.cancelled,
        duration_ms: finalization.duration_ms,
        error_count,
        warning_count: warn_count,
        task: finalization.task,
        origin: finalization.origin,
        cancelled_by: finalization.cancelled_by,
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

/// Error returned when the build slot is taken, by every front door.
pub const BUILD_ALREADY_RUNNING: &str = "A Gradle build is already running";

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
    reserve_build_slot(build_state, task, started_at, None)
        .await
        .map(|_| ())
}

/// [`try_reserve_build_slot`] for a build started by `origin`. Returns the
/// reservation and the status it replaced.
async fn reserve_build_slot(
    build_state: &BuildState,
    task: &str,
    started_at: &str,
    origin: Option<BuildActor>,
) -> Result<(u64, BuildStatus), String> {
    let mut bs = build_state.inner.lock().await;
    if bs.starting || bs.current_build.is_some() || matches!(bs.status, BuildStatus::Running { .. })
    {
        return Err(BUILD_ALREADY_RUNNING.to_string());
    }
    bs.starting = true;
    bs.reservation += 1;
    let previous = std::mem::replace(
        &mut bs.status,
        BuildStatus::Running {
            task: task.to_owned(),
            started_at: started_at.to_owned(),
        },
    );
    bs.status_origin = origin;
    bs.status_cancelled_by = None;
    bs.current_errors.clear();
    Ok((bs.reservation, previous))
}

/// Cancel the running build, whoever started it, recording `by` as the
/// canceller. Returns `true` if a build was running, `false` otherwise.
pub async fn cancel_build(
    build_state: &BuildState,
    process_manager: &ProcessManager,
    by: BuildActor,
) -> bool {
    stop_build(
        build_state,
        process_manager,
        None,
        StopReason::Cancelled(by),
    )
    .await
}

/// Stop the running build (only if it is `only`, when given) for `reason`.
/// The run's finalization records why.
async fn stop_build(
    build_state: &BuildState,
    process_manager: &ProcessManager,
    only: Option<ProcessId>,
    reason: StopReason,
) -> bool {
    let pid = {
        let mut bs = build_state.inner.lock().await;
        // Set synchronously right after spawn, so it can be ahead of `current_build`.
        let pid = build_state.active_process_id().or(bs.current_build);
        if only.is_some() && pid != only {
            return false;
        }
        let key = match pid {
            Some(pid) => Some(RunKey::Run(pid)),
            None if bs.starting => Some(RunKey::Starting(bs.reservation)),
            None => None,
        };
        if key.is_none() && !matches!(bs.status, BuildStatus::Running { .. }) {
            return false;
        }
        if let Some(pid) = pid {
            build_state.release_active_process_id(pid);
            if bs.current_build == Some(pid) {
                bs.current_build = None;
            }
        }
        bs.starting = false;
        bs.status = BuildStatus::Cancelled;
        bs.status_cancelled_by = match &reason {
            StopReason::Cancelled(by) => Some(by.clone()),
            StopReason::TimedOut { .. } => None,
        };
        bs.stop = key.map(|key| (key, reason));
        pid
    };
    if let Some(id) = pid {
        process_manager::cancel(&process_manager.0, id).await;
    }
    true
}

/// Clear all build history from memory and disk. The in-memory clear always
/// happens; a failure to clear the file is returned.
pub async fn clear_history(build_state: &BuildState) -> Result<(), String> {
    build_state.inner.lock().await.history.clear();
    tokio::task::spawn_blocking(|| clear_history_in(&data_dir()))
        .await
        .map_err(|e| format!("Failed to clear build history: {e}"))?
}

/// Save an empty history and remove the mapping snapshots it no longer names.
fn clear_history_in(dir: &Path) -> Result<(), String> {
    with_data_lock_in(dir, || {
        let empty = VecDeque::new();
        save_build_history_to(dir, &empty)?;
        mapping_snapshots::prune_snapshots(dir, &mapping_snapshots::mappings_to_keep(&empty));
        Ok(())
    })?
}

/// Record the completed build result and push it to history.
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
    let status = if cancelled {
        BuildStatus::Cancelled
    } else if result.success {
        BuildStatus::Success(result)
    } else {
        BuildStatus::Failed(result)
    };
    record_run(
        build_state,
        run_id,
        log,
        BuildRecord {
            id: 0,
            task,
            status,
            errors,
            started_at,
            project_root,
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings: Vec::new(),
        },
        None,
    )
    .await;
}

/// Push `record` (its ID is allocated when persisted) to history. Returns its ID.
///
/// Every run gets a history entry, but only the latest run updates the shared
/// status, errors, and cancellable process. A run cancelled and replaced by a
/// newer one can finish seconds later; letting it write those would show its
/// outcome as the newer build's, make the newer build uncancellable, and free
/// the build slot while the newer Gradle is still running.
async fn record_run(
    build_state: &BuildState,
    run_id: ProcessId,
    log: &BuildLog,
    record: BuildRecord,
    mapping_source: Option<MappingSource>,
) -> u32 {
    // Snapshot the run's log before taking the inner lock so we don't hold two
    // locks simultaneously.
    let raw_lines: VecDeque<String> = log.lock().map(|g| g.clone()).unwrap_or_default();

    build_state.release_active_process_id(run_id);

    {
        let mut bs = build_state.inner.lock().await;
        if !bs.starting && bs.latest_run == Some(run_id) {
            bs.status = record.status.clone();
            bs.status_origin = record.origin.clone();
            bs.status_cancelled_by = record.cancelled_by.clone();
            bs.current_errors = record.errors.clone();
            bs.current_build = None;
        }
    }

    // Disk I/O runs off the async runtime and outside the build-state lock.
    // Mappings are copied before the data lock is taken: they can be large,
    // and other processes wait on that lock for settings and history.
    let record_for_io = record.clone();
    let persisted = tokio::task::spawn_blocking(move || {
        let (settings, _) = crate::services::settings_manager::load_settings();
        let dir = data_dir();
        let mappings = mapping_source
            .map(|source| mapping_snapshots::prepare_snapshots(&dir, &source).mappings)
            .unwrap_or_default();
        persist_build_record_in(
            &dir,
            record_for_io,
            &raw_lines,
            settings.build.build_log_retention_days,
            settings.build.build_log_max_folder_mb,
            mappings,
        )
    })
    .await
    .map_err(|e| format!("Build persistence task failed: {e}"))
    .and_then(|result| result);

    let mut bs = build_state.inner.lock().await;
    match persisted {
        Ok((id, persisted_history)) => {
            bs.history = merge_history(&bs.history, persisted_history);
            id
        }
        Err(e) => {
            // Keep the build visible in this session even though it was not
            // saved. A failure here used to be silent.
            tracing::warn!("Failed to persist build history: {e}");
            let mut record = record;
            let id = bs.history.iter().map(|r| r.id).max().unwrap_or(0) + 1;
            record.id = id;
            bs.history.push_back(record);
            while bs.history.len() > MAX_HISTORY {
                bs.history.pop_front();
            }
            id
        }
    }
}

/// Record `timing` on history record `id`: in the persisted history, under the
/// data lock (re-read inside it, since other processes append to it), and in
/// memory. Only a successful build takes a launch time.
pub async fn attach_launch_timing(
    build_state: &BuildState,
    id: u32,
    timing: LaunchTiming,
) -> Result<(), AppError> {
    let for_io = timing.clone();
    let persisted =
        tokio::task::spawn_blocking(move || attach_launch_timing_in(&data_dir(), id, for_io))
            .await
            .map_err(|e| AppError::Other(format!("Launch timing task failed: {e}")))?;

    let mut bs = build_state.inner.lock().await;
    match persisted {
        Ok(history) => {
            bs.history = merge_history(&bs.history, history);
            Ok(())
        }
        // A record only this session holds (its save failed) is updated in memory.
        Err(AppError::NotFound(message)) => match bs.history.iter_mut().find(|r| r.id == id) {
            Some(record) => set_launch(record, timing),
            None => Err(AppError::NotFound(message)),
        },
        Err(e) => Err(e),
    }
}

fn set_launch(record: &mut BuildRecord, timing: LaunchTiming) -> Result<(), AppError> {
    if !matches!(record.status, BuildStatus::Success(_)) {
        return Err(AppError::InvalidInput(format!(
            "Build #{} did not succeed; it has no launch to record",
            record.id
        )));
    }
    record.launch = Some(timing);
    Ok(())
}

fn attach_launch_timing_in(
    dir: &Path,
    id: u32,
    timing: LaunchTiming,
) -> Result<VecDeque<BuildRecord>, AppError> {
    with_data_lock_in(dir, || {
        let mut history = load_build_history_from(dir);
        let record = history
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::NotFound(format!("Build #{id} is not in the history")))?;
        set_launch(record, timing)?;
        save_build_history_to(dir, &history).map_err(AppError::Io)?;
        Ok(history)
    })
    .map_err(AppError::Io)?
}

/// Environment for a Gradle process in `gradle_root`, with `gradlew` made
/// executable, for a project the user trusted. The only way to prepare a
/// Gradle run: an untrusted project is refused before anything is changed or
/// spawned.
pub fn trusted_gradle_env(
    settings: &crate::models::settings::AppSettings,
    project_root: &Path,
    gradle_root: &Path,
) -> Result<Vec<(String, String)>, String> {
    crate::services::project_trust::require_trusted(settings, project_root)?;
    Ok(build_env_vars(settings, gradle_root))
}

/// Build environment variables for a Gradle process, and ensure `gradlew` is executable.
fn build_env_vars(
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

// ── Running a build ───────────────────────────────────────────────────────────

/// How often a run's output is sent to the app as `build:lines`.
pub const BUILD_LINES_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
/// Most lines in one `build:lines` event.
pub const MAX_LINES_PER_BATCH: usize = 500;
/// Most lines waiting for the next flush; past it the oldest are dropped.
pub const MAX_PENDING_BUILD_LINES: usize = 10_000;
/// Longest task name a run reports as its current task.
const MAX_CURRENT_TASK_BYTES: usize = 256;

/// A Gradle build to run.
pub struct BuildRequest {
    pub task: String,
    pub extra_args: Vec<String>,
    pub gradle_root: PathBuf,
    pub gradlew: PathBuf,
    /// From [`trusted_gradle_env`].
    pub env: Vec<(String, String)>,
    /// Recorded with the build; history is scoped by it.
    pub project_root: Option<String>,
    pub origin: BuildActor,
}

/// Why a build did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartBuildError {
    /// Another build of this process holds the slot.
    Busy(String),
    /// Another Keynobi process is building the same project.
    BusyElsewhere(String),
    /// Gradle could not be started.
    Spawn(String),
}

impl std::fmt::Display for StartBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartBuildError::Busy(msg)
            | StartBuildError::BusyElsewhere(msg)
            | StartBuildError::Spawn(msg) => f.write_str(msg),
        }
    }
}

/// How a canceller reads after "cancelled", for messages to agents.
pub fn describe_canceller(by: &BuildActor) -> String {
    match by {
        BuildActor::App => "in the Keynobi app".into(),
        BuildActor::AppQuit => "because Keynobi quit".into(),
        BuildActor::Agent(agent) => match &agent.client_name {
            Some(name) => format!("by an agent ({name})"),
            None => "by an agent".into(),
        },
    }
}

/// How a run ended.
#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub run_id: ProcessId,
    pub success: bool,
    pub cancelled: bool,
    pub cancelled_by: Option<BuildActor>,
    /// Set when a caller's wait ran out and stopped the build.
    pub timed_out_after_sec: Option<u64>,
    pub duration_ms: u64,
    pub errors: Vec<BuildError>,
}

/// A started run. Dropping it, or the future waiting on it, does not affect
/// the build: the run finishes and is recorded on its own.
pub struct BuildHandle {
    pub run_id: ProcessId,
    outcome: watch::Receiver<Option<BuildOutcome>>,
    current_task: watch::Receiver<Option<String>>,
}

impl BuildHandle {
    /// The Gradle task the run started last (`:app:compileDebugKotlin`), if any.
    pub fn current_task(&self) -> Option<String> {
        self.current_task.borrow().clone()
    }

    /// Wait for the run to be recorded.
    pub async fn wait(&mut self) -> BuildOutcome {
        if let Ok(outcome) = self.outcome.wait_for(Option::is_some).await {
            if let Some(outcome) = outcome.clone() {
                return outcome;
            }
        }
        // The run's task ended without an outcome (the runtime is shutting down).
        BuildOutcome {
            run_id: self.run_id,
            success: false,
            cancelled: false,
            cancelled_by: None,
            timed_out_after_sec: None,
            duration_ms: 0,
            errors: vec![BuildError {
                message: "The build ended without a result".into(),
                file: None,
                line: None,
                col: None,
                severity: BuildErrorSeverity::Error,
            }],
        }
    }
}

/// Start a Gradle build: the one implementation behind the app's Build and
/// Run and the MCP build tools.
///
/// Takes the cross-process lock for the project and this process's build
/// slot, spawns Gradle, and returns once it runs. A task owned by the run then
/// parses its output, streams it to the app (`build:started`, `build:lines`,
/// `build:complete`, only with an `app_handle`), and records the result,
/// whether or not anyone waits on the returned handle.
///
/// # Errors
/// [`StartBuildError::Busy`] when a build is running in this or another
/// Keynobi process; [`StartBuildError::Spawn`] when Gradle cannot start.
pub async fn start_build(
    build_state: &BuildState,
    process_manager: &ProcessManager,
    app_handle: Option<&tauri::AppHandle>,
    request: BuildRequest,
) -> Result<BuildHandle, StartBuildError> {
    use crate::services::process_manager::SpawnOptions;
    use tauri::Emitter;

    let BuildRequest {
        task,
        extra_args,
        gradle_root,
        gradlew,
        env,
        project_root,
        origin,
    } = request;
    let started_at = chrono::Utc::now().to_rfc3339();
    let started = std::time::SystemTime::now();

    // The lock first: a build of this process on the same project already
    // holds it, and then the slot below gives the usual answer.
    let project_lock = build_state
        .acquire_project_lock(&gradle_root)
        .map_err(StartBuildError::BusyElsewhere)?;
    let (reservation, _) =
        reserve_build_slot(build_state, &task, &started_at, Some(origin.clone()))
            .await
            .map_err(StartBuildError::Busy)?;
    let in_flight = RunInFlight::new(build_state);

    // Connected tests hold the devices' UI Automator until the run ends.
    let instrumentation =
        crate::services::ui_automator_lock::begin_instrumentation_for_task(&task, &env);
    // This run's own log. Starting it before the spawn means no line is lost.
    let log = build_state.build_log.start_run();
    let collector = Arc::new(RunCollector::new(app_handle.is_some()));
    let current_task_rx = collector.current_task.subscribe();
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<ProcessTermination>();
    let exit_tx = StdMutex::new(Some(exit_tx));

    let mut args: Vec<&str> = vec![&task, "--console=plain"];
    args.extend(extra_args.iter().map(String::as_str));
    let spawned = process_manager::spawn(
        &process_manager.0,
        gradlew.to_str().unwrap_or("./gradlew"),
        &args,
        gradle_root.clone(),
        env,
        SpawnOptions {
            on_line: Box::new({
                let collector = collector.clone();
                let log = log.clone();
                move |line| collector.on_line(&log, line.text)
            }),
            on_exit: Box::new(move |_, termination| {
                if let Some(tx) = exit_tx.lock().ok().and_then(|mut tx| tx.take()) {
                    let _ = tx.send(termination);
                }
            }),
        },
    )
    .await;

    let run_id = match spawned {
        Ok(run_id) => run_id,
        Err(e) => {
            let mut bs = build_state.inner.lock().await;
            // Release the slot only if it is still this build's; without
            // this every later build is refused until the app restarts.
            if bs.reservation == reservation {
                mark_build_spawn_failed(&mut bs);
            }
            return Err(StartBuildError::Spawn(e));
        }
    };

    // Set before any `.await` so a cancel always finds the process.
    build_state.set_active_process_id(Some(run_id));
    let cancelled_while_starting = {
        let mut bs = build_state.inner.lock().await;
        match &mut bs.stop {
            Some((key, _)) if *key == RunKey::Starting(reservation) => {
                *key = RunKey::Run(run_id);
                true
            }
            _ => {
                bs.latest_run = Some(run_id);
                bs.starting = false;
                bs.current_build = Some(run_id);
                false
            }
        }
    };
    if cancelled_while_starting {
        // Cancelled before Gradle was up: stop it now; it is still recorded.
        build_state.release_active_process_id(run_id);
        process_manager::cancel(&process_manager.0, run_id).await;
    }

    if let Some(app) = app_handle {
        let _ = app.emit(
            BUILD_STARTED_EVENT,
            BuildStartedEvent {
                run_id,
                task: task.clone(),
                origin: origin.clone(),
                started_at: started_at.clone(),
                project_root: project_root.clone(),
            },
        );
    }

    let (outcome_tx, outcome_rx) = watch::channel(None);
    let run = Run {
        state: build_state.clone(),
        app: app_handle.cloned(),
        run_id,
        task,
        started_at,
        project_root,
        origin,
        mappings: MappingSource {
            gradle_root,
            build_started: started,
        },
        log,
        collector,
        _instrumentation: instrumentation,
        _project_lock: project_lock,
        _in_flight: in_flight,
    };
    tokio::spawn(async move {
        let termination = run.wait_for_exit(exit_rx).await;
        let outcome = run.finish(termination).await;
        let _ = outcome_tx.send(Some(outcome));
    });

    Ok(BuildHandle {
        run_id,
        outcome: outcome_rx,
        current_task: current_task_rx,
    })
}

/// Cancel `run` only: a no-op when another build (or none) is running.
/// For a caller that stops wanting the build it started.
pub async fn cancel_run(
    build_state: &BuildState,
    process_manager: &ProcessManager,
    run: ProcessId,
    by: BuildActor,
) -> bool {
    stop_build(
        build_state,
        process_manager,
        Some(run),
        StopReason::Cancelled(by),
    )
    .await
}

/// Stop the running build because the wait of the caller that started it
/// (`run`, or whichever build runs when `None`) ran out after `after_sec`.
/// It is recorded as failed with a timeout error, not as cancelled.
pub async fn time_out_build(
    build_state: &BuildState,
    process_manager: &ProcessManager,
    run: Option<ProcessId>,
    after_sec: u64,
) -> bool {
    stop_build(
        build_state,
        process_manager,
        run,
        StopReason::TimedOut { after_sec },
    )
    .await
}

/// Counts a run in [`BuildState::wait_for_runs`] until dropped.
struct RunInFlight(Arc<watch::Sender<usize>>);

impl RunInFlight {
    fn new(state: &BuildState) -> Self {
        state.runs_in_flight.send_modify(|n| *n += 1);
        Self(state.runs_in_flight.clone())
    }
}

impl Drop for RunInFlight {
    fn drop(&mut self) {
        self.0.send_modify(|n| *n = n.saturating_sub(1));
    }
}

/// What a run's output told us so far. Filled from the process reader.
struct RunCollector {
    errors: StdMutex<Vec<BuildError>>,
    /// The last summary line said `BUILD SUCCESSFUL`.
    succeeded: std::sync::atomic::AtomicBool,
    duration_ms: std::sync::atomic::AtomicU64,
    /// Lines waiting for the next `build:lines`; `None` with no app to send them to.
    pending: Option<StdMutex<VecDeque<BuildLine>>>,
    /// The last `> Task :…` line's task, for progress reports.
    current_task: watch::Sender<Option<String>>,
}

impl RunCollector {
    fn new(stream_to_app: bool) -> Self {
        Self {
            errors: StdMutex::default(),
            succeeded: Default::default(),
            duration_ms: Default::default(),
            pending: stream_to_app.then(StdMutex::default),
            current_task: watch::channel(None).0,
        }
    }

    fn on_line(&self, log: &BuildLog, text: String) {
        use std::sync::atomic::Ordering;
        let line = parse_build_line(&text);
        push_build_log(log, text);
        if matches!(line.kind, BuildLineKind::Error | BuildLineKind::Warning) {
            if let Ok(mut errors) = self.errors.lock() {
                push_build_error(
                    &mut errors,
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
        if line.kind == BuildLineKind::TaskStart {
            let mut task = line.content.clone();
            if task.len() > MAX_CURRENT_TASK_BYTES {
                let mut end = MAX_CURRENT_TASK_BYTES;
                while !task.is_char_boundary(end) {
                    end -= 1;
                }
                task.truncate(end);
            }
            self.current_task.send_replace(Some(task));
        }
        if line.kind == BuildLineKind::Summary {
            self.duration_ms
                .store(parse_build_duration(&line.content), Ordering::Relaxed);
            self.succeeded
                .store(line.content.contains("BUILD SUCCESSFUL"), Ordering::Relaxed);
        }
        if let Some(pending) = &self.pending {
            if let Ok(mut pending) = pending.lock() {
                if pending.len() >= MAX_PENDING_BUILD_LINES {
                    pending.pop_front();
                }
                pending.push_back(line);
            }
        }
    }

    fn take_pending(&self) -> Vec<BuildLine> {
        self.pending
            .as_ref()
            .and_then(|pending| pending.lock().ok().map(|mut p| p.drain(..).collect()))
            .unwrap_or_default()
    }
}

/// A spawned run, owned by the task that finishes it.
struct Run {
    state: BuildState,
    app: Option<tauri::AppHandle>,
    run_id: ProcessId,
    task: String,
    started_at: String,
    project_root: Option<String>,
    origin: BuildActor,
    mappings: MappingSource,
    log: BuildLog,
    collector: Arc<RunCollector>,
    _instrumentation: Option<crate::services::ui_automator_lock::InstrumentationRun>,
    _project_lock: Option<Arc<BuildLock>>,
    _in_flight: RunInFlight,
}

impl Run {
    /// Wait for Gradle to exit, sending its output to the app meanwhile.
    async fn wait_for_exit(
        &self,
        exit: tokio::sync::oneshot::Receiver<ProcessTermination>,
    ) -> ProcessTermination {
        // Only the process manager's reader drops the sender unsent, when its task dies.
        let lost = ProcessTermination::Signal(0);
        if self.app.is_none() {
            return exit.await.unwrap_or(lost);
        }
        tokio::pin!(exit);
        let mut flush = tokio::time::interval(BUILD_LINES_FLUSH_INTERVAL);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                termination = &mut exit => {
                    self.send_lines();
                    return termination.unwrap_or(lost);
                }
                _ = flush.tick() => self.send_lines(),
            }
        }
    }

    fn send_lines(&self) {
        use tauri::Emitter;
        let Some(app) = &self.app else { return };
        let lines = self.collector.take_pending();
        for batch in lines.chunks(MAX_LINES_PER_BATCH) {
            let _ = app.emit(
                BUILD_LINES_EVENT,
                BuildLinesEvent {
                    run_id: self.run_id,
                    lines: batch.to_vec(),
                },
            );
        }
    }

    /// Record the run and report it. Consumes the run so its locks are
    /// released before anyone waiting hears about it.
    async fn finish(self, termination: ProcessTermination) -> BuildOutcome {
        use std::sync::atomic::Ordering;
        let stop = {
            let mut bs = self.state.inner.lock().await;
            match &bs.stop {
                Some((RunKey::Run(id), _)) if *id == self.run_id => {
                    bs.stop.take().map(|(_, reason)| reason)
                }
                _ => None,
            }
        };
        let killed = termination == ProcessTermination::Cancelled;
        let (cancelled, cancelled_by, timed_out_after_sec) = match (killed, stop) {
            (true, Some(StopReason::TimedOut { after_sec })) => (false, None, Some(after_sec)),
            (true, Some(StopReason::Cancelled(by))) => (true, Some(by), None),
            (true, None) => (true, None, None),
            // It exited on its own before the stop reached it.
            (false, _) => (false, None, None),
        };
        let mut errors = self
            .collector
            .errors
            .lock()
            .map(|e| e.clone())
            .unwrap_or_default();
        if let Some(after_sec) = timed_out_after_sec {
            push_build_error(
                &mut errors,
                BuildError {
                    message: format!("Build timed out after {after_sec}s and was cancelled"),
                    file: None,
                    line: None,
                    col: None,
                    severity: BuildErrorSeverity::Error,
                },
            );
        }
        // Exit code is authoritative: stray "BUILD SUCCESSFUL" text in the
        // output must not override a non-zero exit.
        let success = !cancelled
            && timed_out_after_sec.is_none()
            && termination == ProcessTermination::ExitCode(0)
            && self.collector.succeeded.load(Ordering::Relaxed);
        let duration_ms = self.collector.duration_ms.load(Ordering::Relaxed);

        emit_build_complete(
            &self.state,
            self.app.as_ref(),
            BuildFinalization {
                run_id: self.run_id,
                log: self.log.clone(),
                task: self.task.clone(),
                started_at: self.started_at.clone(),
                project_root: self.project_root.clone(),
                success,
                cancelled,
                duration_ms,
                errors: errors.clone(),
                origin: Some(self.origin.clone()),
                cancelled_by: cancelled_by.clone(),
                mappings: Some(self.mappings.clone()),
            },
        )
        .await;

        BuildOutcome {
            run_id: self.run_id,
            success,
            cancelled,
            cancelled_by,
            timed_out_after_sec,
            duration_ms,
            errors,
        }
    }
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

    fn gradlew_mode(project: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(project.join("gradlew"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }

    fn project_with_plain_gradlew() -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let project = tempfile::tempdir().unwrap();
        let gradlew = project.path().join("gradlew");
        std::fs::write(&gradlew, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&gradlew, std::fs::Permissions::from_mode(0o644)).unwrap();
        project
    }

    #[test]
    fn an_untrusted_project_gets_no_gradle_env_and_its_gradlew_is_untouched() {
        let project = project_with_plain_gradlew();
        let settings = crate::models::settings::AppSettings::default();

        let err = trusted_gradle_env(&settings, project.path(), project.path()).unwrap_err();

        assert!(err.contains("not trusted"), "{err}");
        assert_eq!(
            gradlew_mode(project.path()),
            0o644,
            "gradlew not made executable"
        );
    }

    #[test]
    fn a_trusted_project_gets_gradle_env_and_an_executable_gradlew() {
        let project = project_with_plain_gradlew();
        let mut settings = crate::models::settings::AppSettings::default();
        settings
            .recent_projects
            .push(crate::models::settings::ProjectEntry {
                path: project.path().to_string_lossy().into_owned(),
                trusted: Some(true),
                ..Default::default()
            });

        trusted_gradle_env(&settings, project.path(), project.path()).unwrap();

        assert_eq!(gradlew_mode(project.path()) & 0o111, 0o111);
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

        let found = find_output_apk(&tmp, None, "release");
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

        let found = find_output_apk(&tmp, None, "release");
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

        let found = find_output_apk(&tmp, None, "release");
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

        let found = find_output_apk(&tmp, None, "release");
        assert!(found.is_err(), "unaligned APK must be excluded");
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Create `app/build/outputs/apk/<rel>` under `root` as an empty file.
    fn apk_at(root: &Path, rel: &str) -> PathBuf {
        let path = apk_outputs_dir(&root.join("app")).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn flavored_variant_matches_its_flavor_and_build_type_dirs() {
        let root = tempfile::tempdir().unwrap();
        let paid = apk_at(root.path(), "paid/debug/app-paid-debug.apk");
        apk_at(root.path(), "free/debug/app-free-debug.apk");

        assert_eq!(
            find_output_apk(root.path(), None, "paidDebug").unwrap(),
            paid
        );
    }

    /// With only a stale output of another flavor present, the old fallback
    /// passes returned it and Run installed the wrong app without a word.
    #[test]
    fn never_falls_back_to_another_variants_apk() {
        let root = tempfile::tempdir().unwrap();
        apk_at(root.path(), "free/debug/app-free-debug.apk");
        apk_at(root.path(), "release/app-release.apk");

        let err = find_output_apk(root.path(), None, "paidDebug").unwrap_err();
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

        assert_eq!(
            find_output_apk(root.path(), None, "demoDebug").unwrap(),
            apk
        );
        assert!(find_output_apk(root.path(), None, "demo").is_err());
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
    fn output_metadata_cannot_point_outside_the_build_outputs() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let foreign = outside.path().join("foreign.apk");
        std::fs::write(&foreign, b"").unwrap();
        let dir = apk_outputs_dir(&root.path().join("app")).join("debug");
        std::fs::create_dir_all(&dir).unwrap();
        let escape = format!(
            "{}{}",
            "../".repeat(dir.canonicalize().unwrap().components().count()),
            foreign.display()
        );
        for output_file in [foreign.display().to_string(), escape] {
            std::fs::write(
                dir.join("output-metadata.json"),
                serde_json::json!({
                    "variantName": "debug",
                    "elements": [{ "type": "SINGLE", "outputFile": output_file }],
                })
                .to_string(),
            )
            .unwrap();

            let err = find_output_apk(root.path(), None, "debug").unwrap_err();

            assert!(err.contains("No APK found"), "{output_file}: {err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn apk_outputs_linked_outside_the_project_are_not_used() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let foreign = outside.path().join("outputs/apk/debug/app-debug.apk");
        std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        std::fs::write(&foreign, b"").unwrap();
        std::fs::write(
            foreign.parent().unwrap().join("output-metadata.json"),
            r#"{"applicationId":"com.foreign","variantName":"debug","elements":[]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join("app/build")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outputs"),
            root.path().join("app/build/outputs"),
        )
        .unwrap();

        let err = find_output_apk(root.path(), None, "debug").unwrap_err();

        assert!(err.contains("outside the project"), "{err}");
        assert!(built_application_ids(root.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_build_dir_linked_inside_the_project_still_finds_its_apk() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("build-cache/app");
        let apk = real.join("outputs/apk/debug/app-debug.apk");
        std::fs::create_dir_all(apk.parent().unwrap()).unwrap();
        std::fs::write(&apk, b"").unwrap();
        std::fs::create_dir_all(root.path().join("app")).unwrap();
        std::os::unix::fs::symlink(&real, root.path().join("app/build")).unwrap();

        assert_eq!(
            find_output_apk(root.path(), None, "debug").unwrap(),
            apk_outputs_dir(&root.path().join("app")).join("debug/app-debug.apk")
        );
    }

    #[test]
    fn more_than_one_candidate_is_an_error_not_a_guess() {
        let root = tempfile::tempdir().unwrap();
        apk_at(root.path(), "debug/app-arm64-v8a-debug.apk");
        apk_at(root.path(), "debug/app-x86_64-debug.apk");

        let err = find_output_apk(root.path(), None, "debug").unwrap_err();
        assert!(err.contains("More than one APK"), "{err}");
    }

    #[test]
    fn empty_variant_accepts_only_a_single_apk() {
        let root = tempfile::tempdir().unwrap();
        let debug = apk_at(root.path(), "debug/app-debug.apk");
        assert_eq!(find_output_apk(root.path(), None, "").unwrap(), debug);

        apk_at(root.path(), "release/app-release.apk");
        assert!(find_output_apk(root.path(), None, "").is_err());
    }

    #[test]
    fn missing_outputs_dir_is_an_actionable_error() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("app")).unwrap();
        let err = find_output_apk(root.path(), None, "debug").unwrap_err();
        assert!(err.contains("Build the variant first"), "{err}");
    }

    const APPLICATION_PLUGIN: &str = "plugins {\n    id(\"com.android.application\")\n}\n";

    fn write_file(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Create `<module>/build/outputs/apk/<rel>` under `root` as an empty file.
    fn module_apk_at(root: &Path, module: &str, rel: &str) -> PathBuf {
        let path = apk_outputs_dir(&root.join(module)).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        path.canonicalize().unwrap()
    }

    #[test]
    fn the_apk_comes_from_an_application_module_not_named_app() {
        let root = tempfile::tempdir().unwrap();
        write_file(root.path(), "settings.gradle.kts", "include(\":mobile\")\n");
        write_file(root.path(), "mobile/build.gradle.kts", APPLICATION_PLUGIN);
        let apk = module_apk_at(root.path(), "mobile", "debug/mobile-debug.apk");

        assert_eq!(find_output_apk(root.path(), None, "debug").unwrap(), apk);
    }

    #[test]
    fn a_library_named_app_does_not_hide_the_application_module() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "settings.gradle",
            "include ':app', ':androidApp'\n",
        );
        write_file(
            root.path(),
            "app/build.gradle",
            "apply plugin: 'com.android.library'\n",
        );
        write_file(
            root.path(),
            "androidApp/build.gradle",
            "apply plugin: 'com.android.application'\n",
        );
        module_apk_at(root.path(), "app", "debug/app-debug.apk");
        let apk = module_apk_at(root.path(), "androidApp", "debug/androidApp-debug.apk");

        assert_eq!(find_output_apk(root.path(), None, "debug").unwrap(), apk);
    }

    #[test]
    fn several_application_modules_need_one_to_be_named() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "settings.gradle.kts",
            "include(\":mobile\", \":wear\")\n",
        );
        write_file(root.path(), "mobile/build.gradle.kts", APPLICATION_PLUGIN);
        write_file(root.path(), "wear/build.gradle.kts", APPLICATION_PLUGIN);
        module_apk_at(root.path(), "mobile", "debug/mobile-debug.apk");
        let wear = module_apk_at(root.path(), "wear", "debug/wear-debug.apk");

        let err = find_output_apk(root.path(), None, "debug").unwrap_err();
        assert!(err.contains(":mobile, :wear"), "{err}");
        assert_eq!(
            find_output_apk(root.path(), Some(":wear:assembleDebug"), "debug").unwrap(),
            wear
        );
    }

    #[test]
    fn built_application_ids_cover_every_application_module() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            root.path(),
            "settings.gradle.kts",
            "include(\":mobile\", \":wear\")\n",
        );
        write_file(root.path(), "mobile/build.gradle.kts", APPLICATION_PLUGIN);
        write_file(root.path(), "wear/build.gradle.kts", APPLICATION_PLUGIN);
        for (module, id) in [
            ("mobile", "com.example.phone"),
            ("wear", "com.example.watch"),
        ] {
            let apk = module_apk_at(root.path(), module, "debug/out.apk");
            std::fs::write(
                apk.parent().unwrap().join("output-metadata.json"),
                format!(r#"{{"applicationId":"{id}","variantName":"debug","elements":[]}}"#),
            )
            .unwrap();
        }

        assert_eq!(
            built_application_ids(root.path()),
            vec!["com.example.phone", "com.example.watch"]
        );
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
        let was_running = cancel_build(&state, &pm, BuildActor::App).await;
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
        let was_running = cancel_build(&state, &pm, BuildActor::App).await;
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

        let was_running = cancel_build(&state, &pm, BuildActor::App).await;
        let inner = state.inner.lock().await;

        assert!(was_running, "starting builds should be cancellable");
        assert!(!inner.starting, "starting flag must be cleared on cancel");
        assert!(matches!(inner.status, BuildStatus::Cancelled));
    }

    #[tokio::test]
    async fn cancel_build_does_not_change_status_when_idle() {
        let state = BuildState::new();
        let pm = ProcessManager::new();
        cancel_build(&state, &pm, BuildActor::App).await;
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

        let was_running = cancel_build(&state, &pm, BuildActor::App).await;
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

        cancel_build(&state, &pm, BuildActor::App).await;

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

        cancel_build(&state, &pm, BuildActor::App).await;

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
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings: Vec::new(),
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
                origin: None,
                cancelled_by: None,
                launch: None,
                mappings: Vec::new(),
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
                    origin: None,
                    cancelled_by: None,
                    launch: None,
                    mappings: Vec::new(),
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
                origin: None,
                cancelled_by: None,
                launch: None,
                mappings: Vec::new(),
            })
            .collect();

        let json = serde_json::to_string_pretty(&records).unwrap();
        let loaded: Vec<BuildRecord> = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.len(), 5);
        assert_eq!(loaded[0].task, "task_1");
        assert_eq!(loaded[4].task, "task_5");
    }

    // ── read_build_log_in tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn read_build_log_returns_the_saved_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut raw: VecDeque<String> = VecDeque::new();
        raw.push_back("> Task :app:compileDebugKotlin".into());
        raw.push_back("e: /src/Foo.kt:1:1: Unresolved reference: bar".into());
        save_build_log_to(7, &raw, dir.path());

        let lines = read_build_log_in(dir.path(), 7).await.unwrap();

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].kind, BuildLineKind::Error);
    }

    #[tokio::test]
    async fn read_build_log_of_a_build_without_output_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        save_build_log_to(8, &VecDeque::new(), dir.path());

        assert!(read_build_log_in(dir.path(), 8).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn read_build_log_removed_by_rotation_is_not_found() {
        let dir = tempfile::tempdir().unwrap();

        let err = read_build_log_in(dir.path(), 9).await.unwrap_err();

        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
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
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings: Vec::new(),
        }
    }

    fn persist(dir: &Path, task: &str) -> u32 {
        let lines = VecDeque::from([format!("output of {task}")]);
        persist_build_record_in(dir, record_named(task), &lines, 7, 100, vec![])
            .unwrap()
            .0
    }

    fn persist_succeeded(dir: &Path, task: &str) -> u32 {
        let record = BuildRecord {
            status: BuildStatus::Success(BuildResult {
                success: true,
                duration_ms: 1_000,
                error_count: 0,
                warning_count: 0,
            }),
            ..record_named(task)
        };
        persist_build_record_in(dir, record, &VecDeque::new(), 7, 100, vec![])
            .unwrap()
            .0
    }

    fn cold_launch(total_ms: u32) -> LaunchTiming {
        LaunchTiming {
            total_ms,
            wait_ms: Some(total_ms + 3),
            launch_state: Some(crate::models::build::LaunchState::Cold),
            measured_at: "2026-01-01T00:01:00Z".into(),
            serial: "emulator-5554".into(),
            avd_name: Some("Pixel_7".into()),
            model: None,
        }
    }

    fn launch_of(dir: &Path, id: u32) -> Option<LaunchTiming> {
        load_build_history_from(dir)
            .into_iter()
            .find(|r| r.id == id)
            .and_then(|r| r.launch)
    }

    #[test]
    fn launch_timing_goes_to_the_named_build_not_the_latest() {
        let dir = tempfile::tempdir().unwrap();
        let deployed = persist_succeeded(dir.path(), "assembleDebug");
        // Another client's build finishes between the build and the launch.
        let later = persist_succeeded(dir.path(), "assembleRelease");

        let history = attach_launch_timing_in(dir.path(), deployed, cold_launch(812)).unwrap();

        assert_eq!(launch_of(dir.path(), deployed), Some(cold_launch(812)));
        assert_eq!(launch_of(dir.path(), later), None);
        assert_eq!(
            history.iter().find(|r| r.id == deployed).unwrap().launch,
            Some(cold_launch(812))
        );
    }

    #[test]
    fn launch_timing_is_refused_for_a_build_that_did_not_succeed_or_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let failed = persist(dir.path(), "assembleDebug");

        let err = attach_launch_timing_in(dir.path(), failed, cold_launch(812)).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert_eq!(launch_of(dir.path(), failed), None);

        let err = attach_launch_timing_in(dir.path(), failed + 100, cold_launch(812)).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
    }

    #[tokio::test]
    async fn launch_timing_of_a_build_kept_only_in_memory_is_recorded_there() {
        let _history = PERSISTED_HISTORY.lock().await;
        let state = BuildState::new();
        // Far above any ID the shared test data directory holds.
        let id = u32::MAX - 7;
        state.inner.lock().await.history.push_back(BuildRecord {
            id,
            status: BuildStatus::Success(BuildResult {
                success: true,
                duration_ms: 1_000,
                error_count: 0,
                warning_count: 0,
            }),
            ..record_named("assembleDebug")
        });

        attach_launch_timing(&state, id, cold_launch(640))
            .await
            .unwrap();

        let bs = state.inner.lock().await;
        assert_eq!(bs.history.back().unwrap().launch, Some(cold_launch(640)));
    }

    #[test]
    fn history_saved_before_launch_times_were_kept_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(BUILD_HISTORY_FILE),
            r#"[{"id":4,"task":"assembleDebug","status":{"state":"success","success":true,
                "durationMs":1000,"errorCount":0,"warningCount":0},"errors":[],
                "startedAt":"2026-01-01T00:00:00Z","projectRoot":"/p",
                "origin":{"kind":"app"},"cancelledBy":null}]"#,
        )
        .unwrap();

        let history = load_build_history_from(dir.path());

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].launch, None);
        // And a launch time can be recorded on it.
        attach_launch_timing_in(dir.path(), 4, cold_launch(812)).unwrap();
        assert_eq!(launch_of(dir.path(), 4), Some(cold_launch(812)));
    }

    // ── R8 mapping snapshots ──────────────────────────────────────────────────

    /// A project whose `app` module just wrote `mapping/<variant>/mapping.txt`.
    fn project_with_mapping(variant: &str, text: &str) -> tempfile::TempDir {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        std::fs::write(root.join("settings.gradle.kts"), "include(\":app\")\n").unwrap();
        let dir = root.join("app/build/outputs/mapping").join(variant);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            root.join("app/build.gradle.kts"),
            "plugins { id(\"com.android.application\") }\n",
        )
        .unwrap();
        std::fs::write(dir.join("mapping.txt"), text).unwrap();
        project
    }

    fn mapping_source(project: &Path) -> MappingSource {
        MappingSource {
            gradle_root: project.to_path_buf(),
            build_started: std::time::SystemTime::now() - std::time::Duration::from_secs(60),
        }
    }

    /// What the finalizer does for a successful build of `project`.
    fn persist_with_mapping(dir: &Path, project: &Path) -> BuildRecord {
        let prepared = mapping_snapshots::prepare_snapshots(dir, &mapping_source(project));
        let record = record_named("assembleRelease");
        let (id, history) =
            persist_build_record_in(dir, record, &VecDeque::new(), 7, 100, prepared.mappings)
                .unwrap();
        history.into_iter().find(|r| r.id == id).unwrap()
    }

    fn snapshot_saved(dir: &Path, record: &BuildRecord) -> bool {
        let sha = &record.mappings[0].sha256;
        mapping_snapshots::snapshot_path(dir, sha)
            .unwrap()
            .is_file()
    }

    #[test]
    fn a_build_record_names_the_mapping_it_saved() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_with_mapping("release", "# pg_map_id: 6b1c2f0\na.B -> a:\n");

        let record = persist_with_mapping(dir.path(), project.path());

        assert_eq!(record.mappings.len(), 1);
        assert_eq!(record.mappings[0].module, ":app");
        assert_eq!(record.mappings[0].variant, "release");
        assert_eq!(record.mappings[0].pg_map_id.as_deref(), Some("6b1c2f0"));
        assert!(snapshot_saved(dir.path(), &record));
        let loaded = load_build_history_from(dir.path());
        assert_eq!(loaded.back().unwrap().mappings, record.mappings);
    }

    #[test]
    fn retention_removes_only_mappings_no_kept_build_names() {
        let dir = tempfile::tempdir().unwrap();
        let first = persist_with_mapping(
            dir.path(),
            project_with_mapping("release", "one -> a:\n").path(),
        );
        let kept = persist_with_mapping(
            dir.path(),
            project_with_mapping("release", "two -> a:\n").path(),
        );
        assert!(snapshot_saved(dir.path(), &first));

        // Builds without mappings push the first one out of the history.
        for _ in 0..MAX_HISTORY - 1 {
            persist(dir.path(), "assembleDebug");
        }

        assert!(!snapshot_saved(dir.path(), &first));
        assert!(snapshot_saved(dir.path(), &kept));
    }

    #[test]
    fn a_mapping_another_process_recorded_is_kept_by_this_process_build() {
        let dir = tempfile::tempdir().unwrap();
        // Another process's build, which this process's history has never seen.
        let theirs = persist_with_mapping(
            dir.path(),
            project_with_mapping("release", "theirs -> a:\n").path(),
        );
        let mine = persist_with_mapping(
            dir.path(),
            project_with_mapping("release", "mine -> a:\n").path(),
        );

        assert!(snapshot_saved(dir.path(), &theirs));
        assert!(snapshot_saved(dir.path(), &mine));
        let merged = merge_history(&VecDeque::from([mine]), load_build_history_from(dir.path()));
        let named: Vec<usize> = merged.iter().map(|r| r.mappings.len()).collect();
        assert_eq!(named, vec![1, 1]);
    }

    #[test]
    fn clearing_the_history_removes_its_mappings() {
        let dir = tempfile::tempdir().unwrap();
        let record = persist_with_mapping(
            dir.path(),
            project_with_mapping("release", "gone -> a:\n").path(),
        );
        clear_history_in(dir.path()).unwrap();
        assert!(!snapshot_saved(dir.path(), &record));
    }

    #[test]
    fn a_mapping_that_cannot_be_copied_leaves_the_build_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_with_mapping("release", "x -> a:\n");
        // The snapshot folder cannot be created: a file has its name.
        std::fs::write(mapping_snapshots::mappings_dir(dir.path()), "").unwrap();

        let record = persist_with_mapping(dir.path(), project.path());

        assert!(record.mappings.is_empty());
        assert_eq!(load_build_history_from(dir.path()).len(), 1);
    }

    #[test]
    fn a_mapping_that_cannot_be_published_leaves_the_build_recorded() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let project = project_with_mapping("release", "x -> a:\n");
        let prepared =
            mapping_snapshots::prepare_snapshots(dir.path(), &mapping_source(project.path()));
        assert_eq!(prepared.mappings.len(), 1);
        let folder = mapping_snapshots::mappings_dir(dir.path());
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();

        let persisted = persist_build_record_in(
            dir.path(),
            record_named("assembleRelease"),
            &VecDeque::new(),
            7,
            100,
            prepared.mappings,
        );

        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_, history) = persisted.unwrap();
        assert!(history.back().unwrap().mappings.is_empty());
    }

    #[test]
    fn history_saved_before_mappings_were_kept_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(BUILD_HISTORY_FILE),
            r#"[{"id":4,"task":"assembleRelease","status":{"state":"cancelled"},"errors":[],
                "startedAt":"2026-01-01T00:00:00Z","projectRoot":"/p"}]"#,
        )
        .unwrap();

        let history = load_build_history_from(dir.path());
        assert_eq!(history.len(), 1);
        assert!(history[0].mappings.is_empty());

        // Its first save with mappings kept prunes nothing it should not.
        let project = project_with_mapping("release", "new -> a:\n");
        let record = persist_with_mapping(dir.path(), project.path());
        assert!(snapshot_saved(dir.path(), &record));
        assert_eq!(load_build_history_from(dir.path()).len(), 2);
    }

    #[tokio::test]
    async fn only_a_successful_build_saves_its_mappings() {
        let _history = PERSISTED_HISTORY.lock().await;
        let project = project_with_mapping("release", "# pg_map_id: 1a2b3c4\nx -> a:\n");
        for (success, cancelled, saved) in [(true, false, 1), (false, false, 0), (false, true, 0)] {
            let bs = BuildState::new();
            let log = start_run(&bs, 7).await;
            let event = finalize_completed_build(
                &bs,
                BuildFinalization {
                    success,
                    cancelled,
                    mappings: Some(mapping_source(project.path())),
                    ..finalization(7, log, "assembleRelease")
                },
            )
            .await;

            let inner = bs.inner.lock().await;
            let record = inner.history.iter().find(|r| r.id == event.record_id);
            let mappings = record.map(|r| r.mappings.len());
            assert_eq!(
                mappings,
                Some(saved),
                "success {success}, cancelled {cancelled}"
            );
        }
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
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings: Vec::new(),
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
            origin: None,
            cancelled_by: None,
            mappings: None,
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
                origin: None,
                cancelled_by: None,
                mappings: None,
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
                origin: None,
                cancelled_by: None,
                mappings: None,
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
        assert_eq!(
            inner.history.back().map(|r| r.id),
            Some(event.record_id),
            "build:complete must name the record the run was saved as"
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
                origin: None,
                cancelled_by: None,
                mappings: None,
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
                origin: None,
                cancelled_by: None,
                mappings: None,
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
        cancel_build(&bs, &pm, BuildActor::App).await;

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

    // ── Running a build ──────────────────────────────────────────────────────

    fn request(dir: &Path, gradlew: &Path, task: &str, origin: BuildActor) -> BuildRequest {
        BuildRequest {
            task: task.to_string(),
            extra_args: vec![],
            gradle_root: dir.to_path_buf(),
            gradlew: gradlew.to_path_buf(),
            env: vec![],
            project_root: Some(dir.to_string_lossy().into_owned()),
            origin,
        }
    }

    fn agent(name: &str) -> BuildActor {
        BuildActor::Agent(AgentActor {
            session_id: Some(3),
            client_name: Some(name.to_string()),
            standalone: false,
        })
    }

    /// Regression: the build reserved the slot and then returned early via `?`
    /// when the spawn failed, leaving `starting = true` forever. Every later
    /// build from either front door was refused until the app restarted.
    #[tokio::test]
    async fn start_build_releases_the_slot_when_the_spawn_fails() {
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let missing_gradlew = dir.path().join("gradlew-does-not-exist");

        let result = start_build(
            &build_state,
            &pm,
            None,
            request(
                dir.path(),
                &missing_gradlew,
                "assembleDebug",
                BuildActor::App,
            ),
        )
        .await;

        assert!(
            matches!(result, Err(StartBuildError::Spawn(_))),
            "a missing gradlew must fail the run"
        );
        {
            let bs = build_state.inner.lock().await;
            assert!(
                !bs.starting,
                "the slot must not stay reserved after a failed spawn"
            );
            assert!(bs.current_build.is_none());
        }
        assert!(build_state.wait_for_runs(std::time::Duration::ZERO).await);

        // The decisive assertion: a subsequent build can still start.
        try_reserve_build_slot(&build_state, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .expect("slot must be free after a failed spawn");
    }

    /// A gradlew that waits for `release` to exist, then succeeds.
    fn gradlew_waiting_for(dir: &Path, release: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let gradlew = dir.join("gradlew");
        std::fs::write(
            &gradlew,
            format!(
                "#!/bin/sh\necho started\nwhile [ ! -e '{}' ]; do sleep 0.05; done\necho 'BUILD SUCCESSFUL in 1s'\n",
                release.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&gradlew, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Run it once, released, so the first run of the new file is not
        // what the tests below wait for.
        std::fs::write(release, "").unwrap();
        crate::utils::process::test_support::run_once(&gradlew);
        std::fs::remove_file(release).unwrap();
        gradlew
    }

    async fn history_record(bs: &BuildState, task: &str) -> Option<BuildRecord> {
        bs.inner
            .lock()
            .await
            .history
            .iter()
            .rev()
            .find(|r| r.task == task)
            .cloned()
    }

    /// Regression: the caller awaited Gradle's exit inside the build call, so
    /// when that future was dropped (an MCP request abandoned by its client)
    /// nothing recorded the build or released the slot, and every later build
    /// was refused until the app restarted.
    #[tokio::test]
    async fn a_build_whose_caller_stops_waiting_still_finishes_and_frees_the_slot() {
        let _history = PERSISTED_HISTORY.lock().await;
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("release");
        let gradlew = gradlew_waiting_for(dir.path(), &release);
        let task = format!("abandoned{}", std::process::id());

        let mut handle = start_build(
            &build_state,
            &pm,
            None,
            request(dir.path(), &gradlew, &task, agent("gone")),
        )
        .await
        .unwrap();
        // The caller waits a moment, then gives up and goes away.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(300), handle.wait()).await;
        drop(handle);
        assert!(build_state.inner.lock().await.current_build.is_some());

        std::fs::write(&release, "").unwrap();
        assert!(
            build_state
                .wait_for_runs(std::time::Duration::from_secs(15))
                .await,
            "the abandoned build never finished"
        );
        let record = history_record(&build_state, &task)
            .await
            .expect("the abandoned build is recorded");
        assert!(matches!(record.status, BuildStatus::Success(_)));
        assert_eq!(record.origin, Some(agent("gone")));
        try_reserve_build_slot(&build_state, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .expect("the slot must be free once the abandoned build ends");
    }

    #[tokio::test]
    async fn a_cancelled_build_records_who_cancelled_it() {
        let _history = PERSISTED_HISTORY.lock().await;
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let gradlew = gradlew_waiting_for(dir.path(), &dir.path().join("never"));
        let task = format!("cancelledByAgent{}", std::process::id());

        let mut handle = start_build(
            &build_state,
            &pm,
            None,
            request(dir.path(), &gradlew, &task, BuildActor::App),
        )
        .await
        .unwrap();
        assert!(cancel_build(&build_state, &pm, agent("Claude Code")).await);
        let outcome = handle.wait().await;

        assert!(outcome.cancelled && !outcome.success);
        assert_eq!(outcome.cancelled_by, Some(agent("Claude Code")));
        let record = history_record(&build_state, &task).await.unwrap();
        assert!(matches!(record.status, BuildStatus::Cancelled));
        assert_eq!(record.origin, Some(BuildActor::App));
        assert_eq!(record.cancelled_by, Some(agent("Claude Code")));
        let bs = build_state.inner.lock().await;
        assert_eq!(bs.status_cancelled_by, Some(agent("Claude Code")));
        assert_eq!(bs.status_origin, Some(BuildActor::App));
    }

    #[tokio::test]
    async fn cancelling_a_run_leaves_a_later_build_alone() {
        let _history = PERSISTED_HISTORY.lock().await;
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let (first_dir, second_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let release = first_dir.path().join("release");
        let first_gradlew = gradlew_waiting_for(first_dir.path(), &release);
        let second_gradlew =
            gradlew_waiting_for(second_dir.path(), &second_dir.path().join("never"));
        let task = format!("cancelRun{}", std::process::id());

        let mut first = start_build(
            &build_state,
            &pm,
            None,
            request(first_dir.path(), &first_gradlew, "first", agent("A")),
        )
        .await
        .unwrap();
        std::fs::write(&release, "").unwrap();
        assert!(first.wait().await.success);
        let mut second = start_build(
            &build_state,
            &pm,
            None,
            request(second_dir.path(), &second_gradlew, &task, BuildActor::App),
        )
        .await
        .unwrap();

        // The first caller cancels late: the build now running is not its own.
        assert!(!cancel_run(&build_state, &pm, first.run_id, agent("A")).await);
        assert!(build_state.inner.lock().await.current_build.is_some());

        assert!(cancel_run(&build_state, &pm, second.run_id, agent("B")).await);
        let outcome = second.wait().await;
        assert!(outcome.cancelled);
        assert_eq!(outcome.cancelled_by, Some(agent("B")));
        let record = history_record(&build_state, &task).await.unwrap();
        assert_eq!(record.cancelled_by, Some(agent("B")));
    }

    #[test]
    fn a_run_reports_the_task_it_is_running() {
        let collector = RunCollector::new(false);
        let current = collector.current_task.subscribe();
        let log = BuildLog::default();
        collector.on_line(&log, "Starting a Gradle Daemon".into());
        assert_eq!(*current.borrow(), None);
        collector.on_line(&log, "> Task :app:compileDebugKotlin".into());
        collector.on_line(&log, "w: something".into());
        assert_eq!(current.borrow().as_deref(), Some(":app:compileDebugKotlin"));

        collector.on_line(&log, format!("> Task :{}", "x".repeat(1_000)));
        assert_eq!(
            current.borrow().as_ref().unwrap().len(),
            MAX_CURRENT_TASK_BYTES
        );
    }

    /// Regression: a build cancelled while Gradle was still being spawned was
    /// killed but never recorded.
    #[tokio::test]
    async fn a_build_cancelled_while_starting_is_recorded_as_cancelled() {
        let _history = PERSISTED_HISTORY.lock().await;
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let gradlew = gradlew_waiting_for(dir.path(), &dir.path().join("never"));
        let task = format!("cancelledStarting{}", std::process::id());

        // Hold the process table so the spawn waits.
        let table = pm.0.lock().await;
        let start = {
            let (bs, pm) = (build_state.clone(), pm.clone());
            let request = request(dir.path(), &gradlew, &task, BuildActor::App);
            tokio::spawn(async move { start_build(&bs, &pm, None, request).await })
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !build_state.inner.lock().await.starting {
            assert!(std::time::Instant::now() < deadline, "never started");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(cancel_build(&build_state, &pm, agent("early")).await);
        drop(table);

        let mut handle = start.await.unwrap().unwrap();
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(20), handle.wait())
            .await
            .expect("the build cancelled while starting kept running");
        assert!(outcome.cancelled);
        assert_eq!(outcome.cancelled_by, Some(agent("early")));
        let record = history_record(&build_state, &task).await.unwrap();
        assert!(matches!(record.status, BuildStatus::Cancelled));
        assert!(matches!(
            build_state.inner.lock().await.status,
            BuildStatus::Cancelled
        ));
    }

    #[tokio::test]
    async fn a_timed_out_build_is_recorded_as_failed_with_the_reason() {
        let _history = PERSISTED_HISTORY.lock().await;
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let gradlew = gradlew_waiting_for(dir.path(), &dir.path().join("never"));
        let task = format!("timedOut{}", std::process::id());

        let mut handle = start_build(
            &build_state,
            &pm,
            None,
            request(dir.path(), &gradlew, &task, agent("slow")),
        )
        .await
        .unwrap();
        // Another run's timeout does not stop this one.
        assert!(!time_out_build(&build_state, &pm, Some(handle.run_id + 1000), 5).await);
        assert!(time_out_build(&build_state, &pm, Some(handle.run_id), 5).await);
        let outcome = handle.wait().await;

        assert!(!outcome.cancelled && !outcome.success);
        assert_eq!(outcome.timed_out_after_sec, Some(5));
        assert_eq!(outcome.cancelled_by, None);
        let record = history_record(&build_state, &task).await.unwrap();
        assert!(matches!(record.status, BuildStatus::Failed(_)));
        assert!(record
            .errors
            .iter()
            .any(|e| e.message == "Build timed out after 5s and was cancelled"));
    }

    /// Two processes (here two build states, each with its own slot) cannot
    /// build one project at once; the second is told another process builds.
    #[tokio::test]
    async fn a_project_being_built_elsewhere_is_busy_until_that_build_ends() {
        let _history = PERSISTED_HISTORY.lock().await;
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("release");
        let gradlew = gradlew_waiting_for(dir.path(), &release);
        let (here, elsewhere) = (BuildState::new(), BuildState::new());

        let mut first = start_build(
            &elsewhere,
            &pm,
            None,
            request(dir.path(), &gradlew, "assembleDebug", BuildActor::App),
        )
        .await
        .unwrap();
        let busy = start_build(
            &here,
            &pm,
            None,
            request(dir.path(), &gradlew, "assembleDebug", agent("x")),
        )
        .await
        .err()
        .unwrap();
        let StartBuildError::BusyElsewhere(message) = busy else {
            panic!("{busy:?}")
        };
        assert!(
            message.starts_with(BUILD_ALREADY_RUNNING)
                && message.contains("another Keynobi process")
                && message.contains(&format!("pid {}", std::process::id())),
            "{message}"
        );
        assert!(
            matches!(here.inner.lock().await.status, BuildStatus::Idle),
            "a refused start leaves the slot alone"
        );

        std::fs::write(&release, "").unwrap();
        assert!(first.wait().await.success);
        let mut second = start_build(
            &here,
            &pm,
            None,
            request(dir.path(), &gradlew, "assembleDebug", agent("x")),
        )
        .await
        .expect("the lock is free once the other build ends");
        assert!(second.wait().await.success);
    }

    /// Cancelling frees the slot at once; a new build of the same project in
    /// this process must not be refused by the lock the old Gradle still holds
    /// while it shuts down.
    #[tokio::test]
    async fn a_new_build_can_start_while_a_cancelled_one_shuts_down() {
        let _history = PERSISTED_HISTORY.lock().await;
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("release");
        let gradlew = gradlew_waiting_for(dir.path(), &release);

        let _old = start_build(
            &build_state,
            &pm,
            None,
            request(dir.path(), &gradlew, "assembleDebug", BuildActor::App),
        )
        .await
        .unwrap();
        cancel_build(&build_state, &pm, BuildActor::App).await;
        let mut new = start_build(
            &build_state,
            &pm,
            None,
            request(dir.path(), &gradlew, "assembleRelease", BuildActor::App),
        )
        .await
        .expect("the old run's lock is shared, not a refusal");
        std::fs::write(&release, "").unwrap();
        assert!(new.wait().await.success);
    }

    #[test]
    fn output_for_the_app_is_parsed_and_capped() {
        let collector = RunCollector::new(true);
        let log = BuildLog::default();
        collector.on_line(&log, "> Task :app:compileDebugKotlin".into());
        collector.on_line(&log, "e: /src/A.kt:1:2: boom".into());
        let lines = collector.take_pending();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].kind, BuildLineKind::TaskStart);
        assert_eq!(lines[1].kind, BuildLineKind::Error);
        assert!(collector.take_pending().is_empty(), "taken once");

        for n in 0..MAX_PENDING_BUILD_LINES + 5 {
            collector.on_line(&log, format!("line {n}"));
        }
        let lines = collector.take_pending();
        assert_eq!(lines.len(), MAX_PENDING_BUILD_LINES);
        assert_eq!(lines[0].content, "line 5", "the oldest are dropped");

        let headless = RunCollector::new(false);
        headless.on_line(&log, "line".into());
        assert!(
            headless.take_pending().is_empty(),
            "nothing kept without an app"
        );
    }

    #[tokio::test]
    async fn connected_tests_keep_the_device_busy_until_the_run_ends() {
        use crate::services::ui_automator_lock::test_support::instrumentation_active_on;

        let serial = "build-runner-connected";
        let build_state = BuildState::new();
        let pm = crate::services::process_manager::ProcessManager::new();
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("release");
        let gradlew = gradlew_waiting_for(dir.path(), &release);

        let mut run_request = request(
            dir.path(),
            &gradlew,
            "connectedDebugAndroidTest",
            BuildActor::App,
        );
        run_request.env = vec![("ANDROID_SERIAL".into(), serial.into())];
        let mut handle = start_build(&build_state, &pm, None, run_request)
            .await
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !instrumentation_active_on(serial) && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let seen_during_run = instrumentation_active_on(serial);
        std::fs::write(&release, "").unwrap();

        assert!(handle.wait().await.success);
        assert!(
            seen_during_run,
            "the device was not marked busy during the run"
        );
        assert!(
            !instrumentation_active_on(serial),
            "still busy after the run"
        );
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
