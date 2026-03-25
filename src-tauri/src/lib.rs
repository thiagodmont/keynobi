mod commands;
mod models;
mod services;

use commands::file_system::*;
use std::path::PathBuf;
use tokio::sync::Mutex;

pub struct AppStateInner {
    pub project_root: Option<PathBuf>,
    pub watcher: Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>,
}

pub struct AppState(pub Mutex<AppStateInner>);

impl AppState {
    pub fn new() -> Self {
        AppState(Mutex::new(AppStateInner {
            project_root: None,
            watcher: None,
        }))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
