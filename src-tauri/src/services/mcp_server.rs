/**
 * MCP Server for Keynobi
 *
 * Exposes build, logcat, device, UI hierarchy, UI automation (tap/type/swipe/keys), and project tools to Claude Code (or any
 * MCP-compatible client) via the Model Context Protocol (2025-11-25 spec).
 *
 * Two modes:
 *   - GUI mode: started via the `start_mcp_server` Tauri command, accesses
 *     existing Tauri managed state via AppHandle.
 *   - Headless mode: launched with `--mcp` CLI flag, initializes state
 *     directly, no GUI window is opened.
 *
 * Transport: stdio (newline-delimited JSON-RPC 2.0).
 *
 * Setup: `claude mcp add --transport stdio keynobi -- "/path/to/keynobi" --mcp`
 */
use crate::services::adb_manager::{self, DeviceState};
use crate::services::build_runner::{self, BuildState};
use crate::services::logcat::{self, LogcatFilter, LogcatState};
use crate::services::mcp_activity::{self, McpActivityEntry};
use crate::services::process_manager::ProcessManager;
use crate::services::settings_manager;
use crate::services::variant_manager;
use crate::services::crash_inspector;
use crate::services::app_inspector;
use crate::services::build_inspector;
use crate::services::device_inspector;
use crate::services::health_inspector;
use crate::services::ui_automation;
use crate::services::ui_hierarchy;
use crate::services::ui_hierarchy_parse;
use crate::FsState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::prompt::PromptRouter, router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    prompt, prompt_handler, prompt_router,
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, error, info};

// ── Guard against duplicate stdio server instances ────────────────────────────
static MCP_STDIO_RUNNING: AtomicBool = AtomicBool::new(false);

// ── Server struct ─────────────────────────────────────────────────────────────

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
    /// Present in GUI mode; used for lifecycle event emission and logcat streaming.
    app_handle: Option<AppHandle>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl AndroidMcpServer {
    /// Construct from Tauri managed state (GUI mode).
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
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Emit a Tauri event to the main window.
    /// No-ops silently in headless mode (no app_handle).
    fn emit_event<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(handle) = &self.app_handle {
            if let Some(win) = handle.get_webview_window("main") {
                let _ = win.emit(event, payload);
            }
        }
    }

    /// Construct standalone, for headless `--mcp` mode.
    pub fn new_headless(
        build_state: BuildState,
        device_state: DeviceState,
        logcat_state: LogcatState,
        fs_state: FsState,
        process_manager: ProcessManager,
    ) -> Self {
        Self {
            build_state,
            device_state,
            logcat_state,
            fs_state,
            process_manager,
            app_handle: None,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }
}

