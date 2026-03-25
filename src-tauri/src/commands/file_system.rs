use crate::models::{FileNode, FsError};
use crate::services::fs_manager;
use crate::FsState;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

// ── Path validation ───────────────────────────────────────────────────────────

/// Verify that `path` is canonically inside `root`.
///
/// Prevents path-traversal attacks where a crafted path string
/// (e.g. `/project/../../../etc/passwd`) escapes the open project directory.
/// Both paths are canonicalised before the prefix check so symlinks and
/// `..` segments are resolved by the OS before comparison.
fn ensure_within_project(path: &Path, root: &Path) -> Result<(), FsError> {
    let canonical_path = if path.exists() {
        path.canonicalize().map_err(|e| {
            FsError::io(path.display().to_string(), e)
        })?
    } else {
        let parent = path.parent().ok_or_else(|| {
            FsError::InvalidPath(format!("Path has no parent: {}", path.display()))
        })?;
        let canonical_parent = parent.canonicalize().map_err(|e| {
            FsError::io(parent.display().to_string(), e)
        })?;
        canonical_parent.join(
            path.file_name()
                .ok_or_else(|| FsError::InvalidPath(format!("Invalid filename: {}", path.display())))?,
        )
    };

    let canonical_root = root.canonicalize().map_err(|e| {
        FsError::io(root.display().to_string(), e)
    })?;

    if !canonical_path.starts_with(&canonical_root) {
        return Err(FsError::PathTraversal(path.display().to_string()));
    }

    Ok(())
}

/// Read the project root from state, dropping the lock immediately.
/// Returns `None` if no project is open.
/// All `canonicalize()` calls happen OUTSIDE the lock to avoid holding
/// the Mutex during blocking syscalls.
async fn get_project_root_unlocked(state: &State<'_, FsState>) -> Option<PathBuf> {
    let guard = state.0.lock().await;
    guard.project_root.clone()
}

/// Validate that a path is within the project (if one is open).
/// The Mutex is only held for the brief clone of `project_root`;
/// the expensive `canonicalize()` calls run after the lock is released.
async fn validate_path(
    path: &Path,
    state: &State<'_, FsState>,
) -> Result<(), String> {
    if let Some(root) = get_project_root_unlocked(state).await {
        ensure_within_project(path, &root).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Validate two paths (for rename operations).
async fn validate_paths(
    path_a: &Path,
    path_b: &Path,
    state: &State<'_, FsState>,
) -> Result<(), String> {
    if let Some(root) = get_project_root_unlocked(state).await {
        ensure_within_project(path_a, &root).map_err(|e| e.to_string())?;
        ensure_within_project(path_b, &root).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn open_project(
    path: String,
    app_handle: AppHandle,
    state: State<'_, FsState>,
) -> Result<FileNode, String> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {path}"));
    }

    let tree = fs_manager::build_file_tree(&root);
    let debouncer = fs_manager::start_watching(root.clone(), app_handle)
        .map_err(|e| e.to_string())?;

    let mut guard = state.0.lock().await;
    guard.watcher = None;
    guard.project_root = Some(root);
    guard.watcher = Some(debouncer);

    Ok(tree)
}

#[tauri::command]
pub async fn get_file_tree(state: State<'_, FsState>) -> Result<FileNode, String> {
    let root = get_project_root_unlocked(&state)
        .await
        .ok_or("No project open")?;
    Ok(fs_manager::build_file_tree(&root))
}

#[tauri::command]
pub async fn get_directory_children(
    path: String,
    state: State<'_, FsState>,
) -> Result<Vec<FileNode>, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    validate_path(&dir, &state).await?;
    Ok(fs_manager::expand_directory(&dir))
}

#[tauri::command]
pub async fn read_file(path: String, state: State<'_, FsState>) -> Result<String, String> {
    let p = PathBuf::from(&path);
    validate_path(&p, &state).await?;
    fs_manager::read_file(&p).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn write_file(
    path: String,
    content: String,
    state: State<'_, FsState>,
) -> Result<(), String> {
    let p = PathBuf::from(&path);
    validate_path(&p, &state).await?;
    fs_manager::write_file(&p, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_file(path: String, state: State<'_, FsState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    validate_path(&p, &state).await?;
    fs_manager::create_file(&p).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_directory(path: String, state: State<'_, FsState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    validate_path(&p, &state).await?;
    fs_manager::create_directory(&p).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_path(path: String, state: State<'_, FsState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    validate_path(&p, &state).await?;
    fs_manager::delete_path(&p).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_path(
    old_path: String,
    new_path: String,
    state: State<'_, FsState>,
) -> Result<(), String> {
    let old = PathBuf::from(&old_path);
    let new = PathBuf::from(&new_path);
    validate_paths(&old, &new, &state).await?;
    fs_manager::rename_path(&old, &new).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    Ok(get_project_root_unlocked(&state)
        .await
        .map(|p| p.to_string_lossy().to_string()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("valid.kt"), "val x = 1").unwrap();
        (dir, root)
    }

    #[test]
    fn allows_path_inside_root() {
        let (_dir, root) = setup();
        let path = root.join("valid.kt");
        assert!(ensure_within_project(&path, &root).is_ok());
    }

    #[test]
    fn allows_new_file_path_inside_root() {
        let (_dir, root) = setup();
        let path = root.join("new_file.kt");
        assert!(
            ensure_within_project(&path, &root).is_ok(),
            "non-existent target inside root should be allowed"
        );
    }

    #[test]
    fn rejects_path_outside_root() {
        let (_dir, root) = setup();
        let escaped = PathBuf::from("/tmp");
        let result = ensure_within_project(&escaped, &root);
        assert!(result.is_err(), "path outside root must be rejected");
        assert!(
            matches!(result.unwrap_err(), FsError::PathTraversal(_)),
            "should be PathTraversal variant"
        );
    }

    #[test]
    fn rejects_dotdot_escape_from_root() {
        let (_dir, root) = setup();
        let escaped = root.join("..").join("etc").join("passwd");
        let result = ensure_within_project(&escaped, &root);
        assert!(result.is_err(), "path escaping root via '..' must be rejected");
    }

    #[test]
    fn rejects_absolute_path_outside_root() {
        let (_dir, root) = setup();
        let outside = PathBuf::from("/tmp");
        let result = ensure_within_project(&outside, &root);
        assert!(result.is_err(), "/tmp is outside any project root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_root() {
        use std::os::unix::fs::symlink;
        let (_dir, root) = setup();
        let link = root.join("escape_link");
        symlink("/tmp", &link).unwrap();
        let result = ensure_within_project(&link, &root);
        assert!(
            result.is_err(),
            "symlink pointing outside root must be rejected"
        );
    }

    #[test]
    fn allows_nested_path_inside_root() {
        let (_dir, root) = setup();
        fs::create_dir_all(root.join("src/main/kotlin")).unwrap();
        fs::write(root.join("src/main/kotlin/Main.kt"), "fun main() {}").unwrap();
        let nested = root.join("src/main/kotlin/Main.kt");
        assert!(ensure_within_project(&nested, &root).is_ok());
    }

    #[test]
    fn rejects_path_with_dotdot_in_middle() {
        let (_dir, root) = setup();
        fs::create_dir_all(root.join("subdir")).unwrap();
        let tricky = root.join("subdir").join("..").join("..").join("..").join("etc/passwd");
        let result = ensure_within_project(&tricky, &root);
        assert!(result.is_err(), "dotdot traversal must be rejected: {:?}", tricky);
    }
}
