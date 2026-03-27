mod commands;
pub mod models;
pub mod services;

use commands::build::{
    cancel_build, find_apk_path, finalize_build, get_build_errors, get_build_history,
    get_build_status, run_gradle_task,
};
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
    clear_logcat, get_logcat_entries, get_logcat_status, list_logcat_packages,
    new_logcat_state, start_logcat, stop_logcat,
};
use commands::mcp::start_mcp_server;
use commands::settings::*;
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
pub struct FsState(pub Mutex<FsStateInner>);

pub struct FsStateInner {
    pub project_root: Option<PathBuf>,
    /// The detected Gradle project root (ancestor with `settings.gradle(.kts)`).
    /// Used as the build workspace root and security boundary.
    /// Falls back to `project_root` when no Gradle root is found.
    pub gradle_root: Option<PathBuf>,
}

impl FsState {
    pub fn new() -> Self {
        FsState(Mutex::new(FsStateInner {
            project_root: None,
            gradle_root: None,
        }))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

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
            // MCP Server
            start_mcp_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
