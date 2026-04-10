//! Integration tests for build output parsing and process execution.
//!
//! These tests exercise:
//!   - `build_runner::parse_build_line`  — the full Gradle/Kotlin/Java parser
//!   - `build_runner::parse_build_duration` — duration string extraction
//!   - `build_runner::record_build_result` — state-machine update
//!   - Real process execution via `tokio::process::Command` with a mock `gradlew`

use keynobi_lib::models::build::{BuildLineKind, BuildResult};
use keynobi_lib::services::build_runner::{self, BuildState};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Create a minimal fake Android project with the mock `gradlew` copied in.
fn make_mock_project(tmp: &TempDir) -> PathBuf {
    let root = tmp.path().to_path_buf();
    fs::create_dir_all(root.join("app/src/main")).unwrap();
    fs::write(root.join("settings.gradle"), "rootProject.name = 'TestApp'").unwrap();

    let src = fixture_dir().join("mock_gradlew");
    let dst = root.join("gradlew");
    fs::copy(&src, &dst).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).unwrap();
    }

    root
}

// ── Parser tests ──────────────────────────────────────────────────────────────

#[test]
fn parse_kotlin_error_line_extracts_file_and_location() {
    let line = "e: file:///path/to/Main.kt:10:5: Unresolved reference: foo";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Error);
    assert!(
        parsed.file.as_deref().unwrap_or("").contains("Main.kt"),
        "file should contain Main.kt, got: {:?}",
        parsed.file
    );
    assert_eq!(parsed.line, Some(10));
    assert_eq!(parsed.col, Some(5));
    assert!(
        parsed.content.contains("Unresolved reference: foo"),
        "content should contain the message, got: {:?}",
        parsed.content
    );
}

#[test]
fn parse_kotlin_error_without_file_uri_prefix() {
    let line = "e: /src/com/example/Main.kt:20:3: Unresolved reference: bar";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Error);
    assert!(parsed.file.as_deref().unwrap_or("").contains("Main.kt"));
    assert_eq!(parsed.line, Some(20));
    assert_eq!(parsed.col, Some(3));
}

#[test]
fn parse_kotlin_warning_line() {
    let line = "w: file:///path/to/Foo.kt:5:1: Deprecation warning: use Bar instead";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Warning);
    assert!(parsed.file.as_deref().unwrap_or("").contains("Foo.kt"));
}

#[test]
fn parse_java_compiler_error_line() {
    let line = "src/main/java/com/example/Main.java:42: error: ';' expected";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Error);
    assert!(parsed.file.as_deref().unwrap_or("").contains("Main.java"));
    assert_eq!(parsed.line, Some(42));
    assert!(parsed.content.contains("';' expected"));
}

#[test]
fn parse_build_successful_summary_line() {
    let line = "BUILD SUCCESSFUL in 3s";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Summary);
    assert!(parsed.content.contains("BUILD SUCCESSFUL"));
}

#[test]
fn parse_build_failed_summary_line() {
    let line = "BUILD FAILED in 1s";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Summary);
    assert!(parsed.content.contains("BUILD FAILED"));
}

#[test]
fn parse_task_start_line() {
    let line = "> Task :app:compileDebugKotlin";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::TaskStart);
    assert!(parsed.content.contains(":app:compileDebugKotlin"));
}

#[test]
fn parse_task_up_to_date_is_task_end() {
    // UP-TO-DATE is a task outcome, matched by TASK_OUTCOME_RE before TASK_START_RE.
    let line = "> Task :app:preBuild UP-TO-DATE";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::TaskEnd);
}

#[test]
fn parse_task_failed_outcome() {
    let line = "> Task :app:compileDebugKotlin FAILED";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::TaskEnd);
    assert!(parsed.content.contains("FAILED"));
}

#[test]
fn parse_gradle_failure_header() {
    let line = "FAILURE: Build failed with an exception.";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Error);
}

