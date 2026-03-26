mod commands;
pub mod models;
pub mod services;

use commands::build::{
    cancel_build, find_apk_path, finalize_build, get_build_errors, get_build_history,
    get_build_status, run_gradle_task,
};
use commands::device::{
    get_selected_device, install_apk_on_device, launch_app_on_device, launch_avd,
    list_adb_devices, list_avd_devices, refresh_devices, select_device, start_device_polling,
    stop_app_on_device, stop_avd, stop_device_polling,
};
use commands::file_system::*;
use commands::health::run_health_checks;
use commands::lsp::*;
use commands::search::{search_in_file, search_project};
use commands::settings::*;
use commands::treesitter::{get_document_symbols, get_symbol_at_position, TreeSitterState};
use commands::variant::{get_build_variants, set_active_variant};
use models::log_entry::LogEntry;
use services::adb_manager::DeviceState;
use services::build_runner::BuildState;
use services::fs_manager::FsWatcher;
use services::process_manager::ProcessManager;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Per-concern state ─────────────────────────────────────────────────────────
//
// Each service domain gets its own state struct with its own Mutex.
// This prevents Phase 2+ services (build, devices, LSP) from blocking
// file operations — a `read_file` will never wait for a device poll.
//
// Pattern for future phases:
//   1. Create `BuildState`, `DeviceState`, etc. in their service modules
//   2. Register each with `.manage(BuildState::new())` below
//   3. Commands accept `State<'_, BuildState>` independently of `FsState`

/// File-system state: the open project root and the file watcher handle.
///
/// Accessed by all file commands. The Mutex is held only for the brief
/// moment needed to read `project_root` or swap the watcher — never during
/// the actual I/O operation.
pub struct FsState(pub Mutex<FsStateInner>);

pub struct FsStateInner {
    pub project_root: Option<PathBuf>,
    /// The detected Gradle project root (ancestor with `settings.gradle(.kts)`).
    /// Used as the LSP workspace root and as the security boundary for file
    /// operations.  Falls back to `project_root` when no Gradle root is found.
    pub gradle_root: Option<PathBuf>,
    pub watcher: Option<FsWatcher>,
}

impl FsState {
    pub fn new() -> Self {
        FsState(Mutex::new(FsStateInner {
            project_root: None,
            gradle_root: None,
            watcher: None,
        }))
    }
}

impl Default for FsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded ring-buffer of structured log entries shared across all log sources
/// (LSP server, Logcat, build output).  Protected by a `tokio::sync::Mutex`
/// so it can be safely written from async notification tasks and read from
/// Tauri commands without holding the lock across any I/O.
pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

/// Maximum entries kept in [`LogBuffer`] before the oldest is evicted.
pub const MAX_LOG_ENTRIES: usize = 2000;

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
        .manage(TreeSitterState::new())
        .manage(LspState::new())
        .manage(BuildState::new())
        .manage(ProcessManager::new())
        .manage(DeviceState::new())
        .manage(Arc::new(Mutex::new(VecDeque::<LogEntry>::new())) as LogBuffer)
        .invoke_handler(tauri::generate_handler![
            // File system
            open_project,
            get_file_tree,
            get_directory_children,
            read_file,
            write_file,
            create_file,
            create_directory,
            delete_path,
            rename_path,
            get_project_root,
            get_gradle_root,
            // Tree-sitter
            get_document_symbols,
            get_symbol_at_position,
            // Search
            search_project,
            search_in_file,
            // LSP
            lsp_check_installed,
            lsp_download,
            lsp_start,
            lsp_stop,
            lsp_status,
            lsp_did_open,
            lsp_did_change,
            lsp_did_save,
            lsp_did_close,
            lsp_complete,
            lsp_hover,
            lsp_definition,
            lsp_references,
            lsp_implementation,
            lsp_document_symbols,
            lsp_workspace_symbols,
            lsp_code_action,
            lsp_rename,
            lsp_format,
            lsp_pull_diagnostics,
            lsp_document_highlight,
            lsp_signature_help,
            lsp_get_logs,
            lsp_get_capabilities,
            lsp_append_client_log,
            lsp_decompile,
            lsp_read_archive_entry,
            lsp_semantic_tokens,
            lsp_code_action_filtered,
            lsp_generate_workspace_json,
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
            get_build_variants,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
