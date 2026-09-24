//! Regex-based parsing of Gradle build output lines (Kotlin, KSP, Java, lint,
//! AAPT2, R8, configuration cache).
//!
//! Separated from `build_runner` so this pure-logic module can be tested
//! independently without spawning processes.

use crate::models::build::{BuildLine, BuildLineKind};
use regex::{Match, Regex};
use std::sync::LazyLock;

// ── Patterns ──────────────────────────────────────────────────────────────────

// Kotlin compiler error / warning. Kotlin 1 and the Gradle script compiler put a
// colon after the column; Kotlin 2 does not:
// `e: file:///path/to/File.kt:10:5: message` / `w: file:///path/to/File.kt:10:5 message`
const KOTLIN_DIAG_PATTERN: &str = r"^([ew]): (?:file://)?(\S.*?):(\d+):(\d+):? (.+)$";

// KSP processor error / warning (no column): `e: [ksp] /path/File.kt:10: message`
const KSP_DIAG_PATTERN: &str = r"^([ew]): \[ksp\] (?:file://)?(\S.*?):(\d+)(?::(\d+))?: (.+)$";

// Java compiler: `path/File.java:10: error: message` or `path/File.java:10: warning: message`.
// Gradle repeats them indented under "What went wrong"; those copies are skipped.
const JAVA_DIAG_PATTERN: &str = r"^([^\s>].*\.java):(\d+): (error|warning): (.+)$";

// Android lint: `path/File.kt:10: Error: message [IssueId]` (also `Warning:` / `Fatal:`).
const LINT_DIAG_PATTERN: &str = r"^([^\s>].*?):(\d+)(?::(\d+))?: (Error|Fatal|Warning): (.+)$";

// AAPT2 resource error: `path/file.xml:10: error: message`. Resource linking
// errors are printed indented inside the "What went wrong" block.
// Also matches `AAPT: error: message` (no file/line)
const AAPT_FILE_PATTERN: &str = r"^\s*(\S.*\.(xml|png|webp|jpg|jpeg)):(\d+): error: (.+)$";
const AAPT_BARE_PATTERN: &str = r"^AAPT(?:2)?: (error|warning): (.+)$";

// Android Gradle plugin / R8 error with a location:
// `ERROR: /path/values.xml:6:3: Resource and asset merger: message`
// `ERROR: /path/proguard-rules.pro:3:19: R8: message`
const AGP_ERROR_PATTERN: &str = r"^ERROR: (\S.*?\.[A-Za-z0-9]+):(\d+)(?::(\d+))?: (.+)$";

// Configuration cache problem list entries (warnings; the build fails separately
// when problems are not allowed):
// `- Build file 'app/build.gradle.kts': line 12: message`
// ``- Task `:app:foo` of type `DefaultTask`: message``
const CONFIG_CACHE_FILE_PATTERN: &str =
    r"^- (?:Build file|Settings file) '([^']+)'(?:: line (\d+))?: (.+)$";
const CONFIG_CACHE_TASK_PATTERN: &str = r"^- (Task `[^`]+` of type `[^`]+`: .+)$";

// Diagnostics without a recognised location. Surfaced with the message only so a
// failing build never drops them: `e: message`, `w: message`, `ERROR: message`
// (R8, AGP), and the per-class lines R8 prints after `ERROR: R8: Missing class`.
const KOTLIN_BARE_PATTERN: &str = r"^([ew]): (.+)$";
const TOOL_ERROR_PATTERN: &str = r"^ERROR: (.+)$";
const R8_MISSING_CLASS_PATTERN: &str = r"^Missing class \S+ \(referenced from: .+\)$";

// In-process resource compiler failure, only reported inside "What went wrong":
// `   > Resource compilation failed (… Cause: …). Check logs for more details.`
const RESOURCE_COMPILE_FAILED_PATTERN: &str = r"^\s*> (Resource compilation failed .+)$";