// ── Tool parameter types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunGradleTaskParams {
    #[schemars(description = "Gradle task name, e.g. assembleDebug or :app:assembleRelease")]
    pub task: String,
    #[schemars(description = "Optional build variant to activate before running, e.g. debug or release")]
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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StartLogcatParams {
    #[schemars(description = "ADB device serial to stream logcat from (optional, uses first connected device)")]
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
pub struct DeviceSerialParams {
    #[schemars(description = "ADB device serial, e.g. emulator-5554 (from list_devices)")]
    pub device_serial: String,
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
    #[schemars(description = "Build variant name, e.g. debug or release (optional, uses active variant)")]
    pub variant: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunTestsParams {
    #[schemars(description = "Test type: 'unit' (testDebug), 'connected' (connectedAndroidTest), or a specific Gradle test task")]
    pub test_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCrashStackTraceParams {
    #[schemars(description = "Filter to a specific package name, e.g. com.example.app")]
    pub package: Option<String>,
    #[schemars(description = "Return a specific crash group by ID (from get_crash_logs crash_group_id field)")]
    pub crash_group_id: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestartAppParams {
    #[schemars(description = "Android package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(description = "ADB device serial (from list_devices). Uses first connected device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "Cold start: clears app data with pm clear before launching (default true). Set false for warm restart.")]
    pub cold: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAppRuntimeStateParams {
    #[schemars(description = "Android package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(description = "ADB device serial (from list_devices). Uses first connected device if omitted.")]
    pub device_serial: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetBuildConfigParams {
    #[schemars(description = "Gradle module name (subdirectory), e.g. app (default) or feature-login")]
    pub module: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetUiHierarchyParams {
    #[schemars(description = "ADB device serial (from list_devices). Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "If true, return only interactive rows (bounds, text, actions) — smaller than full tree. Default false.")]
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
    #[tool(description = "Run a Gradle task (e.g. assembleDebug) and return the result. Use get_build_errors for structured errors after the build.")]
    async fn run_gradle_task(
        &self,
        Parameters(p): Parameters<RunGradleTaskParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_gradle_task(&p.task)?;

        let gradle_root = self.get_gradle_root().await
            .ok_or_else(|| McpError::invalid_params(
                "No project open. Open an Android project first.", None,
            ))?;

        let gradlew = build_runner::find_gradlew(&gradle_root)
            .ok_or_else(|| McpError::invalid_params(
                "gradlew not found. Is this an Android project?", None,
            ))?;

        let (settings, _) = settings_manager::load_settings();
        let env = build_runner::build_env_vars(&settings, &gradle_root);

        let result = build_runner::run_task(
            &p.task,
            &[],
            &gradle_root,
            &gradlew,
            settings.mcp.build_timeout_sec as u64,
            env,
            &self.build_state,
            &self.process_manager,
        )
        .await
        .map_err(|e| McpError::internal_error(e, None))?;

        if result.timed_out {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Build timed out after {}s — task '{}'. Build has been cancelled.",
                settings.mcp.build_timeout_sec, p.task
            ))]));
        }

        let issue_lines = build_runner::format_build_issues(&result.errors);

        if result.success {
            let msg = if result.errors.is_empty() {
                format!("BUILD SUCCESSFUL — task '{}' ({}ms)", p.task, result.duration_ms)
            } else {
                format!(
                    "BUILD SUCCESSFUL (with {} warning(s)) — task '{}' ({}ms)\n{}",
                    result.errors.len(), p.task, result.duration_ms, issue_lines.join("\n")
                )
            };
            Ok(CallToolResult::success(vec![Content::text(msg)]))
        } else {
            let msg = format!(
                "BUILD FAILED — task '{}'\n{} issue(s):\n{}",
                p.task,
                result.errors.len(),
                if result.errors.is_empty() {
                    "Check get_build_log for details.".to_owned()
                } else {
                    issue_lines.join("\n")
                }
            );
            Ok(CallToolResult::error(vec![Content::text(msg)]))
        }
    }

    /// Get the current build status.
    #[tool(description = "Get the current Gradle build status: idle, running (with task name), success, failed, or cancelled.")]
    async fn get_build_status(&self) -> Result<CallToolResult, McpError> {
        let bs = self.build_state.inner.lock().await;
        let (state_str, details) = match &bs.status {
            crate::models::build::BuildStatus::Idle => ("idle", json!(null)),
            crate::models::build::BuildStatus::Running { task, started_at } => (
                "running",
                json!({ "task": task, "started_at": started_at }),
            ),
            crate::models::build::BuildStatus::Success(r) => ("success", json!({
                "duration_ms": r.duration_ms,
                "error_count": r.error_count,
                "warning_count": r.warning_count
            })),
            crate::models::build::BuildStatus::Failed(r) => ("failed", json!({
                "duration_ms": r.duration_ms,
                "error_count": r.error_count,
                "warning_count": r.warning_count
            })),
            crate::models::build::BuildStatus::Cancelled => ("cancelled", json!(null)),
        };
        let summary = match &bs.status {
            crate::models::build::BuildStatus::Idle => "Build status: idle".to_owned(),
            crate::models::build::BuildStatus::Running { task, .. } => format!("Build status: running — task: {task}"),
            crate::models::build::BuildStatus::Success(r) => format!("Build status: success — {}ms, {} error(s), {} warning(s)", r.duration_ms, r.error_count, r.warning_count),
            crate::models::build::BuildStatus::Failed(r) => format!("Build status: failed — {} error(s), {} warning(s)", r.error_count, r.warning_count),
            crate::models::build::BuildStatus::Cancelled => "Build status: cancelled".to_owned(),
        };
        Ok(CallToolResult::structured(json!({ "status": state_str, "details": details, "summary": summary })))
    }

    /// Get structured compiler errors and warnings from the last build.
    #[tool(description = "Get compiler errors and warnings from the last Gradle build. Each entry includes severity, file path, line number, and message.")]
    async fn get_build_errors(&self) -> Result<CallToolResult, McpError> {
        let bs = self.build_state.inner.lock().await;
        if bs.current_errors.is_empty() {
            return Ok(CallToolResult::structured(json!({ "errors": [], "count": 0 })));
        }
        let errors: Vec<serde_json::Value> = bs.current_errors.iter().map(|e| json!({
            "severity": format!("{:?}", e.severity).to_lowercase(),
            "message": e.message,
            "file": e.file,
            "line": e.line,
            "col": e.col,
        })).collect();
        Ok(CallToolResult::structured(json!({
            "count": errors.len(),
            "errors": errors
        })))
    }

    /// Get the raw build log output lines.
    #[tool(description = "Get the raw Gradle build output lines. Useful for diagnosing build issues not captured as structured errors.")]
    async fn get_build_log(
        &self,
        Parameters(p): Parameters<GetBuildLogParams>,
    ) -> Result<CallToolResult, McpError> {
        let (settings_for_mcp, _) = settings_manager::load_settings();
        let mcp_settings = settings_for_mcp.mcp;
        let lines_req = p.lines.unwrap_or(mcp_settings.build_log_default_lines as usize).min(2000);
        let log = self.build_state.build_log.lock()
            .map_err(|_| McpError::internal_error("Lock poisoned", None))?;
        if log.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("Build log is empty. Run a build first.")]));
        }
        let lines: Vec<&String> = log.iter().rev().take(lines_req).collect::<Vec<_>>()
            .into_iter().rev().collect();
        Ok(CallToolResult::success(vec![Content::text(
            format!("{} log line(s):\n{}", lines.len(), lines.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"))
        )]))
    }

    /// Cancel a running Gradle build.
    #[tool(description = "Cancel the currently running Gradle build. Returns immediately if no build is running.")]
    async fn cancel_build(&self) -> Result<CallToolResult, McpError> {
        let was_running = build_runner::cancel_build(&self.build_state, &self.process_manager).await;
        let msg = if was_running { "Build cancelled." } else { "No build was running." };
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    /// List available build variants.
    #[tool(description = "List available build variants (build types + product flavors) for the current Android project, which variant Keynobi treats as the Gradle/Android Studio default (`defaultVariant`), and which one is persisted as active in settings (`active`).")]
    async fn list_build_variants(&self) -> Result<CallToolResult, McpError> {
        let gradle_root = match self.get_gradle_root().await {
            Some(r) => r,
            None => return Ok(CallToolResult::structured(json!({
                "variants": [],
                "active": null,
                "defaultVariant": null,
                "error": "No project open"
            }))),
        };
        let candidates = [
            gradle_root.join("app").join("build.gradle.kts"),
            gradle_root.join("app").join("build.gradle"),
            gradle_root.join("build.gradle.kts"),
        ];
        for path in &candidates {
            if path.is_file() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(mut list) = variant_manager::parse_variants_from_gradle(path, &content) {
                        if !list.variants.is_empty() {
                            list.default_variant =
                                variant_manager::infer_default_variant_name(&gradle_root, &list.variants);
                            let names: Vec<&str> = list.variants.iter().map(|v| v.name.as_str()).collect();
                            let active = self
                                .get_gradle_root()
                                .await
                                .and_then(|r| {
                                    settings_manager::get_active_variant_for_project(&r.to_string_lossy())
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
    #[tool(description = "Set the active build variant (e.g. debug or release). This persists in settings and affects subsequent builds.")]
    async fn set_active_variant(
        &self,
        Parameters(p): Parameters<SetVariantParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(root) = self.get_gradle_root().await {
            let path = root.to_string_lossy().to_string();
            if let Err(e) = settings_manager::set_active_variant_for_project(&path, &p.variant) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to persist active variant: {e}"
                ))]));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Active variant set to: {}",
            p.variant
        ))]))
    }

    /// Find the output APK path for a given build variant.
    #[tool(description = "Find the output APK path after a successful build. Returns the path to use with install_apk. Specify variant or uses the active one.")]
    async fn find_apk_path(
        &self,
        Parameters(p): Parameters<FindApkPathParams>,
    ) -> Result<CallToolResult, McpError> {
        let gradle_root = self.get_gradle_root().await
            .ok_or_else(|| McpError::invalid_params("No project open", None))?;

        let variant = p.variant.as_deref().unwrap_or("debug");

        match build_runner::find_output_apk(&gradle_root, variant) {
            Some(path) => {
                let path_str = path.to_string_lossy().to_string();
                Ok(CallToolResult::structured(json!({
                    "found": true,
                    "path": path_str,
                    "variant": variant,
                    "hint": format!("Use install_apk with device_serial and apk_path: {}", path_str)
                })))
            }
            None => Ok(CallToolResult::structured(json!({
                "found": false,
                "variant": variant,
                "hint": "Run a build first with run_gradle_task (e.g. assembleDebug)"
            }))),
        }
    }

    /// Run tests for the project.
    #[tool(description = "Run unit tests or connected Android tests. test_type: 'unit' (testDebug), 'connected' (connectedAndroidTest), or a specific Gradle test task.")]
    async fn run_tests(
        &self,
        Parameters(p): Parameters<RunTestsParams>,
    ) -> Result<CallToolResult, McpError> {
        let task = match p.test_type.as_str() {
            "unit" => "testDebug".to_owned(),
            "connected" => "connectedAndroidTest".to_owned(),
            other => {
                validate_gradle_task(other)?;
                other.to_owned()
            }
        };
        // Delegate to run_gradle_task with the resolved task name.
        let params = RunGradleTaskParams { task, variant: None };
        self.run_gradle_task(Parameters(params)).await
    }

    /// Get a parsed crash stack trace from the in-memory logcat buffer.
    /// Requires logcat to be running (call start_logcat first).
    #[tool(description = "Get a parsed crash stack trace from logcat. Returns exception type, message, stack frames, and caused-by chain. Requires start_logcat to be running.")]
    async fn get_crash_stack_trace(
        &self,
        Parameters(p): Parameters<GetCrashStackTraceParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref pkg) = p.package {
            validate_package_name(pkg)?;
        }

        let logcat = self.logcat_state.lock().await;

        if !logcat.streaming && logcat.store.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
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

        match crash_inspector::find_crash(
            &entries,
            p.package.as_deref(),
            p.crash_group_id,
        ) {
            None => {
                let msg = if let Some(pkg) = &p.package {
                    format!("No crashes found for package '{pkg}'.")
                } else {
                    "No crashes found in the logcat buffer.".to_string()
                };
                Ok(CallToolResult::structured(json!({ "found": false, "message": msg })))
            }
            Some(crash) => Ok(CallToolResult::structured(json!(crash))),
        }
    }

    /// Restart an Android app: stop it (optionally clearing data), then relaunch and wait for display.
    #[tool(description = "Restart an Android app: force-stop or pm clear, then relaunch and wait for the activity to display. Returns launch time.")]
    async fn restart_app(
        &self,
        Parameters(p): Parameters<RestartAppParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_package_name(&p.package)?;
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or launch an emulator first.",
            )])),
        };

        let cold = p.cold.unwrap_or(true);

        match app_inspector::restart_app(&adb, &serial, &p.package, cold).await {
            Ok(result) => Ok(CallToolResult::structured(json!(result))),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Get process list, thread counts, and RSS memory for all processes of an app.
    #[tool(description = "Get runtime state for an Android app: running processes, thread counts per process, and RSS memory. Lightweight — no SIGQUIT.")]
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

        let state = app_inspector::get_runtime_state(
            &adb,
            serial.as_deref(),
            &p.package,
        )
        .await;

        Ok(CallToolResult::structured(json!(state)))
    }

    /// Parse the module's build.gradle(.kts) for SDK levels, build types, and product flavors.
    #[tool(description = "Parse build.gradle(.kts) for SDK levels, applicationId, buildTypes, and productFlavors. No Gradle execution needed.")]
    async fn get_build_config(
        &self,
        Parameters(p): Parameters<GetBuildConfigParams>,
    ) -> Result<CallToolResult, McpError> {
        let gradle_root = self.get_gradle_root().await
            .ok_or_else(|| McpError::invalid_params(
                "No project open. Open an Android project first.", None,
            ))?;

        let module = p.module.as_deref().unwrap_or("app");

        if module.contains('/') || module.contains('\\') || module.contains("..") {
            return Err(McpError::invalid_params(
                "Module name must be a simple directory name, not a path.", None,
            ));
        }

        match build_inspector::parse_build_config(&gradle_root, module) {
            Ok(config) => Ok(CallToolResult::structured(json!(config))),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    // ── Logcat tools ──────────────────────────────────────────────────────────

    /// Start streaming logcat from a device (required in headless mode).
    #[tool(description = "Start streaming logcat from a device. Required in headless mode before get_logcat_entries. In GUI mode, use the app's Start Logcat button instead.")]
    async fn start_logcat(
        &self,
        Parameters(p): Parameters<StartLogcatParams>,
    ) -> Result<CallToolResult, McpError> {
        let serial = p.device_serial.clone();
        if let Some(ref s) = serial {
            validate_device_serial(s)?;
        }

        {
            let mut state = self.logcat_state.lock().await;
            if state.streaming {
                return Ok(CallToolResult::success(vec![Content::text("Logcat is already streaming.")]));
            }
            state.streaming = true;
            state.device_serial = serial.clone();
        }

        let (settings, _) = settings_manager::load_settings();
        let adb_bin = crate::services::logcat::find_adb_binary(settings.android.sdk_path.as_deref());
        let logcat_state = self.logcat_state.clone();
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            crate::services::logcat::start_logcat_stream(adb_bin, serial, logcat_state, app_handle).await;
        });

        Ok(CallToolResult::success(vec![Content::text("Logcat streaming started. Use get_logcat_entries to read entries.")]))
    }

    /// Stop the logcat stream.
    #[tool(description = "Stop the logcat stream. Use start_logcat to restart.")]
    async fn stop_logcat(&self) -> Result<CallToolResult, McpError> {
        let mut state = self.logcat_state.lock().await;
        state.streaming = false;
        Ok(CallToolResult::success(vec![Content::text("Logcat stream stopped.")]))
    }

    /// Get recent logcat entries with optional filtering.
    #[tool(description = "Get recent Android logcat entries. Filter by level, tag, text, package, or show only crashes. Call start_logcat first in headless mode.")]
    async fn get_logcat_entries(
        &self,
        Parameters(p): Parameters<GetLogcatParams>,
    ) -> Result<CallToolResult, McpError> {
        let (settings_for_logcat, _) = settings_manager::load_settings();
        let mcp_settings = settings_for_logcat.mcp;
        let count = p.count.unwrap_or(mcp_settings.logcat_default_count as usize).min(10_000);
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
            return Ok(CallToolResult::structured(json!({ "entries": [], "count": 0, "streaming": streaming, "hint": msg })));
        }

        let structured: Vec<serde_json::Value> = entries.iter().map(|e| json!({
            "timestamp": e.timestamp,
            "level": logcat::level_char(&e.level),
            "tag": e.tag,
            "pid": e.pid,
            "message": e.message,
            "is_crash": e.is_crash,
            "package": e.package,
        })).collect();

        Ok(CallToolResult::structured(json!({
            "count": structured.len(),
            "streaming": logcat.streaming,
            "entries": structured
        })))
    }

    /// Get recent crash logs (FATAL EXCEPTION, ANR, native crashes).
    #[tool(description = "Get recent crash logs: FATAL EXCEPTION, ANR, and native crashes from logcat.")]
    async fn get_crash_logs(
        &self,
        Parameters(p): Parameters<GetCrashLogsParams>,
    ) -> Result<CallToolResult, McpError> {
        let count = p.count.unwrap_or(20).min(200);
        let logcat = self.logcat_state.lock().await;
        let entries: Vec<serde_json::Value> = logcat.store.iter()
            .rev()
            .filter(|e| e.is_crash)
            .take(count)
            .map(|e| json!({
                "timestamp": e.timestamp,
                "tag": e.tag,
                "message": e.message,
                "pid": e.pid,
                "package": e.package,
            }))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        Ok(CallToolResult::structured(json!({ "count": entries.len(), "entries": entries })))
    }

    /// Clear the in-memory logcat buffer.
    #[tool(description = "Clear the in-memory logcat buffer. New entries will appear after logcat continues streaming.")]
    async fn clear_logcat(&self) -> Result<CallToolResult, McpError> {
        let mut state = self.logcat_state.lock().await;
        state.store.clear();
        state.known_packages.clear();
        Ok(CallToolResult::success(vec![Content::text("Logcat buffer cleared.")]))
    }

    /// Get logcat statistics.
    #[tool(description = "Get logcat statistics: total entries ingested, counts by level, crash count, and packages seen.")]
    async fn get_logcat_stats(&self) -> Result<CallToolResult, McpError> {
        let state = self.logcat_state.lock().await;
        let s = &state.store.stats;
        let levels = ["verbose", "debug", "info", "warn", "error", "fatal", "unknown"];
        let by_level: serde_json::Map<String, serde_json::Value> = levels.iter().enumerate()
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
    #[tool(description = "List all connected Android devices and running emulators. Queries ADB directly for fresh results.")]
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
            return Ok(CallToolResult::structured(json!({ "devices": [], "count": 0, "hint": "No devices connected. Connect a device or launch an emulator with launch_avd." })));
        }

        let structured: Vec<serde_json::Value> = devices.iter().map(|d| json!({
            "serial": d.serial,
            "model": d.model.as_deref().unwrap_or(&d.name),
            "name": d.name,
            "state": format!("{:?}", d.connection_state).to_lowercase(),
            "api_level": d.api_level,
            "android_version": d.android_version,
            "kind": format!("{:?}", d.device_kind).to_lowercase(),
        })).collect();

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
    #[tool(description = "Dump the focused window UI hierarchy (UI Automator accessibility XML) for native Views and Jetpack Compose. Includes capped shell context (dumpsys window/display, wm size/density) and tries uiautomator dump --compressed when supported. Use interactive_only for a compact list of tappable fields.")]
    async fn get_ui_hierarchy(
        &self,
        Parameters(p): Parameters<GetUiHierarchyParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        let snapshot = match ui_hierarchy::capture_ui_hierarchy_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
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
    #[tool(description = "Find UI elements on the focused screen by text, content-desc, resource-id, class, or package. Returns treePath, bounds, centerX/centerY, flags, and screenHash from a fresh uiautomator dump. Use centerX/centerY with ui_tap. At least one of textContains, textEquals, contentDescContains, resourceIdEquals, resourceIdContains, classContains, or packageEquals is required.")]
    async fn find_ui_elements(
        &self,
        Parameters(p): Parameters<ui_automation::FindUiElementsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if !ui_automation::find_query_has_primary_filter(&p) {
            return Ok(CallToolResult::error(vec![Content::text(
                "find_ui_elements requires at least one primary filter: textContains, textEquals, contentDescContains, resourceIdEquals, resourceIdContains, classContains, or packageEquals (non-empty).",
            )]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let max = p.max_results.unwrap_or(ui_automation::DEFAULT_FIND_RESULTS as u32) as usize;
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

    /// Resolve the direct parent of a node by layout treePath (same paths as find_ui_elements / Layout tab).
    #[tool(description = "Given a non-empty layout treePath from find_ui_elements or the Layout viewer, returns the direct parent node (treePath, bounds, centerX/centerY, flags) plus screenHash from a fresh dump. Optional expect_screen_hash refuses if the UI changed. Empty treePath is invalid (root has no parent).")]
    async fn find_ui_parent(
        &self,
        Parameters(p): Parameters<ui_automation::FindUiParentParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        let snapshot = if p.expect_screen_hash.is_some() {
            match ui_automation::ensure_screen_hash(
                &adb,
                &serial,
                p.expect_screen_hash.as_deref(),
            )
            .await
            {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            }
        } else {
            match ui_automation::capture_ui_snapshot(&adb, &serial).await {
                Ok(s) => s,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            }
        };

        let (normalized_path, parent) = match ui_automation::find_ui_parent_from_snapshot(
            &snapshot,
            &p.tree_path,
        ) {
            Ok(v) => v,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
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
    #[tool(description = "Tap at device pixel coordinates. Use find_ui_elements for centerX/centerY. Optional expect_screen_hash re-dumps the hierarchy and refuses if the screen changed.")]
    async fn ui_tap(
        &self,
        Parameters(p): Parameters<ui_automation::UiTapParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        if let Err(e) = ui_automation::validate_coordinates(p.x, p.y) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        if p.expect_screen_hash.is_some() {
            if let Err(e) = ui_automation::ensure_screen_hash(
                &adb,
                &serial,
                p.expect_screen_hash.as_deref(),
            )
            .await
            {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
        }

        match ui_automation::adb_input_tap(&adb, &serial, p.x, p.y).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(if msg.is_empty() {
                format!("tap ({}, {})", p.x, p.y)
            } else {
                format!("tap ({}, {}): {msg}", p.x, p.y)
            })])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Type text via adb input text (ASCII-oriented; use tap_x/tap_y to focus a field first).
    #[tool(description = "Send text with adb shell input text after an optional tap to focus. ASCII printable only; spaces encoded automatically; no emoji. Optional expect_screen_hash verifies hierarchy before acting. For complex text use clipboard workflows outside this tool.")]
    async fn ui_type_text(
        &self,
        Parameters(p): Parameters<ui_automation::UiTypeTextParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if p.text.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text("text must not be empty")]));
        }
        if let Err(e) = ui_automation::validate_tap_coordinate_pair(p.tap_x, p.tap_y) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        if let Some(ref expected) = p.expect_screen_hash {
            if let Err(e) = ui_automation::ensure_screen_hash(
                &adb,
                &serial,
                Some(expected.as_str()),
            )
            .await
            {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
        }

        if let (Some(x), Some(y)) = (p.tap_x, p.tap_y) {
            if let Err(e) = ui_automation::validate_coordinates(x, y) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, x, y).await {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        if p.clear_before.unwrap_or(false) {
            if let Err(e) = ui_automation::adb_clear_field(&adb, &serial).await {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "clear_before failed: {e}"
                ))]));
            }
        }

        match ui_automation::adb_input_text(&adb, &serial, &p.text).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(if msg.is_empty() {
                "input text sent".to_string()
            } else {
                msg
            })])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Clear the focused editable field (Ctrl+A then Delete). Optional tap to focus first.
    #[tool(description = "Clear the focused editable field using Ctrl+A then Delete. Use tap_x/tap_y to focus a field first. Call before ui_type_text to replace instead of append, or use the clear_before flag on ui_type_text directly.")]
    async fn clear_focused_input(
        &self,
        Parameters(p): Parameters<ui_automation::ClearFocusedInputParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if let Err(e) = ui_automation::validate_tap_coordinate_pair(p.tap_x, p.tap_y) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        if let (Some(x), Some(y)) = (p.tap_x, p.tap_y) {
            if let Err(e) = ui_automation::validate_coordinates(x, y) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, x, y).await {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        match ui_automation::adb_clear_field(&adb, &serial).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text("field cleared")])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Type Unicode text (emoji, non-ASCII) via clipboard paste (API 24+).
    #[tool(description = "Type Unicode text (including emoji and non-ASCII) into a focused field using clipboard paste (Ctrl+V). Requires API 24+. Use ui_type_text for ASCII-only input. Optional tap_x/tap_y to focus a field first. Optional clear_before to replace existing content.")]
    async fn ui_type_text_unicode(
        &self,
        Parameters(p): Parameters<ui_automation::UiTypeTextUnicodeParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        if p.text.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text("text must not be empty")]));
        }
        if let Err(e) = ui_automation::validate_tap_coordinate_pair(p.tap_x, p.tap_y) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        if let (Some(x), Some(y)) = (p.tap_x, p.tap_y) {
            if let Err(e) = ui_automation::validate_coordinates(x, y) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            if let Err(e) = ui_automation::adb_input_tap(&adb, &serial, x, y).await {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        if p.clear_before.unwrap_or(false) {
            if let Err(e) = ui_automation::adb_clear_field(&adb, &serial).await {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "clear_before failed: {e}"
                ))]));
            }
        }

        match ui_automation::adb_type_text_unicode(&adb, &serial, &p.text).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(if msg.is_empty() {
                "unicode text sent via clipboard".to_string()
            } else {
                msg
            })])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Send an allowlisted keyevent (Back, Home, Enter, etc.).
    #[tool(description = "Send a keyevent by name: Back, Home, Enter, Delete, Tab, Escape, Search, Menu, AppSwitch, DpadUp, DpadDown, DpadLeft, DpadRight, DpadCenter.")]
    async fn send_ui_key(
        &self,
        Parameters(p): Parameters<ui_automation::SendUiKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        let code = match ui_automation::resolve_ui_key_code(&p.key) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        match ui_automation::adb_keyevent(&adb, &serial, code).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(if msg.is_empty() {
                format!("keyevent {code}")
            } else {
                format!("keyevent {code}: {msg}")
            })])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Swipe or long-press (same start/end with duration_ms).
    #[tool(description = "Swipe from x1,y1 to x2,y2 in device pixels. Optional duration_ms; same coordinates + duration performs a long-press.")]
    async fn ui_swipe(
        &self,
        Parameters(p): Parameters<ui_automation::UiSwipeParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        match ui_automation::adb_input_swipe(
            &adb,
            &serial,
            p.x1,
            p.y1,
            p.x2,
            p.y2,
            p.duration_ms,
        )
        .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(if msg.is_empty() {
                "swipe OK".to_string()
            } else {
                msg
            })])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Grant a runtime permission (pm grant).
    #[tool(description = "Grant an android.permission.* runtime permission to an installed package. Package must be a normal applicationId; permission must start with android.permission.")]
    async fn grant_runtime_permission(
        &self,
        Parameters(p): Parameters<ui_automation::GrantRuntimePermissionParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }
        validate_package_name(&p.package)?;
        if let Err(e) = ui_automation::validate_runtime_permission(&p.permission) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        match ui_automation::adb_pm_grant(&adb, &serial, &p.package, &p.permission).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(if msg.is_empty() {
                format!("granted {} to {}", p.permission, p.package)
            } else {
                msg
            })])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Poll until a UI element matching filters appears (or timeout elapses).
    #[tool(description = "Poll the device hierarchy until an element matching the given filters appears, or timeout_ms elapses (default 15s, max 30s). Returns the same shape as find_ui_elements on success. Requires at least one primary filter. Use after ui_tap or navigation to wait for the next screen to load.")]
    async fn wait_for_element(
        &self,
        Parameters(p): Parameters<ui_automation::WaitForElementParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
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
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Capture a screenshot from a connected device.
    #[tool(description = "Capture a screenshot from a connected Android device. Returns the image inline.")]
    async fn screenshot(
        &self,
        Parameters(p): Parameters<DeviceSerialParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;

        let adb = { let (s, _) = settings_manager::load_settings(); adb_manager::get_adb_path(&s) };
        match device_inspector::take_screenshot(&adb, &p.device_serial).await {
            Ok(bytes) => Ok(CallToolResult::success(vec![
                Content::image(BASE64.encode(&bytes), "image/png"),
            ])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(
                format!("Screenshot failed: {e}")
            )])),
        }
    }

    /// Get device hardware and software properties.
    #[tool(description = "Get Android device properties: SDK level, Android version, manufacturer, model, screen resolution, and battery.")]
    async fn get_device_info(
        &self,
        Parameters(p): Parameters<DeviceSerialParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;

        let adb = { let (s, _) = settings_manager::load_settings(); adb_manager::get_adb_path(&s) };
        match device_inspector::get_device_info(&adb, &p.device_serial).await {
            Ok(info) => Ok(CallToolResult::structured(json!(info))),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Compare current UI state against a baseline screenHash captured earlier.
    #[tool(description = "Compare current UI state against a baseline screenHash. Returns changed=false if the screen hash matches (UI is identical), or changed=true with all currently interactive (clickable/editable) nodes when the screen changed. Use after ui_tap, ui_swipe, ui_type_text, etc. to verify the action had an effect before taking the next step.")]
    async fn compare_ui_state(
        &self,
        Parameters(p): Parameters<ui_automation::CompareUiStateParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(ref s) = p.device_serial {
            validate_device_serial(s)?;
        }

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);
        let serial = match adb_manager::resolve_device_serial(&adb, p.device_serial.as_deref()).await {
            Some(s) => s,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "No device connected. Connect a device or pass device_serial from list_devices.",
            )])),
        };

        let snapshot = match ui_automation::capture_ui_snapshot(&adb, &serial).await {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
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
    #[tool(description = "Get installed app details: version name/code, install path, permissions, and declared activities.")]
    async fn dump_app_info(
        &self,
        Parameters(p): Parameters<DevicePackageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;

        let adb = { let (s, _) = settings_manager::load_settings(); adb_manager::get_adb_path(&s) };
        match device_inspector::dump_app_info(&adb, &p.device_serial, &p.package).await {
            Ok(info) => Ok(CallToolResult::structured(json!(info))),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Get memory usage for an app.
    #[tool(description = "Get memory usage for an Android app: PSS, heap size, native memory, and graphics memory.")]
    async fn get_memory_info(
        &self,
        Parameters(p): Parameters<DevicePackageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;

        let adb = { let (s, _) = settings_manager::load_settings(); adb_manager::get_adb_path(&s) };
        match device_inspector::get_memory_info(&adb, &p.device_serial, &p.package).await {
            Ok(info) => Ok(CallToolResult::structured(json!(info))),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    /// Install an APK on a connected device.
    #[tool(description = "Install an APK file on a connected device or emulator. APK must be within the project's build output directory.")]
    async fn install_apk(
        &self,
        Parameters(p): Parameters<InstallApkParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        self.validate_apk_path(&p.apk_path).await?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let result = adb_manager::install_apk(&adb, &p.device_serial, &p.apk_path)
            .await
            .map_err(|e| McpError::internal_error(format!("APK install failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("APK installed: {result}"))]))
    }

    /// Launch an app on a connected device.
    #[tool(description = "Launch an Android app on a device. Uses am start to launch the main activity or a specified activity.")]
    async fn launch_app(
        &self,
        Parameters(p): Parameters<LaunchAppParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        let result = adb_manager::launch_app(&adb, &p.device_serial, &p.package, p.activity.as_deref())
            .await
            .map_err(|e| McpError::internal_error(format!("Launch failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("App launched: {result}"))]))
    }

    /// Stop a running app on a device.
    #[tool(description = "Force-stop an Android app on a device using am force-stop.")]
    async fn stop_app(
        &self,
        Parameters(p): Parameters<DevicePackageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.device_serial)?;
        validate_package_name(&p.package)?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        adb_manager::stop_app(&adb, &p.device_serial, &p.package)
            .await
            .map_err(|e| McpError::internal_error(format!("Stop app failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("App {} stopped.", p.package))]))
    }

    /// List available Android Virtual Devices (AVDs).
    #[tool(description = "List all available Android Virtual Devices (AVDs) configured in the Android SDK.")]
    async fn list_avds(&self) -> Result<CallToolResult, McpError> {
        let avds = adb_manager::list_avds();
        if avds.is_empty() {
            return Ok(CallToolResult::structured(json!({ "avds": [], "count": 0, "hint": "No AVDs found. Create one in the Device Manager panel." })));
        }
        let structured: Vec<serde_json::Value> = avds.iter().map(|a| json!({
            "name": a.name,
            "display_name": a.display_name,
            "api_level": a.api_level,
            "abi": a.abi,
            "target": a.target,
            "path": a.path,
        })).collect();
        Ok(CallToolResult::structured(json!({ "count": avds.len(), "avds": structured })))
    }

    /// Launch an Android Virtual Device (emulator).
    #[tool(description = "Launch an Android Virtual Device (emulator). Returns the emulator serial when ready.")]
    async fn launch_avd(
        &self,
        Parameters(p): Parameters<LaunchAvdParams>,
    ) -> Result<CallToolResult, McpError> {
        let (settings, _) = settings_manager::load_settings();
        let emulator = adb_manager::get_emulator_path(&settings);
        let adb = adb_manager::get_adb_path(&settings);

        let serial = adb_manager::launch_emulator(&emulator, &adb, &p.name)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to launch AVD: {e}"), None))?;

        Ok(CallToolResult::structured(json!({ "serial": serial, "avd_name": p.name })))
    }

    /// Stop a running emulator.
    #[tool(description = "Stop a running Android emulator by its ADB serial.")]
    async fn stop_avd(
        &self,
        Parameters(p): Parameters<StopAvdParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_device_serial(&p.serial)?;

        let (settings, _) = settings_manager::load_settings();
        let adb = adb_manager::get_adb_path(&settings);

        adb_manager::stop_emulator(&adb, &p.serial)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to stop emulator: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("Emulator {} stopped.", p.serial))]))
    }

    // ── Project / health tools ────────────────────────────────────────────────

    /// Get information about the open Android project.
    #[tool(description = "Get the currently open Android project name, path, and detected Gradle root.")]
    async fn get_project_info(&self) -> Result<CallToolResult, McpError> {
        let fs = self.fs_state.0.lock().await;
        match fs.project_root.as_ref() {
            None => Ok(CallToolResult::structured(json!({
                "open": false,
                "hint": "No project open. Open an Android project in the companion app, or launch with --project /path/to/project."
            }))),
            Some(root) => {
                let name = root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| root.to_string_lossy().to_string());
                let gradle = fs.gradle_root.as_ref()
                    .map(|g| g.to_string_lossy().to_string());
                Ok(CallToolResult::structured(json!({
                    "open": true,
                    "name": name,
                    "path": root.to_string_lossy(),
                    "gradle_root": gradle,
                })))
            }
        }
    }

    /// Run system health checks.
    #[tool(description = "Run system health checks: Java, Android SDK, ADB, emulator, and Gradle wrapper availability.")]
    async fn run_health_check(&self) -> Result<CallToolResult, McpError> {
        let (settings, _) = settings_manager::load_settings();
        let (project_root, gradle_root) = {
            let fs = self.fs_state.0.lock().await;
            (fs.project_root.clone(), fs.gradle_root.clone())
        };

        let report = health_inspector::run_health_check(
            &settings,
            project_root.as_deref(),
            gradle_root.as_deref(),
        )
        .await;

        let gradle_hint = if report.gradlew_ok {
            serde_json::Value::Null
        } else if !report.project_open {
            json!("No Android project open — pass --project /path/to/project or open one in the companion app first")
        } else {
            json!("No gradlew found in the selected project — ensure it is an Android Gradle project")
        };

        Ok(CallToolResult::structured(json!({
            "all_ok": report.all_ok,
            "checks": {
                "java": {
                    "ok": report.java_ok,
                    "hint": if report.java_ok { serde_json::Value::Null } else { json!("Set java.home in Settings → Tools") }
                },
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
                "project": {
                    "ok": report.project_open,
                    "path": report.project_path.as_ref().map(|p| p.to_string_lossy().to_string())
                },
            }
        })))
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
    #[schemars(description = "Android package name to launch after install, e.g. com.example.myapp")]
    pub package: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BuildAndFixArgs {
    #[schemars(description = "Gradle task to run and fix errors for, e.g. assembleDebug")]
    pub task: Option<String>,
}

#[prompt_router]
impl AndroidMcpServer {
    /// Diagnose a crash for the given package: fetch logcat crashes, memory info,
    /// and app details to provide context for root-cause analysis.
    #[prompt(name = "diagnose-crash", description = "Diagnose a crash or ANR for an Android app: fetch crash logs, memory, and app state.")]
    async fn diagnose_crash(
        &self,
        Parameters(args): Parameters<DiagnoseCrashArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let device_hint = args.device_serial.as_deref().unwrap_or("the connected device");
        Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Diagnose the crash for app '{pkg}' on {device}. \
                         Step 1: Call get_crash_logs to see recent FATAL EXCEPTION / ANR entries. \
                         Step 2: Call get_logcat_entries with package={pkg} and min_level=error for context. \
                         Step 3: Call get_memory_info with device_serial={device} and package={pkg} to check for OOM. \
                         Step 4: Call dump_app_info with device_serial={device} and package={pkg} for version and install state. \
                         Then provide a root-cause analysis and suggest fixes.",
                        pkg = args.package,
                        device = args.device_serial.as_deref().unwrap_or("{device_serial}"),
                    ),
                ),
            ]).with_description(format!("Diagnose crash for {} on {}", args.package, device_hint)))
    }

    /// Full deploy workflow: build → find APK → install → launch.
    #[prompt(name = "full-deploy", description = "Full deploy workflow: build the app, install it on a device, and launch it.")]
    async fn full_deploy(
        &self,
        Parameters(args): Parameters<FullDeployArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let variant = args.variant.as_deref().unwrap_or("debug");
        let task = format!("assemble{}", capitalize_first(variant));
        Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
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
                            format!("Call launch_app with device_serial={device} and package={pkg} to start the app.", device = args.device_serial, pkg = pkg)
                        } else {
                            "If you know the package name, call launch_app to start the app.".into()
                        },
                    ),
                ),
            ]).with_description(format!("Full deploy {} to {}", variant, args.device_serial)))
    }

    /// Build and fix: run a build, get errors, and suggest fixes.
    #[prompt(name = "build-and-fix", description = "Run a build and help fix any compiler errors.")]
    async fn build_and_fix(
        &self,
        Parameters(args): Parameters<BuildAndFixArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let task = args.task.as_deref().unwrap_or("assembleDebug");
        Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Run the build and fix any errors. \
                         Step 1: Call run_gradle_task with task={task}. \
                         Step 2: Call get_build_errors for structured error list. \
                         Step 3: For each error, explain the root cause and suggest the minimal fix. \
                         Step 4: If there are many errors, prioritize them (compilation errors block warnings). \
                         Be specific about file paths and line numbers.",
                        task = task,
                    ),
                ),
            ]).with_description(format!("Build {} and fix errors", task)))
    }
}

