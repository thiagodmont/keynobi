/**
 * MCP Server for Keynobi
 *
 * Exposes build, logcat, device, UI hierarchy, UI automation (tap/type/swipe/keys), and project tools to Claude Code (or any
 * MCP-compatible client) via the Model Context Protocol (2025-11-25 spec).
 *
 * Two modes:
 *   - Attached: the running app serves the session over its socket (see
 *     `mcp_attach`) using its own managed state; `keynobi --mcp` only relays
 *     the client's stdio.
 *   - Standalone: `keynobi --mcp` could not attach, so it serves stdio itself
 *     with fresh state that the app does not see.
 *
 * Transport: newline-delimited JSON-RPC 2.0.
 *
 * Setup: `claude mcp add --scope user --transport stdio keynobi -- "/path/to/keynobi" --mcp`
 */
use crate::services::adb_manager::{self, DeviceState};
use crate::services::agent_skill;
use crate::services::android_cli;
use crate::services::app_exit_info;
use crate::services::app_inspector;
use crate::services::build_inspector;
use crate::services::build_runner::{self, AgentActor, BuildActor, BuildState};
use crate::services::crash_inspector;
use crate::services::device_inspector;
use crate::services::gradle_modules;
use crate::services::health_inspector;
use crate::services::installed_builds;
use crate::services::jdk;
use crate::services::logcat::{self, LogcatFilter, LogcatState};
use crate::services::mcp_activity::{self, McpActivityEntry};
use crate::services::mcp_sessions::McpSessionRegistry;
use crate::services::process_manager::ProcessManager;
use crate::services::project_trust;
use crate::services::retrace;
use crate::services::settings_manager;
use crate::services::ui_automation;
use crate::services::ui_hierarchy;
use crate::services::ui_hierarchy_parse;
use crate::services::variant_manager;
use crate::FsState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rmcp::{
    handler::server::{
        router::prompt::PromptRouter, router::tool::ToolRouter, wrapper::Parameters,
    },
    model::*,
    prompt, prompt_handler, prompt_router, schemars,
    service::RequestContext,
    tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tracing::info;

// ── Server struct ─────────────────────────────────────────────────────────────

/// How the server chose its project, reported by `get_project_info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSelection {
    /// The project open in the Keynobi app (the app's own server).
    App,
    /// `--project <path>`.
    Argument,
    /// The Gradle build containing the working directory.
    WorkingDirectory,
    /// The project last active in the Keynobi app.
    LastActiveProject,
}

/// Whether a session shares the app's state, reported by `get_project_info`,
/// the build tools, and `initialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMode {
    /// Served by the running app over its socket, on the app's own state.
    Attached {
        /// The project the client asked for; `None` follows the app's project.
        pinned_project: Option<PathBuf>,
    },
    /// A `keynobi --mcp` process with its own state, which the app does not see.
    Standalone {
        /// Why it could not attach.
        reason: String,
    },
}

impl SessionMode {
    fn name(&self) -> &'static str {
        match self {
            SessionMode::Attached { .. } => "attached",
            SessionMode::Standalone { .. } => "standalone",
        }
    }

    fn standalone_reason(&self) -> Option<&str> {
        match self {
            SessionMode::Standalone { reason } => Some(reason),
            SessionMode::Attached { .. } => None,
        }
    }

    /// One line for build results and the activity log.
    fn summary(&self) -> String {
        match self {
            SessionMode::Attached { .. } => {
                "mode: attached — shared with the Keynobi app".to_string()
            }
            SessionMode::Standalone { reason } => {
                format!("mode: standalone ({reason}) — not visible in the Keynobi app")
            }
        }
    }
}

/// Tools that never read or act on the open project, so they keep working in
/// a pinned session after the app switched projects. Every other tool is
/// refused in that case.
const PROJECT_INDEPENDENT_TOOLS: &[&str] = &[
    "get_project_info",
    "run_health_check",
    "start_logcat",
    "stop_logcat",
    "clear_logcat",
    "get_logcat_entries",
    "get_logcat_stats",
    "get_crash_logs",
    "get_crash_stack_trace",
    "list_devices",
    "get_device_info",
    "screenshot",
    "dump_app_info",
    "get_memory_info",
    "get_app_runtime_state",
    "launch_app",
    "list_avds",
    "launch_avd",
    "stop_avd",
    "get_ui_hierarchy",
    "list_clickable_elements",
    "find_ui_elements",
    "find_ui_parent",
    "compare_ui_state",
    "wait_for_element",
    "ui_wait_for_idle",
    "ui_assert_element",
    "ui_tap",
    "ui_tap_element",
    "ui_type_text",
    "ui_fill_input",
    "ui_type_text_unicode",
    "clear_focused_input",
    "hide_soft_keyboard",
    "send_ui_key",
    "ui_swipe",
    "ui_scroll_until_element",
    "open_deep_link",
    "open_app_settings",
    "set_device_orientation",
    "set_network_state",
];

/// Holds references to all app state needed by MCP tools.
///
/// All state structs are backed by `Arc<Mutex<>>` internally, so `Clone` here
/// just copies the Arc pointers — all clones share the same underlying data.
#[derive(Clone)]
pub struct AndroidMcpServer {
    build_state: BuildState,
    device_state: DeviceState,
    logcat_state: LogcatState,
    fs_state: FsState,
    process_manager: ProcessManager,
    /// Present when the app serves the session; used for GUI events and logcat streaming.
    app_handle: Option<AppHandle>,
    /// How the project was chosen; `None` when no project was found.
    project_selection: Option<ProjectSelection>,
    mode: SessionMode,
    /// The app's id for this session, when attached.
    session_id: Option<u32>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl AndroidMcpServer {
    /// Construct from the app's managed state, for a session attached to the app.
    pub fn from_app_handle(app: &AppHandle) -> Self {
        let build_state = app.state::<BuildState>().inner().clone();
        let device_state = app.state::<DeviceState>().inner().clone();
        let logcat_state = app.state::<LogcatState>().inner().clone();
        let fs_state = app.state::<FsState>().inner().clone();
        let process_manager = app.state::<ProcessManager>().inner().clone();
        Self {
            build_state,
            device_state,
            logcat_state,
            fs_state,
            process_manager,
            app_handle: Some(app.clone()),
            project_selection: Some(ProjectSelection::App),
            mode: SessionMode::Attached {
                pinned_project: None,
            },
            session_id: None,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Construct over state the caller owns (standalone `--mcp`, and tests).
    pub fn new_headless(
        build_state: BuildState,
        device_state: DeviceState,
        logcat_state: LogcatState,
        fs_state: FsState,
        process_manager: ProcessManager,
        project_selection: Option<ProjectSelection>,
    ) -> Self {
        Self {
            build_state,
            device_state,
            logcat_state,
            fs_state,
            process_manager,
            app_handle: None,
            project_selection,
            mode: SessionMode::Standalone {
                reason: "not attached to the Keynobi app".into(),
            },
            session_id: None,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Serve this server's state to a client attached over the app's socket.
    pub fn attached(
        mut self,
        pinned_project: Option<PathBuf>,
        selection: ProjectSelection,
    ) -> Self {
        self.mode = SessionMode::Attached { pinned_project };
        self.project_selection = Some(selection);
        self
    }

    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        self.mode = mode;
        self
    }

    /// The app's registry id of the attached session this server serves.
    pub fn with_session_id(mut self, id: u32) -> Self {
        self.session_id = Some(id);
        self
    }

    /// This session as the starter or canceller of a build.
    fn agent(&self, peer: &rmcp::Peer<RoleServer>) -> BuildActor {
        BuildActor::Agent(AgentActor {
            session_id: self.session_id,
            client_name: peer.peer_info().map(|i| i.client_info.name.clone()),
            standalone: matches!(self.mode, SessionMode::Standalone { .. }),
        })
    }

    /// For a session pinned to a project the app no longer has open, the
    /// message to return instead of acting on the app's current project.
    async fn project_mismatch(&self) -> Option<String> {
        let SessionMode::Attached {
            pinned_project: Some(pinned),
        } = &self.mode
        else {
            return None;
        };
        let app = crate::services::mcp_attach::AppProject::of(&self.fs_state).await;
        if app.is(pinned) {
            return None;
        }
        let now = match app.display_path() {
            Some(open) => format!("Keynobi now has {} open", open.display()),
            None => "Keynobi now has no project open".to_string(),
        };
        Some(format!(
            "{now}; this session is for {}. Open that project in Keynobi again, or restart \
             the Keynobi MCP server in your AI client.",
            pinned.display()
        ))
    }

    /// Refuse `tool` in a pinned session whose project the app closed.
    pub async fn check_session_project(&self, tool: &str) -> Option<CallToolResult> {
        if PROJECT_INDEPENDENT_TOOLS.contains(&tool) {
            return None;
        }
        self.project_mismatch()
            .await
            .map(|msg| CallToolResult::error(vec![ContentBlock::text(msg)]))
    }
}

// ── Tool parameter types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunGradleTaskParams {
    #[schemars(description = "Gradle task name, e.g. assembleDebug or :app:assembleRelease")]
    pub task: String,
    #[schemars(
        description = "Optional build variant to activate before running, e.g. debug or release"
    )]
    pub variant: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetBuildLogParams {
    #[schemars(description = "Max log lines to return (default 200, max 2000)")]
    pub lines: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetVariantParams {
    #[schemars(description = "Build variant name to activate, e.g. debug or release")]
    pub variant: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetLogcatParams {
    #[schemars(description = "Max entries to return (default 200, max 10000)")]
    pub count: Option<usize>,
    #[schemars(description = "Minimum log level: verbose, debug, info, warn, error, fatal")]
    pub min_level: Option<String>,
    #[schemars(description = "Filter by tag substring (case-insensitive)")]
    pub tag: Option<String>,
    #[schemars(description = "Filter by message text (case-insensitive substring)")]
    pub text: Option<String>,
    #[schemars(description = "Filter by app package name (case-insensitive substring)")]
    pub package: Option<String>,
    #[schemars(description = "If true, return only crash/ANR entries")]
    pub only_crashes: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCrashLogsParams {
    #[schemars(description = "Max crash entries to return (default 20, max 200)")]
    pub count: Option<usize>,
    #[schemars(
        description = "If true, also deobfuscate the newest crashes among the entries (at most 5) with the saved R8 mapping of the build that produced them, matched by the map id in the trace or the installed APK's hash; default false"
    )]
    pub retrace: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StartLogcatParams {
    #[schemars(
        description = "ADB device serial to stream logcat from (optional, uses first connected device)"
    )]
    pub device_serial: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InstallApkParams {
    #[schemars(description = "ADB device serial, e.g. emulator-5554 (from list_devices)")]
    pub device_serial: String,
    #[schemars(description = "Absolute path to the APK file within the project build directory")]
    pub apk_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LaunchAppParams {
    #[schemars(description = "ADB device serial (from list_devices)")]
    pub device_serial: String,
    #[schemars(description = "Android package name, e.g. com.example.myapp")]
    pub package: String,
    #[schemars(description = "Optional activity name, e.g. .MainActivity")]
    pub activity: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DevicePackageParams {
    #[schemars(description = "ADB device serial")]
    pub device_serial: String,
    #[schemars(description = "Android package name")]
    pub package: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StopAppParams {
    #[schemars(description = "ADB device serial")]
    pub device_serial: String,
    #[schemars(description = "Android package name")]
    pub package: String,
    #[schemars(description = ALLOW_FOREIGN_PACKAGE_DESCRIPTION)]
    pub allow_foreign_package: Option<bool>,
}

const ALLOW_FOREIGN_PACKAGE_DESCRIPTION: &str = "Set true to act on a package that is not the open project's app (its applicationId or a variant of it). Only when the user explicitly asked for that package. Default false.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeviceSerialParams {
    #[schemars(description = "ADB device serial, e.g. emulator-5554 (from list_devices)")]
    pub device_serial: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenshotParams {
    #[schemars(description = "ADB device serial, e.g. emulator-5554 (from list_devices)")]
    pub device_serial: String,
    #[schemars(
        description = "Longest image edge in pixels (default 1280, min 256, max 8192). Larger captures are downscaled; a value at or above the screen's long edge returns it unchanged."
    )]
    pub max_dimension: Option<u32>,
    #[schemars(
        description = "Return the capture at the device's full resolution (costs more context). Do not combine with max_dimension."
    )]
    pub full_size: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LaunchAvdParams {
    #[schemars(description = "AVD name from list_avds, e.g. Pixel_8_API_35")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StopAvdParams {
    #[schemars(description = "Emulator serial from list_devices, e.g. emulator-5554")]
    pub serial: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindApkPathParams {
    #[schemars(
        description = "Build variant name, e.g. debug or release (optional, uses active variant)"
    )]
    pub variant: Option<String>,
    #[schemars(
        description = "Application module, e.g. :mobile, or the task that built it, e.g. :mobile:assembleDebug (optional; required when the project has several application modules)"
    )]
    pub module: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunTestsParams {
    #[schemars(
        description = "Test type: 'unit' (testDebug), 'connected' (connectedAndroidTest), or a specific Gradle test task"
    )]
    pub test_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCrashStackTraceParams {
    #[schemars(description = "Filter to a specific package name, e.g. com.example.app")]
    pub package: Option<String>,
    #[schemars(
        description = "Return a specific crash group by ID (from get_crash_logs crash_group_id field)"
    )]
    pub crash_group_id: Option<u64>,
    #[schemars(
        description = "If true, also deobfuscate the trace with the saved R8 mapping of the build that produced it, matched by the map id in the trace or the installed APK's hash; default false"
    )]
    pub retrace: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestartAppParams {
    #[schemars(description = "Android package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(
        description = "ADB device serial (from list_devices). Uses first connected device if omitted. Required when clear_data is true."
    )]
    pub device_serial: Option<String>,
    #[schemars(
        description = "Also wipe the app's data and runtime permissions (pm clear) before relaunching. Default false. Destructive: requires device_serial."
    )]
    pub clear_data: Option<bool>,
    #[schemars(description = ALLOW_FOREIGN_PACKAGE_DESCRIPTION)]
    pub allow_foreign_package: Option<bool>,
    /// Removed parameter (it used to default to `true` and wipe app data).
    /// Accepted only so old callers get an explicit error instead of a
    /// silently different behavior. `Some` whenever the key is present,
    /// including `"cold": null`.
    #[schemars(skip)]
    #[serde(default, deserialize_with = "deserialize_present")]
    pub cold: Option<serde_json::Value>,
}

/// Deserialize a field as `Some(value)` whenever its key is present, even when
/// the value is JSON `null` (plain `Option` would turn `null` into `None`).
fn deserialize_present<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <serde_json::Value as Deserialize>::deserialize(deserializer).map(Some)
}

/// Whether `restart_app` should clear app data. Rejects the removed `cold`
/// parameter, and data clearing without an explicitly chosen device.
fn restart_clears_data(p: &RestartAppParams) -> Result<bool, McpError> {
    if p.cold.is_some() {
        return Err(McpError::invalid_params(
            "`cold` was removed: restart_app now preserves app data by default. \
             Pass `clear_data: true` with `device_serial` to wipe data before relaunching.",
            None,
        ));
    }
    let clear_data = p.clear_data.unwrap_or(false);
    if clear_data && p.device_serial.is_none() {
        return Err(McpError::invalid_params(
            "clear_data wipes the app's data: pass device_serial explicitly (from list_devices).",
            None,
        ));
    }
    Ok(clear_data)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAppRuntimeStateParams {
    #[schemars(description = "Android package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(
        description = "ADB device serial (from list_devices). Uses first connected device if omitted."
    )]
    pub device_serial: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetExitReasonsParams {
    #[schemars(
        description = "ADB device serial (from list_devices). Uses first connected device if omitted."
    )]
    pub device_serial: Option<String>,
    #[schemars(
        description = "Android package name, e.g. com.example.app. Defaults to the open project's app (the one build of its applicationId installed on the device); required when the project has several application ids or several of its builds are installed."
    )]
    pub package: Option<String>,
    #[schemars(description = "Most recent exits to list (default 20, max 100).")]
    pub limit: Option<u32>,
}