// Gradle build exception header: `FAILURE: Build failed with an exception.`
const GRADLE_FAILURE_PATTERN: &str = r"^(FAILURE: .+|> Could not resolve .+|> Could not find .+|> Failed to resolve .+|> Configuration cache .+|Configuration cache problems found in this build\.|Error while executing process .+|Caused by: .+)$";

// Gradle "What went wrong" detail: `* What went wrong:` followed by explanation
const WHAT_WENT_WRONG_PATTERN: &str = r"^\* What went wrong:$";

// General error keyword line (catch-all for unrecognised errors)
const GENERIC_ERROR_PATTERN: &str = r"^\s*(?:error|Error): (.+)$";

// Gradle download / progress lines (suppress as info)
const DOWNLOAD_PATTERN: &str = r"^(?:Download|Downloading) .+$";

const TASK_START_PATTERN: &str = r"^> Task (:.+)$";
const TASK_OUTCOME_PATTERN: &str =
    r"^> Task (:.+) (FAILED|UP-TO-DATE|SKIPPED|NO-SOURCE|FROM-CACHE)$";
const BUILD_SUCCESS_PATTERN: &str = r"^BUILD SUCCESSFUL(?: in (.+))?$";
const BUILD_FAILED_PATTERN: &str = r"^BUILD FAILED(?: in (.+))?$";

// ── Compiled regexes ──────────────────────────────────────────────────────────

static KOTLIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(KOTLIN_DIAG_PATTERN).unwrap());
static KSP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(KSP_DIAG_PATTERN).expect("KSP_DIAG_PATTERN"));
static JAVA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(JAVA_DIAG_PATTERN).unwrap());
static LINT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(LINT_DIAG_PATTERN).expect("LINT_DIAG_PATTERN"));
static AAPT_FILE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(AAPT_FILE_PATTERN).unwrap());
static AAPT_BARE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(AAPT_BARE_PATTERN).unwrap());
static AGP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(AGP_ERROR_PATTERN).unwrap());
static CONFIG_CACHE_FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(CONFIG_CACHE_FILE_PATTERN).expect("CONFIG_CACHE_FILE_PATTERN"));
static CONFIG_CACHE_TASK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(CONFIG_CACHE_TASK_PATTERN).expect("CONFIG_CACHE_TASK_PATTERN"));
static KOTLIN_BARE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(KOTLIN_BARE_PATTERN).expect("KOTLIN_BARE_PATTERN"));
static TOOL_ERROR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(TOOL_ERROR_PATTERN).expect("TOOL_ERROR_PATTERN"));
static R8_MISSING_CLASS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(R8_MISSING_CLASS_PATTERN).expect("R8_MISSING_CLASS_PATTERN"));
static RESOURCE_COMPILE_FAILED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(RESOURCE_COMPILE_FAILED_PATTERN).expect("RESOURCE_COMPILE_FAILED_PATTERN")
});
static GRADLE_FAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(GRADLE_FAILURE_PATTERN).unwrap());
static WHAT_WENT_WRONG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(WHAT_WENT_WRONG_PATTERN).unwrap());
static GENERIC_ERR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(GENERIC_ERROR_PATTERN).unwrap());
static DOWNLOAD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(DOWNLOAD_PATTERN).unwrap());
static TASK_OUTCOME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(TASK_OUTCOME_PATTERN).unwrap());
static TASK_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(TASK_START_PATTERN).unwrap());
static SUCCESS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(BUILD_SUCCESS_PATTERN).unwrap());
static FAILED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(BUILD_FAILED_PATTERN).unwrap());
static DURATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"in (?:(\d+)m )?(\d+)(?:\.(\d+))?s").unwrap());

/// Build a diagnostic line from regex captures; absent captures stay `None`.
fn diagnostic(
    kind: BuildLineKind,
    content: Option<Match>,
    file: Option<Match>,
    line: Option<Match>,
    col: Option<Match>,
) -> BuildLine {
    BuildLine {
        kind,
        content: content.map(|m| m.as_str().to_owned()).unwrap_or_default(),
        file: file.map(|m| m.as_str().to_owned()),
        line: line.and_then(|m| m.as_str().parse().ok()),
        col: col.and_then(|m| m.as_str().parse().ok()),
    }
}