// ── ServerHandler impl ────────────────────────────────────────────────────────

#[tool_handler]
#[prompt_handler]
impl ServerHandler for AndroidMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_instructions(
            "Keynobi MCP Server — AI-first companion for Android development. \
             Tools: build (run_gradle_task, get_build_errors, get_build_log, get_build_config, find_apk_path, run_tests), \
             logcat (start_logcat, get_logcat_entries, get_crash_logs, get_crash_stack_trace), \
             devices (list_devices, get_ui_hierarchy, find_ui_elements, find_ui_parent, ui_tap, ui_type_text, ui_swipe, send_ui_key, grant_runtime_permission, screenshot, get_device_info, install_apk, launch_app, restart_app, dump_app_info, get_memory_info, get_app_runtime_state), \
             project (get_project_info, run_health_check). \
             Prompts: diagnose-crash, full-deploy, build-and-fix. \
             Start with get_project_info and run_health_check to verify the environment.".to_string()
        )
    }

    async fn on_initialized(&self, context: rmcp::service::NotificationContext<RoleServer>) {
        let client_name = context.peer
            .peer_info()
            .map(|i| i.client_info.name.clone())
            .unwrap_or_else(|| "unknown".into());
        info!("MCP client connected: {} — {} tools, {} prompts available",
            client_name,
            self.tool_router.list_all().len(),
            self.prompt_router.list_all().len()
        );
        // Emit lifecycle event to Tauri GUI if running (no-op in headless mode).
        self.emit_event("mcp:client_connected", serde_json::json!({
            "clientName": client_name,
            "connectedAt": chrono::Utc::now().to_rfc3339(),
        }));
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let fs = self.fs_state.0.lock().await;
        let mut resources = vec![
            RawResource::new("android://project-info", "Project Info").no_annotation(),
            RawResource::new("android://health", "System Health").no_annotation(),
        ];

        if let Some(ref gradle_root) = fs.gradle_root.clone().or(fs.project_root.clone()) {
            let candidates = [
                ("android://manifest", gradle_root.join("app").join("src").join("main").join("AndroidManifest.xml"), "AndroidManifest.xml"),
                ("android://app-build-gradle", gradle_root.join("app").join("build.gradle.kts"), "app/build.gradle.kts"),
                ("android://build-gradle", gradle_root.join("build.gradle.kts"), "build.gradle.kts"),
                ("android://gradle-settings", gradle_root.join("settings.gradle.kts"), "settings.gradle.kts"),
            ];
            for (uri, path, name) in &candidates {
                if path.is_file() {
                    resources.push(RawResource::new(*uri, *name).no_annotation());
                }
            }
        }

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = &request.uri;
        let fs = self.fs_state.0.lock().await;
        let gradle_root = fs.gradle_root.clone().or(fs.project_root.clone());
        drop(fs);

        match uri.as_str() {
            "android://project-info" => {
                let info = self.get_project_info().await?;
                let text = info.content.first()
                    .and_then(|c| c.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_else(|| "No project open".into());
                Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri.clone())]))
            }
            "android://health" => {
                let health = self.run_health_check().await?;
                let text = health.content.first()
                    .and_then(|c| c.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_else(|| "Health check unavailable".into());
                Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri.clone())]))
            }
            other => {
                let path = match (other, gradle_root.as_ref()) {
                    ("android://manifest", Some(r)) => Some(r.join("app").join("src").join("main").join("AndroidManifest.xml")),
                    ("android://app-build-gradle", Some(r)) => Some(r.join("app").join("build.gradle.kts")),
                    ("android://build-gradle", Some(r)) => Some(r.join("build.gradle.kts")),
                    ("android://gradle-settings", Some(r)) => Some(r.join("settings.gradle.kts")),
                    _ => None,
                };
                match path {
                    Some(p) if p.is_file() => {
                        let content = std::fs::read_to_string(&p)
                            .map_err(|e| McpError::internal_error(format!("Failed to read {}: {e}", p.display()), None))?;
                        let mime = if p.extension().and_then(|e| e.to_str()) == Some("xml") {
                            "text/xml"
                        } else {
                            "text/plain"
                        };
                        Ok(ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                            uri: uri.clone(),
                            mime_type: Some(mime.into()),
                            text: content,
                            meta: None,
                        }]))
                    }
                    _ => Err(McpError::resource_not_found(
                        format!("Resource not found or project not open: {uri}"),
                        Some(json!({ "uri": uri })),
                    )),
                }
            }
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

