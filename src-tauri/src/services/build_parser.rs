//! Regex-based parsing of Gradle/Kotlin/Java/AAPT2 build output lines.
//!
//! Separated from `build_runner` so this pure-logic module can be tested
//! independently without spawning processes.

use crate::models::build::{BuildLine, BuildLineKind};
use regex::Regex;
use std::sync::LazyLock;

// ── Patterns ──────────────────────────────────────────────────────────────────

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

// ── Compiled regexes ──────────────────────────────────────────────────────────

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

/// Parse a single raw output line into a structured [`BuildLine`].
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

/// Extract build duration in milliseconds from a Gradle summary line.
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