/// Exits `get_exit_reasons` lists when the call gives no `limit`.
const DEFAULT_EXIT_REASONS: usize = 20;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetBuildConfigParams {
    #[schemars(
        description = "Gradle module name (subdirectory), e.g. app (default) or feature-login"
    )]
    pub module: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetUiHierarchyParams {
    #[schemars(
        description = "ADB device serial (from list_devices). Uses first online device if omitted."
    )]
    pub device_serial: Option<String>,
    #[schemars(
        description = "If true, return only interactive rows (bounds, text, actions) — smaller than full tree. Default false."
    )]
    pub interactive_only: Option<bool>,
    #[schemars(description = "Max rows when interactive_only is true (default 80, max 500).")]
    pub max_interactive_rows: Option<u32>,
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl AndroidMcpServer {
    // ── Build tools ───────────────────────────────────────────────────────────

    /// Run a Gradle task and wait for completion. Returns exit status + error summary.
    /// After the build, call get_build_errors for structured diagnostics.
    #[tool(
        description = "Run a Gradle task (e.g. assembleDebug) and return the result. Use get_build_errors for structured errors after the build.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn run_gradle_task(
        &self,
        Parameters(p): Parameters<RunGradleTaskParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.run_build(p.task, self.agent(&ctx.peer), Some(&ctx))
            .await
    }

    /// Get the current build status.
    #[tool(
        description = "Get the current Gradle build status: idle, running (with task name), success, failed, or cancelled, who started it (origin: the Keynobi app or an agent), and who cancelled it (cancelled_by).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_build_status(&self) -> Result<CallToolResult, McpError> {
        let bs = self.build_state.inner.lock().await;
        let (state_str, details) = match &bs.status {
            crate::models::build::BuildStatus::Idle => ("idle", json!(null)),
            crate::models::build::BuildStatus::Running { task, started_at } => {
                ("running", json!({ "task": task, "started_at": started_at }))
            }
            crate::models::build::BuildStatus::Success(r) => (
                "success",
                json!({
                    "duration_ms": r.duration_ms,
                    "error_count": r.error_count,
                    "warning_count": r.warning_count
                }),
            ),
            crate::models::build::BuildStatus::Failed(r) => (
                "failed",
                json!({
                    "duration_ms": r.duration_ms,
                    "error_count": r.error_count,
                    "warning_count": r.warning_count
                }),
            ),
            crate::models::build::BuildStatus::Cancelled => ("cancelled", json!(null)),
        };
        let summary = match &bs.status {
            crate::models::build::BuildStatus::Idle => "Build status: idle".to_owned(),
            crate::models::build::BuildStatus::Running { task, .. } => {
                format!("Build status: running — task: {task}")
            }
            crate::models::build::BuildStatus::Success(r) => format!(
                "Build status: success — {}ms, {} error(s), {} warning(s)",
                r.duration_ms, r.error_count, r.warning_count
            ),
            crate::models::build::BuildStatus::Failed(r) => format!(
                "Build status: failed — {} error(s), {} warning(s)",
                r.error_count, r.warning_count
            ),
            crate::models::build::BuildStatus::Cancelled => "Build status: cancelled".to_owned(),
        };
        Ok(CallToolResult::structured(json!({
            "status": state_str,
            "details": details,
            "summary": summary,
            "origin": bs.status_origin,
            "cancelled_by": bs.status_cancelled_by,
            "mode": self.mode.name(),
            "standalone_reason": self.mode.standalone_reason(),
        })))
    }

    /// Get structured compiler errors and warnings from the last build.
    #[tool(
        description = "Get compiler errors and warnings from the last Gradle build. Each entry includes severity, file path, line number, and message.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_build_errors(&self) -> Result<CallToolResult, McpError> {
        let bs = self.build_state.inner.lock().await;
        if bs.current_errors.is_empty() {
            return Ok(CallToolResult::structured(
                json!({ "errors": [], "count": 0 }),
            ));
        }
        let errors: Vec<serde_json::Value> = bs
            .current_errors
            .iter()
            .map(|e| {
                json!({
                    "severity": format!("{:?}", e.severity).to_lowercase(),
                    "message": e.message,
                    "file": e.file,
                    "line": e.line,
                    "col": e.col,
                })
            })
            .collect();
        Ok(CallToolResult::structured(json!({
            "count": errors.len(),
            "errors": errors
        })))
    }

    /// Get the raw build log output lines.
    #[tool(
        description = "Get the raw Gradle build output lines. Useful for diagnosing build issues not captured as structured errors.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_build_log(
        &self,
        Parameters(p): Parameters<GetBuildLogParams>,
    ) -> Result<CallToolResult, McpError> {
        let (settings_for_mcp, _) = settings_manager::load_settings();
        let mcp_settings = settings_for_mcp.mcp;
        let lines_req = p
            .lines
            .unwrap_or(mcp_settings.build_log_default_lines as usize)
            .min(2000);
        let current_log = self.build_state.build_log.current();
        let log = current_log
            .lock()
            .map_err(|_| McpError::internal_error("Lock poisoned", None))?;
        if log.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "Build log is empty. Run a build first.",
            )]));
        }
        let lines: Vec<&String> = log
            .iter()
            .rev()
            .take(lines_req)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{} log line(s):\n{}",
            lines.len(),
            lines
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        ))]))
    }

    /// Cancel a running Gradle build.
    #[tool(
        description = "Cancel the currently running Gradle build, whoever started it (the Keynobi app or an agent). Returns immediately if no build is running.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn cancel_build(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.cancel_build_as(self.agent(&ctx.peer)).await
    }

    /// List available build variants.
    #[tool(
        description = "List available build variants (build types + product flavors) for the current Android project, which variant Keynobi treats as the Gradle/Android Studio default (`defaultVariant`), and which one is persisted as active in settings (`active`).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_build_variants(&self) -> Result<CallToolResult, McpError> {
        let gradle_root = match self.get_gradle_root().await {
            Some(r) => r,
            None => {
                return Ok(CallToolResult::structured(json!({
                    "variants": [],
                    "active": null,
                    "defaultVariant": null,
                    "error": "No project open"
                })))
            }
        };
        let candidates = match gradle_modules::application_build_files(&gradle_root) {
            Ok(files) => files,
            Err(e) => {
                return Ok(CallToolResult::structured(json!({
                    "variants": [],
                    "active": null,
                    "defaultVariant": null,
                    "error": e
                })))
            }
        };
        for path in &candidates {
            if path.is_file() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(mut list) =
                        variant_manager::parse_variants_from_gradle(path, &content)
                    {
                        if !list.variants.is_empty() {
                            list.default_variant = variant_manager::infer_default_variant_name(
                                &gradle_root,
                                &list.variants,
                            );
                            let names: Vec<&str> =
                                list.variants.iter().map(|v| v.name.as_str()).collect();
                            let active = self.get_gradle_root().await.and_then(|r| {
                                settings_manager::get_active_variant_for_project(
                                    &r.to_string_lossy(),
                                )
                            });
                            return Ok(CallToolResult::structured(json!({
                                "active": active,
                                "defaultVariant": list.default_variant,
                                "variants": names,
                            })));
                        }
                    }
                }
            }
        }
        Ok(CallToolResult::structured(json!({
            "variants": [],
            "active": null,
            "defaultVariant": null,
            "error": "Could not parse build variants. Ensure a project is open and build.gradle.kts exists."
        })))
    }

    /// Set the active build variant.
    #[tool(
        description = "Set the active build variant (e.g. debug or release). This persists in settings and affects subsequent builds.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn set_active_variant(
        &self,
        Parameters(p): Parameters<SetVariantParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(root) = self.get_gradle_root().await {
            let path = root.to_string_lossy().to_string();
            if let Err(e) = settings_manager::set_active_variant_for_project(&path, &p.variant) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Failed to persist active variant: {e}"
                ))]));
            }
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Active variant set to: {}",
            p.variant
        ))]))
    }

    /// Find the output APK path for a given build variant.
    #[tool(
        description = "Find the output APK path after a successful build. Returns the path to use with install_apk. Specify variant or uses the active one.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn find_apk_path(
        &self,
        Parameters(p): Parameters<FindApkPathParams>,
    ) -> Result<CallToolResult, McpError> {
        let gradle_root = self
            .get_gradle_root()
            .await
            .ok_or_else(|| McpError::invalid_params("No project open", None))?;

        let persisted_variant =
            settings_manager::get_active_variant_for_project(&gradle_root.to_string_lossy());
        let variant = resolve_variant(p.variant.as_deref(), persisted_variant.as_deref());
        let variant = variant.as_str();
        if let Some(module) = p.module.as_deref() {
            validate_gradle_task(module)?;
        }

        match build_runner::find_output_apk(&gradle_root, p.module.as_deref(), variant) {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_string();
                Ok(CallToolResult::structured(json!({
                    "found": true,
                    "path": path_str,
                    "variant": variant,
                    "hint": format!("Use install_apk with device_serial and apk_path: {}", path_str)
                })))
            }
            Err(reason) => Ok(CallToolResult::structured(json!({
                "found": false,
                "variant": variant,
                "reason": reason,
                "hint": "Run a build for this variant first with run_gradle_task (e.g. assembleDebug)"
            }))),
        }
    }

    /// Run tests for the project.
    #[tool(
        description = "Run unit tests or connected Android tests. test_type: 'unit' (testDebug), 'connected' (connectedAndroidTest), or a specific Gradle test task.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn run_tests(
        &self,
        Parameters(p): Parameters<RunTestsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.run_build(test_task(&p.test_type)?, self.agent(&ctx.peer), Some(&ctx))
            .await
    }

    /// Get a parsed crash stack trace from the in-memory logcat buffer.
    /// Requires logcat to be running (call start_logcat first).
    #[tool(
        description = "Get a parsed crash stack trace from logcat. Returns exception type, message, stack frames, and caused-by chain. Requires start_logcat to be running. With retrace: true, also returns `retrace`: the trace deobfuscated with the saved R8 mapping of the build that produced it, and a mapping_line naming the build, variant, and map id and how the mapping was matched (matched_by: map_id from the trace, device_hash of the installed APK, or install_record); when no mapping can be identified with certainty, or retrace or a JDK 17+ is missing, it returns the original trace and the reason.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_crash_stack_trace(
        &self,
        Parameters(p): Parameters<GetCrashStackTraceParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref pkg) = p.package {
            validate_package_name(pkg)?;
        }

        let logcat = self.logcat_state.lock().await;

        if !logcat.streaming && logcat.store.is_empty() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "Logcat not running — call start_logcat first.",
            )]));
        }

        let entries: Vec<_> = logcat
            .store
            .iter()
            .filter(|e| e.crash_group_id.is_some())
            .cloned()
            .collect();
        drop(logcat);

        match crash_inspector::find_crash(&entries, p.package.as_deref(), p.crash_group_id) {
            None => {
                let msg = if let Some(pkg) = &p.package {
                    format!("No crashes found for package '{pkg}'.")
                } else {
                    "No crashes found in the logcat buffer.".to_string()
                };
                Ok(CallToolResult::structured(
                    json!({ "found": false, "message": msg }),
                ))
            }
            Some(crash) if p.retrace.unwrap_or(false) => {
                let outcome = self.retrace_crash_group(crash.crash_group_id).await;
                let mut result = json!(crash);
                result["retrace"] = outcome;
                Ok(CallToolResult::structured(result))
            }
            Some(crash) => Ok(CallToolResult::structured(json!(crash))),
        }
    }

    /// Restart an Android app: stop it (optionally clearing data), then relaunch and wait for display.
    #[tool(
        description = "Restart an Android app: force-stop, then relaunch and wait for the activity to display. Returns launch time (total_time_ms, wait_time_ms, launch_state from am start -W, and display_time_ms). App data is preserved unless clear_data is true (runs pm clear; requires device_serial). Only the open project's app (applicationId or a variant) unless allow_foreign_package is true.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn restart_app(
        &self,
        Parameters(p): Parameters<RestartAppParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_package_name(&p.package)?;
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        let clear_data = restart_clears_data(&p)?;
        self.check_package_scope("restart_app", &p.package, p.allow_foreign_package)
            .await?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "No device connected. Connect a device or launch an emulator first.",
                    )]))
                }
            };

        match app_inspector::restart_app(&adb, &serial, &p.package, clear_data).await {
            Ok(result) => Ok(CallToolResult::structured(json!(result))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Get process list, thread counts, and RSS memory for all processes of an app.
    #[tool(
        description = "Get runtime state for an Android app: running processes, thread counts per process, and RSS memory. Lightweight — no SIGQUIT.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_app_runtime_state(
        &self,
        Parameters(p): Parameters<GetAppRuntimeStateParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_package_name(&p.package)?;
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let serial = adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await;

        let state = app_inspector::get_runtime_state(&adb, serial.as_deref(), &p.package).await;

        Ok(CallToolResult::structured(json!(state)))
    }

    /// Parse the module's build.gradle(.kts) for SDK levels, build types, and product flavors.
    #[tool(
        description = "Parse build.gradle(.kts) for SDK levels, applicationId, buildTypes, and productFlavors. No Gradle execution needed.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_build_config(
        &self,
        Parameters(p): Parameters<GetBuildConfigParams>,
    ) -> Result<CallToolResult, McpError> {
        let gradle_root = self.get_gradle_root().await.ok_or_else(|| {
            McpError::invalid_params("No project open. Open an Android project first.", None)
        })?;

        let default_module;
        let module = match p.module.as_deref() {
            Some(module) => module,
            None => match gradle_modules::resolve_application_module(&gradle_root, None) {
                Ok(m) => {
                    default_module = m.relative_dir(&gradle_root);
                    default_module.as_str()
                }
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            },
        };

        if p.module.is_some()
            && (module.contains('/') || module.contains('\\') || module.contains(".."))
        {
            return Err(McpError::invalid_params(
                "Module name must be a simple directory name, not a path.",
                None,
            ));
        }

        match build_inspector::parse_build_config(&gradle_root, module) {
            Ok(config) => Ok(CallToolResult::structured(json!(config))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    // ── Logcat tools ──────────────────────────────────────────────────────────

    /// Start streaming logcat from a device.
    #[tool(
        description = "Start streaming logcat from a device. Required before get_logcat_entries unless the stream is already running (a session attached to the Keynobi app shares the app's logcat).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn start_logcat(
        &self,
        Parameters(p): Parameters<StartLogcatParams>,
    ) -> Result<CallToolResult, McpError> {
        let serial = p.device_serial.clone();
        if let Some(ref s) = serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb_bin =
            crate::services::logcat::find_adb_binary(settings.android.sdk_path.as_deref());

        match logcat::request_start(&self.logcat_state, adb_bin, serial, self.app_handle.clone())
            .await
        {
            Ok(logcat::StartOutcome::AlreadyStreaming) => {
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "Logcat is already streaming.",
                )]))
            }
            Ok(logcat::StartOutcome::Started) => {
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "Logcat streaming started. Use get_logcat_entries to read entries.",
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Stop the logcat stream.
    #[tool(
        description = "Stop the logcat stream. Use start_logcat to restart.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn stop_logcat(&self) -> Result<CallToolResult, McpError> {
        logcat::request_stop(&self.logcat_state).await;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "Logcat stream stopped.",
        )]))
    }

    /// Get recent logcat entries with optional filtering.
    #[tool(
        description = "Get recent Android logcat entries. Filter by level, tag, text, package, or show only crashes. Call start_logcat first if the stream is not running.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_logcat_entries(
        &self,
        Parameters(p): Parameters<GetLogcatParams>,
    ) -> Result<CallToolResult, McpError> {
        let (settings_for_logcat, _) = settings_manager::load_settings();
        let mcp_settings = settings_for_logcat.mcp;
        let count = p
            .count
            .unwrap_or(mcp_settings.logcat_default_count as usize)
            .min(10_000);
        let only_crashes = p.only_crashes.unwrap_or(false);
        let min_level = p.min_level.as_deref().map(logcat::parse_level_str);
        let filter = LogcatFilter::new(min_level, p.tag, p.text, p.package, only_crashes);

        let logcat = self.logcat_state.lock().await;
        let entries = logcat.store.query(&filter, count);

        if entries.is_empty() {
            let streaming = logcat.streaming;
            let msg = if streaming {
                "No logcat entries matching the filter."
            } else {
                "No logcat entries. Call start_logcat first."
            };
            return Ok(CallToolResult::structured(
                json!({ "entries": [], "count": 0, "streaming": streaming, "hint": msg }),
            ));
        }

        let structured: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                json!({
                    "timestamp": e.timestamp,
                    "level": logcat::level_char(&e.level),
                    "tag": e.tag,
                    "pid": e.pid,
                    "message": e.message,
                    "is_crash": e.is_crash,
                    "package": e.package,
                })
            })
            .collect();

        Ok(CallToolResult::structured(json!({
            "count": structured.len(),
            "streaming": logcat.streaming,
            "entries": structured
        })))
    }

    /// Get recent crash logs (FATAL EXCEPTION, ANR, native crashes).
    #[tool(
        description = "Get recent crash logs: FATAL EXCEPTION, ANR, and native crashes from logcat. With retrace: true, also returns `retraced`: the newest crashes among the entries (at most 5), each deobfuscated with the saved R8 mapping of the build that produced it, with a mapping_line naming the build, variant, and map id, or the original trace and the reason it was not deobfuscated.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_crash_logs(
        &self,
        Parameters(p): Parameters<GetCrashLogsParams>,
    ) -> Result<CallToolResult, McpError> {
        let count = p.count.unwrap_or(20).min(200);
        let logcat = self.logcat_state.lock().await;
        let newest: Vec<_> = logcat
            .store
            .iter()
            .rev()
            .filter(|e| e.is_crash)
            .take(count)
            .collect();
        // Newest first, one per crash group.
        let mut groups: Vec<u64> = Vec::new();
        for gid in newest.iter().filter_map(|e| e.crash_group_id) {
            if !groups.contains(&gid) && groups.len() < retrace::MAX_RETRACED_CRASH_GROUPS {
                groups.push(gid);
            }
        }
        let entries: Vec<serde_json::Value> = newest
            .into_iter()
            .rev()
            .map(|e| {
                json!({
                    "timestamp": e.timestamp,
                    "tag": e.tag,
                    "message": e.message,
                    "pid": e.pid,
                    "package": e.package,
                })
            })
            .collect();
        drop(logcat);

        let mut result = json!({ "count": entries.len(), "entries": entries });
        if p.retrace.unwrap_or(false) {
            let mut retraced = Vec::new();
            for gid in groups {
                let mut outcome = self.retrace_crash_group(gid).await;
                outcome["crash_group_id"] = json!(gid);
                retraced.push(outcome);
            }
            result["retraced"] = json!(retraced);
        }
        Ok(CallToolResult::structured(result))
    }

    /// Clear the in-memory logcat buffer.
    #[tool(
        description = "Clear the in-memory logcat buffer. New entries will appear after logcat continues streaming.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn clear_logcat(&self) -> Result<CallToolResult, McpError> {
        logcat::request_clear(&self.logcat_state, self.app_handle.as_ref()).await;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "Logcat buffer cleared.",
        )]))
    }

    /// Get logcat statistics.
    #[tool(
        description = "Get logcat statistics: total entries ingested, counts by level, crash count, and packages seen.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_logcat_stats(&self) -> Result<CallToolResult, McpError> {
        let state = self.logcat_state.lock().await;
        let s = &state.store.stats;
        let levels = [
            "verbose", "debug", "info", "warn", "error", "fatal", "unknown",
        ];
        let by_level: serde_json::Map<String, serde_json::Value> = levels
            .iter()
            .enumerate()
            .filter(|(i, _)| s.counts_by_level[*i] > 0)
            .map(|(i, name)| (name.to_string(), json!(s.counts_by_level[i])))
            .collect();
        Ok(CallToolResult::structured(json!({
            "total_ingested": s.total_ingested,
            "by_level": by_level,
            "crash_count": s.crash_count,
            "packages_seen": s.packages_seen,
            "streaming": state.streaming,
        })))
    }

    // ── Device tools ──────────────────────────────────────────────────────────

    /// List connected ADB devices (always queries ADB for fresh results).
    #[tool(
        description = "List all connected Android devices and running emulators. Queries ADB directly for fresh results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_devices(&self) -> Result<CallToolResult, McpError> {
        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let mut devices = adb_manager::list_devices(&adb).await;
        for d in &mut devices {
            adb_manager::enrich_device_props(&adb, d).await;
        }

        // Update the cached state so other tools stay consistent.
        {
            let mut state = self.device_state.0.lock().await;
            state.devices = devices.clone();
        }

        if devices.is_empty() {
            return Ok(CallToolResult::structured(
                json!({ "devices": [], "count": 0, "hint": "No devices connected. Connect a device or launch an emulator with launch_avd." }),
            ));
        }

        let structured: Vec<serde_json::Value> = devices
            .iter()
            .map(|d| {
                json!({
                    "serial": d.serial,
                    "model": d.model.as_deref().unwrap_or(&d.name),
                    "name": d.name,
                    "state": format!("{:?}", d.connection_state).to_lowercase(),
                    "api_level": d.api_level,
                    "android_version": d.android_version,
                    "kind": format!("{:?}", d.device_kind).to_lowercase(),
                })
            })
            .collect();

        let any_offline = structured.iter().any(|d| d["state"] == "offline");
        let hint: Option<&str> = if any_offline {
            Some("One or more devices are offline. Try: adb kill-server && adb start-server, then reconnect.")
        } else {
            None
        };
        Ok(CallToolResult::structured(json!({
            "count": devices.len(),
            "devices": structured,
            "hint": hint,
        })))
    }

    /// Dump UI Automator / accessibility hierarchy for the focused window (native Views + Compose).
    #[tool(
        description = "Dump the focused window UI hierarchy (UI Automator accessibility XML) for native Views and Jetpack Compose. Includes capped shell context (dumpsys window/display, wm size/density) and tries uiautomator dump --compressed when supported. Use interactive_only for a compact list of tappable fields.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_ui_hierarchy(
        &self,
        Parameters(p): Parameters<GetUiHierarchyParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot = match ui_hierarchy::capture_ui_hierarchy_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        if p.interactive_only.unwrap_or(false) {
            let max = p.max_interactive_rows.unwrap_or(80).clamp(1, 500) as usize;
            let rows = ui_hierarchy_parse::extract_interactive_rows(&snapshot.root, max);
            return Ok(CallToolResult::structured(json!({
                "interactiveOnly": true,
                "capturedAt": snapshot.captured_at,
                "truncated": snapshot.truncated,
                "warnings": snapshot.warnings,
                "screenHash": snapshot.screen_hash,
                "interactiveCount": snapshot.interactive_count,
                "foregroundActivity": snapshot.foreground_activity,
                "layoutContext": {
                    "wmSize": snapshot.layout_context.wm_size,
                    "wmDensity": snapshot.layout_context.wm_density,
                },
                "rows": rows,
            })));
        }

        Ok(CallToolResult::structured(json!({
            "capturedAt": snapshot.captured_at,
            "truncated": snapshot.truncated,
            "warnings": snapshot.warnings,
            "screenHash": snapshot.screen_hash,
            "interactiveCount": snapshot.interactive_count,
            "foregroundActivity": snapshot.foreground_activity,
            "layoutContext": {
                "wmSize": snapshot.layout_context.wm_size,
                "wmDensity": snapshot.layout_context.wm_density,
            },
            "root": serde_json::to_value(&snapshot.root).unwrap_or(serde_json::Value::Null),
        })))
    }

    /// Search the focused window hierarchy for nodes matching text, content-desc, resource-id, class, or package. Returns centers for use with ui_tap. Requires at least one primary filter (not only clickable/editable flags).
    #[tool(
        description = "Find UI elements on the focused screen by text, content-desc, resource-id, class, or package. Returns treePath, bounds, centerX/centerY, flags, and screenHash from a fresh uiautomator dump. Use centerX/centerY with ui_tap. At least one of textContains, textEquals, contentDescContains, resourceIdEquals, resourceIdContains, classContains, or packageEquals is required.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn find_ui_elements(
        &self,
        Parameters(p): Parameters<ui_automation::FindUiElementsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if !ui_automation::find_query_has_primary_filter(&p) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "find_ui_elements requires at least one primary filter: textContains, textEquals, contentDescContains, resourceIdEquals, resourceIdContains, classContains, or packageEquals (non-empty).",
            )]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let max = p
            .max_results
            .unwrap_or(ui_automation::DEFAULT_FIND_RESULTS as u32) as usize;
        let matches = ui_automation::find_ui_elements(&snapshot, &p, max);
        let matches_json: Vec<serde_json::Value> = matches
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();

        Ok(CallToolResult::structured(json!({
            "capturedAt": snapshot.captured_at,
            "truncated": snapshot.truncated,
            "warnings": snapshot.warnings,
            "screenHash": snapshot.screen_hash,
            "foregroundActivity": snapshot.foreground_activity,
            "matchCount": matches_json.len(),
            "matches": matches_json,
        })))
    }

    /// List clickable nodes without requiring a text/id/class/package filter.
    #[tool(
        description = "List all clickable UI elements on the focused screen from a fresh uiautomator dump. Returns treePath, bounds, centerX/centerY, flags, and screenHash. Use this when you need to discover available buttons/tap targets before choosing one.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_clickable_elements(
        &self,
        Parameters(p): Parameters<ui_automation::ListClickableElementsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let max = p
            .max_results
            .unwrap_or(ui_automation::MAX_FIND_RESULTS as u32) as usize;
        let mut matches = if p.enabled_only.unwrap_or(false) {
            // When filtering for enabled elements, collect more candidates first
            // to avoid truncating potentially valid enabled elements that appear later
            ui_automation::collect_clickable_nodes(&snapshot, ui_automation::MAX_FIND_RESULTS)
        } else {
            ui_automation::collect_clickable_nodes(&snapshot, max)
        };
        if p.enabled_only.unwrap_or(false) {
            matches.retain(|m| m.enabled);
        }
        // Apply final limit after filtering
        matches.truncate(max);
        let matches_json: Vec<serde_json::Value> = matches
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();

        Ok(CallToolResult::structured(json!({
            "capturedAt": snapshot.captured_at,
            "truncated": snapshot.truncated,
            "warnings": snapshot.warnings,
            "screenHash": snapshot.screen_hash,
            "foregroundActivity": snapshot.foreground_activity,
            "matchCount": matches_json.len(),
            "matches": matches_json,
        })))
    }

    /// Resolve the direct parent of a node by layout treePath (same paths as find_ui_elements / Layout tab).
    #[tool(
        description = "Given a non-empty layout treePath from find_ui_elements or the Layout viewer, returns the direct parent node (treePath, bounds, centerX/centerY, flags) plus screenHash from a fresh dump. Optional expect_screen_hash refuses if the UI changed. Empty treePath is invalid (root has no parent).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn find_ui_parent(
        &self,
        Parameters(p): Parameters<ui_automation::FindUiParentParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot = if p.expect_screen_hash.is_some() {
            match ui_automation::ensure_screen_hash(&adb, &serial, p.expect_screen_hash.as_deref())
                .await
            {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            }
        } else {
            match ui_automation::capture_ui_snapshot(&adb, &serial).await {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            }
        };

        let (normalized_path, parent) =
            match ui_automation::find_ui_parent_from_snapshot(&snapshot, &p.tree_path) {
                Ok(v) => v,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };

        let parent_json = match serde_json::to_value(&parent) {
            Ok(v) => v,
            Err(e) => return Err(McpError::internal_error(e.to_string(), None)),
        };

        Ok(CallToolResult::structured(json!({
            "capturedAt": snapshot.captured_at,
            "truncated": snapshot.truncated,
            "warnings": snapshot.warnings,
            "screenHash": snapshot.screen_hash,
            "foregroundActivity": snapshot.foreground_activity,
            "treePath": normalized_path,
            "parentTreePath": parent.tree_path,
            "parent": parent_json,
        })))
    }

    /// Tap device coordinates (usually from find_ui_elements centerX/centerY).
    #[tool(
        description = "Tap at device pixel coordinates. Use find_ui_elements for centerX/centerY. Optional expect_screen_hash re-dumps the hierarchy and refuses if the screen changed.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_tap(
        &self,
        Parameters(p): Parameters<ui_automation::UiTapParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        if let Err(e) = ui_automation::validate_coordinates(p.x, p.y) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        if p.expect_screen_hash.is_some() {
            if let Err(e) =
                ui_automation::ensure_screen_hash(&adb, &serial, p.expect_screen_hash.as_deref())
                    .await
            {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
        }

        match ui_automation::adb_input_tap(&adb, &serial, p.x, p.y).await {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    format!("tap ({}, {})", p.x, p.y)
                } else {
                    format!("tap ({}, {}): {msg}", p.x, p.y)
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Tap an element by treePath from a fresh hierarchy dump.
    #[tool(
        description = "Tap a UI element by treePath from find_ui_elements/list_clickable_elements instead of passing raw coordinates. Captures the hierarchy, verifies optional expect_screen_hash, resolves the current center, then taps.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_tap_element(
        &self,
        Parameters(p): Parameters<ui_automation::UiTapElementParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot =
            match ui_automation::ensure_screen_hash(&adb, &serial, p.expect_screen_hash.as_deref())
                .await
            {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };

        let target = match ui_automation::resolve_tap_element_target(&snapshot, &p) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        match ui_automation::adb_input_tap(&adb, &serial, target.x, target.y).await {
            Ok(msg) => Ok(CallToolResult::structured(json!({
                "message": if msg.is_empty() { "element tapped".to_string() } else { msg },
                "screenHash": snapshot.screen_hash,
                "target": target,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Type text via adb input text (ASCII-oriented; use tap_x/tap_y to focus a field first).
    #[tool(
        description = "Low-level text send with adb shell input text after an optional tap. Prefer ui_fill_input when filling a known input field because it always focuses the target before typing. ASCII printable only; spaces encoded automatically; no emoji. Optional expect_screen_hash verifies hierarchy before acting.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_type_text(
        &self,
        Parameters(p): Parameters<ui_automation::UiTypeTextParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if p.text.is_empty() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "text must not be empty",
            )]));
        }
        if let Err(e) = ui_automation::validate_tap_coordinate_pair(p.tap_x, p.tap_y) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        if let Some(ref expected) = p.expect_screen_hash {
            if let Err(e) =
                ui_automation::ensure_screen_hash(&adb, &serial, Some(expected.as_str())).await
            {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
        }

        if let (Some(x), Some(y)) = (p.tap_x, p.tap_y) {
            if let Err(e) = ui_automation::validate_coordinates(x, y) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, x, y).await {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        if p.clear_before.unwrap_or(false) {
            if let Err(e) = ui_automation::adb_clear_field(&adb, &serial).await {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "clear_before failed: {e}"
                ))]));
            }
        }

        match ui_automation::adb_input_text(&adb, &serial, &p.text).await {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    "input text sent".to_string()
                } else {
                    msg
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Fill an input by focusing it first, optionally clearing, then typing.
    #[tool(
        description = "Fill an editable input field. Requires either treePath (preferred, from find_ui_elements/list_clickable_elements) or x/y. Captures the hierarchy, verifies optional expect_screen_hash, taps the target first to focus/select it, clears existing text by default, then types ASCII text.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_fill_input(
        &self,
        Parameters(p): Parameters<ui_automation::UiFillInputParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if p.text.is_empty() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "text must not be empty",
            )]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot =
            match ui_automation::ensure_screen_hash(&adb, &serial, p.expect_screen_hash.as_deref())
                .await
            {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };

        let target = match ui_automation::resolve_fill_input_target(&snapshot, &p) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, target.x, target.y).await {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let clear_before = p.clear_before.unwrap_or(true);
        if clear_before {
            if let Err(e) = ui_automation::adb_clear_field(&adb, &serial).await {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "clear_before failed: {e}"
                ))]));
            }
        }

        match ui_automation::adb_input_text(&adb, &serial, &p.text).await {
            Ok(msg) => Ok(CallToolResult::structured(json!({
                "message": if msg.is_empty() { "input filled".to_string() } else { msg },
                "screenHash": snapshot.screen_hash,
                "target": target,
                "clearBefore": clear_before,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Clear the focused editable field (Ctrl+A then Delete). Optional tap to focus first.
    #[tool(
        description = "Clear the focused editable field using Ctrl+A then Delete. Use tap_x/tap_y to focus a field first. Call before ui_type_text to replace instead of append, or use the clear_before flag on ui_type_text directly.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn clear_focused_input(
        &self,
        Parameters(p): Parameters<ui_automation::ClearFocusedInputParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if let Err(e) = ui_automation::validate_tap_coordinate_pair(p.tap_x, p.tap_y) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        if let (Some(x), Some(y)) = (p.tap_x, p.tap_y) {
            if let Err(e) = ui_automation::validate_coordinates(x, y) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, x, y).await {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        match ui_automation::adb_clear_field(&adb, &serial).await {
            Ok(()) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "field cleared",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Type Unicode text (emoji, non-ASCII) via clipboard paste (API 24+).
    #[tool(
        description = "Type Unicode text (including emoji and non-ASCII) into a focused field using clipboard paste (Ctrl+V). Requires API 24+. Use ui_type_text for ASCII-only input. Optional tap_x/tap_y to focus a field first. Optional clear_before to replace existing content.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_type_text_unicode(
        &self,
        Parameters(p): Parameters<ui_automation::UiTypeTextUnicodeParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if p.text.is_empty() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "text must not be empty",
            )]));
        }
        if let Err(e) = ui_automation::validate_tap_coordinate_pair(p.tap_x, p.tap_y) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        if let (Some(x), Some(y)) = (p.tap_x, p.tap_y) {
            if let Err(e) = ui_automation::validate_coordinates(x, y) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, x, y).await {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        if p.clear_before.unwrap_or(false) {
            if let Err(e) = ui_automation::adb_clear_field(&adb, &serial).await {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "clear_before failed: {e}"
                ))]));
            }
        }

        match ui_automation::adb_type_text_unicode(&adb, &serial, &p.text).await {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    "unicode text sent via clipboard".to_string()
                } else {
                    msg
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Send an allowlisted keyevent (Back, Home, Enter, etc.).
    #[tool(
        description = "Send a keyevent by name: Back, Home, Enter, Delete, Tab, Escape, Search, Menu, AppSwitch, DpadUp, DpadDown, DpadLeft, DpadRight, DpadCenter.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn send_ui_key(
        &self,
        Parameters(p): Parameters<ui_automation::SendUiKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        let code = match ui_automation::resolve_ui_key_code(&p.key) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_keyevent(&adb, &serial, code).await {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    format!("keyevent {code}")
                } else {
                    format!("keyevent {code}: {msg}")
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Hide the soft keyboard without accidentally navigating when it is already hidden.
    #[tool(
        description = "Hide the Android soft keyboard. Checks dumpsys input_method first and sends Back only when the keyboard appears visible. If visibility cannot be detected, returns a no-op unless force=true.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn hide_soft_keyboard(
        &self,
        Parameters(p): Parameters<ui_automation::HideSoftKeyboardParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_hide_soft_keyboard(&adb, &serial, p.force.unwrap_or(false)).await {
            Ok(outcome) => Ok(CallToolResult::structured(json!(outcome))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Swipe or long-press (same start/end with duration_ms).
    #[tool(
        description = "Swipe from x1,y1 to x2,y2 in device pixels. Optional duration_ms; same coordinates + duration performs a long-press.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_swipe(
        &self,
        Parameters(p): Parameters<ui_automation::UiSwipeParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_input_swipe(&adb, &serial, p.x1, p.y1, p.x2, p.y2, p.duration_ms)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    "swipe OK".to_string()
                } else {
                    msg
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Scroll until an element appears.
    #[tool(
        description = "Repeatedly swipe and dump the hierarchy until an element matching text/content-desc/resource-id/class/package appears, or max_swipes is reached. Uses an inferred vertical scroll gesture unless x1/y1/x2/y2 are supplied.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_scroll_until_element(
        &self,
        Parameters(p): Parameters<ui_automation::UiScrollUntilElementParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        let q = ui_automation::find_params_from_scroll_until(&p);
        if !ui_automation::find_query_has_primary_filter(&q) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "ui_scroll_until_element requires at least one primary filter: textContains, textEquals, contentDescContains, resourceIdEquals, resourceIdContains, classContains, or packageEquals.",
            )]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let max_swipes = p
            .max_swipes
            .unwrap_or(ui_automation::DEFAULT_SCROLL_ATTEMPTS)
            .min(ui_automation::MAX_SCROLL_ATTEMPTS);
        let max_results =
            p.max_results
                .unwrap_or(ui_automation::DEFAULT_FIND_RESULTS as u32) as usize;
        let poll_ms = p.poll_interval_ms.unwrap_or(500).max(200);
        let mut last_hash = String::new();
        let mut last_swipe: Option<ui_automation::ResolvedSwipe> = None;

        for swipe_count in 0..=max_swipes {
            let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };
            last_hash = snapshot.screen_hash.clone();
            let matches = ui_automation::find_ui_elements(&snapshot, &q, max_results);
            if !matches.is_empty() {
                let matches_json: Vec<serde_json::Value> = matches
                    .iter()
                    .filter_map(|m| serde_json::to_value(m).ok())
                    .collect();
                return Ok(CallToolResult::structured(json!({
                    "found": true,
                    "swipesPerformed": swipe_count,
                    "screenHash": snapshot.screen_hash,
                    "foregroundActivity": snapshot.foreground_activity,
                    "matchCount": matches_json.len(),
                    "matches": matches_json,
                    "lastSwipe": last_swipe,
                })));
            }

            if swipe_count == max_swipes {
                break;
            }

            let swipe = match ui_automation::resolve_scroll_swipe(&snapshot, &p) {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };
            if let Err(e) = ui_automation::adb_input_swipe(
                &adb,
                &serial,
                swipe.x1,
                swipe.y1,
                swipe.x2,
                swipe.y2,
                Some(swipe.duration_ms),
            )
            .await
            {
                return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
            }
            last_swipe = Some(swipe);
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(poll_ms))).await;
        }

        Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "ui_scroll_until_element did not find a matching element after {max_swipes} swipe(s). Last screenHash: {last_hash}"
        ))]))
    }

    /// Grant a runtime permission (pm grant).
    #[tool(
        description = "Grant an android.permission.* runtime permission to an installed package; permission must start with android.permission. Only the open project's app (applicationId or a variant) unless allow_foreign_package is true.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn grant_runtime_permission(
        &self,
        Parameters(p): Parameters<ui_automation::GrantRuntimePermissionParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        validate_package_name(&p.package)?;
        self.check_package_scope(
            "grant_runtime_permission",
            &p.package,
            p.allow_foreign_package,
        )
        .await?;
        if let Err(e) = ui_automation::validate_runtime_permission(&p.permission) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_pm_grant(&adb, &serial, &p.package, &p.permission).await {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    format!("granted {} to {}", p.permission, p.package)
                } else {
                    msg
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Revoke a runtime permission (pm revoke).
    #[tool(
        description = "Revoke an android.permission.* runtime permission from an installed package. Useful for testing permission request flows. Only the open project's app (applicationId or a variant) unless allow_foreign_package is true.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn revoke_runtime_permission(
        &self,
        Parameters(p): Parameters<ui_automation::GrantRuntimePermissionParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        validate_package_name(&p.package)?;
        self.check_package_scope(
            "revoke_runtime_permission",
            &p.package,
            p.allow_foreign_package,
        )
        .await?;
        if let Err(e) = ui_automation::validate_runtime_permission(&p.permission) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_pm_revoke(&adb, &serial, &p.package, &p.permission).await {
            Ok(msg) => Ok(CallToolResult::success(vec![ContentBlock::text(
                if msg.is_empty() {
                    format!("revoked {} from {}", p.permission, p.package)
                } else {
                    msg
                },
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Poll until a UI element matching filters appears (or timeout elapses).
    #[tool(
        description = "Poll the device hierarchy until an element matching the given filters appears, or timeout_ms elapses (default 15s, max 30s). Returns the same shape as find_ui_elements on success. Requires at least one primary filter. Use after ui_tap or navigation to wait for the next screen to load.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn wait_for_element(
        &self,
        Parameters(p): Parameters<ui_automation::WaitForElementParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::wait_for_element(&adb, &serial, &p).await {
            Ok((snapshot, matches)) => {
                let matches_json: Vec<serde_json::Value> = matches
                    .iter()
                    .filter_map(|m| serde_json::to_value(m).ok())
                    .collect();
                Ok(CallToolResult::structured(json!({
                    "found": true,
                    "capturedAt": snapshot.captured_at,
                    "screenHash": snapshot.screen_hash,
                    "foregroundActivity": snapshot.foreground_activity,
                    "matchCount": matches_json.len(),
                    "matches": matches_json,
                })))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Wait until consecutive hierarchy dumps have the same screenHash.
    #[tool(
        description = "Wait until the focused UI appears idle by polling screenHash until it is stable for consecutive samples. Use after taps, scrolls, launches, or keyboard actions before the next UI query.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_wait_for_idle(
        &self,
        Parameters(p): Parameters<ui_automation::UiWaitForIdleParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let cfg = ui_automation::WaitForIdleConfig::from_params(&p);
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(u64::from(cfg.timeout_ms));
        let mut last_hash: Option<String> = None;
        let mut stable_count = 0u32;
        let mut samples = 0u32;

        loop {
            let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };
            samples += 1;
            let current_hash = snapshot.screen_hash.clone();
            if last_hash.as_deref() == Some(snapshot.screen_hash.as_str()) {
                stable_count += 1;
            } else {
                stable_count = 1;
                last_hash = Some(snapshot.screen_hash);
            }

            if stable_count >= cfg.stable_polls {
                return Ok(CallToolResult::structured(json!({
                    "idle": true,
                    "screenHash": current_hash,
                    "samples": samples,
                    "stableSamples": stable_count,
                    "pollIntervalMs": cfg.poll_interval_ms,
                })));
            }

            if std::time::Instant::now() >= deadline {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "ui_wait_for_idle timed out after {} ms; last screenHash: {}",
                    cfg.timeout_ms, current_hash
                ))]));
            }

            tokio::time::sleep(std::time::Duration::from_millis(u64::from(
                cfg.poll_interval_ms,
            )))
            .await;
        }
    }

    /// Assert element presence and state.
    #[tool(
        description = "Assert that an element matching text/content-desc/resource-id/class/package exists or does not exist, optionally checking clickable/editable/enabled/focused/checked/selected flags. Returns an error when the assertion fails.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn ui_assert_element(
        &self,
        Parameters(p): Parameters<ui_automation::UiAssertElementParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };
        let max = p
            .max_results
            .unwrap_or(ui_automation::DEFAULT_FIND_RESULTS as u32) as usize;

        match ui_automation::assert_ui_element_state(&snapshot, &p, max) {
            Ok(outcome) => Ok(CallToolResult::structured(json!({
                "screenHash": snapshot.screen_hash,
                "foregroundActivity": snapshot.foreground_activity,
                "assertion": outcome,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "{e} screenHash={}",
                snapshot.screen_hash
            ))])),
        }
    }

    /// Capture a screenshot from a connected device.
    #[tool(
        description = "Capture a screenshot from a connected Android device. Returns a PNG whose long edge is at most max_dimension (default 1280), followed by JSON geometry: deviceWidth/deviceHeight (the pixel space ui_tap uses), imageWidth/imageHeight, and scale (device pixels per image pixel). To tap something you see, prefer ui_tap_element with a treePath from find_ui_elements or list_clickable_elements; for ui_tap, multiply image coordinates by scale first. full_size: true returns the original resolution.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn screenshot(
        &self,
        Parameters(p): Parameters<ScreenshotParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        let max_dimension = device_inspector::screenshot_max_dimension(
            p.max_dimension,
            p.full_size.unwrap_or(false),
        )
        .map_err(|e| McpError::invalid_params(e, None))?;

        let adb = {
            let (s, _) = settings_manager::load_settings();
            adb_manager::get_adb_path(&s)
        };
        match device_inspector::take_screenshot_scaled(&adb, &p.device_serial, max_dimension).await
        {
            Ok(shot) => {
                let g = shot.geometry;
                let geometry = json!({
                    "deviceWidth": g.device_width,
                    "deviceHeight": g.device_height,
                    "imageWidth": g.image_width,
                    "imageHeight": g.image_height,
                    "scale": g.scale,
                    "hint": "Multiply image coordinates by scale before ui_tap, or prefer ui_tap_element with a treePath.",
                });
                Ok(CallToolResult::success(vec![
                    ContentBlock::image(BASE64.encode(&shot.png), "image/png"),
                    ContentBlock::text(geometry.to_string()),
                ]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Screenshot failed: {e}"
            ))])),
        }
    }

    /// Get device hardware and software properties.
    #[tool(
        description = "Get Android device properties: SDK level, Android version, manufacturer, model, screen resolution, and battery.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_device_info(
        &self,
        Parameters(p): Parameters<DeviceSerialParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;

        let adb = {
            let (s, _) = settings_manager::load_settings();
            adb_manager::get_adb_path(&s)
        };
        match device_inspector::get_device_info(&adb, &p.device_serial).await {
            Ok(info) => Ok(CallToolResult::structured(json!(info))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Open a deep link URI on the device.
    #[tool(
        description = "Open a deep link URI with `am start -a android.intent.action.VIEW -d <uri>`. Optional package constrains resolution to one app.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn open_deep_link(
        &self,
        Parameters(p): Parameters<ui_automation::OpenDeepLinkParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if let Some(ref package) = p.package {
            validate_package_name(package)?;
        }
        if let Err(e) = ui_automation::validate_deep_link_uri(&p.uri) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_open_deep_link(&adb, &serial, &p.uri, p.package.as_deref()).await {
            Ok(output) => Ok(CallToolResult::structured(json!({
                "opened": true,
                "uri": p.uri,
                "package": p.package,
                "output": output,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Open Android settings for an app.
    #[tool(
        description = "Open Android Settings for an app package. panel may be appInfo (default), permissions, or notifications.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn open_app_settings(
        &self,
        Parameters(p): Parameters<ui_automation::OpenAppSettingsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        validate_package_name(&p.package)?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_open_app_settings(&adb, &serial, &p.package, p.panel.as_deref())
            .await
        {
            Ok(output) => Ok(CallToolResult::structured(json!({
                "opened": true,
                "package": p.package,
                "panel": p.panel.unwrap_or_else(|| "appInfo".to_string()),
                "output": output,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Set fixed or auto device orientation.
    #[tool(
        description = "Set device orientation using Android system settings. orientation: portrait, landscape, reversePortrait, reverseLandscape, or auto.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn set_device_orientation(
        &self,
        Parameters(p): Parameters<ui_automation::SetDeviceOrientationParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_set_device_orientation(&adb, &serial, &p.orientation).await {
            Ok(steps) => Ok(CallToolResult::structured(json!({
                "orientation": p.orientation,
                "steps": steps,
                "success": true,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Best-effort network toggles.
    #[tool(
        description = "Best-effort network controls for emulator/device: wifi, mobileData, and airplaneMode booleans. Returns each adb command, whether it succeeded, and `previous` (the prior state; pass it back to revert). Turning Wi-Fi off or airplane mode on is refused for wireless-ADB devices, since it would drop the connection.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_network_state(
        &self,
        Parameters(p): Parameters<ui_automation::SetNetworkStateParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        match ui_automation::adb_set_network_state(&adb, &serial, &p).await {
            Ok(change) => {
                let (steps, success) = (change.steps, change.success);
                Ok(CallToolResult::structured(json!({
                    "success": success,
                    "bestEffort": true,
                    "wifi": p.wifi,
                    "mobileData": p.mobile_data,
                    "airplaneMode": p.airplane_mode,
                    "previous": change.previous,
                    "steps": steps,
                    "hint": if success { serde_json::Value::Null } else { json!("Some Android versions restrict network toggles. Inspect failed step output and verify actual device state.") },
                })))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Compare current UI state against a baseline screenHash captured earlier.
    #[tool(
        description = "Compare current UI state against a baseline screenHash. Returns changed=false if the screen hash matches (UI is identical), or changed=true with all currently interactive (clickable/editable) nodes when the screen changed. Use after ui_tap, ui_swipe, ui_type_text, etc. to verify the action had an effect before taking the next step.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn compare_ui_state(
        &self,
        Parameters(p): Parameters<ui_automation::CompareUiStateParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial =
            match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
                Some(s) => s,
                None => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )]))
                }
            };

        let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let changed = snapshot.screen_hash != p.baseline_screen_hash;

        if !changed {
            return Ok(CallToolResult::structured(json!({
                "changed": false,
                "message": "UI state is identical to baseline — screen hash unchanged.",
                "previousHash": p.baseline_screen_hash,
                "currentHash": snapshot.screen_hash,
                "capturedAt": snapshot.captured_at,
                "foregroundActivity": snapshot.foreground_activity,
            })));
        }

        let max = p.max_results.unwrap_or(30).clamp(1, 100) as usize;
        let interactive = ui_automation::collect_interactive_nodes(&snapshot, max);
        let interactive_json: Vec<serde_json::Value> = interactive
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();

        Ok(CallToolResult::structured(json!({
            "changed": true,
            "message": format!("UI state changed — {} interactive nodes found in new state.", interactive_json.len()),
            "previousHash": p.baseline_screen_hash,
            "currentHash": snapshot.screen_hash,
            "capturedAt": snapshot.captured_at,
            "foregroundActivity": snapshot.foreground_activity,
            "truncated": snapshot.truncated,
            "warnings": snapshot.warnings,
            "interactiveCount": interactive_json.len(),
            "interactiveNodes": interactive_json,
        })))
    }

    /// Get installed app details from a device.
    #[tool(
        description = "Get installed app details: version name/code, install path, permissions, and declared activities.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn dump_app_info(
        &self,
        Parameters(p): Parameters<DevicePackageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;

        let adb = {
            let (s, _) = settings_manager::load_settings();
            adb_manager::get_adb_path(&s)
        };
        match device_inspector::dump_app_info(&adb, &p.device_serial, &p.package).await {
            Ok(info) => Ok(CallToolResult::structured(json!(info))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Get memory usage for an app.
    #[tool(
        description = "Get memory usage for an Android app: PSS, heap size, native memory, and graphics memory.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_memory_info(
        &self,
        Parameters(p): Parameters<DevicePackageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;

        let adb = {
            let (s, _) = settings_manager::load_settings();
            adb_manager::get_adb_path(&s)
        };
        match device_inspector::get_memory_info(&adb, &p.device_serial, &p.package).await {
            Ok(info) => Ok(CallToolResult::structured(json!(info))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        }
    }

    /// Why the app's processes exited (Android 11+ exit history).
    #[tool(
        description = "Why an app's processes exited, newest first: crash, native crash, ANR, low memory, killed by the user or the system, and more, with time, importance, memory, and the system's description. Reads the device's exit history (Android 11+), so it includes crashes and ANRs that never reached logcat. package defaults to the open project's app.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_exit_reasons(
        &self,
        Parameters(p): Parameters<GetExitReasonsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if let Some(ref pkg) = p.package {
            validate_package_name(pkg)?;
        }
        let limit = p
            .limit
            .map_or(DEFAULT_EXIT_REASONS, |l| l as usize)
            .clamp(1, app_exit_info::MAX_EXIT_RECORDS);

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let Some(serial) =
            adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await
        else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "No device connected. Connect a device or launch an emulator first.",
            )]));
        };
        let root = self.get_gradle_root().await;

        match app_exit_info::read_exit_reasons(&adb, &serial, root.as_deref(), p.package.as_deref())
            .await
        {
            Ok(result) => Ok(CallToolResult::success(vec![ContentBlock::text(
                app_exit_info::agent_summary(&result, limit),
            )])),
            Err(app_exit_info::ExitInfoError::InvalidInput(e)) => {
                Err(McpError::invalid_params(e, None))
            }
            Err(app_exit_info::ExitInfoError::Failed(e)) => {
                Ok(CallToolResult::error(vec![ContentBlock::text(e)]))
            }
        }
    }

    /// Install an APK on a connected device.
    #[tool(
        description = "Install an APK file on a connected device or emulator. APK must be within the project's build output directory.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn install_apk(
        &self,
        Parameters(p): Parameters<InstallApkParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        let apk = self.validate_apk_path(&p.apk_path).await?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let aapt2 = adb_manager::find_aapt2(&settings);

        let outcome = installed_builds::install_and_record(
            &adb,
            aapt2.as_deref(),
            &p.device_serial,
            &apk,
            &self.device_state,
        )
        .await
        .map_err(|e| McpError::internal_error(format!("APK install failed: {e}"), None))?;

        let recorded = match &outcome.recorded {
            Some(entry) => format!("\n{}", installed_builds::describe_install(entry)),
            None => String::new(),
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "APK installed: {}{recorded}",
            outcome.output
        ))]))
    }

    /// Launch an app on a connected device.
    #[tool(
        description = "Launch an Android app on a device. Uses am start -W to launch the main activity or a specified activity and reports the launch time and launch state (cold, warm, hot) when the device measures them.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn launch_app(
        &self,
        Parameters(p): Parameters<LaunchAppParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;
        if let Some(ref activity) = p.activity {
            crate::utils::validation::validate_activity_name(activity)
                .map_err(|e| McpError::invalid_params(e, None))?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let result =
            adb_manager::launch_app(&adb, &p.device_serial, &p.package, p.activity.as_deref())
                .await
                .map_err(|e| McpError::internal_error(format!("Launch failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "App launched: {}\n{}",
            result.description,
            adb_manager::describe_launch_timing(result.timing.as_ref())
        ))]))
    }

    /// Stop a running app on a device.
    #[tool(
        description = "Force-stop an Android app on a device using am force-stop. Only the open project's app (applicationId or a variant) unless allow_foreign_package is true.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn stop_app(
        &self,
        Parameters(p): Parameters<StopAppParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;
        self.check_package_scope("stop_app", &p.package, p.allow_foreign_package)
            .await?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        adb_manager::stop_app(&adb, &p.device_serial, &p.package)
            .await
            .map_err(|e| McpError::internal_error(format!("Stop app failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "App {} stopped.",
            p.package
        ))]))
    }

    /// List available Android Virtual Devices (AVDs).
    #[tool(
        description = "List all available Android Virtual Devices (AVDs) configured in the Android SDK.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_avds(&self) -> Result<CallToolResult, McpError> {
        let avds = adb_manager::list_avds();
        if avds.is_empty() {
            return Ok(CallToolResult::structured(
                json!({ "avds": [], "count": 0, "hint": "No AVDs found. Create one in the Device Manager panel." }),
            ));
        }
        let structured: Vec<serde_json::Value> = avds
            .iter()
            .map(|a| {
                json!({
                    "name": a.name,
                    "display_name": a.display_name,
                    "api_level": a.api_level,
                    "abi": a.abi,
                    "target": a.target,
                    "path": a.path,
                })
            })
            .collect();
        Ok(CallToolResult::structured(
            json!({ "count": avds.len(), "avds": structured }),
        ))
    }

    /// Launch an Android Virtual Device (emulator).
    #[tool(
        description = "Launch an Android Virtual Device (emulator) and return the serial of the emulator running it once it is online. An AVD that is already running is not started again (already_running: true).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn launch_avd(
        &self,
        Parameters(p): Parameters<LaunchAvdParams>,
    ) -> Result<CallToolResult, McpError> {
        adb_manager::validate_avd_name(&p.name).map_err(|e| McpError::invalid_params(e, None))?;

        let (settings, _) = settings_manager::load_settings();
        let emulator = adb_manager::get_emulator_path(&settings);
        let adb = adb_manager::get_adb_path(&settings);

        match adb_manager::launch_emulator(&emulator, &adb, &p.name).await {
            Ok(launched) => Ok(CallToolResult::structured(json!({
                "serial": launched.serial,
                "avd_name": p.name,
                "already_running": launched.already_running,
            }))),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to launch AVD '{}': {e}",
                p.name
            ))])),
        }
    }

    /// Stop a running emulator.
    #[tool(
        description = "Stop a running Android emulator by its ADB serial. Succeeds only once the emulator has left the device list.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn stop_avd(
        &self,
        Parameters(p): Parameters<StopAvdParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.serial)?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        if let Err(e) = adb_manager::stop_emulator(&adb, &p.serial).await {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to stop emulator: {e}"
            ))]));
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Emulator {} stopped.",
            p.serial
        ))]))
    }

    // ── Project / health tools ────────────────────────────────────────────────

    /// Get information about the open Android project.
    #[tool(
        description = "Get the currently open Android project name, path, detected Gradle root, how the project was selected (selected_by), whether it is trusted to run its Gradle build (trusted), the JDK Gradle builds use (path, major version, and where it was found), and whether this session is attached to the Keynobi app or standalone (mode, standalone_reason, follows_app, pinned_project).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_project_info(&self) -> Result<CallToolResult, McpError> {
        let (project_root, gradle_root) = {
            let fs = self.fs_state.0.lock().await;
            (fs.project_root.clone(), fs.gradle_root.clone())
        };
        let pinned_project = match &self.mode {
            SessionMode::Attached { pinned_project } => pinned_project.clone(),
            SessionMode::Standalone { .. } => None,
        };
        let session = json!({
            "mode": self.mode.name(),
            "standalone_reason": self.mode.standalone_reason(),
            "follows_app": matches!(self.mode, SessionMode::Attached { pinned_project: None }),
            "pinned_project": pinned_project,
        });
        if let Some(mismatch) = self.project_mismatch().await {
            let mut info = session;
            info["open"] = json!(false);
            info["app_project"] = json!(gradle_root.or(project_root));
            info["hint"] = json!(mismatch);
            return Ok(CallToolResult::structured(info));
        }
        let (settings, _) = settings_manager::load_settings();
        let java = jdk::check_project_java(
            &settings,
            project_root.as_deref(),
            gradle_root.as_deref(),
            &jdk::JdkSearchRoots::system(),
        )
        .await
        .to_json();
        let mut info = match project_root.as_ref() {
            None => json!({
                "open": false,
                "hint": "No project open. Open an Android project in the companion app, or launch with --project /path/to/project.",
                "java": java,
            }),
            Some(root) => {
                let name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| root.to_string_lossy().to_string());
                let gradle = gradle_root
                    .as_ref()
                    .map(|g| g.to_string_lossy().to_string());
                let trusted = project_trust::is_trusted(&settings, root);
                let trust_hint = (!trusted).then_some(
                    "Safe Mode: run_gradle_task and run_tests are refused until the user opens \
                     this project in the Keynobi app and chooses Trust. Other tools still work.",
                );
                json!({
                    "open": true,
                    "name": name,
                    "path": root.to_string_lossy(),
                    "gradle_root": gradle,
                    "selected_by": self.project_selection,
                    "trusted": trusted,
                    "trust_hint": trust_hint,
                    "java": java,
                })
            }
        };
        if let (Some(info), Some(session)) = (info.as_object_mut(), session.as_object()) {
            info.extend(session.clone());
        }
        Ok(CallToolResult::structured(info))
    }

    /// Run system health checks.
    #[tool(
        description = "Run system health checks: Java, Android SDK, ADB, emulator, and Gradle wrapper availability.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn run_health_check(&self) -> Result<CallToolResult, McpError> {
        let (settings, _) = settings_manager::load_settings();
        let (project_root, gradle_root) = {
            let fs = self.fs_state.0.lock().await;
            (fs.project_root.clone(), fs.gradle_root.clone())
        };

        let jdk_roots = jdk::JdkSearchRoots::system();
        let (report, android_cli) = tokio::join!(
            health_inspector::run_health_check(
                &settings,
                project_root.as_deref(),
                gradle_root.as_deref(),
                &jdk_roots,
            ),
            android_cli::detect(),
        );

        Ok(CallToolResult::structured(health_check_json(
            &report,
            &settings,
            &android_cli,
        )))
    }
}