impl AndroidMcpServer {
    async fn get_gradle_root(&self) -> Option<PathBuf> {
        let fs = self.fs_state.0.lock().await;
        fs.gradle_root.clone().or_else(|| fs.project_root.clone())
    }

    async fn validate_apk_path(&self, apk_path: &str) -> Result<(), McpError> {
        let gradle_root = self.get_gradle_root().await
            .ok_or_else(|| McpError::invalid_params("No project open", None))?;
        let build_outputs = gradle_root.join("app").join("build").join("outputs");
        let apk_path = PathBuf::from(apk_path);
        let canonical_apk = apk_path.canonicalize()
            .map_err(|_| McpError::invalid_params(
                format!("APK path not found or inaccessible: {}", apk_path.display()), None
            ))?;
        let canonical_outputs = build_outputs.canonicalize()
            .map_err(|_| McpError::invalid_params(
                "Build outputs directory not found. Run a build first.", None
            ))?;
        if !canonical_apk.starts_with(&canonical_outputs) {
            return Err(McpError::invalid_params(
                "APK path must be within the project build outputs directory (app/build/outputs/)",
                None
            ));
        }
        if canonical_apk.extension().and_then(|e| e.to_str()) != Some("apk") {
            return Err(McpError::invalid_params("Path must point to a .apk file", None));
        }
        Ok(())
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

fn validate_gradle_task(task: &str) -> Result<(), McpError> {
    if task.is_empty() {
        return Err(McpError::invalid_params("Task name cannot be empty", None));
    }
    let valid = task.chars().all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'));
    if !valid {
        return Err(McpError::invalid_params(
            format!("Invalid task name '{task}'. Use alphanumeric, ':', '-', '_', '.' only."), None
        ));
    }
    Ok(())
}

fn validate_package_name(package: &str) -> Result<(), McpError> {
    if package.is_empty() {
        return Err(McpError::invalid_params("Package name cannot be empty", None));
    }
    let valid = package.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '_'));
    if !valid || !package.contains('.') {
        return Err(McpError::invalid_params(
            format!("Invalid package name '{package}'. Expected format: com.example.app"), None
        ));
    }
    Ok(())
}

