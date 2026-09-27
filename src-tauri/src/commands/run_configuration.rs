use crate::models::build::BuildActor;
use crate::models::error::AppError;
use crate::models::run_configuration::{
    DeployPhaseEvent, DeployResult, ProjectRunConfigurations, ResolvedRun, RunConfiguration,
    TargetPreference,
};
use crate::services::adb_manager::{find_aapt2, get_adb_path, DeviceState};
use crate::services::build_runner::{BuildOutcome, BuildState};
use crate::services::deploy::{self, DeployEnv, DeployHooks, OpenProject};
use crate::services::logcat::LogcatState;
use crate::services::process_manager::ProcessManager;
use crate::services::run_plan::{self, Devices, RunRequest};
use crate::services::{gradle_modules, run_configurations, settings_manager};
use crate::FsState;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// The shortest and longest wait for the build of the app's run
/// (`mcp.buildTimeoutSec`, clamped).
const RUN_BUILD_TIMEOUT_SECS: (u64, u64) = (60, 3_600);

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
/// same name. `shared` moves it into the project's shared file (`true`) or
/// back to the local configurations (`false`); without it, it stays where it
/// is.
#[tauri::command]
pub async fn save_run_configuration(
    config: RunConfiguration,
    shared: Option<bool>,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::save(&project_root, config, shared)).await
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
    let project = OpenProject::of(&fs_state).await?;
    let request = RunRequest {
        name,
        build_only: build_only.unwrap_or(false),
        device: None,
    };
    resolve(project, request, selected_serial, &device_state).await
}

async fn resolve(
    project: OpenProject,
    request: RunRequest,
    selected_serial: Option<String>,
    device_state: &DeviceState,
) -> Result<ResolvedRun, AppError> {
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
    blocking(move || {
        run_plan::resolve(
            project.run_project(),
            &request,
            Devices {
                list: &devices,
                selected: selected.as_deref(),
            },
        )
    })
    .await
}

/// Run the configuration named `name` (default: the active one): resolve it
/// as `resolve_run_configuration` does, build its task, install the APK that
/// build wrote, and launch it. Progress arrives as `build:*` and
/// `deploy:phase` events. `project_root` is the project the app resolved the
/// run for; the run is refused when another one is open.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_run_configuration(
    name: Option<String>,
    selected_serial: Option<String>,
    project_root: Option<String>,
    app: AppHandle,
    fs_state: State<'_, FsState>,
    device_state: State<'_, DeviceState>,
    build_state: State<'_, BuildState>,
    logcat_state: State<'_, LogcatState>,
    process_manager: State<'_, ProcessManager>,
) -> Result<DeployResult, AppError> {
    let project = OpenProject::of(&fs_state).await?;
    if project_root.is_some_and(|root| root != project.registry_root) {
        return Err(AppError::InvalidInput(
            "The project changed before the run started. Nothing was built.".into(),
        ));
    }
    let request = RunRequest {
        name,
        ..RunRequest::default()
    };
    let run = resolve(project.clone(), request, selected_serial, &device_state).await?;
    let (settings, _) = settings_manager::load_settings();
    let (min, max) = RUN_BUILD_TIMEOUT_SECS;
    let timeout = Duration::from_secs(u64::from(settings.mcp.build_timeout_sec).clamp(min, max));
    let env = DeployEnv {
        fs_state: fs_state.inner().clone(),
        build_state: build_state.inner().clone(),
        device_state: device_state.inner().clone(),
        logcat_state: logcat_state.inner().clone(),
        app: Some(app.clone()),
        adb: get_adb_path(&settings),
        aapt2: find_aapt2(&settings),
    };
    let mut hooks = AppRun {
        app,
        project: project.clone(),
        build_state: build_state.inner().clone(),
        process_manager: process_manager.inner().clone(),
        timeout,
    };
    deploy::run_configuration(&env, &project, run, BuildActor::App, &mut hooks).await
}

/// The app's run: its build is the app's, cancelled with Cancel, and its
/// phases go to the app.
struct AppRun {
    app: AppHandle,
    project: OpenProject,
    build_state: BuildState,
    process_manager: ProcessManager,
    timeout: Duration,
}

impl DeployHooks for AppRun {
    async fn build(&mut self, task: &str) -> Result<BuildOutcome, AppError> {
        let request = deploy::build_request(&self.project, task.to_string(), BuildActor::App)?;
        deploy::build_and_wait(
            &self.build_state,
            &self.process_manager,
            Some(&self.app),
            request,
            self.timeout,
        )
        .await
    }

    async fn phase(&mut self, event: DeployPhaseEvent) {
        let _ = self.app.emit(deploy::DEPLOY_PHASE_EVENT, event);
    }
}

/// The open project's application modules (Gradle paths), which a run
/// configuration can build.
#[tauri::command]
pub async fn list_application_modules(
    fs_state: State<'_, FsState>,
) -> Result<Vec<String>, AppError> {
    let gradle_root = {
        let fs = fs_state.0.lock().await;
        fs.gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or_else(|| AppError::NotFound("No project is open".into()))?
    };
    blocking(move || {
        Ok(gradle_modules::application_modules(&gradle_root)
            .into_iter()
            .map(|m| m.path)
            .collect())
    })
    .await
}

/// Set which device the configuration named `name` runs on.
#[tauri::command]
pub async fn set_run_configuration_target(
    name: String,
    target: TargetPreference,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::set_target(&project_root, &name, target)).await
}

/// Approve running the shared configuration named `name` as the project's
/// shared file is now; `sha256` is the file's hash the user reviewed.
#[tauri::command]
pub async fn approve_shared_run_configuration(
    name: String,
    sha256: String,
    fs_state: State<'_, FsState>,
) -> Result<ProjectRunConfigurations, AppError> {
    let project_root = open_project_root(&fs_state).await?;
    blocking(move || run_configurations::approve_shared(&project_root, &name, &sha256)).await
}
