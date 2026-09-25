use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kind of a single line of build output.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum BuildLineKind {
    /// Plain stdout/stderr line from Gradle.
    Output,
    /// Compiler error line with an optional file location.
    Error,
    /// Compiler warning line with an optional file location.
    Warning,
    /// Informational progress line (downloads, configuration, etc.).
    Info,
    /// Gradle task progress line (e.g. `> Task :app:compileDebugKotlin`).
    TaskStart,
    /// Gradle task outcome line (e.g. `> Task :app:compileDebugKotlin FAILED`).
    TaskEnd,
    /// Final BUILD SUCCESSFUL / BUILD FAILED summary line.
    Summary,
}

/// A single parsed line of build output, streamed from Rust to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildLine {
    pub kind: BuildLineKind,
    /// Raw content of the line (may include ANSI escape codes).
    pub content: String,
    /// Source file path for error/warning lines.
    pub file: Option<String>,
    /// 1-based line number for error/warning lines.
    pub line: Option<u32>,
    /// 1-based column number for error/warning lines.
    pub col: Option<u32>,
}

impl BuildLine {
    pub fn output(content: impl Into<String>) -> Self {
        Self {
            kind: BuildLineKind::Output,
            content: content.into(),
            file: None,
            line: None,
            col: None,
        }
    }
}

/// Severity of a build diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum BuildErrorSeverity {
    Error,
    Warning,
}

/// A structured build error or warning with an optional location reference.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildError {
    pub message: String,
    /// Source file path, if known (None for dependency/configuration errors).
    pub file: Option<String>,
    /// 1-based line number, if known.
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub severity: BuildErrorSeverity,
}

/// Summary of a completed build.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildResult {
    pub success: bool,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
}

/// Current status of the build system.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "state")]
#[ts(export, export_to = "../../src/bindings/")]
#[derive(Default)]
pub enum BuildStatus {
    /// No build running or queued.
    #[default]
    Idle,
    /// A build is currently executing.
    Running { task: String, started_at: String },
    /// Last build completed successfully.
    Success(BuildResult),
    /// Last build failed.
    Failed(BuildResult),
    /// Build was cancelled by the user.
    Cancelled,
}

/// Who started or cancelled a build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum BuildActor {
    /// The user, in the Keynobi app.
    App,
    /// Keynobi quitting. Only ever a canceller.
    AppQuit,
    /// An MCP client.
    Agent(AgentActor),
}

/// The MCP client behind a [`BuildActor::Agent`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AgentActor {
    /// The app's id for an attached session; `None` for a standalone server.
    pub session_id: Option<u32>,
    /// The client's name from its `initialize` request, when known.
    pub client_name: Option<String>,
    /// A standalone `keynobi --mcp` process rather than a session attached to the app.
    pub standalone: bool,
}

/// A record of a past build kept in the build history ring-buffer.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildRecord {
    pub id: u32,
    pub task: String,
    pub status: BuildStatus,
    pub errors: Vec<BuildError>,
    pub started_at: String,
    /// Absolute path of the project root at the time the build ran.
    /// Used to scope history to the currently active project.
    pub project_root: Option<String>,
    /// Who started the build; `None` in records written before this was kept.
    #[serde(default)]
    pub origin: Option<BuildActor>,
    /// Who cancelled the build, when it was cancelled.
    #[serde(default)]
    pub cancelled_by: Option<BuildActor>,
}

/// Payload of `build:started`, emitted for every build the app's process runs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildStartedEvent {
    /// The run's ID (its Gradle process ID), shared by `build:lines` and `build:complete`.
    pub run_id: u32,
    pub task: String,
    pub origin: BuildActor,
    pub started_at: String,
    pub project_root: Option<String>,
}

/// Payload of `build:lines`: output of one run, batched.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildLinesEvent {
    pub run_id: u32,
    pub lines: Vec<BuildLine>,
}

/// Payload of `build:complete`, emitted when a build finishes, fails, or is cancelled.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildCompleteEvent {
    /// The run this event belongs to (the Gradle process ID the build started with).
    pub run_id: u32,
    pub success: bool,
    pub cancelled: bool,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub task: String,
    pub origin: Option<BuildActor>,
    pub cancelled_by: Option<BuildActor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_line_serializes() {
        let line = BuildLine {
            kind: BuildLineKind::Error,
            content: "e: /src/Main.kt:10:5: Unresolved reference: foo".into(),
            file: Some("/src/Main.kt".into()),
            line: Some(10),
            col: Some(5),
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Main.kt"));
    }

    /// History files written before origins were kept must still load.
    #[test]
    fn a_record_saved_before_origins_were_kept_still_loads() {
        let old = r#"{"id":3,"task":"assembleDebug","status":{"state":"cancelled"},
            "errors":[],"startedAt":"2026-01-01T00:00:00Z","projectRoot":"/p"}"#;
        let record: BuildRecord = serde_json::from_str(old).unwrap();
        assert_eq!(record.id, 3);
        assert_eq!(record.origin, None);
        assert_eq!(record.cancelled_by, None);
    }

    #[test]
    fn actors_serialize_with_a_kind_tag() {
        let agent = BuildActor::Agent(AgentActor {
            session_id: Some(2),
            client_name: Some("Claude Code".into()),
            standalone: false,
        });
        assert_eq!(
            serde_json::to_value(&agent).unwrap(),
            serde_json::json!({
                "kind": "agent", "sessionId": 2, "clientName": "Claude Code", "standalone": false
            })
        );
        assert_eq!(
            serde_json::to_value(BuildActor::AppQuit).unwrap(),
            serde_json::json!({ "kind": "appQuit" })
        );
        let back: BuildActor =
            serde_json::from_value(serde_json::to_value(&agent).unwrap()).unwrap();
        assert_eq!(back, agent);
    }

    #[test]
    fn build_status_default_is_idle() {
        let status = BuildStatus::default();
        assert!(matches!(status, BuildStatus::Idle));
    }
}