// ── Prompt definitions ────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DiagnoseCrashArgs {
    #[schemars(description = "Android package name to diagnose, e.g. com.example.myapp")]
    pub package: String,
    #[schemars(description = "ADB device serial to read crash logs from (from list_devices)")]
    pub device_serial: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FullDeployArgs {
    #[schemars(description = "ADB device serial to deploy to (from list_devices)")]
    pub device_serial: String,
    #[schemars(description = "Build variant to use, e.g. debug or release (default: debug)")]
    pub variant: Option<String>,
    #[schemars(
        description = "Android package name to launch after install, e.g. com.example.myapp"
    )]
    pub package: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BuildAndFixArgs {
    #[schemars(description = "Gradle task to run and fix errors for, e.g. assembleDebug")]
    pub task: Option<String>,
}

#[prompt_router]
impl AndroidMcpServer {
    /// Diagnose a crash for the given package: fetch logcat crashes, the
    /// device's exit history, memory info, and app details for root-cause analysis.
    #[prompt(
        name = "diagnose-crash",
        description = "Diagnose a crash or ANR for an Android app: fetch crash logs, exit reasons, memory, and app state."
    )]
    async fn diagnose_crash(
        &self,
        Parameters(args): Parameters<DiagnoseCrashArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let device_hint = args
            .device_serial
            .as_deref()
            .unwrap_or("the connected device");
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            diagnose_crash_text(&args),
        )])
        .with_description(format!(
            "Diagnose crash for {} on {}",
            args.package, device_hint
        )))
    }

    /// Full deploy workflow: build → find APK → install → launch.
    #[prompt(
        name = "full-deploy",
        description = "Full deploy workflow: build the app, install it on a device, and launch it."
    )]
    async fn full_deploy(
        &self,
        Parameters(args): Parameters<FullDeployArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let variant = args.variant.as_deref().unwrap_or("debug");
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            full_deploy_text(&args),
        )])
        .with_description(format!("Full deploy {} to {}", variant, args.device_serial)))
    }

    /// Build and fix: run a build, get errors, and suggest fixes.
    #[prompt(
        name = "build-and-fix",
        description = "Run a build and help fix any compiler errors."
    )]
    async fn build_and_fix(
        &self,
        Parameters(args): Parameters<BuildAndFixArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let task = args.task.as_deref().unwrap_or("assembleDebug");
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            build_and_fix_text(&args),
        )])
        .with_description(format!("Build {} and fix errors", task)))
    }
}

