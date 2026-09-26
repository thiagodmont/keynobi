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
    /// How long the app took to launch when Run App installed this build's APK.
    #[serde(default)]
    pub launch: Option<LaunchTiming>,
    /// The R8 mappings this build wrote, as saved in the data directory. Empty
    /// for builds that wrote none and for records saved before mappings were kept.
    #[serde(default)]
    pub mappings: Vec<MappingSnapshot>,
    /// The APKs this build wrote, hashed when it finished, so an install of
    /// one can be traced back to this build. Empty for builds that wrote none
    /// and for records saved before APKs were hashed.
    #[serde(default)]
    pub apks: Vec<BuiltApk>,
}

/// One APK a successful build wrote, as listed in AGP's `output-metadata.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuiltApk {
    /// Gradle path of the application module (`:app`; `:` for the root project).
    pub module: String,
    /// The variant AGP names in `output-metadata.json` (`debug`, `paidRelease`).
    pub variant: String,
    /// The variant's application ID, `applicationIdSuffix` included.
    pub application_id: Option<String>,
    pub version_code: Option<u32>,
    /// SHA-256 of the APK, lowercase hex.
    pub sha256: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// Relative to the Gradle root.
    pub path: String,
}

/// The APK Keynobi last installed on one device for one package, and the
/// build that produced it. Saved in `installed-builds.json` in the data
/// directory, so it outlives the build's history record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct InstalledBuild {
    /// The adb serial the APK was installed on.
    pub serial: String,
    /// The emulator's AVD name, when it reported one. Emulator serials are
    /// reused, so an emulator is identified by this, not by its serial.
    pub avd_name: Option<String>,
    /// The device model, for display only.
    pub model: Option<String>,
    /// The package the APK installs.
    pub package: String,
    /// SHA-256 of the installed APK, lowercase hex.
    pub apk_sha256: String,
    /// The history record of the build that wrote this APK; `None` when no
    /// recorded build did (for example an APK another tool built).
    pub build_id: Option<u32>,
    pub version_code: Option<u32>,
    /// The saved R8 mappings of the APK's module and variant, copied from the
    /// build's record. Kept while this entry names them.
    pub mappings: Vec<MappingSnapshot>,
    /// RFC 3339.
    pub installed_at: String,
}

/// A copy of one R8 `mapping.txt` a build wrote, saved as
/// `mappings/<sha256>.txt` in the data directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MappingSnapshot {
    /// Gradle path of the application module (`:app`; `:` for the root project).
    pub module: String,
    /// The directory AGP wrote the mapping to, named after the variant
    /// (`release`, `paidRelease`).
    pub variant: String,
    /// SHA-256 of the mapping, lowercase hex. Names the saved copy.
    pub sha256: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// The `# pg_map_id:` header, when the mapping has one.
    pub pg_map_id: Option<String>,
}

/// How Android started an activity, as `am start -W` reports it (Android 10+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum LaunchState {
    /// A new process was started.
    Cold,
    /// The process was running; the activity was created.
    Warm,
    /// The activity was brought back to the front.
    Hot,
    /// The activity was recreated (for example after a configuration change).
    Relaunch,
}

/// How long an activity launch took, measured with `am start -W`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LaunchTiming {
    /// `TotalTime`: from the start request until the activity drew its first frame.
    pub total_ms: u32,
    /// `WaitTime`: how long `am start` waited, including its own overhead.
    pub wait_ms: Option<u32>,
    /// `None` when the device did not report it (before Android 10, or `UNKNOWN`).
    pub launch_state: Option<LaunchState>,
    /// When the launch finished (RFC 3339).
    pub measured_at: String,
    /// ADB serial of the device the app launched on.
    pub serial: String,
    /// For an emulator, its AVD name, which identifies it across serials.
    pub avd_name: Option<String>,
    /// Device model, for display.
    pub model: Option<String>,
    /// Time to initial display, from the logcat `Displayed` line; `None`
    /// when this process's logcat stream did not show it for this launch.
    #[serde(default)]
    pub displayed_ms: Option<u32>,
    /// Time to full display, from the logcat `Fully drawn` line the app's
    /// `reportFullyDrawn` produces; `None` when none arrived in time.
    #[serde(default)]
    pub fully_drawn_ms: Option<u32>,
}

/// Payload of `build:launch_timing`: display times that arrived after
/// `launch_app_on_device` returned, now recorded on the build.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LaunchTimingEvent {
    /// The build history record the launch belongs to.
    pub record_id: u32,
    pub launch: LaunchTiming,
}

/// Result of `launch_app_on_device`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LaunchResult {
    /// What `am start` (or its fallback) printed.
    pub output: String,
    /// `None` when the launch method reports no timing (the monkey and intent fallbacks).
    pub timing: Option<LaunchTiming>,
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
    /// ID of the history record the run was saved as.
    pub record_id: u32,
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
        assert_eq!(record.launch, None);
        assert!(record.mappings.is_empty());
        assert!(record.apks.is_empty());
    }

    #[test]
    fn a_record_keeps_its_apks_through_json() {
        let json = r#"{"id":4,"task":"assembleDebug","status":{"state":"cancelled"},
            "errors":[],"startedAt":"2026-01-01T00:00:00Z","projectRoot":"/p",
            "apks":[{"module":":app","variant":"debug","applicationId":"com.example.debug",
            "versionCode":7,"sha256":"ab","bytes":12,
            "path":"app/build/outputs/apk/debug/app-debug.apk"}]}"#;
        let record: BuildRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.apks.len(), 1);
        assert_eq!(record.apks[0].version_code, Some(7));
        let back: BuildRecord =
            serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
        assert_eq!(back.apks, record.apks);
    }

    #[test]
    fn a_record_keeps_its_mappings_through_json() {
        let json = r#"{"id":4,"task":"assembleRelease","status":{"state":"cancelled"},
            "errors":[],"startedAt":"2026-01-01T00:00:00Z","projectRoot":"/p",
            "mappings":[{"module":":app","variant":"release","sha256":"ab","bytes":12,
            "pgMapId":"6b1c2f0"},{"module":":app","variant":"paidRelease","sha256":"cd",
            "bytes":3,"pgMapId":null}]}"#;
        let record: BuildRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.mappings.len(), 2);
        assert_eq!(record.mappings[0].pg_map_id.as_deref(), Some("6b1c2f0"));
        assert_eq!(record.mappings[1].pg_map_id, None);
        let back: BuildRecord =
            serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
        assert_eq!(back.mappings, record.mappings);
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
