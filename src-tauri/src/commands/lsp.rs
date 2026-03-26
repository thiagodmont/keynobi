use crate::models::lsp::*;
use crate::models::log_entry::{LogEntry, LogLevel};
use crate::services::{lsp_client::{LspClient, LspEnv}, lsp_downloader, lsp_log_file::LspLogFile};
use crate::services::settings_manager;
use crate::{FsState, LogBuffer, MAX_LOG_ENTRIES};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::Emitter;
use tokio::sync::{mpsc, Mutex};

/// Pure helper: parse a `$/progress` `params` JSON value and return the
/// fields we care about.  Extracted from the notification loop so it can be
/// unit-tested without a running Tauri runtime.
pub(crate) struct ProgressEvent<'a> {
    pub token: &'a str,
    pub kind: Option<&'a str>,
    pub display_msg: Option<String>,
}

pub(crate) fn parse_progress_params(params: &Value) -> ProgressEvent<'_> {
    let token = params
        .get("token")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let value = params.get("value");
    let kind = value.and_then(|v| v.get("kind")).and_then(|k| k.as_str());
    let title = value.and_then(|v| v.get("title")).and_then(|t| t.as_str());
    let message = value.and_then(|v| v.get("message")).and_then(|m| m.as_str());
    let display_msg = message.or(title).map(str::to_string);
    ProgressEvent { token, kind, display_msg }
}

/// Push a [`LogEntry`] into the bounded log buffer, evicting the oldest entry
/// if the cap is reached.  Lock is held only for the duration of the push.
async fn push_log(buffer: &LogBuffer, entry: LogEntry) {
    let mut buf = buffer.lock().await;
    if buf.len() >= MAX_LOG_ENTRIES {
        buf.pop_front();
    }
    buf.push_back(entry);
}

/// Map an LSP `window/logMessage` `type` integer to a [`LogLevel`].
/// Spec: 1=Error, 2=Warning, 3=Info, 4=Log, 5=Debug.
fn lsp_msg_type_to_level(msg_type: u64) -> LogLevel {
    match msg_type {
        1 => LogLevel::Error,
        2 => LogLevel::Warn,
        3 => LogLevel::Info,
        _ => LogLevel::Debug,
    }
}

pub struct LspState(pub Mutex<LspStateInner>);

pub struct LspStateInner {
    pub client: Option<LspClient>,
    pub status: LspStatusState,
    pub installation: Option<LspInstallation>,
}

/// Validate that `path` is inside the effective project boundary.
/// Uses the Gradle root when available so that LSP navigation targets in
/// sibling modules (e.g. go-to-definition across Gradle modules) pass the
/// security check.  Falls back to the user-opened project root.
async fn ensure_path_in_project(
    path: &str,
    fs_state: &tauri::State<'_, FsState>,
) -> Result<(), String> {
    let fs = fs_state.0.lock().await;
    let root = fs
        .gradle_root
        .as_ref()
        .or(fs.project_root.as_ref())
        .ok_or("No project open")?;

    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root: {e}"))?;
    drop(fs);

    let target = Path::new(path);
    let canonical_target = if target.exists() {
        target
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?
    } else if let Some(parent) = target.parent() {
        if parent.exists() {
            let cp = parent
                .canonicalize()
                .map_err(|e| format!("Failed to resolve path: {e}"))?;
            cp.join(target.file_name().unwrap_or_default())
        } else {
            return Err("Path parent does not exist".into());
        }
    } else {
        return Err("Invalid path".into());
    };

    if !canonical_target.starts_with(&canonical_root) {
        return Err("Path is outside the project directory".into());
    }
    Ok(())
}

impl LspState {
    pub fn new() -> Self {
        Self(Mutex::new(LspStateInner {
            client: None,
            status: LspStatusState::Stopped,
            installation: None,
        }))
    }
}