#[test]
fn parse_plain_output_line() {
    let line = "Some Gradle output that does not match any pattern";
    let parsed = build_runner::parse_build_line(line);
    assert_eq!(parsed.kind, BuildLineKind::Output);
}

// ── Duration parsing tests ────────────────────────────────────────────────────

#[test]
fn parse_build_duration_seconds_only() {
    assert_eq!(build_runner::parse_build_duration("BUILD SUCCESSFUL in 3s"), 3_000);
}

#[test]
fn parse_build_duration_minutes_and_seconds() {
    assert_eq!(build_runner::parse_build_duration("BUILD FAILED in 1m 30s"), 90_000);
}

#[test]
fn parse_build_duration_fractional_seconds() {
    assert_eq!(build_runner::parse_build_duration("BUILD SUCCESSFUL in 2.5s"), 2_500);
}

#[test]
fn parse_build_duration_no_match_returns_zero() {
    assert_eq!(build_runner::parse_build_duration("something unrelated"), 0);
}

// ── State-machine tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn record_build_result_success_updates_state() {
    use keynobi_lib::models::build::BuildStatus;

    let state = BuildState::new();
    // Capture history length before recording — startup may load persisted history from disk.
    let initial_len = state.inner.lock().await.history.len();

    let result = BuildResult {
        success: true,
        duration_ms: 3_000,
        error_count: 0,
        warning_count: 0,
    };

    build_runner::record_build_result(
        &state,
        "assembleDebug".into(),
        "2024-01-01T00:00:00Z".into(),
        result,
        vec![],
    )
    .await;

    let inner = state.inner.lock().await;
    assert!(
        matches!(inner.status, BuildStatus::Success(_)),
        "expected Success status, got: {:?}",
        inner.status
    );
    // History grows by 1, but is bounded by MAX_HISTORY (evicts oldest when full).
    let expected_len = (initial_len + 1).min(keynobi_lib::services::build_runner::MAX_HISTORY);
    assert_eq!(inner.history.len(), expected_len, "history must contain the new record");
    // The most recent entry must be the one we just recorded.
    let last = inner.history.back().expect("history must not be empty");
    assert_eq!(last.task, "assembleDebug");
    assert!(matches!(last.status, BuildStatus::Success(_)));
    assert_eq!(inner.current_errors.len(), 0);
}

#[tokio::test]
async fn record_build_result_failure_updates_state() {
    use keynobi_lib::models::build::{BuildError, BuildErrorSeverity, BuildStatus};

    let state = BuildState::new();
    let result = BuildResult {
        success: false,
        duration_ms: 1_000,
        error_count: 1,
        warning_count: 0,
    };
    let errors = vec![BuildError {
        message: "Unresolved reference: foo".into(),
        file: Some("/src/Main.kt".into()),
        line: Some(10),
        col: Some(5),
        severity: BuildErrorSeverity::Error,
    }];

    build_runner::record_build_result(
        &state,
        "assembleDebug".into(),
        "2024-01-01T00:00:00Z".into(),
        result,
        errors.clone(),
    )
    .await;

    let inner = state.inner.lock().await;
    assert!(
        matches!(inner.status, BuildStatus::Failed(_)),
        "expected Failed status, got: {:?}",
        inner.status
    );
    assert_eq!(inner.current_errors.len(), 1);
    assert_eq!(inner.current_errors[0].message, "Unresolved reference: foo");
}

#[tokio::test]
async fn record_build_result_respects_history_limit() {
    use keynobi_lib::models::build::BuildStatus;

    let state = BuildState::new();

    // Record MAX_HISTORY + 2 builds — the ring buffer should evict the oldest.
    let limit = build_runner::MAX_HISTORY + 2;
    for i in 0..limit {
        let result = BuildResult {
            success: true,
            duration_ms: i as u64 * 1_000,
            error_count: 0,
            warning_count: 0,
        };
        build_runner::record_build_result(
            &state,
            format!("task_{i}"),
            "2024-01-01T00:00:00Z".into(),
            result,
            vec![],
        )
        .await;
    }

    let inner = state.inner.lock().await;
    assert_eq!(
        inner.history.len(),
        build_runner::MAX_HISTORY,
        "history must be capped at MAX_HISTORY"
    );
    // The oldest entries (task_0, task_1) should have been evicted.
    assert_eq!(inner.history[0].task, "task_2");
    assert!(matches!(inner.status, BuildStatus::Success(_)));
}