/// Kotlin/KSP severity marker: `w` is a warning, anything else an error.
fn marker_kind(marker: Option<Match>) -> BuildLineKind {
    if marker.map(|m| m.as_str()) == Some("w") {
        BuildLineKind::Warning
    } else {
        BuildLineKind::Error
    }
}

/// Parse a single raw output line into a structured [`BuildLine`].
pub fn parse_build_line(raw: &str) -> BuildLine {
    // ── KSP processor error / warning ────────────────────────────────────────
    if let Some(caps) = KSP_RE.captures(raw) {
        return diagnostic(
            marker_kind(caps.get(1)),
            caps.get(5),
            caps.get(2),
            caps.get(3),
            caps.get(4),
        );
    }

    // ── Kotlin compiler error / warning ──────────────────────────────────────
    if let Some(caps) = KOTLIN_RE.captures(raw) {
        return diagnostic(
            marker_kind(caps.get(1)),
            caps.get(5),
            caps.get(2),
            caps.get(3),
            caps.get(4),
        );
    }

    // ── Java compiler error / warning ─────────────────────────────────────────
    if let Some(caps) = JAVA_RE.captures(raw) {
        let is_warning = caps.get(3).map(|m| m.as_str()) == Some("warning");
        return BuildLine {
            kind: if is_warning {
                BuildLineKind::Warning
            } else {
                BuildLineKind::Error
            },
            content: caps
                .get(4)
                .map(|m| m.as_str().to_owned())
                .unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(2).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── Android lint issue ────────────────────────────────────────────────────
    if let Some(caps) = LINT_RE.captures(raw) {
        let kind = if caps.get(4).map(|m| m.as_str()) == Some("Warning") {
            BuildLineKind::Warning
        } else {
            BuildLineKind::Error
        };
        return diagnostic(kind, caps.get(5), caps.get(1), caps.get(2), caps.get(3));
    }

    // ── AAPT2 resource error with file location ───────────────────────────────
    if let Some(caps) = AAPT_FILE_RE.captures(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            content: caps
                .get(4)
                .map(|m| m.as_str().to_owned())
                .unwrap_or_default(),
            file: caps.get(1).map(|m| m.as_str().to_owned()),
            line: caps.get(3).and_then(|m| m.as_str().parse().ok()),
            col: None,
        };
    }

    // ── AAPT2 bare error / warning (no file location) ────────────────────────
    if let Some(caps) = AAPT_BARE_RE.captures(raw) {
        let is_warning = caps.get(1).map(|m| m.as_str()) == Some("warning");
        return BuildLine {
            kind: if is_warning {
                BuildLineKind::Warning
            } else {
                BuildLineKind::Error
            },
            content: format!("AAPT: {}", caps.get(2).map(|m| m.as_str()).unwrap_or(raw)),
            file: None,
            line: None,
            col: None,
        };
    }

    // ── AGP / R8 error with file:line[:col] ───────────────────────────────────
    if let Some(caps) = AGP_RE.captures(raw) {
        return diagnostic(
            BuildLineKind::Error,
            caps.get(4),
            caps.get(1),
            caps.get(2),
            caps.get(3),
        );
    }

    // ── Configuration cache problems ──────────────────────────────────────────
    if let Some(caps) = CONFIG_CACHE_FILE_RE.captures(raw) {
        return diagnostic(
            BuildLineKind::Warning,
            caps.get(3),
            caps.get(1),
            caps.get(2),
            None,
        );
    }
    if let Some(caps) = CONFIG_CACHE_TASK_RE.captures(raw) {
        return diagnostic(BuildLineKind::Warning, caps.get(1), None, None, None);
    }

    // ── Diagnostics without a recognised location ─────────────────────────────
    if let Some(caps) = KOTLIN_BARE_RE.captures(raw) {
        return diagnostic(marker_kind(caps.get(1)), caps.get(2), None, None, None);
    }
    if let Some(caps) = TOOL_ERROR_RE.captures(raw) {
        return diagnostic(BuildLineKind::Error, caps.get(1), None, None, None);
    }
    if R8_MISSING_CLASS_RE.is_match(raw) {
        return BuildLine {
            kind: BuildLineKind::Error,
            ..BuildLine::output(raw)
        };
    }
    if let Some(caps) = RESOURCE_COMPILE_FAILED_RE.captures(raw) {
        return diagnostic(BuildLineKind::Error, caps.get(1), None, None, None);
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
        let task = caps
            .get(1)
            .map(|m| m.as_str().to_owned())
            .unwrap_or_default();
        let outcome = caps
            .get(2)
            .map(|m| m.as_str().to_owned())
            .unwrap_or_default();
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
        let task = caps
            .get(1)
            .map(|m| m.as_str().to_owned())
            .unwrap_or_default();
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
            content: caps
                .get(1)
                .map(|m| m.as_str().to_owned())
                .unwrap_or_else(|| raw.to_owned()),
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
        let mins: u64 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let secs: u64 = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
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

#[cfg(test)]
mod tests {
    //! Fixture tests against complete console output of real Gradle builds.
    //! See `tests/fixtures/build_output/README.md` for how each was produced.

    use super::*;
    use BuildLineKind::{Error, Warning};

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../tests/fixtures/build_output/", $name))
        };
    }

    const FIXTURES: &[(&str, &str)] = &[
        (
            "aapt2_compile_invalid_value",
            fixture!("aapt2_compile_invalid_value.txt"),
        ),
        ("aapt2_link_errors", fixture!("aapt2_link_errors.txt")),
        (
            "configuration_cache_problems",
            fixture!("configuration_cache_problems.txt"),
        ),
        (
            "configuration_cache_problems_warn",
            fixture!("configuration_cache_problems_warn.txt"),
        ),
        (
            "gradle_script_compile_error",
            fixture!("gradle_script_compile_error.txt"),
        ),
        ("javac_errors", fixture!("javac_errors.txt")),
        ("kotlin_k2_errors", fixture!("kotlin_k2_errors.txt")),
        ("kotlin_k2_warnings", fixture!("kotlin_k2_warnings.txt")),
        ("ksp_room_errors", fixture!("ksp_room_errors.txt")),
        ("lint_abort_on_error", fixture!("lint_abort_on_error.txt")),
        ("lint_text_report", fixture!("lint_text_report.txt")),
        ("r8_keep_rule_syntax", fixture!("r8_keep_rule_syntax.txt")),
        ("r8_missing_classes", fixture!("r8_missing_classes.txt")),
        (
            "resource_merger_malformed_xml",
            fixture!("resource_merger_malformed_xml.txt"),
        ),
        ("successful_build", fixture!("successful_build.txt")),
    ];

    const SRC: &str = "/Users/dev/project/app/src/main/java/com/example/r18";

    /// A parsed diagnostic, as `get_build_errors` and the Problems view see it.
    #[derive(Debug, PartialEq)]
    struct Diag {
        kind: BuildLineKind,
        file: Option<String>,
        line: Option<u32>,
        col: Option<u32>,
        message: String,
    }

    fn at(kind: BuildLineKind, file: &str, line: u32, col: Option<u32>, message: &str) -> Diag {
        Diag {
            kind,
            file: Some(file.to_owned()),
            line: Some(line),
            col,
            message: message.to_owned(),
        }
    }

    fn bare(kind: BuildLineKind, message: &str) -> Diag {
        Diag {
            kind,
            file: None,
            line: None,
            col: None,
            message: message.to_owned(),
        }
    }

    /// The two lines every failed Gradle build prints.
    fn gradle_failure() -> Vec<Diag> {
        vec![
            bare(Error, "FAILURE: Build failed with an exception."),
            bare(Error, "* What went wrong:"),
        ]
    }

    fn diagnostics(log: &str) -> Vec<Diag> {
        log.lines()
            .map(parse_build_line)
            .filter(|l| matches!(l.kind, Error | Warning))
            .map(|l| Diag {
                kind: l.kind,
                file: l.file,
                line: l.line,
                col: l.col,
                message: l.content,
            })
            .collect()
    }

    #[test]
    fn kotlin_2_errors_without_colon_after_column() {
        let mut expected = vec![
            at(
                Error,
                &format!("{SRC}/Greeter.kt"),
                4,
                Some(54),
                "Unresolved reference 'missingExtension'.",
            ),
            at(
                Error,
                &format!("{SRC}/MainActivity.kt"),
                10,
                Some(26),
                "Initializer type mismatch: expected 'Int', actual 'String'.",
            ),
            at(
                Error,
                &format!("{SRC}/MainActivity.kt"),
                11,
                Some(9),
                "Unresolved reference 'undefinedFunction'.",
            ),
        ];
        expected.extend(gradle_failure());
        assert_eq!(diagnostics(fixture!("kotlin_k2_errors.txt")), expected);
    }

    #[test]
    fn kotlin_2_warnings() {
        assert_eq!(
            diagnostics(fixture!("kotlin_k2_warnings.txt")),
            vec![
                at(
                    Warning,
                    &format!("{SRC}/MainActivity.kt"),
                    10,
                    Some(32),
                    "'fun oldGreet(): String' is deprecated. Use greet(name) instead.",
                ),
                at(
                    Warning,
                    &format!("{SRC}/MainActivity.kt"),
                    11,
                    Some(27),
                    "No cast needed.",
                ),
            ]
        );
    }

    #[test]
    fn gradle_script_errors_keep_colon_after_column() {
        let mut expected = vec![at(
            Error,
            "/Users/dev/project/app/build.gradle.kts",
            23,
            Some(23),
            "Unresolved reference 'io'.",
        )];
        expected.extend(gradle_failure());
        assert_eq!(
            diagnostics(fixture!("gradle_script_compile_error.txt")),
            expected
        );
    }

    #[test]
    fn javac_errors_are_not_duplicated_from_the_failure_summary() {
        let file = format!("{SRC}/JavaHelper.java");
        let mut expected = vec![
            at(
                Error,
                &file,
                8,
                None,
                "incompatible types: int cannot be converted to String",
            ),
            at(Error, &file, 9, None, "cannot find symbol"),
        ];
        expected.extend(gradle_failure());
        assert_eq!(diagnostics(fixture!("javac_errors.txt")), expected);
    }

    #[test]
    fn ksp_errors_and_warnings() {
        let file = format!("{SRC}/data/Db.kt");
        let mut expected = vec![
            at(
                Error,
                &file,
                10,
                None,
                "An entity must have at least 1 property annotated with @PrimaryKey",
            ),
            at(
                Error,
                &file,
                18,
                None,
                "There is a problem with the query: [SQLITE_ERROR] SQL error or missing database (no such table: missing_table)",
            ),
            at(
                Error,
                &file,
                18,
                None,
                "Not sure how to convert the query result to this function's return type (kotlin.collections.List<com.example.r18.`data`.Note>).",
            ),
            at(
                Warning,
                &file,
                22,
                None,
                "Schema export directory was not provided to the annotation processor so Room cannot export the schema. You can either provide `room.schemaLocation` annotation processor argument by applying the Room Gradle plugin (id 'androidx.room') OR set exportSchema to false.",
            ),
        ];
        expected.extend(gradle_failure());
        assert_eq!(diagnostics(fixture!("ksp_room_errors.txt")), expected);
    }

    #[test]
    fn lint_abort_on_error_reports_the_first_failure() {
        // Lint prints its first failure once from the report task and once from
        // the failing task; the indented copy under "What went wrong" is skipped.
        let mut expected: Vec<Diag> = (0..2)
            .map(|_| {
                at(
                    Error,
                    &format!("{SRC}/MainActivity.kt"),
                    11,
                    None,
                    "Call requires API level 33 (current min is 29): android.app.Activity#getOnBackInvokedDispatcher [NewApi]",
                )
            })
            .collect();
        expected.extend(gradle_failure());
        assert_eq!(diagnostics(fixture!("lint_abort_on_error.txt")), expected);
    }

    #[test]
    fn lint_text_report_errors_and_warnings() {
        let gradle = "/Users/dev/project/app/build.gradle.kts";
        let res = "/Users/dev/project/app/src/main";
        assert_eq!(
            diagnostics(fixture!("lint_text_report.txt")),
            vec![
                at(
                    Error,
                    &format!("{SRC}/MainActivity.kt"),
                    11,
                    None,
                    "Call requires API level 33 (current min is 29): android.app.Activity#getOnBackInvokedDispatcher [NewApi]",
                ),
                at(
                    Error,
                    &format!("{SRC}/MainActivity.kt"),
                    11,
                    None,
                    "Call requires API level 33 (current min is 29): android.window.OnBackInvokedDispatcher#registerOnBackInvokedCallback [NewApi]",
                ),
                at(
                    Warning,
                    gradle,
                    13,
                    None,
                    "Not targeting the latest versions of Android; compatibility modes apply. Consider testing and updating this version. Consult the android.os.Build.VERSION_CODES javadoc for details. [OldTargetApi]",
                ),
                at(
                    Warning,
                    gradle,
                    8,
                    None,
                    "A newer version of compileSdk than 36 is available: 37 [GradleDependency]",
                ),
                at(
                    Warning,
                    gradle,
                    29,
                    None,
                    "A newer version of androidx.room:room-runtime than 2.8.4 is available: 2.8.5 [GradleDependency]",
                ),
                at(
                    Warning,
                    gradle,
                    30,
                    None,
                    "A newer version of androidx.room:room-compiler than 2.8.4 is available: 2.8.5 [GradleDependency]",
                ),
                at(
                    Warning,
                    &format!("{res}/AndroidManifest.xml"),
                    3,
                    None,
                    "Should explicitly set android:icon, there is no default [MissingApplicationIcon]",
                ),
                at(
                    Warning,
                    &format!("{res}/res/layout/activity_main.xml"),
                    9,
                    None,
                    "Hardcoded string \"Hello lint\", should use @string resource [HardcodedText]",
                ),
            ]
        );
    }

    #[test]
    fn r8_missing_classes() {
        let mut expected = vec![
            bare(
                Error,
                "Missing classes detected while running R8. Please add the missing classes or apply additional keep rules that are generated in /Users/dev/project/app/build/outputs/mapping/release/missing_rules.txt.",
            ),
            bare(
                Error,
                "R8: Missing class com.example.stub.Analytics (referenced from: void com.example.lib.LibTracker.send(java.lang.String))",
            ),
            bare(
                Error,
                "Missing class com.example.stub.BaseTracker (referenced from: void com.example.lib.LibTracker.<init>() and 1 other context)",
            ),
        ];
        expected.extend(gradle_failure());
        assert_eq!(diagnostics(fixture!("r8_missing_classes.txt")), expected);
    }

    #[test]
    fn r8_keep_rule_syntax_error() {
        let mut expected = vec![at(
            Error,
            "/Users/dev/project/app/proguard-rules.pro",
            3,
            Some(19),
            "R8: Expected [!]interface|@interface|class|enum",
        )];
        expected.extend(gradle_failure());
        assert_eq!(diagnostics(fixture!("r8_keep_rule_syntax.txt")), expected);
    }

    #[test]
    fn resource_merger_malformed_xml() {
        let mut expected = vec![at(
            Error,
            "/Users/dev/project/app/src/main/res/values/colors.xml",
            6,
            Some(3),
            "Resource and asset merger: The element type \"string\" must be terminated by the matching end-tag \"</string>\".",
        )];
        expected.extend(gradle_failure());
        assert_eq!(
            diagnostics(fixture!("resource_merger_malformed_xml.txt")),
            expected
        );
    }

    #[test]
    fn aapt2_compile_failure_surfaces_the_cause() {
        let mut expected = gradle_failure();
        expected.push(bare(
            Error,
            "Resource compilation failed (Failed to compile values resource file /Users/dev/project/app/build/intermediates/incremental/debug/mergeDebugResources/merged.dir/values/values.xml. Cause: java.lang.Exception: Unable to parse hex color '#GG0000'.). Check logs for more details.",
        ));
        assert_eq!(
            diagnostics(fixture!("aapt2_compile_invalid_value.txt")),
            expected
        );
    }

    #[test]
    fn aapt2_link_errors_inside_the_failure_summary() {
        let file = "com.example.r18.app-main-7:/layout/activity_main.xml";
        let mut expected = gradle_failure();
        expected.extend([
            at(
                Error,
                file,
                11,
                None,
                "resource color/does_not_exist (aka com.example.r18:color/does_not_exist) not found.",
            ),
            at(
                Error,
                file,
                11,
                None,
                "resource string/missing_label (aka com.example.r18:string/missing_label) not found.",
            ),
            at(
                Error,
                file,
                17,
                None,
                "resource drawable/nope (aka com.example.r18:drawable/nope) not found.",
            ),
            at(
                Error,
                file,
                17,
                None,
                "attribute android:bogusAttribute not found.",
            ),
            bare(Error, "failed linking file resources."),
        ]);
        assert_eq!(diagnostics(fixture!("aapt2_link_errors.txt")), expected);
    }

    #[test]
    fn configuration_cache_problems_fail_the_build() {
        assert_eq!(
            diagnostics(fixture!("configuration_cache_problems.txt")),
            vec![
                bare(Error, "FAILURE: Build completed with 2 failures."),
                bare(Error, "* What went wrong:"),
                bare(Error, "* What went wrong:"),
                bare(Error, "Configuration cache problems found in this build."),
                Diag {
                    kind: Warning,
                    file: Some("app/build.gradle.kts".to_owned()),
                    line: None,
                    col: None,
                    message: "registration of listener on 'Gradle.buildFinished' is unsupported"
                        .to_owned(),
                },
                bare(
                    Warning,
                    "Task `:app:printStamp` of type `org.gradle.api.DefaultTask`: cannot serialize Gradle script object references as these are not supported with the configuration cache.",
                ),
                bare(
                    Warning,
                    "Task `:app:printStamp` of type `org.gradle.api.DefaultTask`: invocation of 'Task.project' at execution time is unsupported with the configuration cache.",
                ),
            ]
        );
    }

    #[test]
    fn configuration_cache_problems_in_warn_mode_are_warnings() {
        assert_eq!(
            diagnostics(fixture!("configuration_cache_problems_warn.txt")),
            vec![Diag {
                kind: Warning,
                file: Some("app/build.gradle.kts".to_owned()),
                line: None,
                col: None,
                message: "registration of listener on 'Gradle.buildFinished' is unsupported"
                    .to_owned(),
            }]
        );
    }

    #[test]
    fn successful_build_has_no_diagnostics() {
        assert_eq!(diagnostics(fixture!("successful_build.txt")), vec![]);
    }

    #[test]
    fn every_error_marker_line_in_the_fixtures_is_an_error() {
        const MARKERS: &[&str] = &["e: ", "ERROR: ", "error: ", "FAILURE: "];
        for (name, log) in FIXTURES {
            for raw in log.lines() {
                if MARKERS.iter().any(|m| raw.trim_start().starts_with(m)) {
                    assert_eq!(
                        parse_build_line(raw).kind,
                        Error,
                        "{name}: error line not reported: {raw}"
                    );
                }
            }
        }
    }

    #[test]
    fn unrecognised_error_lines_still_surface_without_a_location() {
        for (raw, kind, message) in [
            (
                "e: [ksp] Unable to process: no symbols",
                Error,
                "[ksp] Unable to process: no symbols",
            ),
            (
                "e: java.lang.OutOfMemoryError: Java heap space",
                Error,
                "java.lang.OutOfMemoryError: Java heap space",
            ),
            (
                "w: Language version 1.9 is deprecated",
                Warning,
                "Language version 1.9 is deprecated",
            ),
            (
                "ERROR: R8: Unexpected failure",
                Error,
                "R8: Unexpected failure",
            ),
        ] {
            assert_eq!(diagnostics(raw), vec![bare(kind, message)], "{raw}");
        }
    }
}
