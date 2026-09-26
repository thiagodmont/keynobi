use crate::models::error::AppError;
use crate::models::logcat::{LogStats, LogcatFilterSpec, ProcessedEntry};
use crate::models::retrace::RetraceOutcome;
use crate::services::adb_manager::DeviceState;
use crate::services::logcat::{self, LogcatFilter, LogcatState, LogcatStateInner};
use crate::services::retrace::{self, RetraceEnv};
use crate::services::settings_manager;
use crate::FsState;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Mutex;

// ── Commands ──────────────────────────────────────────────────────────────────

/// Start streaming logcat from the specified device (or the selected device).
/// Spawns a background task; events arrive as `logcat:entries` on the frontend.
#[tauri::command]
pub async fn start_logcat(
    device_serial: Option<String>,
    logcat_state: State<'_, LogcatState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let (settings, _) = settings_manager::load_settings();
    let adb_bin = logcat::find_adb_binary(settings.android.sdk_path.as_deref());
    logcat::request_start(&logcat_state, adb_bin, device_serial, Some(app_handle))
        .await
        .map(|_| ())
}

/// Stop the logcat stream.
#[tauri::command]
pub async fn stop_logcat(logcat_state: State<'_, LogcatState>) -> Result<(), String> {
    logcat::request_stop(&logcat_state).await;
    Ok(())
}

/// Clear the in-memory logcat buffer and emit a `logcat:cleared` event.
#[tauri::command]
pub async fn clear_logcat(
    logcat_state: State<'_, LogcatState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    logcat::request_clear(&logcat_state, Some(&app_handle)).await;
    Ok(())
}

/// Return recent logcat entries from the ring buffer.
/// Optionally filter by minimum log level, tag, text, or package name.
#[tauri::command]
pub async fn get_logcat_entries(
    count: Option<usize>,
    min_level: Option<String>,
    tag: Option<String>,
    text: Option<String>,
    package: Option<String>,
    only_crashes: bool,
    logcat_state: State<'_, LogcatState>,
) -> Result<Vec<ProcessedEntry>, String> {
    let filter = LogcatFilter::new(
        min_level.as_deref().map(logcat::parse_level_str),
        tag,
        text,
        package,
        only_crashes,
    );

    let state = logcat_state.lock().await;
    let cap = state.store.capacity();
    let limit = count.unwrap_or(1000).min(cap);
    Ok(state.store.query(&filter, limit))
}

/// Return unfiltered entries adjacent to an anchor entry in the ring buffer.
#[tauri::command]
pub async fn get_logcat_context_entries(
    anchor_id: u64,
    direction: String,
    count: Option<usize>,
    logcat_state: State<'_, LogcatState>,
) -> Result<Vec<ProcessedEntry>, String> {
    let state = logcat_state.lock().await;
    let limit = count.unwrap_or(10).min(state.store.capacity());
    match direction.as_str() {
        "before" => Ok(state.store.context_before(anchor_id, limit)),
        "after" => Ok(state.store.context_after(anchor_id, limit)),
        other => Err(format!("Invalid logcat context direction: {other}")),
    }
}

/// Return whether logcat is currently streaming.
#[tauri::command]
pub async fn get_logcat_status(logcat_state: State<'_, LogcatState>) -> Result<bool, String> {
    Ok(logcat_state.lock().await.streaming)
}

/// Return the sorted list of all known package names seen in this session.
#[tauri::command]
pub async fn list_logcat_packages(
    logcat_state: State<'_, LogcatState>,
) -> Result<Vec<String>, String> {
    Ok(logcat_state.lock().await.known_packages_sorted())
}

/// Update the active stream filter.
///
/// After this call, the batcher will only emit entries that match the new
/// filter. Pass an empty `LogcatFilterSpec` (all fields `None`) to clear
/// the filter and forward all entries again.
///
/// The frontend should follow up with `get_logcat_entries` using the same
/// filter to obtain a fresh snapshot of the stored buffer.
#[tauri::command]
pub async fn set_logcat_filter(
    filter_spec: LogcatFilterSpec,
    logcat_state: State<'_, LogcatState>,
) -> Result<(), String> {
    let filter = LogcatFilter::from_spec(&filter_spec);

    // If all fields are empty/false, clear the filter.
    let is_empty = filter_spec.min_level.is_none()
        && filter_spec.tag.is_none()
        && filter_spec.text.is_none()
        && filter_spec.package.is_none()
        && !filter_spec.only_crashes;

    let mut state = logcat_state.lock().await;
    if is_empty {
        state.stream_state.set_filter(None);
    } else {
        state.stream_state.set_filter(Some(filter));
    }
    Ok(())
}

/// Return running statistics for the current logcat session.
/// Useful for the status bar (crash count, level distribution, etc.).
#[tauri::command]
pub async fn get_logcat_stats(logcat_state: State<'_, LogcatState>) -> Result<LogStats, String> {
    let state = logcat_state.lock().await;
    let mut stats = state.store.stats.clone();
    let len = state.store.len() as u64;
    stats.buffer_entry_count = len;
    let cap = state.store.capacity().max(1) as f32;
    stats.buffer_usage_pct = (len as f32 / cap) * 100.0;
    Ok(stats)
}

/// Ask the user where to save `contents` (the entries the panel displays) and
/// write the file. Returns the saved path, or `None` if the user cancels.
///
/// The dialog and the write both run here so the webview needs no filesystem
/// access.
#[tauri::command]
pub async fn export_logcat(app: AppHandle, contents: String) -> Result<Option<String>, AppError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Log", &["log", "txt"])
        .set_file_name("logcat.log")
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
    tokio::fs::write(&path, contents)
        .await
        .map_err(|e| AppError::io(path.display(), e))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Deobfuscate crash `crash_group_id` from the logcat buffer with the saved
/// R8 mapping of the build that produced it (see `services::retrace`).
/// `NotFound` when the crash left the buffer; a missing tool, a refusal, or a
/// failed run is reported in the outcome.
#[tauri::command]
pub async fn retrace_crash(
    crash_group_id: u64,
    logcat_state: State<'_, LogcatState>,
    device_state: State<'_, DeviceState>,
    fs_state: State<'_, FsState>,
) -> Result<RetraceOutcome, AppError> {
    let (project_root, gradle_root) = {
        let fs = fs_state.0.lock().await;
        (fs.project_root.clone(), fs.gradle_root.clone())
    };
    let (settings, _) = settings_manager::load_settings();
    let env = RetraceEnv::new(
        settings,
        project_root,
        gradle_root,
        device_state.inner().clone(),
    );
    retrace::retrace_crash_group(&env, &logcat_state, crash_group_id).await
}

pub fn new_logcat_state() -> LogcatState {
    Arc::new(Mutex::new(LogcatStateInner::new()))
}