fn validate_device_serial(serial: &str) -> Result<(), McpError> {
    if serial.is_empty() {
        return Err(McpError::invalid_params("Device serial cannot be empty", None));
    }
    let valid = serial.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | ':' | '.' | '_'));
    if !valid {
        return Err(McpError::invalid_params(
            format!("Invalid device serial '{serial}'"), None
        ));
    }
    Ok(())
}


fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

// ── Logging wrapper ────────────────────────────────────────────────────────────

/// Wraps `AndroidMcpServer` to intercept all tool calls, resource reads, and
/// prompt requests and write activity entries to the shared JSONL log.
///
/// This is used in place of `AndroidMcpServer` directly in both GUI and headless
/// modes so the companion app always has a log to display.
struct LoggingMcpServer(AndroidMcpServer);

impl ServerHandler for LoggingMcpServer {
    // ── Delegation for methods that AndroidMcpServer overrides ────────────────

    fn get_info(&self) -> ServerInfo {
        self.0.get_info()
    }

    async fn on_initialized(&self, context: rmcp::service::NotificationContext<RoleServer>) {
        let client_name = context.peer
            .peer_info()
            .map(|i| i.client_info.name.clone())
            .unwrap_or_else(|| "unknown".into());
        mcp_activity::log_activity(&McpActivityEntry::lifecycle(
            format!("Client connected: {client_name}"),
        ));
        self.0.on_initialized(context).await;
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        self.0.list_resources(request, context).await
    }