impl Default for LspState {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
pub async fn lsp_check_installed() -> Result<Option<LspInstallation>, String> {
    Ok(lsp_downloader::check_installed())
}

#[tauri::command]
pub async fn lsp_download(app: tauri::AppHandle) -> Result<LspInstallation, String> {
    let app_clone = app.clone();

    let installation = lsp_downloader::download_and_install(move |progress| {
        let _ = app_clone.emit("lsp:download_progress", &progress);
    })
    .await?;

    Ok(installation)
}

#[tauri::command]
pub async fn lsp_start(
    project_root: String,
    lsp_state: tauri::State<'_, LspState>,
    fs_state: tauri::State<'_, FsState>,
    log_buffer: tauri::State<'_, LogBuffer>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Validate the project root matches the actually open project, and read the
    // detected Gradle root (if any) to use as the LSP workspace root.
    let fs = fs_state.0.lock().await;
    let actual_root = fs
        .project_root
        .as_ref()
        .ok_or("No project open")?
        .clone();
    let gradle_root = fs.gradle_root.clone();
    drop(fs);

    let requested = Path::new(&project_root);
    let canonical_requested = requested
        .canonicalize()
        .map_err(|e| format!("Invalid project root: {e}"))?;
    let canonical_actual = actual_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root: {e}"))?;
    if canonical_requested != canonical_actual {
        return Err("Requested project root does not match the open project".into());
    }

    // Use the Gradle root as the LSP workspace root so the Kotlin LSP can find
    // gradlew, settings.gradle, and resolve packages across all modules.
    let workspace_root = gradle_root.unwrap_or_else(|| actual_root.clone());

    let installation = lsp_downloader::check_installed()
        .ok_or("Kotlin LSP is not installed. Please download it first.")?;

    let mut state = lsp_state.0.lock().await;
    if state.client.is_some() {
        return Err("LSP is already running".into());
    }
    state.status = LspStatusState::Starting;
    state.installation = Some(installation.clone());

    let _ = app.emit(
        "lsp:status",
        LspStatus {
            state: LspStatusState::Starting,
            message: Some("Starting Kotlin LSP...".into()),
        },
    );

    drop(state);

    let launch_script = PathBuf::from(&installation.launch_script);
    let workspace = workspace_root;

    // Use a dedicated LSP system directory (separate from the download cache)
    // for the server's indices and runtime data.
    let system_dir = lsp_downloader::get_lsp_system_dir();
    tokio::fs::create_dir_all(&system_dir).await.ok();

    // Load persisted settings so we can forward ANDROID_HOME / JAVA_HOME to the
    // LSP process.  On macOS the Tauri app doesn't inherit shell env vars, so
    // Gradle won't find the Android SDK unless we set them explicitly.
    let settings = settings_manager::load_settings();

    // Expand `~/` in paths before passing them as environment variables.
    // `cmd.env("ANDROID_HOME", "~/…")` does NOT expand tilde — the shell
    // never sees it — so we must resolve it here.
    fn expand_tilde_str(s: Option<String>) -> Option<String> {
        s.map(|p| {
            if let Some(rest) = p.strip_prefix("~/") {
                dirs::home_dir()
                    .map(|h| h.join(rest).to_string_lossy().into_owned())
                    .unwrap_or(p)
            } else {
                p
            }
        })
    }

    let lsp_env = LspEnv {
        android_sdk_path: expand_tilde_str(settings.android.sdk_path.clone()),
        java_home: expand_tilde_str(settings.java.home.clone()),
        jdk_for_resolution: expand_tilde_str(settings.java.home.clone()),
    };

    let (notif_tx, mut notif_rx) = mpsc::unbounded_channel::<(String, Value)>();

    // Open a timestamped session log file in ~/.androidide/logs/lsp/.
    // Best-effort: if we can't create the file, log the reason and continue
    // without file logging — never block the LSP start on disk I/O.
    let mut session_log = match LspLogFile::create().await {
        Ok(lf) => {
            tracing::info!("LSP session log: {:?}", lf.path);
            Some(lf)
        }
        Err(e) => {
            tracing::warn!("Could not create LSP log file: {e}");
            None
        }
    };

    // Emit the log file path as the very first Output panel entry so the
    // developer knows exactly where to find the file when reporting issues.
    let startup_msg = format!(
        "Kotlin LSP {} starting | workspace: {} | project: {} | ANDROID_HOME: {} | JAVA_HOME: {} | log: {}",
        installation.version,
        workspace.display(),
        project_root,
        settings.android.sdk_path.as_deref().unwrap_or("(not set — Gradle may fail to find Android SDK)"),
        settings.java.home.as_deref().unwrap_or("(not set)"),
        session_log
            .as_ref()
            .map(|lf| lf.path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(no log file)".into()),
    );
    let startup_entry = LogEntry::new(LogLevel::Info, "lsp:startup", startup_msg);
    push_log(&log_buffer.inner().clone(), startup_entry.clone()).await;
    let _ = app.emit("lsp:log", &startup_entry);
    if let Some(ref mut lf) = session_log {
        lf.write_separator("LSP session started").await;
        lf.write_entry(&startup_entry).await;
    }

    let app_for_notifications = app.clone();
    let log_buffer_for_notifications = log_buffer.inner().clone();
    // Track active work-done progress tokens so we can transition the LSP
    // status between "indexing" and "ready" based on what the server reports.
    let active_progress_tokens = std::sync::Arc::new(Mutex::new(std::collections::HashSet::<String>::new()));
    tokio::spawn(async move {
        while let Some((method, params)) = notif_rx.recv().await {
            match method.as_str() {
                "window/logMessage" => {
                    let level = params
                        .get("type")
                        .and_then(|v| v.as_u64())
                        .map(lsp_msg_type_to_level)
                        .unwrap_or(LogLevel::Info);
                    if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                        tracing::info!(target: "kotlin_lsp", "{}", msg);
                        let entry = LogEntry::new(level, "lsp:server", msg);
                        push_log(&log_buffer_for_notifications, entry.clone()).await;
                        let _ = app_for_notifications.emit("lsp:log", &entry);
                        if let Some(ref mut lf) = session_log {
                            lf.write_entry(&entry).await;
                        }
                    }
                }
                "kotlin/stderr" => {
                    if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                        let entry = LogEntry::new(LogLevel::Debug, "lsp:stderr", msg);
                        push_log(&log_buffer_for_notifications, entry.clone()).await;
                        let _ = app_for_notifications.emit("lsp:log", &entry);
                        if let Some(ref mut lf) = session_log {
                            lf.write_entry(&entry).await;
                        }
                    }
                }
                // Internal client-side debug messages (e.g. workspace/configuration
                // requests) forwarded from handle_message so they appear in the
                // Output panel alongside server logs.
                "kotlin/client-debug" => {
                    if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                        let entry = LogEntry::new(LogLevel::Debug, "lsp:client", msg);
                        push_log(&log_buffer_for_notifications, entry.clone()).await;
                        let _ = app_for_notifications.emit("lsp:log", &entry);
                        if let Some(ref mut lf) = session_log {
                            lf.write_entry(&entry).await;
                        }
                    }
                }
                "window/showMessage" => {
                    if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                        let _ = app_for_notifications.emit("lsp:message", msg);
                    }
                }
                "$/progress" => {
                    // Parse WorkDoneProgress notifications so the IDE can show
                    // an "indexing" state while the Kotlin LSP builds its model.
                    let ev = parse_progress_params(&params);
                    let token = ev.token.to_string();
                    let display_msg = ev.display_msg.clone();

                    match ev.kind {
                        Some("begin") => {
                            active_progress_tokens.lock().await.insert(token);
                            let _ = app_for_notifications.emit(
                                "lsp:status",
                                LspStatus {
                                    state: LspStatusState::Indexing,
                                    message: display_msg.clone(),
                                },
                            );
                            // Log the indexing begin so it appears in the Output panel.
                            let log_msg = display_msg.unwrap_or_else(|| "Indexing started".into());
                            let entry = LogEntry::new(LogLevel::Info, "lsp:progress", &log_msg);
                            push_log(&log_buffer_for_notifications, entry.clone()).await;
                            let _ = app_for_notifications.emit("lsp:log", &entry);
                            if let Some(ref mut lf) = session_log {
                                lf.write_entry(&entry).await;
                            }
                        }
                        Some("report") => {
                            if !active_progress_tokens.lock().await.is_empty() {
                                if let Some(msg) = display_msg {
                                    let _ = app_for_notifications.emit(
                                        "lsp:status",
                                        LspStatus {
                                            state: LspStatusState::Indexing,
                                            message: Some(msg.clone()),
                                        },
                                    );
                                    // Log progress reports at debug level to avoid noise.
                                    let entry = LogEntry::new(LogLevel::Debug, "lsp:progress", &msg);
                                    push_log(&log_buffer_for_notifications, entry.clone()).await;
                                    let _ = app_for_notifications.emit("lsp:log", &entry);
                                    if let Some(ref mut lf) = session_log {
                                        lf.write_entry(&entry).await;
                                    }
                                }
                            }
                        }
                        Some("end") => {
                            let mut set = active_progress_tokens.lock().await;
                            set.remove(&token);
                            let log_msg = display_msg.unwrap_or_else(|| "Indexing complete".into());
                            let entry = LogEntry::new(LogLevel::Info, "lsp:progress", &log_msg);
                            push_log(&log_buffer_for_notifications, entry.clone()).await;
                            let _ = app_for_notifications.emit("lsp:log", &entry);
                            if let Some(ref mut lf) = session_log {
                                lf.write_entry(&entry).await;
                            }
                            if set.is_empty() {
                                let _ = app_for_notifications.emit(
                                    "lsp:status",
                                    LspStatus {
                                        state: LspStatusState::Ready,
                                        message: None,
                                    },
                                );
                            }
                        }
                        _ => {}
                    }
                    // Also forward raw progress params so the frontend can extract
                    // the optional `percentage` field for the progress bar.
                    let _ = app_for_notifications.emit("lsp:progress", &params);
                }
                "textDocument/publishDiagnostics" => {
                    let _ = app_for_notifications.emit("lsp:diagnostics_push", &params);
                }
                _ => {
                    tracing::debug!("Unhandled LSP notification: {}", method);
                }
            }
        }

