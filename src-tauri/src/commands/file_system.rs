use crate::models::FileNode;
use crate::services::fs_manager;
use crate::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn open_project(
    path: String,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {path}"));
    }

    let tree = fs_manager::build_file_tree(&root);
    let debouncer = fs_manager::start_watching(root.clone(), app_handle)?;

    let mut guard = state.0.lock().await;
    guard.project_root = Some(root);
    guard.watcher = Some(debouncer);

    Ok(tree)
}

#[tauri::command]
pub async fn get_file_tree(state: State<'_, AppState>) -> Result<FileNode, String> {
    let guard = state.0.lock().await;
    let root = guard
        .project_root
        .as_ref()
        .ok_or("No project open")?
        .clone();
    drop(guard);
    Ok(fs_manager::build_file_tree(&root))
}

#[tauri::command]
pub async fn get_directory_children(path: String) -> Result<Vec<FileNode>, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    Ok(fs_manager::expand_directory(&dir))
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    fs_manager::read_file(&PathBuf::from(&path))
}

#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    fs_manager::write_file(&PathBuf::from(&path), &content)
}

#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    fs_manager::create_file(&PathBuf::from(&path))
}

#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    fs_manager::create_directory(&PathBuf::from(&path))
}

#[tauri::command]
pub async fn delete_path(path: String) -> Result<(), String> {
    fs_manager::delete_path(&PathBuf::from(&path))
}

#[tauri::command]
pub async fn rename_path(old_path: String, new_path: String) -> Result<(), String> {
    fs_manager::rename_path(&PathBuf::from(&old_path), &PathBuf::from(&new_path))
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard
        .project_root
        .as_ref()
        .map(|p| p.to_string_lossy().to_string()))
}