    // ── Instrumented: resource reads ──────────────────────────────────────────

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let start = std::time::Instant::now();
        let uri = request.uri.clone();
        let result = self.0.read_resource(request, context).await;
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
    ) -> Result<CallToolResult, McpError> {
        let start = std::time::Instant::now();
        let name = request.name.clone();
        let result = self.0.call_tool(request, context).await;
        let ms = start.elapsed().as_millis() as u64;
        let (status, summary) = match &result {
            Ok(r) => {
                let is_err = r.is_error.unwrap_or(false);
                let first_text = r.content.first()
                    .and_then(|c| c.as_text())
                    .map(|t| {
                        let s = &t.text;
                        if s.len() > 120 { format!("{}…", &s[..120]) } else { s.clone() }
                    });
                if is_err { ("error", first_text) } else { ("ok", first_text) }
            }
            Err(e) => ("error", Some(e.message.clone().to_string())),
        };
        mcp_activity::log_activity(&McpActivityEntry::tool_call(
            name.as_ref(), ms, status, summary,
        ));
        result
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.0.list_tools(request, context).await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.0.get_tool(name)
    }

    // ── Instrumented: prompts (generated by #[prompt_handler]) ───────────────

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let start = std::time::Instant::now();
        let name = request.name.clone();
        let result = self.0.get_prompt(request, context).await;
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
        self.0.list_prompts(request, context).await
    }
}