        // The channel closed — LSP process has stopped.  Write a footer and
        // flush so all buffered bytes reach disk before the file handle drops.
        if let Some(mut lf) = session_log {
            lf.write_separator("LSP session ended").await;
            lf.flush().await;
        }
    });

    let client = LspClient::start(&launch_script, &workspace, &system_dir, lsp_env, notif_tx).await?;

    // Emit server capabilities so the frontend can gate feature calls on what
    // the server actually supports, preventing noisy "no handler" errors.
    let caps = client.server_capabilities().await;
    let _ = app.emit("lsp:capabilities", &caps);

    let mut state = lsp_state.0.lock().await;
    state.client = Some(client);
    state.status = LspStatusState::Ready;

    let _ = app.emit(
        "lsp:status",
        LspStatus {
            state: LspStatusState::Ready,
            message: None,
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn lsp_stop(
    lsp_state: tauri::State<'_, LspState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut state = lsp_state.0.lock().await;
    if let Some(client) = state.client.take() {
        if client.is_running().await {
            client.shutdown().await.ok();
        }
        client.kill().await;
    }
    state.status = LspStatusState::Stopped;

    let _ = app.emit(
        "lsp:status",
        LspStatus {
            state: LspStatusState::Stopped,
            message: None,
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn lsp_status(
    lsp_state: tauri::State<'_, LspState>,
) -> Result<LspStatus, String> {
    let state = lsp_state.0.lock().await;
    Ok(LspStatus {
        state: state.status.clone(),
        message: None,
    })
}

#[tauri::command]
pub async fn lsp_did_open(
    path: String,
    content: String,
    language: String,
    lsp_state: tauri::State<'_, LspState>,
    fs_state: tauri::State<'_, FsState>,
) -> Result<(), String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client
        .did_open(Path::new(&path), &language, 1, &content)
        .await
}

#[tauri::command]
pub async fn lsp_did_change(
    path: String,
    content: String,
    version: i32,
    lsp_state: tauri::State<'_, LspState>,
    fs_state: tauri::State<'_, FsState>,
) -> Result<(), String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client
        .did_change(Path::new(&path), version, &content)
        .await
}

#[tauri::command]
pub async fn lsp_did_save(
    path: String,
    content: String,
    lsp_state: tauri::State<'_, LspState>,
    fs_state: tauri::State<'_, FsState>,
) -> Result<(), String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.did_save(Path::new(&path), &content).await
}

#[tauri::command]
pub async fn lsp_did_close(
    path: String,
    lsp_state: tauri::State<'_, LspState>,
    fs_state: tauri::State<'_, FsState>,
) -> Result<(), String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.did_close(Path::new(&path)).await
}

#[tauri::command]
pub async fn lsp_complete(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.completion(Path::new(&path), line, col).await
}

#[tauri::command]
pub async fn lsp_hover(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.hover(Path::new(&path), line, col).await
}

#[tauri::command]
pub async fn lsp_definition(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.definition(Path::new(&path), line, col).await
}

#[tauri::command]
pub async fn lsp_references(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.references(Path::new(&path), line, col, true).await
}

#[tauri::command]
pub async fn lsp_implementation(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.implementation(Path::new(&path), line, col).await
}

#[tauri::command]
pub async fn lsp_document_symbols(
    path: String,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.document_symbol(Path::new(&path)).await
}

#[tauri::command]
pub async fn lsp_workspace_symbols(
    query: String,
    lsp_state: tauri::State<'_, LspState>,
) -> Result<Value, String> {
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.workspace_symbol(&query).await
}

#[tauri::command]
pub async fn lsp_code_action(
    path: String, start_line: u32, start_col: u32, end_line: u32, end_col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.code_action(Path::new(&path), start_line, start_col, end_line, end_col).await
}

#[tauri::command]
pub async fn lsp_rename(
    path: String, line: u32, col: u32, new_name: String,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.rename(Path::new(&path), line, col, &new_name).await
}

#[tauri::command]
pub async fn lsp_format(
    path: String,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.formatting(Path::new(&path)).await
}

#[tauri::command]
pub async fn lsp_pull_diagnostics(
    path: String,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.pull_diagnostics(Path::new(&path)).await
}

#[tauri::command]
pub async fn lsp_document_highlight(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.document_highlight(Path::new(&path), line, col).await
}

#[tauri::command]
pub async fn lsp_signature_help(
    path: String, line: u32, col: u32,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.signature_help(Path::new(&path), line, col).await
}

/// Return a snapshot of all buffered log entries for the initial panel load.
/// The lock is held only for the clone — no I/O is done while locked.
#[tauri::command]
pub async fn lsp_get_logs(
    log_buffer: tauri::State<'_, LogBuffer>,
) -> Result<Vec<LogEntry>, String> {
    let buf = log_buffer.lock().await;
    Ok(buf.iter().cloned().collect())
}

/// Return the server capabilities from the most recent `initialize` handshake.
/// The frontend uses this to skip requests for methods the server doesn't
/// support, avoiding unnecessary error noise in the Output panel.
#[tauri::command]
pub async fn lsp_get_capabilities(
    lsp_state: tauri::State<'_, LspState>,
) -> Result<Value, String> {
    let state = lsp_state.0.lock().await;
    match state.client.as_ref() {
        Some(client) => Ok(client.server_capabilities().await),
        None => Ok(Value::Null),
    }
}

/// Persist a client-side log entry (e.g. a navigation log) into the shared
/// log buffer and session log file so it survives the in-memory store.
///
/// Called fire-and-forget from the frontend `navLog` helper.  Errors are
/// silently ignored — a failed log write must never break the user flow.
#[tauri::command]
pub async fn lsp_append_client_log(
    message: String,
    level: String,
    source: String,
    log_buffer: tauri::State<'_, LogBuffer>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let lvl = match level.as_str() {
        "error" => LogLevel::Error,
        "warn"  => LogLevel::Warn,
        "debug" => LogLevel::Debug,
        "trace" => LogLevel::Trace,
        _       => LogLevel::Info,
    };
    let entry = LogEntry::new(lvl, &source, &message);
    push_log(&log_buffer.inner().clone(), entry.clone()).await;
    // Re-emit so any panel that joined late can see the entry.
    let _ = app.emit("lsp:log", &entry);
    Ok(())
}

