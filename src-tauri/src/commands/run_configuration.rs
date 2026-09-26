use crate::models::error::AppError;
use crate::models::run_configuration::{ProjectRunConfigurations, ResolvedRun, RunConfiguration};
use crate::services::adb_manager::{get_adb_path, DeviceState};
use crate::services::run_plan::{self, Devices, RunProject, RunRequest};
use crate::services::{run_configurations, settings_manager, ui_automation};
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

/// What Run App would do with the configuration named `name` (default: the
/// active one): its task, the online device it installs on (none when
/// `build_only`), what it launches, and the plan in one line.
/// `selected_serial` is the device selected in the app for this run (the
/// backend's selection when absent), used by a target of `ask` or `lastUsed`.
#[tauri::command]
pub async fn resolve_run_configuration(
    name: Option<String>,
    selected_serial: Option<String>,
    build_only: Option<bool>,
    fs_state: State<'_, FsState>,
    device_state: State<'_, DeviceState>,
) -> Result<ResolvedRun, AppError> {
    let (registry_root, gradle_root, trust_root) = {
        let fs = fs_state.0.lock().await;
        let gradle_root = fs
            .gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or_else(|| AppError::NotFound("No project is open".into()))?;
        let project_root = fs
            .project_root
            .clone()
            .unwrap_or_else(|| gradle_root.clone());
        (
            project_root.to_string_lossy().into_owned(),
            gradle_root,
            project_root,
        )
    };
    if let Some(serial) = &selected_serial {
        crate::utils::validation::validate_device_serial(serial).map_err(AppError::InvalidInput)?;
    }
    let (devices, selected) = {
        let state = device_state.0.lock().await;
        (
            state.devices.clone(),
            selected_serial.or_else(|| state.selected_serial.clone()),
        )
    };
    let request = RunRequest {
        name,
        build_only: build_only.unwrap_or(false),
    };
    blocking(move || {
        run_plan::resolve(
            RunProject {
                registry_root: &registry_root,
                gradle_root: &gradle_root,
                trust_root: &trust_root,
            },
            &request,
            Devices {
                list: &devices,
                selected: selected.as_deref(),
            },
        )
    })
    .await
}

/// Remember the device a run of the configuration named `name` installed on.
#[tauri::command]
pub async fn record_run_device(
    name: String,
    serial: String,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::record_last_device(&project_root, &name, &serial)).await
}

/// Open `uri` in `package` on the device (`am start -a VIEW -d <uri> -p
/// <package>`): a run configuration's deep-link launch. Android reports no
/// launch time for it.
#[tauri::command]
pub async fn open_deep_link_on_device(
    serial: String,
    uri: String,
    package: String,
) -> Result<String, AppError> {
    crate::utils::validation::validate_device_serial(&serial).map_err(AppError::InvalidInput)?;
    crate::utils::validation::validate_package_name(&package).map_err(AppError::InvalidInput)?;
    ui_automation::validate_deep_link_uri(&uri).map_err(AppError::InvalidInput)?;
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    ui_automation::adb_open_deep_link(&adb, &serial, &uri, Some(&package))
        .await
        .map_err(AppError::ProcessFailed)
}
