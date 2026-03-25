use crate::models::FileNode;
use crate::services::fs_manager;
use crate::AppState;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

// ── Path validation ───────────────────────────────────────────────────────────

/// Verify that `path` is canonically inside `root`.
///
/// This prevents path-traversal attacks where a crafted path string
/// (e.g. `/project/../../../etc/passwd`) escapes the open project directory.
/// We canonicalise both paths before comparing so symlinks and `..` segments
/// are resolved before the prefix check.
fn ensure_within_project(path: &Path, root: &Path) -> Result<(), String> {
    // Use the path as-is if it doesn't exist yet (e.g. a new file being created).
    // In that case we canonicalise the parent instead.
    let canonical_path = if path.exists() {
        path.canonicalize()
            .map_err(|e| format!("Cannot resolve path '{}': {e}", path.display()))?
    } else {
        // Canonicalise the parent; the final component may not exist yet.
        let parent = path
            .parent()
            .ok_or_else(|| format!("Path has no parent: {}", path.display()))?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| format!("Cannot resolve parent '{}': {e}", parent.display()))?;
        canonical_parent.join(path.file_name().ok_or("Invalid filename")?)
    };

    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Cannot resolve project root: {e}"))?;

    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "Access denied: '{}' is outside the open project directory",
            path.display()
        ));
    }

    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────

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
    // Dropping the old debouncer here stops the previous watcher cleanly.
    guard.watcher = None;
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
pub async fn get_directory_children(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileNode>, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    // Confinement check: directory must be inside the open project.
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&dir, root)?;
    }
    drop(guard);

    Ok(fs_manager::expand_directory(&dir))
}

#[tauri::command]
pub async fn read_file(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&p, root)?;
    }
    drop(guard);
    fs_manager::read_file(&p)
}

#[tauri::command]
pub async fn write_file(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&p, root)?;
    }
    drop(guard);
    fs_manager::write_file(&p, &content)
}

#[tauri::command]
pub async fn create_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&p, root)?;
    }
    drop(guard);
    fs_manager::create_file(&p)
}

#[tauri::command]
pub async fn create_directory(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&p, root)?;
    }
    drop(guard);
    fs_manager::create_directory(&p)
}

#[tauri::command]
pub async fn delete_path(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&p, root)?;
    }
    drop(guard);
    fs_manager::delete_path(&p)
}

#[tauri::command]
pub async fn rename_path(
    old_path: String,
    new_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let old = PathBuf::from(&old_path);
    let new = PathBuf::from(&new_path);
    let guard = state.0.lock().await;
    if let Some(root) = &guard.project_root {
        ensure_within_project(&old, root)?;
        ensure_within_project(&new, root)?;
    }
    drop(guard);
    fs_manager::rename_path(&old, &new)
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard
        .project_root
        .as_ref()
        .map(|p| p.to_string_lossy().to_string()))
}
