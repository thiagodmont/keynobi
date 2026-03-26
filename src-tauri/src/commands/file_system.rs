use crate::services::fs_manager;
use crate::FsState;
use std::path::PathBuf;
use tauri::State;

// ── Commands ──────────────────────────────────────────────────────────────────

/// Open an Android project folder and detect the Gradle root.
/// Returns the detected project name on success.
#[tauri::command]
pub async fn open_project(
    path: String,
    state: State<'_, FsState>,
) -> Result<String, String> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {path}"));
    }

    let gradle_root = fs_manager::find_gradle_root(&root);
    if let Some(ref gr) = gradle_root {
        tracing::info!(
            "Gradle root detected: {} (opened: {})",
            gr.display(),
            root.display()
        );
    } else {
        tracing::info!(
            "No Gradle root found above {}; using it as workspace root",
            root.display()
        );
    }

    let project_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    let mut guard = state.0.lock().await;
    guard.project_root = Some(root);
    guard.gradle_root = gradle_root;

    Ok(project_name)
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard.project_root.as_ref().map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn get_gradle_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard.gradle_root.as_ref().map(|p| p.to_string_lossy().to_string()))
}

/// Try to read the `applicationId` from the app-level build.gradle(.kts).
/// Called once on project open so the frontend can resolve `package:mine`.
#[tauri::command]
pub async fn get_application_id(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    let root = guard
        .gradle_root
        .as_ref()
        .or(guard.project_root.as_ref())
        .cloned();
    drop(guard);

    let Some(root) = root else { return Ok(None) };

    let candidates = [
        root.join("app").join("build.gradle.kts"),
        root.join("app").join("build.gradle"),
        root.join("build.gradle.kts"),
        root.join("build.gradle"),
    ];

    for path in &candidates {
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Some(id) = extract_application_id(&content) {
                    return Ok(Some(id));
                }
            }
        }
    }
    Ok(None)
}

fn extract_application_id(content: &str) -> Option<String> {
    // Matches: applicationId "com.example" or applicationId = "com.example"
    let re = regex::Regex::new(r#"applicationId\s*=?\s*"([^"]+)""#).ok()?;
    let caps = re.captures(content)?;
    Some(caps.get(1)?.as_str().to_owned())
}
