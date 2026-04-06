mod commands;
pub mod models;
pub mod services;
pub mod utils;

use commands::build::{
    cancel_build, find_apk_path, finalize_build, get_build_errors, get_build_history,
    get_build_status, run_gradle_task,
};
use tauri::{Emitter, Manager};
use commands::device::{
    create_avd_device, delete_avd_device, download_system_image_cmd, get_selected_device,
    install_apk_on_device, launch_app_on_device, launch_avd, list_adb_devices, list_avd_devices,
    list_available_system_images_cmd, list_device_definitions_cmd, list_system_images_cmd,
    refresh_devices, select_device, start_device_polling, stop_app_on_device, stop_avd,
    stop_device_polling, wipe_avd_data_cmd,
};
use commands::file_system::{
    get_application_id, get_gradle_root, get_last_active_project, get_project_app_info,
    get_project_root, list_projects, open_project, pin_project, remove_project,
    rename_project, save_project_app_info, update_project_meta,
};
use commands::health::run_health_checks;
use commands::logcat::{
    clear_logcat, get_logcat_entries, get_logcat_stats, get_logcat_status,
    list_logcat_packages, new_logcat_state, set_logcat_filter, start_logcat, stop_logcat,
};
use commands::mcp::{configure_mcp_in_claude, get_mcp_activity, get_mcp_server_status, get_mcp_setup_status, clear_mcp_activity, start_mcp_server};
use commands::settings::*;
use commands::studio::open_in_studio;
use commands::variant::{get_variants_from_gradle, get_variants_preview, set_active_variant};
use models::log_entry::LogEntry;
use services::adb_manager::DeviceState;
use services::build_runner::BuildState;
use services::process_manager::ProcessManager;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Per-concern state ─────────────────────────────────────────────────────────

/// File-system state: the open project root and detected Gradle root.
///
/// Accessed by all file commands. The Mutex is held only for the brief
/// moment needed to read `project_root` — never during I/O operations.
pub struct FsState(pub Arc<Mutex<FsStateInner>>);

pub struct FsStateInner {
    pub project_root: Option<PathBuf>,
    /// The detected Gradle project root (ancestor with `settings.gradle(.kts)`).
    /// Used as the build workspace root and security boundary.
    /// Falls back to `project_root` when no Gradle root is found.
    pub gradle_root: Option<PathBuf>,
}

impl FsState {
    pub fn new() -> Self {
        FsState(Arc::new(Mutex::new(FsStateInner {
            project_root: None,
            gradle_root: None,
        })))
    }
}

impl Clone for FsState {
    fn clone(&self) -> Self {
        FsState(self.0.clone())
    }
}

impl Default for FsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded ring-buffer of structured log entries shared across all log sources
/// (Logcat, build output).  Protected by a `tokio::sync::Mutex` so it can be
/// safely written from async tasks and read from Tauri commands without holding
/// the lock across I/O.
pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

/// Maximum entries kept in [`LogBuffer`] before the oldest is evicted.
pub const MAX_LOG_ENTRIES: usize = 50_000;

