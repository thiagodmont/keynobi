//! Debug session commands: thin wrappers over `services::debug_sessions`.

use crate::models::debug_session::{
    DebugSessionCapture, DebugSessionDetail, DebugSessionEvent, DebugSessionExitRefresh,
    DebugSessionSummary,
};
use crate::models::error::AppError;
use crate::services::adb_manager::{get_adb_path, DeviceState};
use crate::services::debug_sessions;
use crate::services::installed_builds::InstallTarget;
use crate::services::settings_manager;
use tauri::State;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Other(format!("Debug session task failed: {e}")))?
}

/// Every debug session, newest first.
#[tauri::command]
pub async fn list_debug_sessions() -> Result<Vec<DebugSessionSummary>, AppError> {
    blocking(|| Ok(debug_sessions::list_sessions())).await
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

/// Read the app's exit reasons from the session's device and add those that
/// belong to the session.
#[tauri::command]
pub async fn refresh_session_exit_reasons(id: String) -> Result<DebugSessionExitRefresh, AppError> {
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    debug_sessions::refresh_exit_reasons(&id, &adb).await
}
