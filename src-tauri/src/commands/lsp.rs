use crate::models::lsp::*;
use crate::services::{lsp_client::LspClient, lsp_downloader};
use crate::FsState;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::Emitter;
use tokio::sync::{mpsc, Mutex};

pub struct LspState(pub Mutex<LspStateInner>);

pub struct LspStateInner {
    pub client: Option<LspClient>,
    pub status: LspStatusState,
    pub installation: Option<LspInstallation>,
}

/// Validate that `path` is inside the open project directory.
/// Uses canonicalization to prevent symlink/`..` traversal bypasses.
async fn ensure_path_in_project(
    path: &str,
    fs_state: &tauri::State<'_, FsState>,
) -> Result<(), String> {
    let fs = fs_state.0.lock().await;
    let root = fs
        .project_root
        .as_ref()
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
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Validate the project root matches the actually open project
    let fs = fs_state.0.lock().await;
    let actual_root = fs
        .project_root
        .as_ref()
        .ok_or("No project open")?
        .clone();
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
    let workspace = PathBuf::from(&project_root);
    let cache_dir = lsp_downloader::get_cache_dir();

    let (notif_tx, mut notif_rx) = mpsc::unbounded_channel::<(String, Value)>();

    let app_for_notifications = app.clone();
    tokio::spawn(async move {
        while let Some((method, params)) = notif_rx.recv().await {
            match method.as_str() {
                "window/logMessage" => {
                    if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                        tracing::info!(target: "kotlin_lsp", "{}", msg);
                    }
                }
                "window/showMessage" => {
                    if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                        let _ = app_for_notifications.emit("lsp:message", msg);
                    }
                }
                "$/progress" => {
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
    });

    let client = LspClient::start(&launch_script, &workspace, &cache_dir, notif_tx).await?;

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