fn diagnose_crash_text(args: &DiagnoseCrashArgs) -> String {
    format!(
        "Diagnose the crash for app '{pkg}' on {device}. \
         Step 1: Call get_crash_logs to see recent FATAL EXCEPTION / ANR entries, then get_crash_stack_trace with package={pkg}. \
         If its frames look obfuscated (names like a.b.c), call get_crash_stack_trace again with retrace: true. \
         Step 2: Call get_exit_reasons with device_serial={device} and package={pkg}: the device's exit history also lists crashes, ANRs, and low-memory kills that never reached logcat. \
         Step 3: Call get_logcat_entries with package={pkg} and min_level=error for context. \
         Step 4: Call get_memory_info with device_serial={device} and package={pkg} to check for OOM. \
         Step 5: Call dump_app_info with device_serial={device} and package={pkg} for version and install state. \
         Then provide a root-cause analysis and suggest fixes.",
        pkg = args.package,
        device = args.device_serial.as_deref().unwrap_or("{device_serial}"),
    )
}

fn full_deploy_text(args: &FullDeployArgs) -> String {
    let variant = args.variant.as_deref().unwrap_or("debug");
    let task = format!("assemble{}", capitalize_first(variant));
    format!(
        "Deploy the {variant} build to device '{device}'. \
         Step 1: Call run_gradle_task with task={task} to build. \
         Step 2: Call find_apk_path with variant={variant} to locate the APK. \
         Step 3: Call install_apk with device_serial={device} and the path from step 2. \
         Step 4: {launch} \
         Report the result of each step.",
        task = task,
        variant = variant,
        device = args.device_serial,
        launch = if let Some(ref pkg) = args.package {
            format!(
                "Call launch_app with device_serial={device} and package={pkg} to start the app.",
                device = args.device_serial,
                pkg = pkg
            )
        } else {
            "If you know the package name, call launch_app to start the app.".into()
        },
    )
}