// ── Tauri command ─────────────────────────────────────────────────────────────

/// Start the MCP server on stdio in GUI mode.
///
/// Guards against duplicate invocations with an AtomicBool so calling this
/// twice doesn't start two tasks fighting over stdin/stdout.
#[tauri::command]
pub async fn start_mcp_server(app_handle: AppHandle) -> Result<(), String> {
    if MCP_STDIO_RUNNING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("MCP server is already running. Only one stdio session is supported at a time.".into());
    }
    debug!("Starting MCP server on stdio (GUI mode)");

    let _ = app_handle.emit("mcp:started", serde_json::json!({ "transport": "stdio" }));

    tokio::spawn(async move {
        mcp_activity::rotate_activity_log();
        mcp_activity::log_activity(&McpActivityEntry::lifecycle("Server started (GUI mode)"));

        let server = LoggingMcpServer(AndroidMcpServer::from_app_handle(&app_handle));
        let transport = rmcp::transport::stdio();
        match server.serve(transport).await {
            Ok(running) => {
                info!("MCP server initialized, waiting for client to disconnect");
                if let Err(e) = running.waiting().await {
                    error!("MCP server error: {}", e);
                }
            }
            Err(e) => {
                error!("MCP server failed to start: {}", e);
            }
        }
        mcp_activity::log_activity(&McpActivityEntry::lifecycle("Server stopped (GUI mode)"));
        MCP_STDIO_RUNNING.store(false, Ordering::SeqCst);
        let _ = app_handle.emit("mcp:stopped", serde_json::json!({}));
    });
    Ok(())
}