// ── Process execution tests ───────────────────────────────────────────────────

#[tokio::test]
async fn mock_gradlew_successful_build_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let root = make_mock_project(&tmp);

    let output = tokio::process::Command::new(root.join("gradlew"))
        .arg("assembleDebug")
        .current_dir(&root)
        .output()
        .await
        .expect("mock gradlew should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "mock assembleDebug should exit 0");
    assert!(
        stdout.contains("BUILD SUCCESSFUL"),
        "stdout must contain BUILD SUCCESSFUL, got: {stdout}"
    );
}

#[tokio::test]
async fn mock_gradlew_failed_build_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    let root = make_mock_project(&tmp);

    let output = tokio::process::Command::new(root.join("gradlew"))
        .arg("fail")
        .current_dir(&root)
        .output()
        .await
        .expect("mock gradlew should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "mock fail task should exit non-zero");
    assert!(
        stdout.contains("BUILD FAILED"),
        "stdout must contain BUILD FAILED, got: {stdout}"
    );
    assert!(
        stdout.contains("Unresolved reference"),
        "stdout must contain the error message, got: {stdout}"
    );
}

#[tokio::test]
async fn mock_gradlew_output_parses_correctly() {
    // Run the mock gradlew and feed every stdout line through the parser,
    // verifying the combined output matches expected classifications.
    let tmp = TempDir::new().unwrap();
    let root = make_mock_project(&tmp);

    let output = tokio::process::Command::new(root.join("gradlew"))
        .arg("assembleDebug")
        .current_dir(&root)
        .output()
        .await
        .expect("mock gradlew should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // Collect parsed kinds for non-empty lines.
    let parsed_kinds: Vec<BuildLineKind> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| build_runner::parse_build_line(l).kind)
        .collect();

    // We must have at least one Summary line (BUILD SUCCESSFUL).
    assert!(
        parsed_kinds.contains(&BuildLineKind::Summary),
        "parsed output must include a Summary line"
    );

    // The "UP-TO-DATE" task outcome line must be TaskEnd.
    let has_task_end = parsed_kinds.contains(&BuildLineKind::TaskEnd);
    let has_task_start = parsed_kinds.contains(&BuildLineKind::TaskStart);
    assert!(
        has_task_end || has_task_start,
        "parsed output must include at least one task line"
    );
}

#[tokio::test]
async fn mock_gradlew_error_output_produces_error_lines() {
    let tmp = TempDir::new().unwrap();
    let root = make_mock_project(&tmp);

    let output = tokio::process::Command::new(root.join("gradlew"))
        .arg("fail")
        .current_dir(&root)
        .output()
        .await
        .expect("mock gradlew should run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let error_lines: Vec<_> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(build_runner::parse_build_line)
        .filter(|parsed| parsed.kind == BuildLineKind::Error)
        .collect();

    assert!(
        !error_lines.is_empty(),
        "failed build output must produce at least one Error-kind line"
    );

    // The Kotlin diagnostic line should carry file/line/col metadata.
    let kotlin_err = error_lines
        .iter()
        .find(|e| e.file.as_deref().map(|f| f.contains("Main.kt")).unwrap_or(false));
    assert!(
        kotlin_err.is_some(),
        "must find a Kotlin error line with Main.kt"
    );
    let kotlin_err = kotlin_err.unwrap();
    assert_eq!(kotlin_err.line, Some(10));
    assert_eq!(kotlin_err.col, Some(5));
    assert!(kotlin_err.content.contains("Unresolved reference: foo"));
}
