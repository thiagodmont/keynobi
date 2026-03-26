use crate::models::search::{SearchOptions, SearchResult};
use crate::services::search_engine;
use crate::FsState;
use std::path::Path;

#[tauri::command]
pub async fn search_project(
    query: String,
    options: SearchOptions,
    fs_state: tauri::State<'_, FsState>,
) -> Result<Vec<SearchResult>, String> {
    let fs = fs_state.0.lock().await;
    let project_root = fs
        .project_root
        .as_ref()
        .ok_or("No project open")?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root: {e}"))?;
    drop(fs);

    tokio::task::spawn_blocking(move || {
        search_engine::search_project(&query, &project_root, &options)
    })
    .await
    .map_err(|e| format!("Search task failed: {e}"))?
}

#[tauri::command]
pub async fn search_in_file(
    path: String,
    query: String,
    options: SearchOptions,
    fs_state: tauri::State<'_, FsState>,
) -> Result<Vec<SearchResult>, String> {
    let fs = fs_state.0.lock().await;
    let project_root = fs
        .project_root
        .as_ref()
        .ok_or("No project open")?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root: {e}"))?;
    drop(fs);

    let file_path = Path::new(&path);
    let canonical_file = file_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;
    if !canonical_file.starts_with(&project_root) {
        return Err("Path is outside the project directory".into());
    }

    let p = path.clone();
    tokio::task::spawn_blocking(move || {
        let file = Path::new(&p);
        let parent = file.parent().unwrap_or(file);
        let mut opts = options;
        opts.include_pattern = Some(
            file.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );
        search_engine::search_project(&query, parent, &opts)
    })
    .await
    .map_err(|e| format!("Search task failed: {e}"))?
}