fn build_and_fix_text(args: &BuildAndFixArgs) -> String {
    format!(
        "Run the build and fix any errors. \
         Step 1: Call run_gradle_task with task={task}. \
         Step 2: Call get_build_errors for structured error list. \
         Step 3: For each error, explain the root cause and suggest the minimal fix. \
         Step 4: If there are many errors, prioritize them (compilation errors block warnings). \
         Be specific about file paths and line numbers.",
        task = args.task.as_deref().unwrap_or("assembleDebug"),
    )
}

// ── ServerHandler impl ────────────────────────────────────────────────────────

#[tool_handler]
#[prompt_handler]
impl ServerHandler for AndroidMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("keynobi", env!("CARGO_PKG_VERSION"))
                .with_title(match &self.mode {
                    SessionMode::Attached { .. } => "Keynobi (attached to the app)".to_string(),
                    SessionMode::Standalone { .. } => "Keynobi (standalone)".to_string(),
                })
                .with_description(self.mode_instructions()),
        )
        .with_instructions(format!(
            "{} \
             Keynobi MCP Server — AI-first companion for Android development. \
             Keynobi covers stateful work (logs, crashes, and builds) and pairs with Android CLI (`android`) for stateless device and SDK tasks; the keynobi://skill resource says which to use when. \
             Tools: build (run_gradle_task, get_build_errors, get_build_log, get_build_config, find_apk_path, run_tests), \
             logcat (start_logcat, get_logcat_entries, get_crash_logs, get_crash_stack_trace), \
             devices (list_devices, get_ui_hierarchy, find_ui_elements, list_clickable_elements, find_ui_parent, ui_tap, ui_tap_element, ui_fill_input, ui_type_text, hide_soft_keyboard, ui_swipe, ui_scroll_until_element, ui_wait_for_idle, ui_assert_element, send_ui_key, open_deep_link, open_app_settings, set_device_orientation, set_network_state, grant_runtime_permission, revoke_runtime_permission, screenshot, get_device_info, install_apk, launch_app, restart_app, dump_app_info, get_memory_info, get_app_runtime_state, get_exit_reasons), \
             project (get_project_info, run_health_check). \
             Prompts: diagnose-crash, full-deploy, build-and-fix. \
             Start with get_project_info and run_health_check to verify the environment.",
            self.mode_instructions()
        ))
    }

    async fn on_initialized(&self, context: rmcp::service::NotificationContext<RoleServer>) {
        let client_name = context
            .peer
            .peer_info()
            .map(|i| i.client_info.name.clone())
            .unwrap_or_else(|| "unknown".into());
        info!(
            "MCP client connected: {} — {} tools, {} prompts available",
            client_name,
            self.tool_router.list_all().len(),
            self.prompt_router.list_all().len()
        );
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let fs = self.fs_state.0.lock().await;
        let mut resources = vec![
            Resource::new("android://project-info", "Project Info"),
            Resource::new("android://health", "System Health"),
            Resource::new(agent_skill::RESOURCE_URI, "Keynobi agent skill")
                .with_description(
                    "SKILL.md telling an agent when to use Keynobi and when to use Android CLI",
                )
                .with_mime_type("text/markdown"),
        ];

        if let Some(ref gradle_root) = fs.gradle_root.clone().or(fs.project_root.clone()) {
            for (uri, relative, name) in project_file_resources(gradle_root) {
                if crate::utils::path::resolve_project_file(gradle_root, &relative).is_ok() {
                    resources.push(Resource::new(uri, name));
                }
            }
        }

        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = &request.uri;
        let fs = self.fs_state.0.lock().await;
        let gradle_root = fs.gradle_root.clone().or(fs.project_root.clone());
        drop(fs);

        match uri.as_str() {
            "android://project-info" => {
                let info = self.get_project_info().await?;
                let text = info
                    .content
                    .first()
                    .and_then(|c| c.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_else(|| "No project open".into());
                Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri.clone())]).into())
            }
            agent_skill::RESOURCE_URI => Ok(ReadResourceResult::new(vec![ResourceContents::text(
                agent_skill::SKILL_MARKDOWN,
                uri.clone(),
            )
            .with_mime_type("text/markdown")])
            .into()),
            "android://health" => {
                let health = self.run_health_check().await?;
                let text = health
                    .content
                    .first()
                    .and_then(|c| c.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_else(|| "Health check unavailable".into());
                Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri.clone())]).into())
            }
            other => {
                let relative = gradle_root.as_ref().and_then(|root| {
                    project_file_resources(root)
                        .into_iter()
                        .find(|(u, _, _)| *u == other)
                        .map(|(_, relative, _)| relative)
                });
                let (Some(relative), Some(root)) = (relative, gradle_root.as_ref()) else {
                    return Err(McpError::resource_not_found(
                        format!("Resource not found or project not open: {uri}"),
                        Some(json!({ "uri": uri })),
                    ));
                };
                let path = match crate::utils::path::resolve_project_file(root, &relative) {
                    Ok(path) => path,
                    Err(crate::models::error::AppError::PermissionDenied(_)) => {
                        return Err(McpError::invalid_request(
                            format!("{relative} resolves outside the project and is not served"),
                            Some(json!({ "uri": uri })),
                        ))
                    }
                    Err(_) => {
                        return Err(McpError::resource_not_found(
                            format!("Resource not found or project not open: {uri}"),
                            Some(json!({ "uri": uri })),
                        ))
                    }
                };
                let text = read_resource_text(&path, MAX_RESOURCE_BYTES).map_err(|e| {
                    McpError::internal_error(format!("Failed to read {relative}: {e}"), None)
                })?;
                let mime = if relative.ends_with(".xml") {
                    "text/xml"
                } else {
                    "text/plain"
                };
                Ok(
                    ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                        uri: uri.clone(),
                        mime_type: Some(mime.into()),
                        text,
                        meta: None,
                    }])
                    .into(),
                )
            }
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

