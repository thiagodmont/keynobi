use crate::models::build::{
    BuildError, BuildLine, BuildLineKind, BuildRecord, BuildResult,
    BuildStatus,
};
use crate::services::process_manager::{self, ProcessId, ProcessManager};
use regex::Regex;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

/// Maximum number of build records kept in history (bounded collection).
pub const MAX_HISTORY: usize = 10;

/// Maximum number of raw build output lines retained for MCP `get_build_log`.
pub const MAX_BUILD_LOG: usize = 5_000;

// ── Regex patterns for parsing Gradle / Kotlin / Java / AAPT2 output ─────────
//
// Kotlin compiler error:  `e: file:///path/to/File.kt:10:5: error message`
// Kotlin compiler warning: `w: file:///path/to/File.kt:10:5: warning message`
const KOTLIN_DIAG_PATTERN: &str =
    r"^[ew]: (?:file://)?(.+?):(\d+):(\d+): (.+)$";

// Java compiler: `path/File.java:10: error: message` or `path/File.java:10: warning: message`
const JAVA_DIAG_PATTERN: &str =
    r"^(.+\.java):(\d+): (error|warning): (.+)$";

// AAPT2 resource error: `path/file.xml:10: error: message`
// Also matches `AAPT: error: message` (no file/line)
const AAPT_FILE_PATTERN: &str =
    r"^(.+\.(xml|png|webp|jpg|jpeg)):(\d+): error: (.+)$";
const AAPT_BARE_PATTERN: &str =
    r"^AAPT(?:2)?: (error|warning): (.+)$";

// Android Gradle plugin resource error:
// `ERROR: /path/file.xml:10: error message`
const AGP_ERROR_PATTERN: &str =
    r"^ERROR: (.+\.(xml|java|kt)):(\d+): (.+)$";

// Gradle build exception header: `FAILURE: Build failed with an exception.`
const GRADLE_FAILURE_PATTERN: &str =
    r"^(FAILURE: .+|> Could not resolve .+|> Could not find .+|> Failed to resolve .+|> Configuration cache .+|Error while executing process .+|Caused by: .+)$";

// Gradle "What went wrong" detail: `* What went wrong:` followed by explanation
const WHAT_WENT_WRONG_PATTERN: &str =
    r"^\* What went wrong:$";

// General error keyword line (catch-all for unrecognised errors)
const GENERIC_ERROR_PATTERN: &str =
    r"^(?:error|Error): (.+)$";

// Gradle download / progress lines (suppress as info)
const DOWNLOAD_PATTERN: &str =
    r"^(?:Download|Downloading) .+$";

const TASK_START_PATTERN: &str = r"^> Task (:.+)$";
const TASK_OUTCOME_PATTERN: &str = r"^> Task (:.+) (FAILED|UP-TO-DATE|SKIPPED|NO-SOURCE|FROM-CACHE)$";
const BUILD_SUCCESS_PATTERN: &str = r"^BUILD SUCCESSFUL(?: in (.+))?$";
const BUILD_FAILED_PATTERN: &str = r"^BUILD FAILED(?: in (.+))?$";

// ── Compiled regexes (lazy-initialized once, reused for all builds) ───────────
static KOTLIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(KOTLIN_DIAG_PATTERN).unwrap());
static JAVA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(JAVA_DIAG_PATTERN).unwrap());
static AAPT_FILE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(AAPT_FILE_PATTERN).unwrap());
static AAPT_BARE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(AAPT_BARE_PATTERN).unwrap());
static AGP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(AGP_ERROR_PATTERN).unwrap());
static GRADLE_FAIL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(GRADLE_FAILURE_PATTERN).unwrap());
static WHAT_WENT_WRONG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(WHAT_WENT_WRONG_PATTERN).unwrap());
static GENERIC_ERR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(GENERIC_ERROR_PATTERN).unwrap());
static DOWNLOAD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(DOWNLOAD_PATTERN).unwrap());
static TASK_OUTCOME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(TASK_OUTCOME_PATTERN).unwrap());
static TASK_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(TASK_START_PATTERN).unwrap());
static SUCCESS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(BUILD_SUCCESS_PATTERN).unwrap());
static FAILED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(BUILD_FAILED_PATTERN).unwrap());
static DURATION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"in (?:(\d+)m )?(\d+)(?:\.(\d+))?s").unwrap());

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

