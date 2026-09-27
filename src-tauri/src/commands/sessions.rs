//! Debug session commands: thin wrappers over `services::debug_sessions`.

use crate::models::debug_session::{
    DebugSessionCapture, DebugSessionDetail, DebugSessionEvent, DebugSessionExitRefresh,
    DebugSessionSummary, SessionExportOptions, SessionExportResult,
};
use crate::models::error::AppError;
use crate::services::adb_manager::{get_adb_path, DeviceState};
use crate::services::debug_sessions;
use crate::services::installed_builds::InstallTarget;
use crate::services::settings_manager;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Other(format!("Debug session task failed: {e}")))?
}

/// Every recorded debug session, newest first, then the imported ones.
#[tauri::command]
pub async fn list_debug_sessions() -> Result<Vec<DebugSessionSummary>, AppError> {
    blocking(|| Ok(debug_sessions::list_all_sessions())).await
}

/// A debug session and its most recent events.
#[tauri::command]
pub async fn get_debug_session(id: String) -> Result<DebugSessionDetail, AppError> {
    blocking(move || debug_sessions::get_session(&id)).await
}

/// End an open debug session.
#[tauri::command]
pub async fn end_debug_session(id: String) -> Result<(), AppError> {
    blocking(move || debug_sessions::end_session(&id)).await
}

/// Keep a debug session (exempt from age pruning; pins its R8 mappings) or stop keeping it.
#[tauri::command]
pub async fn set_debug_session_kept(id: String, kept: bool) -> Result<(), AppError> {
    blocking(move || debug_sessions::set_kept(&id, kept)).await
}

/// Bookmark a debug session: `session_id`, else the newest open session on
/// the selected device (or on any device when none is selected).
#[tauri::command]
pub async fn add_session_bookmark(
    session_id: Option<String>,
    note: String,
    log_entry_id: Option<u64>,
    device_state: State<'_, DeviceState>,
) -> Result<DebugSessionEvent, AppError> {
    let device = if session_id.is_some() {
        None
    } else {
        let state = device_state.0.lock().await;
        state.selected_serial.as_ref().map(|serial| InstallTarget {
            serial: serial.clone(),
            avd_name: state
                .devices
                .iter()
                .find(|d| &d.serial == serial)
                .and_then(|d| d.avd_name.clone()),
            model: None,
        })
    };
    blocking(move || {
        debug_sessions::add_bookmark(session_id.as_deref(), device.as_ref(), &note, log_entry_id)
    })
    .await
}

/// The log lines kept with crash event `seq` of a debug session: the newest
/// `limit` (at most `MAX_CAPTURE_ENTRIES`), ending with the crash.
#[tauri::command]
pub async fn get_session_capture(
    id: String,
    seq: u32,
    limit: Option<u32>,
) -> Result<DebugSessionCapture, AppError> {
    blocking(move || debug_sessions::get_capture(&id, seq, limit)).await
}

/// Save a debug session as a zip bundle, redacted as `options` says, to a
/// file the user chooses in the save dialog. `None` when the dialog was
/// cancelled.
#[tauri::command]
pub async fn export_debug_session(
    app: AppHandle,
    id: String,
    options: SessionExportOptions,
) -> Result<Option<SessionExportResult>, AppError> {
    let name = {
        let id = id.clone();
        blocking(move || debug_sessions::export_file_name(&id)).await?
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Zip archive", &["zip"])
        .set_file_name(&name)
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx
        .await
        .map_err(|_| AppError::Other("Save dialog closed unexpectedly".into()))?
    else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    blocking(move || debug_sessions::export_session_to(&id, &options, &path))
        .await
        .map(Some)
}

/// Read the app's exit reasons from the session's device and add those that
/// belong to the session.
#[tauri::command]
pub async fn refresh_session_exit_reasons(id: String) -> Result<DebugSessionExitRefresh, AppError> {
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    debug_sessions::refresh_exit_reasons(&id, &adb).await
}

/// Import a debug session bundle the user chooses in the open dialog, as a
/// read-only imported session. `None` when the dialog was cancelled.
#[tauri::command]
pub async fn import_debug_session(app: AppHandle) -> Result<Option<DebugSessionSummary>, AppError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Zip archive", &["zip"])
        .pick_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx
        .await
        .map_err(|_| AppError::Other("Open dialog closed unexpectedly".into()))?
    else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    blocking(move || debug_sessions::import_session_from(&path))
        .await
        .map(Some)
}

/// Delete an imported debug session.
#[tauri::command]
pub async fn delete_imported_debug_session(id: String) -> Result<(), AppError> {
    blocking(move || debug_sessions::delete_imported_session(&id)).await
}