impl AndroidMcpServer {
    /// The first sentence of `instructions`: which mode this session runs in and why.
    fn mode_instructions(&self) -> String {
        match &self.mode {
            SessionMode::Attached {
                pinned_project: Some(project),
            } => format!(
                "Mode: attached to the running Keynobi app for {}. Builds, logcat, and \
                 devices are shared with the app.",
                project.display()
            ),
            SessionMode::Attached {
                pinned_project: None,
            } => "Mode: attached to the running Keynobi app; the project is whatever the app \
                  has open. Builds, logcat, and devices are shared with the app."
                .to_string(),
            SessionMode::Standalone { reason } => format!(
                "Mode: standalone, because {reason}. This server has its own state: its \
                 builds and logcat are not visible in the Keynobi app."
            ),
        }
    }

    /// Deobfuscate a crash from the logcat buffer through the shared service,
    /// as the `retrace` object `get_crash_stack_trace` and `get_crash_logs`
    /// return. Never an error: a crash that left the buffer is reported in it.
    async fn retrace_crash_group(&self, crash_group_id: u64) -> serde_json::Value {
        let (project_root, gradle_root) = {
            let fs = self.fs_state.0.lock().await;
            (fs.project_root.clone(), fs.gradle_root.clone())
        };
        let (settings, _) = settings_manager::load_settings();
        let env = retrace::RetraceEnv::new(
            settings,
            project_root,
            gradle_root,
            self.device_state.clone(),
        );
        match retrace::retrace_crash_group(&env, &self.logcat_state, crash_group_id).await {
            Ok(outcome) => retrace::outcome_json(&outcome),
            Err(e) => json!({ "status": "refused", "reason": e.to_string() }),
        }
    }

    async fn get_gradle_root(&self) -> Option<PathBuf> {
        let fs = self.fs_state.0.lock().await;
        fs.gradle_root.clone().or_else(|| fs.project_root.clone())
    }

    /// `run_gradle_task` and `run_tests`: start `task` for `origin` through
    /// the shared build service and wait for it. The wait is bounded by
    /// `mcp.buildTimeoutSec`; when it runs out the build is stopped and
    /// recorded as timed out. The build does not depend on this call: if the
    /// client goes away, it still finishes and is recorded.
    ///
    /// With the tool call's `request`, the wait reports progress when the
    /// client sent a progress token, and a client that cancels the request
    /// cancels this build (only this one), recorded as cancelled by `origin`.
    async fn run_build(
        &self,
        task: String,
        origin: BuildActor,
        request: Option<&RequestContext<RoleServer>>,
    ) -> Result<CallToolResult, McpError> {
        validate_gradle_task(&task)?;
        if !settings_manager::load_settings()
            .0
            .mcp
            .allow_unrestricted_gradle
        {
            crate::utils::validation::check_agent_gradle_task(&task)
                .map_err(|e| McpError::invalid_params(e, None))?;
        }

        // Snapshot both roots under a single FsState lock (mirroring the UI
        // path in commands/build.rs) so a project switch mid-request cannot
        // pair one project's gradle_root with another's history root.
        let (gradle_root, project_root_for_history, trust_root) = {
            let fs = self.fs_state.0.lock().await;
            let root = fs
                .gradle_root
                .clone()
                .or_else(|| fs.project_root.clone())
                .ok_or_else(|| {
                    McpError::invalid_params(
                        "No project open. Open an Android project first.",
                        None,
                    )
                })?;
            let history_root = fs
                .project_root
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            let trust_root = fs.project_root.clone().unwrap_or_else(|| root.clone());
            (root, history_root, trust_root)
        };

        let (settings, _) = settings_manager::load_settings();
        let env = build_runner::trusted_gradle_env(&settings, &trust_root, &gradle_root)
            .map_err(|e| McpError::invalid_params(e, None))?;

        let gradlew = build_runner::find_gradlew(&gradle_root).ok_or_else(|| {
            McpError::invalid_params("gradlew not found. Is this an Android project?", None)
        })?;

        let mode = self.mode.summary();
        let started = build_runner::start_build(
            &self.build_state,
            &self.process_manager,
            // Streams the build into the app's Build panel when attached.
            self.app_handle.as_ref(),
            build_runner::BuildRequest {
                task: task.clone(),
                extra_args: vec![],
                gradle_root,
                gradlew,
                env,
                project_root: project_root_for_history,
                origin: origin.clone(),
            },
        )
        .await;
        let mut handle = match started {
            Ok(handle) => handle,
            Err(build_runner::StartBuildError::Busy(e)) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "{e}. Wait for it (get_build_status) or cancel it (cancel_build), \
                     then try again.\n[{mode}]"
                ))]));
            }
            Err(build_runner::StartBuildError::BusyElsewhere(e)) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "{e}. Wait for it to finish, then try again.\n[{mode}]"
                ))]));
            }
            Err(build_runner::StartBuildError::Spawn(e)) => {
                return Err(McpError::internal_error(
                    format!("Failed to spawn Gradle: {e}"),
                    None,
                ))
            }
        };

        let timeout_sec = settings.mcp.build_timeout_sec as u64;
        let started_at = tokio::time::Instant::now();
        let deadline = tokio::time::sleep(std::time::Duration::from_secs(timeout_sec));
        tokio::pin!(deadline);
        let progress_token = request.and_then(|r| r.meta.get_progress_token());
        let mut progress = tokio::time::interval_at(
            started_at + BUILD_PROGRESS_INTERVAL,
            BUILD_PROGRESS_INTERVAL,
        );
        let mut watch_cancel = request.is_some();

        enum Waited {
            Done(build_runner::BuildOutcome),
            TimedOut,
            RequestCancelled,
            ReportProgress,
        }
        let result = loop {
            let waited = tokio::select! {
                outcome = handle.wait() => Waited::Done(outcome),
                _ = &mut deadline => Waited::TimedOut,
                _ = request_cancelled(request), if watch_cancel => Waited::RequestCancelled,
                _ = progress.tick(), if progress_token.is_some() => Waited::ReportProgress,
            };
            match waited {
                Waited::Done(outcome) => break Some(outcome),
                Waited::TimedOut => break None,
                Waited::ReportProgress => {
                    let (Some(token), Some(request)) = (&progress_token, request) else {
                        continue;
                    };
                    if request.peer.is_transport_closed() {
                        continue;
                    }
                    let elapsed = started_at.elapsed().as_secs();
                    let message = match handle.current_task() {
                        Some(current) => {
                            format!("Building '{task}' — {elapsed}s elapsed — > Task {current}")
                        }
                        None => format!("Building '{task}' — {elapsed}s elapsed"),
                    };
                    let _ = request
                        .peer
                        .notify_progress(
                            ProgressNotificationParam::new(token.clone(), elapsed as f64)
                                .with_message(message),
                        )
                        .await;
                }
                Waited::RequestCancelled => {
                    watch_cancel = false;
                    // The request's token also fires when the client goes away;
                    // that must not stop the build.
                    if request.is_some_and(|r| r.peer.is_transport_closed()) {
                        continue;
                    }
                    build_runner::cancel_run(
                        &self.build_state,
                        &self.process_manager,
                        handle.run_id,
                        origin.clone(),
                    )
                    .await;
                }
            }
        };
        let result = match result {
            Some(result) => result,
            None => {
                build_runner::time_out_build(
                    &self.build_state,
                    &self.process_manager,
                    Some(handle.run_id),
                    timeout_sec,
                )
                .await;
                // Let the run record the timeout before answering.
                let _ = tokio::time::timeout(TIMEOUT_RECORD_GRACE, handle.wait()).await;
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Build timed out after {timeout_sec}s — task '{task}'. Build has been cancelled.\n[{mode}]"
                ))]));
            }
        };

        if result.cancelled {
            let by = result
                .cancelled_by
                .as_ref()
                .map(|by| format!(" {}", build_runner::describe_canceller(by)))
                .unwrap_or_default();
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "BUILD CANCELLED — task '{task}' was cancelled{by}.\n[{mode}]"
            ))]));
        }

        let issue_lines = build_runner::format_build_issues(&result.errors);

        if result.success {
            let msg = if result.errors.is_empty() {
                format!(
                    "BUILD SUCCESSFUL — task '{}' ({}ms)\n[{mode}]",
                    task, result.duration_ms
                )
            } else {
                format!(
                    "BUILD SUCCESSFUL (with {} warning(s)) — task '{}' ({}ms)\n{}\n[{mode}]",
                    result.errors.len(),
                    task,
                    result.duration_ms,
                    issue_lines.join("\n")
                )
            };
            Ok(CallToolResult::success(vec![ContentBlock::text(msg)]))
        } else {
            let msg = format!(
                "BUILD FAILED — task '{}'\n{} issue(s):\n{}\n[{mode}]",
                task,
                result.errors.len(),
                if result.errors.is_empty() {
                    "Check get_build_log for details.".to_owned()
                } else {
                    issue_lines.join("\n")
                }
            );
            Ok(CallToolResult::error(vec![ContentBlock::text(msg)]))
        }
    }

    /// `cancel_build`: cancel whatever build runs, recorded as cancelled by `by`.
    async fn cancel_build_as(&self, by: BuildActor) -> Result<CallToolResult, McpError> {
        let was_running =
            build_runner::cancel_build(&self.build_state, &self.process_manager, by).await;
        let msg = if was_running {
            "Build cancelled."
        } else {
            "No build was running."
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(msg)]))
    }

    /// Refuse a destructive or permission-changing call on a package outside
    /// the open project unless the client opted in with `allow_foreign_package`.
    async fn check_package_scope(
        &self,
        tool: &str,
        package: &str,
        allow_foreign_package: Option<bool>,
    ) -> Result<(), McpError> {
        if allow_foreign_package == Some(true) {
            return Ok(());
        }
        let scope = match self.get_gradle_root().await {
            Some(root) => build_inspector::project_package_scope(&root),
            None => Default::default(),
        };
        crate::utils::validation::check_agent_package_scope(tool, package, &scope)
            .map_err(|e| McpError::invalid_params(e, None))
    }

    /// The canonical APK to install: `apk_path` must resolve to an `.apk`
    /// under the project's build outputs.
    async fn validate_apk_path(&self, apk_path: &str) -> Result<PathBuf, McpError> {
        let gradle_root = self
            .get_gradle_root()
            .await
            .ok_or_else(|| McpError::invalid_params("No project open", None))?;
        crate::utils::path::validate_apk_within_build_outputs(&gradle_root, apk_path)
            .map_err(|e| McpError::invalid_params(e.to_string(), None))
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

/// Thin wrappers over the shared validators (see utils::validation), so the MCP
/// tools and the Tauri commands can never drift apart again.
fn validate_gradle_task(task: &str) -> Result<(), McpError> {
    crate::utils::validation::validate_gradle_task(task)
        .map_err(|e| McpError::invalid_params(e, None))
}

fn validate_package_name(package: &str) -> Result<(), McpError> {
    crate::utils::validation::validate_package_name(package)
        .map_err(|e| McpError::invalid_params(e, None))
}

fn validate_device_serial(serial: &str) -> Result<(), McpError> {
    crate::utils::validation::validate_device_serial(serial)
        .map_err(|e| McpError::invalid_params(e, None))
}

/// Resolve the variant to use for a variant-optional build tool: an explicit
/// argument wins; otherwise the variant persisted as active for the project
/// is used; otherwise the literal `"debug"`. A value that is empty or only
/// whitespace is treated as not set for both the explicit argument and the
/// persisted value, so it never reaches `build_runner::find_output_apk`,
/// which would otherwise match any APK in the application module's APK outputs.
fn resolve_variant(explicit: Option<&str>, persisted: Option<&str>) -> String {
    if let Some(v) = explicit {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    match persisted {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => "debug".to_string(),
    }
}

/// How long a timed-out build call waits for the stopped build to be
/// recorded before answering. Gradle is killed 5 s after being asked to stop.
const TIMEOUT_RECORD_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// How often a build call reports progress to a client that asked for it.
const BUILD_PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Resolves when the client cancels `request`; never without one.
async fn request_cancelled(request: Option<&RequestContext<RoleServer>>) {
    match request {
        Some(request) => request.ct.cancelled().await,
        None => std::future::pending().await,
    }
}

/// The Gradle task `run_tests` runs for `test_type`.
fn test_task(test_type: &str) -> Result<String, McpError> {
    Ok(match test_type {
        "unit" => "testDebug".to_owned(),
        "connected" => "connectedAndroidTest".to_owned(),
        other => {
            validate_gradle_task(other)?;
            other.to_owned()
        }
    })
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

/// Truncate `s` to at most `max_bytes` bytes without splitting a multi-byte
/// character. Slicing a `&str` at a raw byte index panics when the index falls
/// inside a multi-byte sequence, so activity summaries built from arbitrary
/// tool output (logcat lines, app UI text) must use this instead.
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut cut = max_bytes;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// The `run_health_check` result. `all_ok` covers what Keynobi needs; the
/// `retrace` and `android_cli` checks are informational and never change it.
fn health_check_json(
    report: &health_inspector::HealthReport,
    settings: &crate::models::settings::AppSettings,
    android_cli: &android_cli::AndroidCli,
) -> serde_json::Value {
    let gradle_hint = if report.gradlew_ok {
        serde_json::Value::Null
    } else if !report.project_open {
        json!("No Android project open — pass --project /path/to/project or open one in the companion app first")
    } else {
        json!("No gradlew found in the selected project — ensure it is an Android Gradle project")
    };

    json!({
        "all_ok": report.all_ok,
        "checks": {
            "java": report.java.to_json(),
            "android_sdk": {
                "ok": report.sdk_ok,
                "detected_path": report.detected_sdk,
                "hint": if report.sdk_ok { serde_json::Value::Null } else { json!("SDK not found — set android.sdkPath in Settings → Android, or ensure ANDROID_HOME is set") }
            },
            "adb": {
                "ok": report.adb_ok,
                "hint": if report.adb_ok { serde_json::Value::Null } else { json!("ADB not found — check Android SDK path") }
            },
            "gradle_wrapper": { "ok": report.gradlew_ok, "hint": gradle_hint },
            "retrace": retrace::health_json(settings),
            "android_cli": android_cli::health_json(android_cli),
            "project": {
                "ok": report.project_open,
                "path": report.project_path.as_ref().map(|p| p.to_string_lossy().to_string())
            },
        }
    })
}

/// Project files served as resources: URI, path relative to the Gradle root,
/// and name. The manifest and module build file are the application module's,
/// and only when the project has exactly one.
fn project_file_resources(gradle_root: &Path) -> Vec<(&'static str, String, &'static str)> {
    let mut resources = Vec::new();
    if let Ok(module) = gradle_modules::resolve_application_module(gradle_root, None) {
        let prefix = match module.relative_dir(gradle_root).as_str() {
            "." => String::new(),
            dir => format!("{dir}/"),
        };
        resources.push((
            "android://manifest",
            format!("{prefix}src/main/AndroidManifest.xml"),
            "AndroidManifest.xml",
        ));
        resources.push((
            "android://app-build-gradle",
            format!("{prefix}build.gradle.kts"),
            "build.gradle.kts (application module)",
        ));
    }
    resources.push((
        "android://build-gradle",
        "build.gradle.kts".to_string(),
        "build.gradle.kts",
    ));
    resources.push((
        "android://gradle-settings",
        "settings.gradle.kts".to_string(),
        "settings.gradle.kts",
    ));
    resources
}

/// Most bytes of a project file one resource read returns.
const MAX_RESOURCE_BYTES: usize = 512 * 1024;

/// The first `max_bytes` of `path` as text (invalid UTF-8 replaced), ending
/// with a note when the file is longer.
fn read_resource_text(path: &std::path::Path, max_bytes: usize) -> std::io::Result<String> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() <= max_bytes {
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    bytes.truncate(max_bytes);
    if let Err(e) = std::str::from_utf8(&bytes) {
        // Drop a character the cut split in two.
        if e.error_len().is_none() {
            bytes.truncate(e.valid_up_to());
        }
    }
    Ok(format!(
        "{}\n\n[truncated: the file is {size} bytes; only the first {} are shown]",
        String::from_utf8_lossy(&bytes),
        bytes.len()
    ))
}

// ── Logging wrapper ────────────────────────────────────────────────────────────

/// Wraps `AndroidMcpServer` to intercept all tool calls, resource reads, and
/// prompt requests and write activity entries to the shared JSONL log.
///
/// This is used in place of `AndroidMcpServer` directly for attached and
/// standalone sessions so the companion app always has a log to display. It
/// also refuses project tools in a pinned session whose project the app closed.
pub struct LoggingMcpServer {
    server: AndroidMcpServer,
    /// The app's registry entry for an attached session.
    session: Option<(McpSessionRegistry, u32)>,
}

impl LoggingMcpServer {
    pub fn new(server: AndroidMcpServer) -> Self {
        Self {
            server,
            session: None,
        }
    }

    /// Record the client's name on this attached session once it initializes.
    pub fn with_session(mut self, registry: McpSessionRegistry, id: u32) -> Self {
        self.session = Some((registry, id));
        self
    }
}

impl ServerHandler for LoggingMcpServer {
    // ── Delegation for methods that AndroidMcpServer overrides ────────────────

    fn get_info(&self) -> ServerConfig {
        self.server.get_info()
    }

    async fn on_initialized(&self, context: rmcp::service::NotificationContext<RoleServer>) {
        let client_name = context
            .peer
            .peer_info()
            .map(|i| i.client_info.name.clone())
            .unwrap_or_else(|| "unknown".into());
        mcp_activity::log_activity(&McpActivityEntry::lifecycle(format!(
            "Client connected: {client_name}"
        )));
        if let Some((registry, id)) = &self.session {
            registry.set_client_name(*id, &client_name);
        }
        self.server.on_initialized(context).await;
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        self.server.list_resources(request, context).await
    }

    // ── Instrumented: resource reads ──────────────────────────────────────────

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let start = std::time::Instant::now();
        let uri = request.uri.clone();
        let result = match self.server.project_mismatch().await {
            Some(msg) if uri != "android://project-info" && uri != agent_skill::RESOURCE_URI => {
                Err(McpError::invalid_request(msg, None))
            }
            _ => self.server.read_resource(request, context).await,
        };
        let ms = start.elapsed().as_millis() as u64;
        let (status, summary) = match &result {
            Ok(_) => ("ok", None),
            Err(e) => ("error", Some(e.message.clone().to_string())),
        };
        mcp_activity::log_activity(&McpActivityEntry::resource_read(&uri, ms, status, summary));
        result
    }

    // ── Instrumented: tool calls (generated by #[tool_handler]) ──────────────

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let start = std::time::Instant::now();
        let name = request.name.clone();
        let result = match self.server.check_session_project(&name).await {
            Some(refused) => Ok(CallToolResponse::Complete(refused)),
            None => self.server.call_tool(request, context).await,
        };
        let ms = start.elapsed().as_millis() as u64;
        let (status, summary) = match &result {
            // rmcp 3 wraps tool results in the MRTR envelope. Every tool here is
            // synchronous, so only `Complete` carries a payload worth logging; the
            // other variants are recorded as a plain ok with no summary.
            Ok(CallToolResponse::Complete(r)) => {
                let is_err = r.is_error.unwrap_or(false);
                let first_text = r.content.first().and_then(|c| c.as_text()).map(|t| {
                    let s = &t.text;
                    if s.len() > 120 {
                        format!("{}…", truncate_at_char_boundary(s, 120))
                    } else {
                        s.clone()
                    }
                });
                if is_err {
                    ("error", first_text)
                } else {
                    ("ok", first_text)
                }
            }
            Ok(_) => ("ok", None),
            Err(e) => ("error", Some(e.message.clone().to_string())),
        };
        mcp_activity::log_activity(&McpActivityEntry::tool_call(
            name.as_ref(),
            ms,
            status,
            summary,
        ));
        result
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.server.list_tools(request, context).await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.server.get_tool(name)
    }

    // ── Instrumented: prompts (generated by #[prompt_handler]) ───────────────

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let start = std::time::Instant::now();
        let name = request.name.clone();
        let result = self.server.get_prompt(request, context).await;
        let ms = start.elapsed().as_millis() as u64;
        let status = if result.is_ok() { "ok" } else { "error" };
        mcp_activity::log_activity(&McpActivityEntry::prompt(&name, ms, status));
        result
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        self.server.list_prompts(request, context).await
    }
}