impl BuildStateInner {
    pub fn new() -> Self {
        Self {
            current_build: None,
            status: BuildStatus::Idle,
            history: VecDeque::new(),
            current_errors: vec![],
            next_id: 1,
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
}

impl BuildState {
    pub fn new() -> Self {
        BuildState {
            inner: Arc::new(Mutex::new(BuildStateInner::new())),
            build_log: Arc::new(std::sync::Mutex::new(VecDeque::new())),
        }
    }
}

impl Clone for BuildState {
    fn clone(&self) -> Self {
        BuildState {
            inner: self.inner.clone(),
            build_log: self.build_log.clone(),
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

/// Parse a single raw line from Gradle / Kotlin / Java / AAPT2 compiler output.
///
/// Returns a structured `BuildLine` that carries enough information for the
/// frontend to display clickable file:line links and populate the Problems tab.
pub fn parse_build_line(raw: &str) -> BuildLine {
    // ── Kotlin compiler error / warning ──────────────────────────────────────
    if let Some(caps) = KOTLIN_RE.captures(raw) {
        let is_warning = raw.starts_with("w:");
        return BuildLine {
            kind: if is_warning { BuildLineKind::Warning } else { BuildLineKind::Error },
            content: caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(2).and_then(|m| m.as_str().parse().ok()),
            col: caps.get(3).and_then(|m| m.as_str().parse().ok()),
        };
    }

    // ── Java compiler error / warning ─────────────────────────────────────────
    if let Some(caps) = JAVA_RE.captures(raw) {
        let is_warning = caps.get(3).map(|m| m.as_str()) == Some("warning");
        return BuildLine {
            kind: if is_warning { BuildLineKind::Warning } else { BuildLineKind::Error },
            content: caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(2).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── AAPT2 resource error with file location ───────────────────────────────
    if let Some(caps) = AAPT_FILE_RE.captures(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── AAPT2 bare error / warning (no file location) ────────────────────────
    if let Some(caps) = AAPT_BARE_RE.captures(raw) {
        let is_warning = caps.get(1).map(|m| m.as_str()) == Some("warning");
        return BuildLine {
            kind: if is_warning { BuildLineKind::Warning } else { BuildLineKind::Error },
            content: format!("AAPT: {}", caps.get(2).map(|m| m.as_str()).unwrap_or(raw)),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── AGP resource error with file:line ─────────────────────────────────────
    if let Some(caps) = AGP_RE.captures(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── Gradle FAILURE / dependency resolution errors ─────────────────────────
    if GRADLE_FAIL_RE.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── "What went wrong" header ──────────────────────────────────────────────
    if WHAT_WENT_WRONG_RE.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── Gradle task outcome (check before task start — more specific) ─────────
    if let Some(caps) = TASK_OUTCOME_RE.captures(raw) {
        let task = caps.get(1).map(|m| m.as_str().to_owned()).unwrap_or_default();
        let outcome = caps.get(2).map(|m| m.as_str().to_owned()).unwrap_or_default();
        return BuildLine {
            kind: BuildLineKind::TaskEnd,
            content: format!("{task} {outcome}"),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── Gradle task start ─────────────────────────────────────────────────────
    if let Some(caps) = TASK_START_RE.captures(raw) {
        let task = caps.get(1).map(|m| m.as_str().to_owned()).unwrap_or_default();
        return BuildLine {
            kind: BuildLineKind::TaskStart,
            content: task,
            file: None,
            line: None,
            col: None,
        };
    }

    // ── BUILD SUCCESSFUL / BUILD FAILED ───────────────────────────────────────
    if SUCCESS_RE.is_match(raw) || FAILED_RE.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Summary,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── Download / progress lines (show as info, not noise) ──────────────────
    if DOWNLOAD_RE.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Info,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── Generic error keyword (catch-all) ─────────────────────────────────────
    if let Some(caps) = GENERIC_ERR_RE.captures(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: caps.get(1).map(|m| m.as_str().to_owned()).unwrap_or_else(|| raw.to_owned()),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── Plain output ──────────────────────────────────────────────────────────
    BuildLine::output(raw)
}

/// Parse the duration string from `BUILD SUCCESSFUL in Xs` / `BUILD FAILED in Xs`.
///
/// Returns duration in milliseconds, or 0 if unparseable.
pub fn parse_build_duration(summary_line: &str) -> u64 {
    if let Some(caps) = DURATION_RE.captures(summary_line) {
        let mins: u64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        let secs: u64 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        let millis_str = caps.get(3).map(|m| m.as_str()).unwrap_or("0");
        // Pad to 3 digits for milliseconds.
        let millis: u64 = format!("{:0<3}", millis_str)
            .chars()
            .take(3)
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        return (mins * 60 + secs) * 1000 + millis;
    }
    0
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
    let base = gradle_root.join("app").join("build").join("outputs").join("apk");
    if !base.is_dir() {
        return None;
    }
    let all_files = walk_dir_for_apk(&base, 4);

    // Only exclude files that are genuinely not installable.
    let is_usable = |name: &str| -> bool {
        !name.ends_with("-unaligned.apk")
    };
    let is_signed = |name: &str| -> bool {
        !name.contains("-unsigned")
    };

    let variant_lc = variant_name.to_lowercase();
    let parent_matches = |path: &Path| -> bool {
        if variant_name.is_empty() { return true; }
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
            let name = p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            p.extension().and_then(|e| e.to_str()) == Some("apk") && is_usable(&name)
        })
        .collect();

    // Pass 1 — signed + variant dir match.
    for p in &apks {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
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
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        if is_signed(&name) {
            return Some((*p).clone());
        }
    }
    // Pass 4 — any usable APK (unsigned, any location).
    apks.into_iter().next().cloned()
}

/// Cancel the currently running build. Returns `true` if a build was running, `false` otherwise.
pub async fn cancel_build(
    build_state: &BuildState,
    process_manager: &ProcessManager,
) -> bool {
    let pid = {
        let mut bs = build_state.inner.lock().await;
        let pid = bs.current_build.take();
        if pid.is_some() {
            bs.status = BuildStatus::Cancelled;
        }
        pid
    };
    if let Some(id) = pid {
        process_manager::cancel(&process_manager.0, id).await;
        true
    } else {
        false
    }
}

/// Record the completed build result and push it to history.
pub async fn record_build_result(
    build_state: &BuildState,
    task: String,
    started_at: String,
    result: BuildResult,
    errors: Vec<BuildError>,
) {
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
    };
    bs.next_id += 1;

    bs.history.push_back(record);
    while bs.history.len() > MAX_HISTORY {
        bs.history.pop_front();
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
    if let Some(jvm_args) = settings.build.gradle_jvm_args.as_deref() {
        env.push(("GRADLE_OPTS".into(), jvm_args.into()));
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
    let done_flag = Arc::new(AtomicBool::new(false));
    let done_flag_clone = done_flag.clone();

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
                done_flag_clone.store(true, Ordering::Release);
            }),
        },
    )
    .await
    .map_err(|e| format!("Failed to spawn Gradle: {e}"))?;

    {
        let mut bs = build_state.inner.lock().await;
        bs.current_build = Some(pid);
    }

    let timeout = std::time::Duration::from_secs(timeout_sec);
    let start = std::time::Instant::now();
    let timed_out;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if done_flag.load(Ordering::Acquire) {
            timed_out = false;
            break;
        }
        if start.elapsed() > timeout {
            timed_out = true;
            break;
        }
    }

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

    let result = BuildResult { success, duration_ms, error_count, warning_count };
    record_build_result(build_state, task.to_owned(), started_at, result, errors.clone()).await;

    Ok(GradleTaskResult { success, timed_out: false, duration_ms, errors })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let line = parse_build_line(
            "e: /Users/dev/app/src/Main.kt:5:1: Expecting member declaration",
        );
        assert_eq!(line.kind, BuildLineKind::Error);
        assert_eq!(line.line, Some(5));
    }

    #[test]
    fn parses_kotlin_warning() {
        let line = parse_build_line(
            "w: file:///src/Foo.kt:10:3: Parameter 'x' is never used",
        );
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
        let line = parse_build_line("src/main/java/com/example/Foo.java:23: error: cannot find symbol");
        assert_eq!(line.kind, BuildLineKind::Error);
        assert_eq!(line.line, Some(23));
        assert!(line.file.as_deref().unwrap().contains("Foo.java"));
    }

    #[test]
    fn parses_aapt_file_error() {
        let line = parse_build_line("app/src/main/res/layout/activity_main.xml:10: error: attribute missing");
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
        let apk_dir = tmp.join("app").join("build").join("outputs").join("apk").join("release");
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
        let apk_dir = tmp.join("app").join("build").join("outputs").join("apk").join("release");
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
        let apk_dir = tmp.join("app").join("build").join("outputs").join("apk").join("release");
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
        let apk_dir = tmp.join("app").join("build").join("outputs").join("apk").join("release");
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
    async fn cancel_build_returns_false_when_idle() {
        let state = BuildState::new();
        let pm = ProcessManager::new();
        let was_running = cancel_build(&state, &pm).await;
        assert!(!was_running, "cancel_build should return false when no build is running");
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
        assert!(was_running, "cancel_build should return true when a build was running");
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
        assert!(inner.current_build.is_none(), "current_build PID must be cleared after cancel");
    }
}
