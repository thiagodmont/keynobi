mod commands;
pub mod models;
pub mod services;

use commands::file_system::*;
use commands::lsp::*;
use commands::search::{search_in_file, search_project};
use commands::settings::*;
use commands::treesitter::{get_document_symbols, get_symbol_at_position, TreeSitterState};
use services::fs_manager::FsWatcher;
use std::path::PathBuf;
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
    pub watcher: Option<FsWatcher>,
}

impl FsState {
    pub fn new() -> Self {
        FsState(Mutex::new(FsStateInner {
            project_root: None,
            watcher: None,
        }))
    }
}

impl Default for FsState {
    fn default() -> Self {
        Self::new()
    }
}

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
            // Settings
            get_settings,
            save_settings,
            get_default_settings,
            reset_settings,
            detect_sdk_path,
            detect_java_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
