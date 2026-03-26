/**
 * MCP Server for Android Dev Companion
 *
 * Exposes build, logcat, and device tools to Claude Code (or any MCP-compatible client).
 * Implements the Model Context Protocol 2024-11-05 specification.
 *
 * Transport: stdio (for `claude mcp add android-companion`)
 */
use crate::services::logcat::{LogcatFilter, LogcatLevel, LogcatState};
use crate::services::settings_manager;
use crate::services::variant_manager;
use crate::FsState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error};

// ── MCP Protocol Types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct McpRequest {
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i32,
    message: String,
}

impl McpError {
    fn invalid_params(msg: impl Into<String>) -> Self {
        McpError { code: -32602, message: msg.into() }
    }
    fn not_found(msg: impl Into<String>) -> Self {
        McpError { code: -32001, message: msg.into() }
    }
}

// ── MCP Tauri Command ─────────────────────────────────────────────────────────

/// Start the MCP server on stdio. Call this from the Tauri command when the
/// app is launched with the `--mcp` flag.
#[tauri::command]
pub async fn start_mcp_server(app_handle: AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        run_mcp_stdio(app_handle).await;
    });
    Ok(())
}

/// Run the MCP server on stdin/stdout.
pub async fn run_mcp_stdio(app_handle: AppHandle) {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    debug!("MCP server started on stdio");

    while let Ok(Some(line)) = reader.next_line().await {
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<McpRequest>(&line) {
            Ok(req) => handle_request(req, &app_handle).await,
            Err(e) => McpResponse {
                id: None,
                result: None,
                error: Some(McpError { code: -32700, message: format!("Parse error: {e}") }),
            },
        };

        let mut output = serde_json::to_string(&response).unwrap_or_default();
        output.push('\n');

        if let Err(e) = stdout.write_all(output.as_bytes()).await {
            error!("MCP stdout write error: {}", e);
            break;
        }
        let _ = stdout.flush().await;
    }
}

async fn handle_request(req: McpRequest, app: &AppHandle) -> McpResponse {
    debug!("MCP request: {}", req.method);

    let result = match req.method.as_str() {
        "initialize" => handle_initialize(),
        "notifications/initialized" | "notifications/cancelled" => {
            return McpResponse { id: req.id, result: Some(json!({})), error: None };
        }
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(req.params.as_ref(), app).await,
        _ => Err(McpError { code: -32601, message: format!("Method not found: {}", req.method) }),
    };

    match result {
        Ok(v) => McpResponse { id: req.id, result: Some(v), error: None },
        Err(e) => McpResponse { id: req.id, result: None, error: Some(e) },
    }
}

fn handle_initialize() -> Result<Value, McpError> {
    Ok(json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "android-dev-companion",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

fn handle_tools_list() -> Result<Value, McpError> {
    Ok(json!({
        "tools": [
            {
                "name": "get_build_status",
                "description": "Get the current Gradle build status (idle/running/success/failed/cancelled).",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_build_errors",
                "description": "Get compiler errors and warnings from the last Gradle build.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_logcat_entries",
                "description": "Get recent Android logcat entries. Optionally filter by level, tag, or text.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "Max entries to return (default 200, max 2000)" },
                        "min_level": { "type": "string", "enum": ["verbose","debug","info","warn","error","fatal"] },
                        "tag": { "type": "string", "description": "Filter by tag (case-insensitive substring)" },
                        "text": { "type": "string", "description": "Filter by message text" },
                        "only_crashes": { "type": "boolean", "description": "Return only crash entries" }
                    }
                }
            },
            {
                "name": "get_crash_logs",
                "description": "Get recent FATAL EXCEPTION crash stack traces from logcat.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "Max crash entries to return (default 20)" }
                    }
                }
            },
            {
                "name": "list_devices",
                "description": "List connected Android devices and running emulators.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_build_variants",
                "description": "List available build variants for the current Android project.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_project_info",
                "description": "Get information about the currently open Android project.",
                "inputSchema": { "type": "object", "properties": {} }
            }
        ]
    }))
}

async fn handle_tools_call(params: Option<&Value>, app: &AppHandle) -> Result<Value, McpError> {
    let params = params.ok_or_else(|| McpError::invalid_params("Missing params"))?;
    let name = params.get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::invalid_params("Missing tool name"))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match name {
        "get_build_status" => tool_get_build_status(app).await,
        "get_build_errors" => tool_get_build_errors(app).await,
        "get_logcat_entries" => tool_get_logcat_entries(app, &args).await,
        "get_crash_logs" => tool_get_crash_logs(app, &args).await,
        "list_devices" => tool_list_devices(app).await,
        "list_build_variants" => tool_list_build_variants(app).await,
        "get_project_info" => tool_get_project_info(app).await,
        _ => Err(McpError::not_found(format!("Unknown tool: {name}"))),
    }
}

// ── Tool Implementations ──────────────────────────────────────────────────────

async fn tool_get_build_status(app: &AppHandle) -> Result<Value, McpError> {
    let build_state = app.state::<crate::services::build_runner::BuildState>();
    let state = build_state.0.lock().await;
    let status = format!("{:?}", state.status).to_lowercase();
    let task = state.current_build.as_ref().map(|_| "running").unwrap_or("");
    Ok(mcp_text(format!("Build status: {status}{}", if task.is_empty() { "".to_string() } else { format!(" ({})", task) })))
}

async fn tool_get_build_errors(app: &AppHandle) -> Result<Value, McpError> {
    let build_state = app.state::<crate::services::build_runner::BuildState>();
    let state = build_state.0.lock().await;
    if state.current_errors.is_empty() {
        return Ok(mcp_text("No errors from the last build."));
    }
    let mut lines = vec![format!("{} error(s) from last build:", state.current_errors.len())];
    for err in &state.current_errors {
        let loc = if err.line > 0 {
            format!("{}:{}", err.file, err.line)
        } else {
            err.file.clone()
        };
        lines.push(format!("[{}] {} — {}", format!("{:?}", err.severity).to_lowercase(), loc, err.message));
    }
    Ok(mcp_text(lines.join("\n")))
}

