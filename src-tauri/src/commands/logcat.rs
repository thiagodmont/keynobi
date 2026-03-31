use crate::models::logcat::{LogStats, LogcatFilterSpec, ProcessedEntry};
use crate::services::logcat::{self, LogcatFilter, LogcatState, LogcatStateInner};
use crate::services::settings_manager;
use std::sync::Arc;
use tauri::{AppHandle, State};
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
    let settings = settings_manager::load_settings();
    let adb_bin = logcat::find_adb_binary(settings.android.sdk_path.as_deref());

    {
        let mut state = logcat_state.lock().await;
        if state.streaming {
            return Ok(()); // already running
        }
        state.streaming = true;
        state.device_serial = device_serial.clone();
    }

    let state_clone = logcat_state.inner().clone();
    tokio::spawn(async move {
        logcat::start_logcat_stream(adb_bin, device_serial, state_clone, app_handle).await;
    });

    Ok(())
}

/// Stop the logcat stream.
#[tauri::command]
pub async fn stop_logcat(logcat_state: State<'_, LogcatState>) -> Result<(), String> {
    let mut state = logcat_state.lock().await;
    state.streaming = false;
    Ok(())
}

/// Clear the in-memory logcat buffer and emit a `logcat:cleared` event.
#[tauri::command]
pub async fn clear_logcat(
    logcat_state: State<'_, LogcatState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    use tauri::Emitter;
    let mut state = logcat_state.lock().await;
    state.store.clear();
    state.known_packages.clear();
    let _ = app_handle.emit("logcat:cleared", ());
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
        min_level.as_deref().map(parse_level),
        tag,
        text,
        package,
        only_crashes,
    );

    let state = logcat_state.lock().await;
    let limit = count.unwrap_or(1000).min(10_000);
    Ok(state.store.query(&filter, limit))
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
    Ok(state.store.stats.clone())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_level(s: &str) -> crate::models::logcat::LogcatLevel {
    use crate::models::logcat::LogcatLevel;
    match s.to_uppercase().as_str() {
        "V" | "VERBOSE" => LogcatLevel::Verbose,
        "D" | "DEBUG" => LogcatLevel::Debug,
        "I" | "INFO" => LogcatLevel::Info,
        "W" | "WARN" | "WARNING" => LogcatLevel::Warn,
        "E" | "ERROR" => LogcatLevel::Error,
        "F" | "FATAL" | "A" | "ASSERT" => LogcatLevel::Fatal,
        _ => LogcatLevel::Verbose,
    }
}

pub fn new_logcat_state() -> LogcatState {
    Arc::new(Mutex::new(LogcatStateInner::new()))
}
