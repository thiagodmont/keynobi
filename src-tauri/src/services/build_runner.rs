use crate::models::build::{BuildError, BuildErrorSeverity, BuildRecord, BuildResult, BuildStatus};
use crate::services::build_parser;
use crate::services::process_manager::{self, ProcessId, ProcessManager};
use crate::services::settings_manager::data_dir;
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
pub fn save_build_history(history: &VecDeque<BuildRecord>) {
    let path = data_dir().join(BUILD_HISTORY_FILE);
    let recent: Vec<&BuildRecord> = history.iter().rev().take(MAX_PERSISTED_HISTORY).collect();
    if let Ok(json) = serde_json::to_string_pretty(&recent) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Load build history from disk. Returns empty VecDeque if file is missing or corrupt.
///
/// The file is written newest-first (see `save_build_history`), so we reverse
/// the loaded entries to restore oldest-first order — matching the invariant that
/// `push_back` adds the newest record and `pop_front` evicts the oldest.
pub fn load_build_history() -> VecDeque<BuildRecord> {
    let path = data_dir().join(BUILD_HISTORY_FILE);
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
    /// Current build status.
    pub status: BuildStatus,
    /// Ring-buffer of past build records.
    pub history: VecDeque<BuildRecord>,
    /// Errors accumulated from the current (or last) build.
    pub current_errors: Vec<BuildError>,
    /// Counter for assigning unique build IDs.
    next_id: u32,
}

impl Default for BuildStateInner {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildStateInner {
    pub fn new() -> Self {
        let history = load_build_history();
        let next_id = history.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        Self {
            current_build: None,
            starting: false,
            status: BuildStatus::Idle,
            history,
            current_errors: vec![],
            next_id,
        }
    }
}

/// Synchronously-accessible ring-buffer of raw build output lines.
///
/// Uses a `std::sync::Mutex` (not tokio) so the `on_line` process callback can
/// push lines without `await`. Capped at `MAX_BUILD_LOG` entries.
pub type BuildLog = Arc<std::sync::Mutex<VecDeque<String>>>;

pub struct BuildState {
    pub inner: Arc<Mutex<BuildStateInner>>,
    /// Raw build output log — accessible from both sync callbacks and async MCP tools.
    pub build_log: BuildLog,
    /// Set synchronously in the same task tick immediately after `spawn` returns (no `.await`
    /// before this), so `cancel_build` can always resolve the `ProcessId` even if it runs
    /// before `inner.current_build` is updated (otherwise cancel saw `None` and did not kill Gradle).
    pub active_process_id: Arc<StdMutex<Option<ProcessId>>>,
}

impl BuildState {
    pub fn new() -> Self {
        BuildState {
            inner: Arc::new(Mutex::new(BuildStateInner::new())),
            build_log: Arc::new(std::sync::Mutex::new(VecDeque::new())),
            active_process_id: Arc::new(StdMutex::new(None)),
        }
    }

    pub fn take_active_process_id(&self) -> Option<ProcessId> {
        match self.active_process_id.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
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

pub fn clear_build_log(build_log: &BuildLog) {
    if let Ok(mut log) = build_log.lock() {
        log.clear();
    }
}

/// Core of save_build_log — accepts a target directory for testability.
pub fn save_build_log_to(id: u32, raw_lines: &VecDeque<String>, build_log_dir: &Path) {
    if std::fs::create_dir_all(build_log_dir).is_err() {
        return;
    }
    let path = build_log_dir.join(format!("build-{id}.jsonl"));
    let tmp = build_log_dir.join(format!("build-{id}.jsonl.tmp"));

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

/// Resolve the APK output path for a given variant.
///
/// Standard AGP layout:
///   `{gradle_root}/app/build/outputs/apk/{buildType}/app-{buildType}.apk`
///   or with flavor:
///   `{gradle_root}/app/build/outputs/apk/{flavor}/{buildType}/app-{flavor}-{buildType}.apk`
///
/// Priority (highest first):
///   1. Signed APK in a directory that matches the variant name.
///   2. Unsigned APK in a directory that matches the variant name.
///   3. Signed APK anywhere under the outputs/apk tree.
///   4. Unsigned APK anywhere (last resort, excludes unaligned only).
///
/// `adb install` works fine with unsigned APKs for development builds.
/// Only `-unaligned.apk` files are excluded (they are not zip-aligned and
/// cannot be installed).
pub fn find_output_apk(gradle_root: &Path, variant_name: &str) -> Option<PathBuf> {
    let base = gradle_root
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk");
    if !base.is_dir() {
        return None;
    }
    let all_files = walk_dir_for_apk(&base, 4);

    // Only exclude files that are genuinely not installable.
    let is_usable = |name: &str| -> bool { !name.ends_with("-unaligned.apk") };
    let is_signed = |name: &str| -> bool { !name.contains("-unsigned") };

    let variant_lc = variant_name.to_lowercase();
    let parent_matches = |path: &Path| -> bool {
        if variant_name.is_empty() {
            return true;
        }
        // Check both the immediate parent dir and the grandparent dir so both
        // `apk/release/app-release.apk` and `apk/flavor/release/app-release.apk` match.
        for ancestor in path.ancestors().skip(1).take(2) {
            if ancestor
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase())
                .as_deref()
                == Some(variant_lc.as_str())
            {
                return true;
            }
        }
        false
    };

    let apks: Vec<&PathBuf> = all_files
        .iter()
        .filter(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            p.extension().and_then(|e| e.to_str()) == Some("apk") && is_usable(&name)
        })
        .collect();

    // Pass 1 — signed + variant dir match.
    for p in &apks {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if is_signed(&name) && parent_matches(p) {
            return Some((*p).clone());
        }
    }
    // Pass 2 — unsigned + variant dir match (e.g. app-release-unsigned.apk).
    for p in &apks {
        if parent_matches(p) {
            return Some((*p).clone());
        }
    }
    // Pass 3 — signed, any location.
    for p in &apks {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if is_signed(&name) {
            return Some((*p).clone());
        }
    }
    // Pass 4 — any usable APK (unsigned, any location).
    apks.into_iter().next().cloned()
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
    pub success: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub task: String,
}

pub struct BuildFinalization {
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
        finalization.task.clone(),
        finalization.started_at,
        result,
        finalization.errors,
        finalization.project_root,
    )
    .await;

    BuildCompleteEvent {
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

/// Clear all build history from memory and disk.
/// Disk persistence is best-effort; failures are silently dropped.
/// The in-memory clear (including ID counter reset) always succeeds.
pub async fn clear_history(build_state: &BuildState) {
    let mut bs = build_state.inner.lock().await;
    bs.history.clear();
    bs.next_id = 1;
    save_build_history(&bs.history);
}

/// Record the completed build result and push it to history.
pub async fn record_build_result(
    build_state: &BuildState,
    task: String,
    started_at: String,
    result: BuildResult,
    errors: Vec<BuildError>,
    project_root: Option<String>,
) {
    // Snapshot the raw build log before taking the inner lock so we don't
    // hold two locks simultaneously.
    let raw_lines: VecDeque<String> = build_state
        .build_log
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();

    let _ = build_state.take_active_process_id();

    let (record_id, history_snapshot) = {
        let mut bs = build_state.inner.lock().await;
        bs.status = if result.success {
            BuildStatus::Success(result.clone())
        } else {
            BuildStatus::Failed(result.clone())
        };
        bs.starting = false;
        bs.current_errors = errors.clone();
        bs.current_build = None;

        let record = BuildRecord {
            id: bs.next_id,
            task,
            status: bs.status.clone(),
            errors,
            started_at,
            project_root,
        };
        let record_id = bs.next_id;
        bs.next_id += 1;

        bs.history.push_back(record);
        while bs.history.len() > MAX_HISTORY {
            bs.history.pop_front();
        }
        let history_snapshot = bs.history.clone();
        (record_id, history_snapshot)
        // Lock dropped here — all disk I/O happens below, off the critical
        // section. save_build_history() used to run while holding it, blocking
        // every other build-state reader for the duration of a file write.
    };

    // Best-effort disk I/O, moved off the async runtime so a slow or large
    // write cannot stall a tokio worker.
    let history_for_io = history_snapshot.clone();
    let persisted = tokio::task::spawn_blocking(move || {
        save_build_history(&history_for_io);
        save_build_log(record_id, &raw_lines);
        let (settings, _) = crate::services::settings_manager::load_settings();
        let build_log_dir = data_dir().join("build-logs");
        rotate_build_logs(
            &build_log_dir,
            settings.build.build_log_retention_days,
            settings.build.build_log_max_folder_mb,
            &history_snapshot,
        );
    })
    .await;

    // A panic here means build history stopped persisting. It is best-effort by
    // design, but failing completely silently leaves the user with a history
    // panel that quietly stops updating and no way to find out why.
    if let Err(e) = persisted {
        tracing::warn!("Failed to persist build artifacts for build {record_id}: {e}");
    }
}

/// Build environment variables for a Gradle process, and ensure `gradlew` is executable.
pub fn build_env_vars(
    settings: &crate::models::settings::AppSettings,
    gradle_root: &Path,
) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(java_home) = settings.java.home.as_deref() {
        env.push(("JAVA_HOME".into(), java_home.into()));
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

    let build_log = build_state.build_log.clone();
    clear_build_log(&build_log);

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
        assert!(found.is_none(), "unaligned APK must be excluded");
        std::fs::remove_dir_all(&tmp).ok();
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
        clear_history(&state).await;
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

    #[tokio::test]
    async fn build_slot_is_released_after_finalization() {
        let bs = BuildState::new();
        try_reserve_build_slot(&bs, "assembleDebug", "2026-01-01T00:00:00Z")
            .await
            .unwrap();

        emit_build_complete(
            &bs,
            None,
            BuildFinalization {
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
        let bs = BuildState::new();
        let event = emit_build_complete(
            &bs,
            None,
            BuildFinalization {
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
        let bs = BuildState::new();
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
        let bs = BuildState::new();
        let event = emit_build_complete(
            &bs,
            None,
            BuildFinalization {
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