fn cleanup_old_logs(log_dir: &std::path::Path, retention_days: u32) {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(u64::from(retention_days) * 86_400))
        .unwrap_or(std::time::UNIX_EPOCH);

    let Ok(entries) = std::fs::read_dir(log_dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        // Only touch files matching app.log.* pattern.
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("app.log") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = std::fs::remove_file(&path);
                    tracing::info!("Removed old log file: {}", path.display());
                }
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Logging setup ─────────────────────────────────────────────────────────
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".keynobi")
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // Daily rotating file appender. Old files are named app.log.YYYY-MM-DD.
    let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = tracing_subscriber::EnvFilter::try_from_env("KEYNOBI_LOG")
        .unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                tracing_subscriber::EnvFilter::new("debug")
            } else {
                tracing_subscriber::EnvFilter::new("warn")
            }
        });

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking_file)
                .with_ansi(false)
                .with_target(true),
        )
        .with(
            // In debug builds, also log to stderr for developer convenience.
            #[cfg(debug_assertions)]
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false),
            #[cfg(not(debug_assertions))]
            tracing_subscriber::layer::Identity::new(),
        )
        .with(env_filter)
        .init();

    // Keep the guard alive for the process lifetime so logs are flushed on exit.
    std::mem::forget(file_guard);

    let log_dir = log_dir.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(FsState::new())
        .manage(BuildState::new())
        .manage(ProcessManager::new())
        .manage(DeviceState::new())
        .manage(new_logcat_state())
        .manage(Arc::new(Mutex::new(VecDeque::<LogEntry>::new())) as LogBuffer)
        .setup(move |app| {
            let (settings, settings_corrupted) = services::settings_manager::load_settings();

            if settings_corrupted {
                let handle = app.handle().clone();
                tokio::spawn(async move {
                    // Small delay to let the frontend finish mounting before showing Toast.
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    if let Some(win) = handle.get_webview_window("main") {
                        let _ = win.emit("settings:corrupted", ());
                    }
                });
            }

            // Clean up log files older than the configured retention period.
            let retention_days = settings.advanced.log_retention_days;
            let log_dir_cleanup = log_dir.clone();
            tokio::spawn(async move {
                cleanup_old_logs(&log_dir_cleanup, retention_days);
            });

            // Auto-start MCP server if the user has enabled it in settings.
            if settings.mcp.auto_start {
                let handle = app.handle().clone();
                tokio::spawn(async move {
                    // Small delay so the window finishes initialising first.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Err(e) = services::mcp_server::start_mcp_server(handle.clone()).await {
                        tracing::warn!("MCP auto-start failed: {}", e);
                        // Notify the frontend so it can disable MCP UI and show an error.
                        if let Some(win) = handle.get_webview_window("main") {
                            let _ = win.emit("mcp:startup-failed", e.to_string());
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tokio::spawn(async move {
                    let cleanup = async {
                        // 1. Cancel any running Gradle build.
                        let build_state = app.state::<BuildState>();
                        let process_manager = app.state::<ProcessManager>();
                        services::build_runner::cancel_build(&build_state, &process_manager).await;

                        // 2. Stop logcat streaming (best-effort).
                        let logcat_state = app.state::<services::logcat::LogcatState>();
                        logcat_state.lock().await.streaming = false;

                        // 3. Stop ADB device polling.
                        let device_state = app.state::<DeviceState>();
                        device_state.0.lock().await.polling = false;
                    };

                    // Never block shutdown longer than 3 seconds.
                    if tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        cleanup,
                    )
                    .await
                    .is_err()
                    {
                        tracing::warn!("Graceful shutdown timed out after 3s — forcing close");
                    }

                    // Allow the window to actually close.
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.destroy();
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            // File system
            open_project,
            get_project_root,
            get_gradle_root,
            get_application_id,
            // Project registry
            list_projects,
            remove_project,
            pin_project,
            get_last_active_project,
            update_project_meta,
            rename_project,
            // Project App Info
            get_project_app_info,
            save_project_app_info,
            // Settings
            get_settings,
            save_settings,
            get_default_settings,
            reset_settings,
            detect_sdk_path,
            detect_java_path,
            // Health
            run_health_checks,
            // Build
            run_gradle_task,
            cancel_build,
            finalize_build,
            get_build_status,
            get_build_errors,
            get_build_history,
            find_apk_path,
            // Variants
            get_variants_preview,
            get_variants_from_gradle,
            set_active_variant,
            // Devices
            list_adb_devices,
            refresh_devices,
            select_device,
            get_selected_device,
            install_apk_on_device,
            launch_app_on_device,
            stop_app_on_device,
            list_avd_devices,
            launch_avd,
            stop_avd,
            start_device_polling,
            stop_device_polling,
            list_system_images_cmd,
            list_device_definitions_cmd,
            create_avd_device,
            delete_avd_device,
            wipe_avd_data_cmd,
            list_available_system_images_cmd,
            download_system_image_cmd,
            // Logcat
            start_logcat,
            stop_logcat,
            clear_logcat,
            get_logcat_entries,
            get_logcat_status,
            list_logcat_packages,
            set_logcat_filter,
            get_logcat_stats,
            // MCP Server
            start_mcp_server,
            get_mcp_setup_status,
            configure_mcp_in_claude,
            get_mcp_activity,
            get_mcp_server_status,
            clear_mcp_activity,
            // Android Studio integration
            open_in_studio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
