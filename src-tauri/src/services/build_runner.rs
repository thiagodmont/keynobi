use crate::models::build::{
    BuildError, BuildLine, BuildLineKind, BuildRecord, BuildResult,
    BuildStatus,
};
use crate::services::process_manager::{self, ProcessId, ProcessManager};
use regex::Regex;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

/// Maximum number of build records kept in history (bounded collection).
pub const MAX_HISTORY: usize = 10;

// ── Regex patterns for parsing Gradle / Kotlin compiler output ────────────────
//
// Kotlin compiler error:  `e: file:///path/to/File.kt:10:5: error message`
// Kotlin compiler warning: `w: file:///path/to/File.kt:10:5: warning message`
// Also handle the format without `file://` prefix.
const ERROR_PATTERN: &str =
    r"^[ew]: (?:file://)?(.+?):(\d+):(\d+): (.+)$";

const TASK_START_PATTERN: &str = r"^> Task (:.+)$";
const TASK_OUTCOME_PATTERN: &str = r"^> Task (:.+) (FAILED|UP-TO-DATE|SKIPPED|NO-SOURCE|FROM-CACHE)$";
const BUILD_SUCCESS_PATTERN: &str = r"^BUILD SUCCESSFUL(?: in (.+))?$";
const BUILD_FAILED_PATTERN: &str = r"^BUILD FAILED(?: in (.+))?$";

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
pub struct BuildState(pub Mutex<BuildStateInner>);

impl BuildState {
    pub fn new() -> Self {
        BuildState(Mutex::new(BuildStateInner::new()))
    }
}

impl Default for BuildState {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a single raw line from Gradle / Kotlin compiler output.
///
/// The result carries structured information about errors and warnings so the
/// frontend can render clickable file:line links.
pub fn parse_build_line(raw: &str) -> BuildLine {
    // Lazily compile patterns once (thread-safe via OnceLock in std or just
    // create here — the function is not on a hot-path; the bottleneck is I/O).
    let error_re = Regex::new(ERROR_PATTERN).expect("static regex");
    let task_start_re = Regex::new(TASK_START_PATTERN).expect("static regex");
    let task_outcome_re = Regex::new(TASK_OUTCOME_PATTERN).expect("static regex");
    let success_re = Regex::new(BUILD_SUCCESS_PATTERN).expect("static regex");
    let failed_re = Regex::new(BUILD_FAILED_PATTERN).expect("static regex");

    // ── Kotlin compiler error / warning ──────────────────────────────────────
    if let Some(caps) = error_re.captures(raw) {
        let is_warning = raw.starts_with("w:");
        let file = caps.get(1).map(|m| m.as_str().to_owned());
        let line = caps.get(2).and_then(|m| m.as_str().parse().ok());
        let col = caps.get(3).and_then(|m| m.as_str().parse().ok());
        let msg = caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default();
        return BuildLine {
            kind: if is_warning { BuildLineKind::Warning } else { BuildLineKind::Error },
            content: msg,
            file,
            line,
            col,
        };
    }

    // ── Gradle task outcome (check before task start — more specific) ─────────
    if let Some(caps) = task_outcome_re.captures(raw) {
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
    if let Some(caps) = task_start_re.captures(raw) {
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
    if success_re.is_match(raw) || failed_re.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Summary,
            content: raw.to_owned(),
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
    // e.g. "BUILD SUCCESSFUL in 1m 30s" or "BUILD SUCCESSFUL in 45s"
    let re = Regex::new(r"in (?:(\d+)m )?(\d+)(?:\.(\d+))?s").expect("static regex");
    if let Some(caps) = re.captures(summary_line) {
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
///   or
///   `{gradle_root}/app/build/outputs/apk/{flavor}/{buildType}/app-{flavor}-{buildType}.apk`
pub fn find_output_apk(gradle_root: &Path, variant_name: &str) -> Option<PathBuf> {
    let base = gradle_root.join("app").join("build").join("outputs").join("apk");
    if !base.is_dir() {
        return None;
    }
    let all_files = walk_dir_for_apk(&base, 3);

    // First pass: prefer APK whose parent dir matches variant.
    for path in &all_files {
        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
            let name = path.file_name()?.to_str()?.to_lowercase();
            if name.contains("unsigned") || name.contains("unaligned") {
                continue;
            }
            let parent_name = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if parent_name == variant_name.to_lowercase() || variant_name.is_empty() {
                return Some(path.to_owned());
            }
        }
    }
    // Second pass: return first APK regardless of directory match.
    for path in &all_files {
        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
            let name = path.file_name()?.to_str()?.to_lowercase();
            if !name.contains("unsigned") && !name.contains("unaligned") {
                return Some(path.to_owned());
            }
        }
    }
    None
}

/// Cancel the currently running build.
pub async fn cancel_build(
    build_state: &Mutex<BuildStateInner>,
    process_manager: &ProcessManager,
) {
    let pid = {
        let mut bs = build_state.lock().await;
        let pid = bs.current_build.take();
        bs.status = BuildStatus::Cancelled;
        pid
    };
    if let Some(id) = pid {
        process_manager::cancel(&process_manager.0, id).await;
    }
}

/// Record the completed build result and push it to history.
pub async fn record_build_result(
    build_state: &Mutex<BuildStateInner>,
    task: String,
    started_at: String,
    result: BuildResult,
    errors: Vec<BuildError>,
) {
    let mut bs = build_state.lock().await;
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
}