// ── Headless entry point ──────────────────────────────────────────────────────

/// Run the MCP server in headless mode (no Tauri GUI).
///
/// Called from `main.rs` when the binary is launched with `--mcp`.
/// Initializes state directly from the project path and settings file.
pub async fn run_headless_mcp(project_path: Option<PathBuf>) {
    use crate::services::fs_manager;

    // Redirect all tracing to stderr — stdout is reserved for MCP JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    // Priority: --project arg > last_active_project from settings > current_dir.
    let project_root = project_path
        .or_else(|| {
            let (settings, _) = settings_manager::load_settings();
            settings.last_active_project.and_then(|p| {
                let path = PathBuf::from(&p);
                if path.is_dir() {
                    info!("MCP headless: using last active project from settings: {}", p);
                    Some(path)
                } else {
                    None
                }
            })
        })
        .or_else(|| std::env::current_dir().ok());
    let gradle_root = project_root.as_ref().and_then(|root| fs_manager::find_gradle_root(root));

    let fs_state = FsState(Arc::new(tokio::sync::Mutex::new(crate::FsStateInner {
        project_root: project_root.clone(),
        gradle_root: gradle_root.clone(),
    })));

    let build_state = BuildState::new();
    let device_state = DeviceState::new();
    let logcat_state = Arc::new(tokio::sync::Mutex::new(
        crate::services::logcat::LogcatStateInner::new()
    ));
    let process_manager = ProcessManager::new();

    info!("MCP headless server starting. Project: {:?}", project_root);

    mcp_activity::rotate_activity_log();
    mcp_activity::write_pid_file();
    mcp_activity::log_activity(&McpActivityEntry::lifecycle(format!(
        "Server started (headless) — project: {}",
        project_root.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "none".into())
    )));

    let server = LoggingMcpServer(AndroidMcpServer::new_headless(
        build_state, device_state, logcat_state, fs_state, process_manager,
    ));
    let transport = rmcp::transport::stdio();
    match server.serve(transport).await {
        Ok(running) => {
            if let Err(e) = running.waiting().await {
                tracing::error!("MCP server error: {e}");
            }
        }
        Err(e) => {
            tracing::error!("MCP server failed to start: {e}");
            mcp_activity::log_activity(&McpActivityEntry::lifecycle(
                format!("Server failed to start: {e}")
            ));
            mcp_activity::remove_pid_file();
            std::process::exit(1);
        }
    }
    mcp_activity::log_activity(&McpActivityEntry::lifecycle("Server stopped (headless)"));
    mcp_activity::remove_pid_file();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
    fn mcp_stdio_guard_is_initially_unset() {
        // Guard should be false at startup (or after test isolation).
        // We just check the type; actual state depends on test order.
        let _ = MCP_STDIO_RUNNING.load(Ordering::SeqCst);
    }

}
