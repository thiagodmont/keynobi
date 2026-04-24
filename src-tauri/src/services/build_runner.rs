use crate::models::build::{BuildError, BuildRecord, BuildResult, BuildStatus};
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

pub struct BuildStateInner {
    /// Process ID of the currently running Gradle process, if any.
    pub current_build: Option<ProcessId>,
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

/// Cancel the currently running build. Returns `true` if a build was running, `false` otherwise.
pub async fn cancel_build(build_state: &BuildState, process_manager: &ProcessManager) -> bool {
    let id = {
        let from_sync = build_state.active_process_id.lock().unwrap().take();
        if let Some(id) = from_sync {
            let mut bs = build_state.inner.lock().await;
            if bs.current_build == Some(id) {
                bs.current_build = None;
            }
            bs.status = BuildStatus::Cancelled;
            Some(id)
        } else {
            let mut bs = build_state.inner.lock().await;
            let pid = bs.current_build.take();
            if pid.is_some() {
                bs.status = BuildStatus::Cancelled;
            }
            pid
        }
    };
    if let Some(id) = id {
        process_manager::cancel(&process_manager.0, id).await;
        true
    } else {
        false
    }
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

    let _ = build_state.active_process_id.lock().unwrap().take();

    let (record_id, history_snapshot) = {
        let mut bs = build_state.inner.lock().await;
        bs.status = if result.success {
            BuildStatus::Success(result.clone())
        } else {
            BuildStatus::Failed(result.clone())
        };
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
        save_build_history(&bs.history);
        let history_snapshot = bs.history.clone();
        (record_id, history_snapshot)
    };

    // Best-effort disk I/O — outside the lock.
    save_build_log(record_id, &raw_lines);
    let (settings, _) = crate::services::settings_manager::load_settings();
    let build_log_dir = data_dir().join("build-logs");
    rotate_build_logs(
        &build_log_dir,
        settings.build.build_log_retention_days,
        settings.build.build_log_max_folder_mb,
        &history_snapshot,
    );
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
    build_state: &BuildState,
    process_manager: &crate::services::process_manager::ProcessManager,
) -> Result<GradleTaskResult, String> {
    use crate::models::build::{BuildError, BuildErrorSeverity, BuildLineKind, BuildResult};
    use crate::services::process_manager::{self as pm, SpawnOptions};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    let started_at = chrono::Utc::now().to_rfc3339();

    {
        let mut bs = build_state.inner.lock().await;
        bs.status = crate::models::build::BuildStatus::Running {
            task: task.to_owned(),
            started_at: started_at.clone(),
        };
        bs.current_errors.clear();
    }

    let build_log = build_state.build_log.clone();
    clear_build_log(&build_log);

    let mut args = vec![task, "--console=plain"];
    args.extend_from_slice(extra_args);

    let errors_buf = Arc::new(std::sync::Mutex::new(Vec::<BuildError>::new()));
    let success_flag = Arc::new(AtomicBool::new(false));
    let duration_buf = Arc::new(AtomicU64::new(0));
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
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
                            e.push(BuildError {
                                message: line.content.clone(),
                                file: line.file.clone(),
                                line: line.line,
                                col: line.col,
                                severity: if line.kind == BuildLineKind::Error {
                                    BuildErrorSeverity::Error
                                } else {
                                    BuildErrorSeverity::Warning
                                },
                            });
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
            on_exit: Box::new(move |_pid, _code| {
                if let Ok(mut g) = done_tx.lock() {
                    if let Some(tx) = g.take() {
                        let _ = tx.send(());
                    }
                }
            }),
        },
    )
    .await
    .map_err(|e| format!("Failed to spawn Gradle: {e}"))?;

    *build_state.active_process_id.lock().unwrap() = Some(pid);
    {
        let mut bs = build_state.inner.lock().await;
        if matches!(bs.status, BuildStatus::Cancelled) {
            return Ok(GradleTaskResult {
                success: false,
                timed_out: false,
                duration_ms: 0,
                errors: Vec::new(),
            });
        }
        bs.current_build = Some(pid);
    }

    let timed_out = tokio::time::timeout(std::time::Duration::from_secs(timeout_sec), done_rx)
        .await
        .is_err();

    if timed_out {
        cancel_build(build_state, process_manager).await;
        return Ok(GradleTaskResult {
            success: false,
            timed_out: true,
            duration_ms: 0,
            errors: Vec::new(),
        });
    }

    let success = success_flag.load(Ordering::Acquire);
    let errors = errors_buf.lock().map(|g| g.clone()).unwrap_or_default();
    let duration_ms = duration_buf.load(Ordering::Relaxed);
    let error_count = errors
        .iter()
        .filter(|e| e.severity == BuildErrorSeverity::Error)
        .count() as u32;
    let warning_count = errors
        .iter()
        .filter(|e| e.severity == BuildErrorSeverity::Warning)
        .count() as u32;

    let result = BuildResult {
        success,
        duration_ms,
        error_count,
        warning_count,
    };
    record_build_result(
        build_state,
        task.to_owned(),
        started_at,
        result,
        errors.clone(),
        None,
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
}
