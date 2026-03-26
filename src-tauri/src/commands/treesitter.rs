use crate::models::lsp::SymbolInfo;
use crate::services::treesitter::TreeSitterService;
use crate::FsState;
use std::path::Path;
use tokio::sync::Mutex;

pub struct TreeSitterState(pub Mutex<TreeSitterService>);

impl TreeSitterState {
    pub fn new() -> Self {
        Self(Mutex::new(TreeSitterService::new()))
    }
}

impl Default for TreeSitterState {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate path is inside the project root using canonicalization.
async fn ensure_in_project(path: &str, fs_state: &tauri::State<'_, FsState>) -> Result<(), String> {
    let fs = fs_state.0.lock().await;
    let root = fs
        .project_root
        .as_ref()
        .ok_or("No project open")?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root: {e}"))?;
    drop(fs);

    let file = Path::new(path);
    let canonical = if file.exists() {
        file.canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?
    } else {
        return Err("File does not exist".into());
    };

    if !canonical.starts_with(&root) {
        return Err("Path is outside the project directory".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn get_document_symbols(
    path: String,
    ts_state: tauri::State<'_, TreeSitterState>,
    fs_state: tauri::State<'_, FsState>,
) -> Result<Vec<SymbolInfo>, String> {
    ensure_in_project(&path, &fs_state).await?;

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let file_path = Path::new(&path);
    let mut ts = ts_state.0.lock().await;
    ts.parse_file(file_path, &content);
    let symbols = ts.extract_symbols(file_path, &content);
    Ok(symbols)
}

#[tauri::command]
pub async fn get_symbol_at_position(
    path: String,
    line: u32,
    col: u32,
    ts_state: tauri::State<'_, TreeSitterState>,
    fs_state: tauri::State<'_, FsState>,
) -> Result<Option<String>, String> {
    ensure_in_project(&path, &fs_state).await?;

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let file_path = Path::new(&path);
    let mut ts = ts_state.0.lock().await;
    if ts.get_cached_tree(file_path).is_none() {
        ts.parse_file(file_path, &content);
    }
    Ok(ts.find_node_at_position(file_path, line, col, &content))
}
