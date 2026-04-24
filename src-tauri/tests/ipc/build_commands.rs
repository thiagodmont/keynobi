use keynobi_lib::models::build::{BuildLine, BuildLineKind, BuildStatus};
use keynobi_lib::services::build_parser::parse_build_line;

#[test]
fn parse_build_line_error_produces_correct_kind() {
    let line = parse_build_line(
        "e: file:///app/src/main/java/com/example/MainActivity.kt:42:8: unresolved reference: Foo",
    );
    assert_eq!(line.kind, BuildLineKind::Error);
    assert_eq!(line.line, Some(42));
}

#[test]
fn parse_build_line_task_start_produces_correct_kind() {
    let line = parse_build_line("> Task :app:assembleDebug");
    assert_eq!(line.kind, BuildLineKind::TaskStart);
}

#[test]
fn build_line_serializes_with_camel_case_kind() {
    let line = BuildLine {
        kind: BuildLineKind::TaskStart,
        content: "> Task :app:assembleDebug".to_string(),
        file: None,
        line: None,
        col: None,
    };
    let json = serde_json::to_string(&line).expect("BuildLine should serialize");
    assert!(
        json.contains("\"kind\":\"taskStart\""),
        "kind must be camelCase"
    );
}

#[test]
fn build_status_idle_serializes_with_state_tag() {
    let status = BuildStatus::Idle;
    let json = serde_json::to_string(&status).expect("BuildStatus should serialize");
    assert!(
        json.contains("\"state\":\"idle\""),
        "BuildStatus must use tag = \"state\""
    );
}