/// Decompile a class/source file from a JAR or JRT archive via the LSP
/// server's custom `decompile` command.  The URI is a `jar:` or `jrt:` URI
/// as returned by `textDocument/definition` when the target is in binary or
/// standard-library code.
///
/// Returns `{ code: String, language: String }` on success, or an error
/// string when the LSP can't decompile the file.
#[tauri::command]
pub async fn lsp_decompile(
    uri: String,
    lsp_state: tauri::State<'_, LspState>,
) -> Result<Value, String> {
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client
        .execute_command("decompile", vec![serde_json::Value::String(uri)])
        .await
}

/// Extract raw text from a JAR/ZIP archive entry.  Used when the definition
/// target is a source file inside an archive (e.g. `kotlin-stdlib-sources.jar`
/// or the JDK `src.zip`).
///
/// The `archive_path` is the filesystem path to the zip/jar, and `entry_path`
/// is the path of the entry within the archive (e.g.
/// `java.base/java/util/UUID.java`).
///
/// Returns the text content of the entry, or an error.
#[tauri::command]
pub async fn lsp_read_archive_entry(
    archive_path: String,
    entry_path: String,
) -> Result<String, String> {
    // Validate the archive path exists and is a file.
    let archive = std::path::Path::new(&archive_path);
    if !archive.exists() {
        return Err(format!("Archive not found: {archive_path}"));
    }

    // Do the ZIP extraction on a blocking thread to avoid holding the async
    // executor while doing synchronous I/O.
    let archive_path_buf = archive.to_path_buf();
    let entry_name = entry_path.clone();
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let file = std::fs::File::open(&archive_path_buf)
            .map_err(|e| format!("Failed to open archive: {e}"))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|e| format!("Invalid archive: {e}"))?;

        // Collect all entry names first to avoid borrow conflicts.
        let entry_names: Vec<String> = (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().map(|e| e.name().to_string()))
            .collect();

        // Find the matching entry index: exact match first, then suffix match.
        let normalized_target = entry_name.replace('\\', "/");
        let idx = entry_names
            .iter()
            .position(|n| n == &entry_name || n.replace('\\', "/") == normalized_target)
            .or_else(|| {
                entry_names.iter().position(|n| {
                    n.ends_with(&entry_name)
                        || n.replace('\\', "/").ends_with(&normalized_target)
                })
            })
            .ok_or_else(|| format!("Entry '{entry_name}' not found in archive"))?;

        let mut entry = zip
            .by_index(idx)
            .map_err(|e| format!("Failed to open entry: {e}"))?;
        use std::io::Read;
        let mut buf = String::new();
        entry
            .read_to_string(&mut buf)
            .map_err(|e| format!("Failed to read entry: {e}"))?;
        Ok(buf)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

