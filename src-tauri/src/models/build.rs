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
        Self { kind: BuildLineKind::Output, content: content.into(), file: None, line: None, col: None }
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

/// A structured build error or warning with a location reference.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildError {
    pub message: String,
    pub file: String,
    pub line: u32,
    pub col: Option<u32>,
    pub severity: BuildErrorSeverity,
}

/// Summary of a completed build.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildResult {
    pub success: bool,
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
}

/// Current status of the build system.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "state")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum BuildStatus {
    /// No build running or queued.
    Idle,
    /// A build is currently executing.
    Running {
        task: String,
        started_at: String,
    },
    /// Last build completed successfully.
    Success(BuildResult),
    /// Last build failed.
    Failed(BuildResult),
    /// Build was cancelled by the user.
    Cancelled,
}

impl Default for BuildStatus {
    fn default() -> Self {
        Self::Idle
    }
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

    #[test]
    fn build_status_default_is_idle() {
        let status = BuildStatus::default();
        assert!(matches!(status, BuildStatus::Idle));
    }
}
