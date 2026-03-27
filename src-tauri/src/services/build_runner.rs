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

/// Parse a single raw line from Gradle / Kotlin / Java / AAPT2 compiler output.
///
/// Returns a structured `BuildLine` that carries enough information for the
/// frontend to display clickable file:line links and populate the Problems tab.
pub fn parse_build_line(raw: &str) -> BuildLine {
    // Compile patterns once per call. These are not on a hot-path; the
    // bottleneck is I/O from the Gradle process.
    let kotlin_re = Regex::new(KOTLIN_DIAG_PATTERN).expect("static regex");
    let java_re = Regex::new(JAVA_DIAG_PATTERN).expect("static regex");
    let aapt_file_re = Regex::new(AAPT_FILE_PATTERN).expect("static regex");
    let aapt_bare_re = Regex::new(AAPT_BARE_PATTERN).expect("static regex");
    let agp_re = Regex::new(AGP_ERROR_PATTERN).expect("static regex");
    let gradle_fail_re = Regex::new(GRADLE_FAILURE_PATTERN).expect("static regex");
    let what_went_wrong_re = Regex::new(WHAT_WENT_WRONG_PATTERN).expect("static regex");
    let generic_err_re = Regex::new(GENERIC_ERROR_PATTERN).expect("static regex");
    let download_re = Regex::new(DOWNLOAD_PATTERN).expect("static regex");
    let task_outcome_re = Regex::new(TASK_OUTCOME_PATTERN).expect("static regex");
    let task_start_re = Regex::new(TASK_START_PATTERN).expect("static regex");
    let success_re = Regex::new(BUILD_SUCCESS_PATTERN).expect("static regex");
    let failed_re = Regex::new(BUILD_FAILED_PATTERN).expect("static regex");

    // ── Kotlin compiler error / warning ──────────────────────────────────────
    if let Some(caps) = kotlin_re.captures(raw) {
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
    if let Some(caps) = java_re.captures(raw) {
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
    if let Some(caps) = aapt_file_re.captures(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── AAPT2 bare error / warning (no file location) ────────────────────────
    if let Some(caps) = aapt_bare_re.captures(raw) {
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
    if let Some(caps) = agp_re.captures(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: caps.get(4).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── Gradle FAILURE / dependency resolution errors ─────────────────────────
    if gradle_fail_re.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── "What went wrong" header ──────────────────────────────────────────────
    if what_went_wrong_re.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
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

    // ── Download / progress lines (show as info, not noise) ──────────────────
    if download_re.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Info,
            content: raw.to_owned(),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── Generic error keyword (catch-all) ─────────────────────────────────────
    if let Some(caps) = generic_err_re.captures(raw) {
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
}