/// Request full semantic tokens for a document from the LSP server.
/// Returns the raw LSP `SemanticTokens` response including the encoded `data`
/// array and the `legend` from server capabilities.
#[tauri::command]
pub async fn lsp_semantic_tokens(
    path: String,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client.semantic_tokens_full(std::path::Path::new(&path)).await
}

/// Generate `workspace.json` in the workspace root by scanning the project's
/// Gradle module structure from the filesystem.
///
/// This is the **IDE-side** fix for the "Package directive does not match the
/// file location" false-positive that appears on Android multi-module projects
/// using AGP convention plugins in a composite build (`includeBuild`).
///
/// Root cause: The kotlin-lsp's `IdeaProjectMapper` relies on the Gradle IDEA
/// model to discover source sets.  When AGP convention plugins come from a
/// composite build, the IDEA model reports every module as having an empty set
/// of source sets.  Without source roots, the LSP treats the module directory
/// as the source root, causing package path mismatches for all files.
///
/// Fix: When `workspace.json` exists in the workspace root, the LSP's
/// `JsonWorkspaceImporter` uses it **instead of** the Gradle import.  Our
/// generated file tells the LSP exactly where each module's `src/main/java`
/// (and `src/main/kotlin`, `src/test/java`, etc.) directories are.
///
/// The file is generated by:
///   1. Parsing `settings.gradle(.kts)` to discover all included modules
///   2. Converting each Gradle module path (`:features:home`) to a filesystem
///      path (`features/home`) using Gradle's default naming convention
///   3. Scanning each module directory for standard Android/Kotlin source dirs
///   4. Writing the `workspace.json` schema understood by `JsonWorkspaceImporter`
///
/// After generating, restart the LSP for the changes to take effect.
/// Returns the absolute path of the written file.
#[tauri::command]
pub async fn lsp_generate_workspace_json(
    fs_state: tauri::State<'_, FsState>,
) -> Result<String, String> {
    use crate::services::workspace_json;

    let fs = fs_state.0.lock().await;
    let workspace_root = fs
        .gradle_root
        .as_ref()
        .or(fs.project_root.as_ref())
        .ok_or("No project open")?
        .clone();
    drop(fs);

    let output_path = workspace_json::generate(&workspace_root)?;
    Ok(output_path.to_string_lossy().to_string())
}

