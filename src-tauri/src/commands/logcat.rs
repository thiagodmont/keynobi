use crate::services::logcat::{
    self, LogcatEntry, LogcatFilter, LogcatLevel, LogcatState, LogcatStateInner,
};
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
    state.buffer.clear();
    // Also clear the pid→package map — PIDs from a previous run are stale.
    state.pid_to_package.clear();
    state.seen_packages.clear();
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
) -> Result<Vec<LogcatEntry>, String> {
    let filter = LogcatFilter::new(
        min_level.as_deref().map(parse_level),
        tag,
        text,
        package,
        only_crashes,
    );

    let state = logcat_state.lock().await;
    let limit = count.unwrap_or(1000).min(10_000);

    let entries: Vec<LogcatEntry> = state
        .buffer
        .entries
        .iter()
        .rev()
        .filter(|e| filter.matches(e))
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    Ok(entries)
}

/// Return whether logcat is currently streaming.
#[tauri::command]
pub async fn get_logcat_status(logcat_state: State<'_, LogcatState>) -> Result<bool, String> {
    Ok(logcat_state.lock().await.streaming)
}

/// Return the sorted list of all known package names seen in this logcat session.
/// Populated from ActivityManager "Start proc" lines.
#[tauri::command]
pub async fn list_logcat_packages(
    logcat_state: State<'_, LogcatState>,
) -> Result<Vec<String>, String> {
    Ok(logcat_state.lock().await.known_packages())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_level(s: &str) -> LogcatLevel {
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
