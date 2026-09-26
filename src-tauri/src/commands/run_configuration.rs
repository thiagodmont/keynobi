use crate::models::error::AppError;
use crate::models::run_configuration::{ProjectRunConfigurations, RunConfiguration};
use crate::services::run_configurations;
use crate::FsState;
use tauri::State;

/// The open project's root, as its registry entry names it.
async fn open_project_root(fs_state: &State<'_, FsState>) -> Result<String, AppError> {
    let fs = fs_state.0.lock().await;
    fs.project_root
        .as_ref()
        .or(fs.gradle_root.as_ref())
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::NotFound("No project is open".into()))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

/// The run configurations of `project_root` (a registered project), or of the
/// open project. The first read creates them from the project's application
/// modules.
#[tauri::command]
pub async fn list_run_configurations(
    project_root: Option<String>,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = match project_root {
        Some(root) => root,
        None => open_project_root(&fs_state).await?,
    };
    blocking(move || run_configurations::list(&project_root)).await
}

/// Save a run configuration of the open project, replacing the one of the
/// same name.
#[tauri::command]
pub async fn save_run_configuration(
    config: RunConfiguration,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::save(&project_root, config)).await
}

/// Delete a run configuration of the open project.
#[tauri::command]
pub async fn delete_run_configuration(
    name: String,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::delete(&project_root, &name)).await
}

/// Make a run configuration of the open project the active one.
#[tauri::command]
pub async fn set_active_run_configuration(
    name: String,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::set_active(&project_root, &name)).await
}