/// Request code actions with a specific `only` filter — used for targeted
/// actions like "Organize Imports" (`source.organizeImports`).
#[tauri::command]
pub async fn lsp_code_action_filtered(
    path: String,
    start_line: u32, start_col: u32,
    end_line: u32, end_col: u32,
    only: Vec<String>,
    lsp_state: tauri::State<'_, LspState>, fs_state: tauri::State<'_, FsState>,
) -> Result<Value, String> {
    ensure_path_in_project(&path, &fs_state).await?;
    let state = lsp_state.0.lock().await;
    let client = state.client.as_ref().ok_or("LSP not running")?;
    client
        .code_action_filtered(
            std::path::Path::new(&path),
            start_line, start_col,
            end_line, end_col,
            only,
        )
        .await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    // ── parse_progress_params ─────────────────────────────────────────────────

    #[test]
    fn parse_begin_with_title_and_message() {
        let params = json!({
            "token": "abc-123",
            "value": { "kind": "begin", "title": "Loading project", "message": "Reading build scripts" }
        });
        let ev = parse_progress_params(&params);
        assert_eq!(ev.token, "abc-123");
        assert_eq!(ev.kind, Some("begin"));
        // message takes precedence over title
        assert_eq!(ev.display_msg.as_deref(), Some("Reading build scripts"));
    }

    #[test]
    fn parse_begin_with_title_only() {
        let params = json!({
            "token": "tok1",
            "value": { "kind": "begin", "title": "Indexing" }
        });
        let ev = parse_progress_params(&params);
        assert_eq!(ev.kind, Some("begin"));
        assert_eq!(ev.display_msg.as_deref(), Some("Indexing"));
    }

    #[test]
    fn parse_report_with_message() {
        let params = json!({
            "token": "tok2",
            "value": { "kind": "report", "message": "Analyzing classes", "percentage": 42 }
        });
        let ev = parse_progress_params(&params);
        assert_eq!(ev.kind, Some("report"));
        assert_eq!(ev.display_msg.as_deref(), Some("Analyzing classes"));
    }

    #[test]
    fn parse_end_with_no_message() {
        let params = json!({ "token": "tok3", "value": { "kind": "end" } });
        let ev = parse_progress_params(&params);
        assert_eq!(ev.kind, Some("end"));
        assert!(ev.display_msg.is_none());
    }

    #[test]
    fn parse_missing_value_field() {
        let params = json!({ "token": "tok4" });
        let ev = parse_progress_params(&params);
        assert_eq!(ev.token, "tok4");
        assert!(ev.kind.is_none());
        assert!(ev.display_msg.is_none());
    }

    #[test]
    fn parse_empty_token() {
        let params = json!({ "value": { "kind": "begin", "title": "Loading" } });
        let ev = parse_progress_params(&params);
        assert_eq!(ev.token, "");
    }

    // ── Token-set state machine ────────────────────────────────────────────────
    //
    // Simulate the active_progress_tokens logic from the notification loop.

    /// Returns (is_now_indexing, is_now_ready) after processing the event.
    fn process_event(set: &mut HashSet<String>, ev: &ProgressEvent<'_>) -> (bool, bool) {
        match ev.kind {
            Some("begin") => {
                set.insert(ev.token.to_string());
                (true, false)
            }
            Some("end") => {
                set.remove(ev.token);
                if set.is_empty() {
                    (false, true)
                } else {
                    (true, false) // still indexing
                }
            }
            Some("report") => (!set.is_empty(), false),
            _ => (false, false),
        }
    }

    #[test]
    fn begin_transitions_to_indexing() {
        let mut set = HashSet::new();
        let params = json!({ "token": "t1", "value": { "kind": "begin", "title": "Loading" } });
        let ev = parse_progress_params(&params);
        let (indexing, ready) = process_event(&mut set, &ev);
        assert!(indexing);
        assert!(!ready);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn end_after_single_begin_transitions_to_ready() {
        let mut set = HashSet::new();
        let begin = json!({ "token": "t1", "value": { "kind": "begin" } });
        let end = json!({ "token": "t1", "value": { "kind": "end" } });

        let ev = parse_progress_params(&begin);
        process_event(&mut set, &ev);

        let ev = parse_progress_params(&end);
        let (indexing, ready) = process_event(&mut set, &ev);
        assert!(!indexing);
        assert!(ready);
        assert!(set.is_empty());
    }

    #[test]
    fn multiple_concurrent_tokens_only_ready_when_all_end() {
        let mut set = HashSet::new();

        for token in ["t1", "t2", "t3"] {
            let p = json!({ "token": token, "value": { "kind": "begin" } });
            let ev = parse_progress_params(&p);
            process_event(&mut set, &ev);
        }
        assert_eq!(set.len(), 3);

        // End first two — still indexing
        for token in ["t1", "t2"] {
            let p = json!({ "token": token, "value": { "kind": "end" } });
            let ev = parse_progress_params(&p);
            let (indexing, ready) = process_event(&mut set, &ev);
            assert!(indexing, "should still be indexing with remaining tokens");
            assert!(!ready);
        }

        // End last — now ready
        let p = json!({ "token": "t3", "value": { "kind": "end" } });
        let ev = parse_progress_params(&p);
        let (indexing, ready) = process_event(&mut set, &ev);
        assert!(!indexing);
        assert!(ready);
    }

    #[test]
    fn spurious_end_without_matching_begin_does_not_emit_ready_when_others_active() {
        let mut set = HashSet::new();
        // Start a real token
        let p = json!({ "token": "real", "value": { "kind": "begin" } });
        let ev = parse_progress_params(&p);
        process_event(&mut set, &ev);

        // Spurious end for a token we never saw a begin for
        let p = json!({ "token": "unknown", "value": { "kind": "end" } });
        let ev = parse_progress_params(&p);
        let (indexing, ready) = process_event(&mut set, &ev);
        // "real" is still in the set → still indexing, not ready
        assert!(indexing);
        assert!(!ready);
    }

    #[test]
    fn spurious_end_on_empty_set_does_not_panic() {
        let mut set: HashSet<String> = HashSet::new();
        let p = json!({ "token": "ghost", "value": { "kind": "end" } });
        let ev = parse_progress_params(&p);
        // Should not panic and the set stays empty.
        // Returns (false, true) because set.is_empty() → ready transition.
        let (_indexing, _ready) = process_event(&mut set, &ev);
        assert!(set.is_empty());
    }

    #[test]
    fn report_while_tokens_active_stays_indexing() {
        let mut set = HashSet::new();
        let p = json!({ "token": "t1", "value": { "kind": "begin" } });
        let ev = parse_progress_params(&p);
        process_event(&mut set, &ev);

        let p = json!({ "token": "t1", "value": { "kind": "report", "message": "still going" } });
        let ev = parse_progress_params(&p);
        let (indexing, ready) = process_event(&mut set, &ev);
        assert!(indexing);
        assert!(!ready);
    }

    // ── lsp_msg_type_to_level ────────────────────────────────────────────────

    #[test]
    fn msg_type_1_is_error() {
        assert_eq!(lsp_msg_type_to_level(1), LogLevel::Error);
    }

    #[test]
    fn msg_type_2_is_warn() {
        assert_eq!(lsp_msg_type_to_level(2), LogLevel::Warn);
    }

    #[test]
    fn msg_type_3_is_info() {
        assert_eq!(lsp_msg_type_to_level(3), LogLevel::Info);
    }

    #[test]
    fn msg_type_unknown_is_debug() {
        assert_eq!(lsp_msg_type_to_level(99), LogLevel::Debug);
    }
}