// ── `--mcp` entry point ───────────────────────────────────────────────────────

/// The MCP server's project: `--project`, else the Gradle build that contains
/// the working directory, else the app's last active project. An agent's
/// working directory beats a project the app happened to leave open.
fn select_headless_project(
    argument: Option<PathBuf>,
    working_dir: Option<PathBuf>,
    last_active_project: impl FnOnce() -> Option<String>,
) -> Option<(PathBuf, ProjectSelection)> {
    use crate::services::fs_manager;

    if let Some(path) = argument {
        let path = path.canonicalize().unwrap_or(path);
        return Some((path, ProjectSelection::Argument));
    }
    if let Some(root) = working_dir
        .as_deref()
        .and_then(fs_manager::find_gradle_root)
    {
        let root = root.canonicalize().unwrap_or(root);
        return Some((root, ProjectSelection::WorkingDirectory));
    }
    last_active_project()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .map(|p| (p, ProjectSelection::LastActiveProject))
}

/// Entry point for `keynobi --mcp`: attach to the running app, or serve
/// stdio standalone. Returns the process exit code.
///
/// Called from `main.rs`. Never launches the app. With `attach_only`, a
/// failed attach exits non-zero instead of running standalone. When the app
/// closes an attached session (it quit), the session continues standalone,
/// or, with `attach_only`, exits non-zero.
pub async fn run_mcp(project_path: Option<PathBuf>, attach_only: bool) -> i32 {
    use crate::services::mcp_attach;

    // Redirect all tracing to stderr — stdout is reserved for MCP JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    let working_dir = std::env::current_dir().ok();
    // The app's last active project is a standalone-only fallback: an
    // attached session without a project follows the app instead.
    let requested = select_headless_project(project_path, working_dir, || None);
    let request = mcp_attach::AttachRequest::new(
        requested
            .as_ref()
            .map(|(root, _)| mcp_attach::attach_project_key(root)),
        requested.as_ref().map(|(_, how)| *how),
    );
    let standalone_project = move || {
        requested.or_else(|| {
            select_headless_project(None, None, || {
                settings_manager::load_settings().0.last_active_project
            })
        })
    };

    let reason = match mcp_attach::try_attach(
        &mcp_attach::socket_path(),
        &request,
        mcp_attach::ATTACH_TIMEOUT,
    )
    .await
    {
        Ok(attached) => {
            info!(
                "Attached to the Keynobi app (version {})",
                attached.reply.version
            );
            let mut stdout = tokio::io::stdout();
            return match mcp_attach::forward(attached, tokio::io::stdin(), &mut stdout).await {
                mcp_attach::ForwardEnd::ClientClosed => 0,
                mcp_attach::ForwardEnd::AppClosed(_) if attach_only => {
                    eprintln!(
                        "keynobi: the Keynobi app closed the MCP session (it may have quit). \
                         Restart the MCP server in your AI client to reconnect."
                    );
                    1
                }
                mcp_attach::ForwardEnd::AppClosed(resume) => {
                    eprintln!(
                        "keynobi: the Keynobi app closed the MCP session (it may have quit); \
                         continuing standalone."
                    );
                    let (server, relay) = tokio::io::duplex(STANDALONE_PIPE_BYTES);
                    let serving = tokio::spawn(run_standalone(
                        standalone_project(),
                        APP_QUIT_REASON.to_string(),
                        tokio::io::split(server),
                    ));
                    let (from_server, to_server) = tokio::io::split(relay);
                    mcp_attach::resume_with(resume, &mut stdout, from_server, to_server).await;
                    serving.await.unwrap_or(1)
                }
            };
        }
        Err(reason) => reason,
    };

    if attach_only {
        eprintln!("keynobi: could not attach to the Keynobi app: {reason}");
        return 2;
    }

    run_standalone(
        standalone_project(),
        reason,
        (tokio::io::stdin(), tokio::io::stdout()),
    )
    .await
}

/// Why a session that was attached continues standalone.
pub const APP_QUIT_REASON: &str = "the Keynobi app quit";
/// Buffer between the stdio relay and a standalone server that took over a session.
const STANDALONE_PIPE_BYTES: usize = 64 * 1024;

/// Serve MCP on `transport` with this process's own state.
async fn run_standalone<R, W>(
    selection: Option<(PathBuf, ProjectSelection)>,
    reason: String,
    transport: (R, W),
) -> i32
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    use crate::services::{fs_manager, mcp_sessions};

    let project_selection = selection.as_ref().map(|(_, how)| *how);
    let project_root = selection.map(|(root, how)| {
        info!(
            "MCP standalone: project {} (selected by {how:?})",
            root.display()
        );
        root
    });
    let gradle_root = project_root
        .as_ref()
        .and_then(|root| fs_manager::find_gradle_root(root));

    let fs_state = FsState(Arc::new(tokio::sync::Mutex::new(crate::FsStateInner {
        project_root: project_root.clone(),
        gradle_root: gradle_root.clone(),
    })));

    let build_state = BuildState::new();
    let device_state = DeviceState::new();
    let logcat_state = Arc::new(tokio::sync::Mutex::new(
        crate::services::logcat::LogcatStateInner::new(),
    ));
    let process_manager = ProcessManager::new();

    info!("MCP standalone server starting ({reason}). Project: {project_root:?}");

    mcp_activity::rotate_activity_log();
    mcp_sessions::remove_legacy_pid_file();
    mcp_sessions::write_standalone_record(project_root.as_deref(), &reason);
    mcp_activity::log_activity(&McpActivityEntry::lifecycle(format!(
        "Server started (standalone: {reason}) — project: {}",
        project_root
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "none".into())
    )));

    let server = LoggingMcpServer::new(
        AndroidMcpServer::new_headless(
            build_state.clone(),
            device_state,
            logcat_state,
            fs_state,
            process_manager.clone(),
            project_selection,
        )
        .with_mode(SessionMode::Standalone { reason }),
    );
    let code = match server.serve(transport).await {
        Ok(running) => {
            if let Err(e) = running.waiting().await {
                tracing::error!("MCP server error: {e}");
            }
            0
        }
        Err(e) => {
            tracing::error!("MCP server failed to start: {e}");
            mcp_activity::log_activity(&McpActivityEntry::lifecycle(format!(
                "Server failed to start: {e}"
            )));
            1
        }
    };
    finish_builds(&build_state, &process_manager).await;
    process_manager
        .shutdown_all(crate::services::process_manager::SHUTDOWN_GRACE)
        .await;
    mcp_activity::log_activity(&McpActivityEntry::lifecycle("Server stopped (standalone)"));
    mcp_sessions::remove_standalone_record();
    code
}