async fn tool_get_logcat_entries(app: &AppHandle, args: &Value) -> Result<Value, McpError> {
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(200) as usize;
    let count = count.min(2000);
    let min_level = args.get("min_level").and_then(Value::as_str).map(parse_level_str);
    let tag = args.get("tag").and_then(Value::as_str).map(String::from);
    let text = args.get("text").and_then(Value::as_str).map(String::from);
    let only_crashes = args.get("only_crashes").and_then(Value::as_bool).unwrap_or(false);

    let filter = LogcatFilter { min_level, tag, text, only_crashes, ..Default::default() };
    let logcat = app.state::<LogcatState>();
    let state = logcat.lock().await;

    let entries: Vec<String> = state.buffer.entries.iter()
        .rev()
        .filter(|e| filter.matches(e))
        .take(count)
        .map(|e| format!("{} {:5}/{:<20} {}", e.timestamp, level_char(&e.level), e.tag, e.message))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if entries.is_empty() {
        return Ok(mcp_text("No logcat entries matching the filter."));
    }
    Ok(mcp_text(entries.join("\n")))
}

async fn tool_get_crash_logs(app: &AppHandle, args: &Value) -> Result<Value, McpError> {
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(20) as usize;
    let logcat = app.state::<LogcatState>();
    let state = logcat.lock().await;

    let entries: Vec<String> = state.buffer.entries.iter()
        .rev()
        .filter(|e| e.is_crash)
        .take(count)
        .map(|e| format!("{} E/{}: {}", e.timestamp, e.tag, e.message))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if entries.is_empty() {
        return Ok(mcp_text("No crash logs found."));
    }
    Ok(mcp_text(format!("{} crash log entries:\n{}", entries.len(), entries.join("\n"))))
}

async fn tool_list_devices(app: &AppHandle) -> Result<Value, McpError> {
    let device_state = app.state::<crate::services::adb_manager::DeviceState>();
    let state = device_state.0.lock().await;
    if state.devices.is_empty() {
        return Ok(mcp_text("No devices connected."));
    }
    let lines: Vec<String> = state.devices.iter()
        .map(|d| format!("{} — {} ({:?})", d.serial, d.model.as_deref().unwrap_or(&d.name), d.connection_state))
        .collect();
    Ok(mcp_text(format!("{} device(s):\n{}", lines.len(), lines.join("\n"))))
}

async fn tool_list_build_variants(app: &AppHandle) -> Result<Value, McpError> {
    let fs_state = app.state::<FsState>();
    let fs = fs_state.0.lock().await;
    let gradle_root = fs.gradle_root.as_ref()
        .or(fs.project_root.as_ref())
        .cloned();
    drop(fs);

    let Some(gradle_root) = gradle_root else {
        return Ok(mcp_text("No project open."));
    };

    let candidates = [
        gradle_root.join("app").join("build.gradle.kts"),
        gradle_root.join("app").join("build.gradle"),
        gradle_root.join("build.gradle.kts"),
    ];

    for path in &candidates {
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Some(list) = variant_manager::parse_variants_from_gradle(path, &content) {
                    if !list.variants.is_empty() {
                        let names: Vec<&str> = list.variants.iter().map(|v| v.name.as_str()).collect();
                        let settings = settings_manager::load_settings();
                        let active = settings.build.build_variant.as_deref().unwrap_or("none");
                        return Ok(mcp_text(format!(
                            "Build variants (active: {}):\n{}",
                            active,
                            names.join(", ")
                        )));
                    }
                }
            }
        }
    }
    Ok(mcp_text("Could not determine build variants. Make sure a project is open and build.gradle.kts exists."))
}

async fn tool_get_project_info(app: &AppHandle) -> Result<Value, McpError> {
    let fs_state = app.state::<FsState>();
    let fs = fs_state.0.lock().await;
    let project_root = fs.project_root.as_ref().map(|p| p.to_string_lossy().to_string());
    let gradle_root = fs.gradle_root.as_ref().map(|p| p.to_string_lossy().to_string());
    drop(fs);

    match project_root {
        None => Ok(mcp_text("No project open. Use Cmd+O in the app to open an Android project.")),
        Some(root) => {
            let name = std::path::Path::new(&root)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| root.clone());
            let gradle_info = gradle_root
                .map(|g| format!("Gradle root: {}", g))
                .unwrap_or_else(|| "Gradle root: same as project root".to_string());
            Ok(mcp_text(format!("Project: {}\nPath: {}\n{}", name, root, gradle_info)))
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn mcp_text(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }]
    })
}

fn level_char(level: &LogcatLevel) -> &'static str {
    match level {
        LogcatLevel::Verbose => "V",
        LogcatLevel::Debug => "D",
        LogcatLevel::Info => "I",
        LogcatLevel::Warn => "W",
        LogcatLevel::Error => "E",
        LogcatLevel::Fatal => "F",
        LogcatLevel::Unknown => "?",
    }
}

fn parse_level_str(s: &str) -> LogcatLevel {
    match s.to_lowercase().as_str() {
        "verbose" | "v" => LogcatLevel::Verbose,
        "debug" | "d" => LogcatLevel::Debug,
        "info" | "i" => LogcatLevel::Info,
        "warn" | "w" => LogcatLevel::Warn,
        "error" | "e" => LogcatLevel::Error,
        "fatal" | "f" => LogcatLevel::Fatal,
        _ => LogcatLevel::Verbose,
    }
}