/// A build keeps running after the client that started it leaves. Before a
/// standalone server exits, let its build finish and be recorded, for at most
/// `mcp.buildTimeoutSec`; then stop it, recorded as timed out.
async fn finish_builds(build_state: &BuildState, process_manager: &ProcessManager) {
    if build_state.wait_for_runs(std::time::Duration::ZERO).await {
        return;
    }
    let timeout_sec = settings_manager::load_settings().0.mcp.build_timeout_sec as u64;
    info!("Waiting up to {timeout_sec}s for the running build before exiting");
    if build_state
        .wait_for_runs(std::time::Duration::from_secs(timeout_sec))
        .await
    {
        return;
    }
    build_runner::time_out_build(build_state, process_manager, None, timeout_sec).await;
    build_state.wait_for_runs(TIMEOUT_RECORD_GRACE).await;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_app_preserves_data_by_default() {
        let p: RestartAppParams =
            serde_json::from_value(json!({ "package": "com.example.app" })).unwrap();
        assert!(!restart_clears_data(&p).unwrap());
    }

    #[test]
    fn restart_app_clears_data_only_with_explicit_device() {
        let p: RestartAppParams = serde_json::from_value(json!({
            "package": "com.example.app",
            "clear_data": true,
            "device_serial": "emulator-5554",
        }))
        .unwrap();
        assert!(restart_clears_data(&p).unwrap());

        let p: RestartAppParams = serde_json::from_value(json!({
            "package": "com.example.app",
            "clear_data": true,
        }))
        .unwrap();
        let err = restart_clears_data(&p).unwrap_err();
        assert!(err.message.contains("device_serial"), "{}", err.message);
    }

    #[test]
    fn restart_app_rejects_removed_cold_param() {
        for cold in [json!(true), json!(false), serde_json::Value::Null] {
            let p: RestartAppParams =
                serde_json::from_value(json!({ "package": "com.example.app", "cold": cold }))
                    .unwrap();
            let err = restart_clears_data(&p).unwrap_err();
            assert!(err.message.contains("clear_data"), "{}", err.message);
        }
    }

    #[test]
    fn restart_app_schema_advertises_clear_data_not_cold() {
        let schema = serde_json::to_string(&schemars::schema_for!(RestartAppParams)).unwrap();
        assert!(schema.contains("\"clear_data\""));
        assert!(!schema.contains("\"cold\""));
    }

    /// The refusal message names `allow_foreign_package`, so every scoped
    /// tool must accept it under that exact name, including camelCase ones.
    #[test]
    fn scoped_tools_accept_allow_foreign_package_in_snake_case() {
        for schema in [
            schemars::schema_for!(RestartAppParams),
            schemars::schema_for!(StopAppParams),
            schemars::schema_for!(ui_automation::GrantRuntimePermissionParams),
        ] {
            let schema = serde_json::to_string(&schema).unwrap();
            assert!(schema.contains("\"allow_foreign_package\""), "{schema}");
        }
        let p: ui_automation::GrantRuntimePermissionParams = serde_json::from_value(json!({
            "package": "com.other.app",
            "permission": "android.permission.CAMERA",
            "allow_foreign_package": true,
        }))
        .unwrap();
        assert_eq!(p.allow_foreign_package, Some(true));
    }

    #[test]
    fn validate_gradle_task_accepts_valid() {
        assert!(validate_gradle_task("assembleDebug").is_ok());
        assert!(validate_gradle_task(":app:assembleRelease").is_ok());
        assert!(validate_gradle_task("test").is_ok());
        assert!(validate_gradle_task("clean-rebuild").is_ok());
        assert!(validate_gradle_task("connectedAndroidTest").is_ok());
    }

    #[test]
    fn validate_gradle_task_rejects_shell_injection() {
        assert!(validate_gradle_task("assemble; rm -rf /").is_err());
        assert!(validate_gradle_task("assemble && echo pwned").is_err());
        assert!(validate_gradle_task("$(malicious)").is_err());
        assert!(validate_gradle_task("").is_err());
    }

    #[test]
    fn validate_package_name_accepts_valid() {
        assert!(validate_package_name("com.example.app").is_ok());
        assert!(validate_package_name("com.example.my_app").is_ok());
    }

    #[test]
    fn validate_package_name_rejects_invalid() {
        assert!(validate_package_name("").is_err());
        assert!(validate_package_name("notapackage").is_err());
        assert!(validate_package_name("com.example; rm -rf").is_err());
    }

    #[test]
    fn validate_device_serial_accepts_valid() {
        assert!(validate_device_serial("emulator-5554").is_ok());
        assert!(validate_device_serial("192.168.1.100:5555").is_ok());
        assert!(validate_device_serial("ABCDEF123456").is_ok());
    }

    #[test]
    fn validate_device_serial_rejects_injection() {
        assert!(validate_device_serial("").is_err());
        assert!(validate_device_serial("emulator; rm -rf /").is_err());
    }

    #[test]
    fn capitalize_first_works() {
        assert_eq!(capitalize_first("debug"), "Debug");
        assert_eq!(capitalize_first("release"), "Release");
        assert_eq!(capitalize_first(""), "");
    }

    #[test]
    fn resolve_variant_prefers_explicit_argument() {
        assert_eq!(resolve_variant(Some("release"), Some("staging")), "release");
    }

    #[test]
    fn resolve_variant_falls_back_to_persisted() {
        assert_eq!(resolve_variant(None, Some("staging")), "staging");
    }

    #[test]
    fn resolve_variant_defaults_to_debug_when_nothing_set() {
        assert_eq!(resolve_variant(None, None), "debug");
    }

    #[test]
    fn resolve_variant_treats_blank_persisted_value_as_not_set() {
        assert_eq!(resolve_variant(None, Some("")), "debug");
        assert_eq!(resolve_variant(None, Some("   ")), "debug");
    }

    #[test]
    fn resolve_variant_treats_blank_explicit_argument_as_not_set() {
        assert_eq!(resolve_variant(Some(""), None), "debug");
        assert_eq!(resolve_variant(Some("   "), None), "debug");
    }

    #[test]
    fn resolve_variant_falls_back_to_persisted_when_explicit_is_blank() {
        assert_eq!(resolve_variant(Some(""), Some("staging")), "staging");
        assert_eq!(resolve_variant(Some("  "), Some("staging")), "staging");
    }

    #[test]
    fn resolve_variant_trims_padded_explicit_argument() {
        assert_eq!(resolve_variant(Some(" debug "), None), "debug");
    }

    #[test]
    fn resolve_variant_trims_padded_persisted_value() {
        assert_eq!(resolve_variant(None, Some(" staging ")), "staging");
    }

    #[test]
    fn truncate_at_char_boundary_handles_multibyte() {
        // ASCII passes through unchanged.
        assert_eq!(truncate_at_char_boundary("abcdefgh", 120), "abcdefgh");
        assert_eq!(
            truncate_at_char_boundary(&"a".repeat(120), 120),
            "a".repeat(120)
        );

        // Multi-byte characters: byte 120 falls exactly on a char boundary
        // (every 漢/字 is 3 bytes), so the full 40 chars are kept.
        let cjk: String = "漢字".repeat(80); // 480 bytes, all 3-byte chars
        let cut = truncate_at_char_boundary(&cjk, 120);
        assert!(cut.len() <= 120);
        assert_eq!(cut, "漢字".repeat(20));

        // Byte 119 falls inside the 40th character — back off to its start.
        assert_eq!(
            truncate_at_char_boundary(&cjk, 119),
            "漢字".repeat(19) + "漢"
        );

        // Emoji (4-byte chars) straddling the limit.
        let emoji = "😀😀😀";
        let cut = truncate_at_char_boundary(emoji, 6);
        assert_eq!(cut, "😀");

        // Cut index landing exactly on a boundary keeps it.
        let mixed = format!("{}{}", "a".repeat(118), "漢字");
        assert_eq!(
            truncate_at_char_boundary(&mixed, 121),
            format!("{}{}", "a".repeat(118), '漢')
        );
    }

    #[test]
    fn resource_text_is_capped_and_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let small = tmp.path().join("small.kts");
        std::fs::write(&small, "plugins {}\n").unwrap();
        let large = tmp.path().join("large.kts");
        // 10 bytes: the 3-byte '漢' straddles the 8-byte cut.
        std::fs::write(&large, "abcdefg漢").unwrap();

        assert_eq!(read_resource_text(&small, 11).unwrap(), "plugins {}\n");
        let cut = read_resource_text(&large, 8).unwrap();
        assert!(cut.starts_with("abcdefg\n\n[truncated"), "{cut}");
        assert!(cut.contains("10 bytes"), "{cut}");
        assert!(cut.contains("first 7 are shown"), "{cut}");
    }

    #[test]
    fn resource_text_replaces_invalid_utf8() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("latin1.xml");
        std::fs::write(&file, b"<a>caf\xe9</a>").unwrap();

        assert_eq!(
            read_resource_text(&file, MAX_RESOURCE_BYTES).unwrap(),
            "<a>caf\u{FFFD}</a>"
        );
    }

    // ── State-mutating tools ─────────────────────────────────────────────────
    //
    // These tools share BuildState and LogcatState with the GUI, so a bug here
    // corrupts the app's view of the world. Constructed headless (app_handle
    // None) so no Tauri runtime is needed.

    fn gradle_build(dir: &std::path::Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("settings.gradle.kts"), "").unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn headless_project_prefers_the_argument() {
        let tmp = tempfile::tempdir().unwrap();
        let arg = gradle_build(&tmp.path().join("arg"));
        let cwd = gradle_build(&tmp.path().join("cwd"));
        let last = gradle_build(&tmp.path().join("last"));

        let picked = select_headless_project(Some(arg.clone()), Some(cwd), || {
            Some(last.to_string_lossy().into_owned())
        });

        assert_eq!(picked, Some((arg, ProjectSelection::Argument)));
    }

    #[test]
    fn headless_project_prefers_the_working_directory_over_the_last_active_project() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = gradle_build(&tmp.path().join("cwd"));
        let last = gradle_build(&tmp.path().join("last"));
        let last_active = || Some(last.to_string_lossy().into_owned());

        let picked = select_headless_project(None, Some(cwd.clone()), last_active);
        assert_eq!(
            picked,
            Some((cwd.clone(), ProjectSelection::WorkingDirectory))
        );

        // A module folder inside the build selects the build.
        let picked = select_headless_project(None, Some(cwd.join("app")), last_active);
        assert_eq!(picked, Some((cwd, ProjectSelection::WorkingDirectory)));
    }

    #[test]
    fn headless_project_falls_back_to_the_last_active_project() {
        let tmp = tempfile::tempdir().unwrap();
        let not_gradle = tmp.path().join("notes");
        std::fs::create_dir_all(&not_gradle).unwrap();
        let last = gradle_build(&tmp.path().join("last"));

        let picked = select_headless_project(None, Some(not_gradle.clone()), || {
            Some(last.to_string_lossy().into_owned())
        });
        assert_eq!(picked, Some((last, ProjectSelection::LastActiveProject)));

        let missing = tmp.path().join("deleted").to_string_lossy().into_owned();
        assert_eq!(
            select_headless_project(None, Some(not_gradle), || Some(missing)),
            None
        );
    }

    fn headless_server() -> AndroidMcpServer {
        AndroidMcpServer::new_headless(
            BuildState::new(),
            DeviceState::new(),
            crate::commands::logcat::new_logcat_state(),
            FsState::new(),
            ProcessManager::new(),
            None,
        )
    }

    #[tokio::test]
    async fn run_gradle_task_refuses_denied_tasks_by_default() {
        let server = headless_server();
        for task in ["publishReleaseBundle", "pRB", ":app:uninstallAll"] {
            let err = server
                .run_build(task.into(), BuildActor::App, None)
                .await
                .unwrap_err();
            assert!(
                err.message.contains("blocked for MCP clients"),
                "{task}: {}",
                err.message
            );
        }

        // run_tests goes through the same build path, so it is covered too.
        let err = server
            .run_build(test_task("publish").unwrap(), BuildActor::App, None)
            .await
            .unwrap_err();
        assert!(
            err.message.contains("blocked for MCP clients"),
            "{}",
            err.message
        );

        // Ordinary tasks pass the policy and fail later only for lack of a project.
        let err = server
            .run_build("assembleDebug".into(), BuildActor::App, None)
            .await
            .unwrap_err();
        assert!(err.message.contains("No project open"), "{}", err.message);
    }

    /// Every tool must declare annotations so MCP clients can ask the user
    /// before destructive or open-world calls, and they must match the Kind
    /// column in references/MCP_SERVER.md (R read-only, W writes, D
    /// destructive, O open-world) so the docs cannot drift from the code.
    #[test]
    fn every_tool_declares_annotations_matching_the_reference_docs() {
        let doc = include_str!("../../../references/MCP_SERVER.md");
        let mut documented = std::collections::HashMap::new();
        for line in doc.lines() {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            if cells.len() < 4 || !matches!(cells[2], "R" | "W" | "D" | "O") {
                continue;
            }
            for name in cells[1].split('`').skip(1).step_by(2) {
                documented.insert(name.to_string(), cells[2]);
            }
        }

        let tools = headless_server().tool_router.list_all();
        assert_eq!(tools.len(), documented.len(), "tool table out of sync");
        for tool in tools {
            let a = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} declares no annotations", tool.name));
            let kind = match (a.read_only_hint, a.destructive_hint, a.open_world_hint) {
                (Some(true), Some(false), Some(false)) => "R",
                (Some(false), Some(false), Some(false)) => "W",
                (Some(false), Some(true), Some(false)) => "D",
                (Some(false), Some(true), Some(true)) => "O",
                other => panic!("{} has incomplete annotations: {other:?}", tool.name),
            };
            assert_eq!(
                Some(&kind),
                documented.get(tool.name.as_ref()),
                "{} annotations disagree with references/MCP_SERVER.md",
                tool.name
            );
        }
    }

    /// The UI and the MCP server share one build slot. Without it, an agent
    /// could start a second Gradle process against the same project and
    /// orphan the first.
    #[tokio::test]
    async fn mcp_build_is_refused_while_a_ui_build_is_running() {
        let server = headless_server();

        crate::services::build_runner::try_reserve_build_slot(
            &server.build_state,
            "assembleDebug",
            "2026-01-01T00:00:00Z",
        )
        .await
        .expect("first reservation succeeds");

        let second = crate::services::build_runner::try_reserve_build_slot(
            &server.build_state,
            "assembleRelease",
            "2026-01-01T00:00:01Z",
        )
        .await;

        assert!(
            second.is_err(),
            "an MCP build must not start while another build holds the slot"
        );
    }

    #[tokio::test]
    async fn get_build_status_reflects_the_shared_build_state() {
        let server = headless_server();
        crate::services::build_runner::try_reserve_build_slot(
            &server.build_state,
            "assembleDebug",
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        let result = server.get_build_status().await.unwrap();
        let text = format!("{:?}", result);
        assert!(
            text.contains("running"),
            "status must surface the shared state, got: {text}"
        );
    }

    #[tokio::test]
    async fn stop_logcat_bumps_the_stream_generation() {
        let server = headless_server();

        let before = {
            let mut s = server.logcat_state.lock().await;
            s.streaming = true;
            s.stream_generation
        };

        server.stop_logcat().await.unwrap();

        let after = server.logcat_state.lock().await;
        assert!(!after.streaming);
        assert_ne!(
            after.stream_generation, before,
            "stop must bump the generation so an in-flight stream task exits"
        );
    }

    #[tokio::test]
    async fn clear_logcat_bumps_the_clear_epoch_and_empties_the_store() {
        let server = headless_server();
        {
            let mut s = server.logcat_state.lock().await;
            s.known_packages.insert("com.example.app".to_string());
        }
        let before = server.logcat_state.lock().await.clear_epoch;

        server.clear_logcat().await.unwrap();

        let after = server.logcat_state.lock().await;
        assert_ne!(after.clear_epoch, before);
        assert!(after.known_packages.is_empty());
        assert_eq!(after.store.len(), 0);
    }

    #[tokio::test]
    async fn cancel_build_reports_when_nothing_is_running() {
        let server = headless_server();
        let result = server.cancel_build_as(BuildActor::App).await.unwrap();
        // Must not panic or wedge state when idle.
        assert!(!format!("{:?}", result).is_empty());
        let bs = server.build_state.inner.lock().await;
        assert!(bs.current_build.is_none());
    }

    fn result_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn open_in_app(fs_state: &FsState, root: &std::path::Path) {
        let mut fs = fs_state.0.lock().await;
        fs.project_root = Some(root.to_path_buf());
        fs.gradle_root = Some(root.to_path_buf());
    }

    #[tokio::test]
    async fn a_pinned_session_refuses_project_tools_after_the_app_switches_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let a = gradle_build(&tmp.path().join("a"));
        let b = gradle_build(&tmp.path().join("b"));
        let fs_state = FsState::new();
        open_in_app(&fs_state, &a).await;
        let server = AndroidMcpServer::new_headless(
            BuildState::new(),
            DeviceState::new(),
            crate::commands::logcat::new_logcat_state(),
            fs_state.clone(),
            ProcessManager::new(),
            None,
        )
        .attached(Some(a.clone()), ProjectSelection::WorkingDirectory);

        assert!(server
            .check_session_project("run_gradle_task")
            .await
            .is_none());

        open_in_app(&fs_state, &b).await;
        for tool in [
            "run_gradle_task",
            "get_build_status",
            "stop_app",
            "cancel_build",
        ] {
            let refused = server
                .check_session_project(tool)
                .await
                .unwrap_or_else(|| panic!("{tool} must be refused"));
            assert_eq!(refused.is_error, Some(true));
            let text = result_text(&refused);
            assert!(
                text.contains(&format!("Keynobi now has {} open", b.display()))
                    && text.contains(&format!("this session is for {}", a.display())),
                "{text}"
            );
        }
        for tool in ["list_devices", "get_logcat_entries", "get_project_info"] {
            assert!(server.check_session_project(tool).await.is_none(), "{tool}");
        }

        let info = result_text(&server.get_project_info().await.unwrap());
        let info: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(info["open"], false);
        assert_eq!(info["mode"], "attached");
        assert_eq!(info["follows_app"], false);
        assert_eq!(info["pinned_project"], json!(a));
        assert_eq!(info["app_project"], json!(b));

        let mut fs = fs_state.0.lock().await;
        fs.project_root = None;
        fs.gradle_root = None;
        drop(fs);
        let text = result_text(&server.check_session_project("run_tests").await.unwrap());
        assert!(text.contains("Keynobi now has no project open"), "{text}");
    }

    #[tokio::test]
    async fn a_session_that_follows_the_app_is_never_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_state = FsState::new();
        let server = AndroidMcpServer::new_headless(
            BuildState::new(),
            DeviceState::new(),
            crate::commands::logcat::new_logcat_state(),
            fs_state.clone(),
            ProcessManager::new(),
            None,
        )
        .attached(None, ProjectSelection::App);
        open_in_app(&fs_state, &gradle_build(&tmp.path().join("b"))).await;

        assert!(server
            .check_session_project("run_gradle_task")
            .await
            .is_none());
        let info = result_text(&server.get_project_info().await.unwrap());
        let info: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(info["open"], true);
        assert_eq!(info["mode"], "attached");
        assert_eq!(info["follows_app"], true);
        assert_eq!(info["selected_by"], "app");
        assert_eq!(info["standalone_reason"], json!(null));
    }

    #[tokio::test]
    async fn standalone_sessions_say_so_and_why() {
        let server = headless_server().with_mode(SessionMode::Standalone {
            reason: "the Keynobi app is not running".into(),
        });

        let info = result_text(&server.get_project_info().await.unwrap());
        let info: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(info["mode"], "standalone");
        assert_eq!(info["standalone_reason"], "the Keynobi app is not running");

        let status = result_text(&server.get_build_status().await.unwrap());
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(status["mode"], "standalone");
        assert_eq!(
            status["standalone_reason"],
            "the Keynobi app is not running"
        );

        let info = server.get_info();
        let instructions = info.instructions.unwrap_or_default();
        assert!(
            instructions.starts_with("Mode: standalone, because the Keynobi app is not running."),
            "{instructions}"
        );
        assert_eq!(info.server_info.name, "keynobi");
        assert_eq!(
            info.server_info.title.as_deref(),
            Some("Keynobi (standalone)")
        );
    }

    #[test]
    fn every_project_independent_tool_exists() {
        let tools = headless_server().tool_router.list_all();
        for name in PROJECT_INDEPENDENT_TOOLS {
            assert!(
                tools.iter().any(|t| t.name == *name),
                "{name} is not a tool"
            );
        }
    }

    /// Every `snake_case` name the skill puts in backticks is a tool, so an
    /// agent following it never calls one that does not exist. (Parameters
    /// are written as `name: value`, which this does not match.)
    #[test]
    fn every_tool_the_skill_names_exists() {
        let tools = headless_server().tool_router.list_all();
        let named: Vec<&str> = agent_skill::SKILL_MARKDOWN
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|s| {
                s.contains('_')
                    && s.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    && !s.starts_with('_')
            })
            .collect();
        assert!(
            named.len() >= 30,
            "only {} tools named: {named:?}",
            named.len()
        );
        for name in named {
            assert!(
                tools.iter().any(|t| t.name == name),
                "the skill names `{name}`, which is not a tool"
            );
        }
    }

    fn healthy_report() -> health_inspector::HealthReport {
        health_inspector::HealthReport {
            all_ok: true,
            java: jdk::JavaCheck {
                jdk: None,
                bin: PathBuf::from("/jdk/bin/java"),
                found: true,
                version_line: Some("openjdk version \"21.0.8\"".into()),
                major: Some(21),
            },
            sdk_ok: true,
            adb_ok: true,
            gradlew_ok: true,
            project_open: true,
            detected_sdk: Some("/sdk".into()),
            project_path: Some(PathBuf::from("/project")),
        }
    }

    #[test]
    fn health_stays_ok_without_android_cli() {
        let settings = crate::models::settings::AppSettings::default();

        let missing = health_check_json(
            &healthy_report(),
            &settings,
            &android_cli::AndroidCli::default(),
        );

        assert_eq!(missing["all_ok"], true, "{missing}");
        assert_eq!(missing["checks"]["android_cli"]["installed"], false);
        assert!(missing["checks"]["android_cli"]["hint"].is_string());

        let installed = health_check_json(
            &healthy_report(),
            &settings,
            &android_cli::AndroidCli {
                path: Some(PathBuf::from("/opt/homebrew/bin/android")),
                version: Some("1.0.16406183".into()),
            },
        );
        assert_eq!(installed["all_ok"], true);
        assert_eq!(installed["checks"]["android_cli"]["installed"], true);
        assert_eq!(
            installed["checks"]["android_cli"]["version"],
            "1.0.16406183"
        );
    }

    /// Every snake_case word a prompt tells the model to use must be a tool
    /// or a tool parameter, so a renamed or removed tool cannot linger there.
    #[test]
    fn prompts_name_only_tools_and_parameters_that_exist() {
        let tools = headless_server().tool_router.list_all();
        let mut known: std::collections::HashSet<String> =
            tools.iter().map(|t| t.name.to_string()).collect();
        for tool in &tools {
            if let Some(props) = tool
                .input_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                known.extend(props.keys().cloned());
            }
        }
        let prompts = [
            diagnose_crash_text(&DiagnoseCrashArgs {
                package: "com.example.app".into(),
                device_serial: Some("emulator-5554".into()),
            }),
            diagnose_crash_text(&DiagnoseCrashArgs {
                package: "com.example.app".into(),
                device_serial: None,
            }),
            full_deploy_text(&FullDeployArgs {
                device_serial: "emulator-5554".into(),
                variant: None,
                package: Some("com.example.app".into()),
            }),
            full_deploy_text(&FullDeployArgs {
                device_serial: "emulator-5554".into(),
                variant: Some("release".into()),
                package: None,
            }),
            build_and_fix_text(&BuildAndFixArgs { task: None }),
        ];
        for text in &prompts {
            let words = text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
            for word in words.filter(|w| w.contains('_')) {
                assert!(
                    known.contains(word),
                    "{word} is not a tool or parameter: {text}"
                );
            }
        }

        let diagnose = &prompts[0];
        for tool in [
            "get_exit_reasons",
            "get_crash_stack_trace",
            "get_crash_logs",
        ] {
            assert!(diagnose.contains(tool), "diagnose-crash must use {tool}");
        }
        assert!(diagnose.contains("retrace: true"), "{diagnose}");
        let stack_trace = tools
            .iter()
            .find(|t| t.name == "get_crash_stack_trace")
            .expect("get_crash_stack_trace is a tool");
        assert!(
            stack_trace.input_schema["properties"]
                .get("retrace")
                .is_some(),
            "get_crash_stack_trace must accept retrace"
        );
    }

    #[tokio::test]
    async fn run_gradle_task_rejects_an_invalid_task_name() {
        let server = headless_server();
        let err = server
            .run_build("assembleDebug; rm -rf /".to_string(), BuildActor::App, None)
            .await;
        assert!(err.is_err(), "shell metacharacters must be rejected");
    }
}
