//! Run App: build a resolved run configuration's task, install the APK that
//! build wrote, record the device, and launch the app the way the
//! configuration says. The app's Run App and agents run configurations
//! through [`run_configuration`].

use crate::models::build::{BuildActor, BuildRecord, LaunchResult, LaunchTiming, RunApk};
use crate::models::device::{Device, DeviceConnectionState};
use crate::models::error::AppError;
use crate::models::run_configuration::{
    DeployOutcome, DeployPhase, DeployPhaseEvent, DeployResult, ResolvedRun, RunDevice, RunLaunch,
};
use crate::services::adb_manager::{self, AmStartTiming, DeviceState};
use crate::services::build_runner::{self, BuildOutcome, BuildRequest, BuildState};
use crate::services::debug_sessions::{self, LaunchRecord};
use crate::services::launch_display::{self, LaunchWatch};
use crate::services::logcat::LogcatState;
use crate::services::process_manager::ProcessManager;
use crate::services::run_plan::RunProject;
use crate::services::{installed_builds, run_configurations, settings_manager, ui_automation};
use crate::utils::validation;
use crate::FsState;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Sent to the app as its own run of a configuration moves on.
pub const DEPLOY_PHASE_EVENT: &str = "deploy:phase";

/// How long a timed-out build gets to record the timeout before the run answers.
const TIMEOUT_RECORD_GRACE: Duration = Duration::from_secs(10);

/// The project open when a run was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenProject {
    /// The project's registry key: the project root, else the Gradle root.
    pub registry_root: String,
    /// Where Gradle runs.
    pub gradle_root: PathBuf,
    /// The root whose trust decides whether Gradle may run.
    pub trust_root: PathBuf,
    /// The build history's scope (the project root).
    pub history_root: Option<String>,
}

impl OpenProject {
    /// The project open in `fs_state` now.
    ///
    /// # Errors
    /// `notFound` when no project is open.
    pub async fn of(fs_state: &FsState) -> Result<Self, AppError> {
        let fs = fs_state.0.lock().await;
        let gradle_root = fs
            .gradle_root
            .clone()
            .or_else(|| fs.project_root.clone())
            .ok_or_else(|| AppError::NotFound("No project is open".into()))?;
        let trust_root = fs
            .project_root
            .clone()
            .unwrap_or_else(|| gradle_root.clone());
        Ok(Self {
            registry_root: trust_root.to_string_lossy().into_owned(),
            gradle_root,
            trust_root,
            history_root: fs
                .project_root
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        })
    }

    /// The project as the run resolver takes it.
    pub fn run_project(&self) -> RunProject<'_> {
        RunProject {
            registry_root: &self.registry_root,
            gradle_root: &self.gradle_root,
            trust_root: &self.trust_root,
        }
    }
}

/// A Gradle build of `task` in `project`, started by `origin`: its
/// `gradlew` and the environment only a trusted project gets.
///
/// # Errors
/// `invalidInput` for an invalid task, `notFound` without a `gradlew`, and
/// `permissionDenied` in Safe Mode.
pub fn build_request(
    project: &OpenProject,
    task: String,
    origin: BuildActor,
) -> Result<BuildRequest, AppError> {
    validation::validate_gradle_task(&task).map_err(AppError::InvalidInput)?;
    let (settings, _) = settings_manager::load_settings();
    let gradlew = build_runner::find_gradlew(&project.gradle_root)
        .ok_or_else(|| AppError::NotFound("gradlew not found at project root".into()))?;
    let env =
        build_runner::trusted_gradle_env(&settings, &project.trust_root, &project.gradle_root)
            .map_err(AppError::PermissionDenied)?;
    Ok(BuildRequest {
        task,
        extra_args: vec![],
        gradle_root: project.gradle_root.clone(),
        gradlew,
        env,
        project_root: project.history_root.clone(),
        origin,
    })
}

/// Start `request` through the build service (the build lock and slot,
/// events, history) and wait for it at most `timeout`; a build still running
/// then is stopped and recorded as timed out.
///
/// # Errors
/// `invalidInput` when another build runs, `processFailed` when Gradle does
/// not start or the build timed out.
pub async fn build_and_wait(
    build_state: &BuildState,
    process_manager: &ProcessManager,
    app: Option<&tauri::AppHandle>,
    request: BuildRequest,
    timeout: Duration,
) -> Result<BuildOutcome, AppError> {
    let mut handle = build_runner::start_build(build_state, process_manager, app, request)
        .await
        .map_err(start_error)?;
    if let Ok(outcome) = tokio::time::timeout(timeout, handle.wait()).await {
        return Ok(outcome);
    }
    let secs = timeout.as_secs();
    build_runner::time_out_build(build_state, process_manager, Some(handle.run_id), secs).await;
    let _ = tokio::time::timeout(TIMEOUT_RECORD_GRACE, handle.wait()).await;
    Err(AppError::ProcessFailed(format!(
        "The build timed out after {secs} s and was stopped. Nothing was installed."
    )))
}

/// Why the app's build did not start: `invalidInput` while another build
/// runs, `processFailed` when Gradle could not start.
pub fn start_error(e: build_runner::StartBuildError) -> AppError {
    match e {
        build_runner::StartBuildError::Busy(msg)
        | build_runner::StartBuildError::BusyElsewhere(msg) => AppError::InvalidInput(msg),
        build_runner::StartBuildError::Spawn(msg) => AppError::ProcessFailed(msg),
    }
}

/// The state a run uses: the app's, or a standalone MCP server's.
#[derive(Clone)]
pub struct DeployEnv {
    pub fs_state: FsState,
    pub build_state: BuildState,
    pub device_state: DeviceState,
    /// This process's logcat stream, for the launch's display times.
    pub logcat_state: LogcatState,
    /// Sends display times that arrive after the launch (`build:launch_timing`).
    pub app: Option<tauri::AppHandle>,
    pub adb: PathBuf,
    pub aapt2: Option<PathBuf>,
}

/// What differs between the front doors that run a configuration.
pub trait DeployHooks: Send {
    /// Build `task` through the build service and wait for it: the caller's
    /// origin, timeout, progress, and cancellation. `Err` when the build did
    /// not run to an end (refused, not started, timed out).
    fn build(&mut self, task: &str) -> impl Future<Output = Result<BuildOutcome, AppError>> + Send;

    /// The run entered `event.phase`.
    fn phase(&mut self, event: DeployPhaseEvent) -> impl Future<Output = ()> + Send;
}

/// Run `run` (resolved with a device in `project`) for `by`: build its task
/// with `hooks`, install the APK that build recorded for the module and
/// variant (else, when Gradle found it up to date, the current one), record
/// the device, and launch per `run.launch`. Install and launch cannot be
/// cancelled. Nothing is installed when the open project is no longer
/// `project`.
///
/// A build that fails or is cancelled is an `Ok` result saying so.
///
/// # Errors
/// The build's own error from `hooks`, `invalidInput` when the project
/// changed, `notFound` when no APK of the module and variant exists, and
/// `processFailed` when the install or the launch fails.
pub async fn run_configuration<H: DeployHooks>(
    env: &DeployEnv,
    project: &OpenProject,
    run: ResolvedRun,
    by: BuildActor,
    hooks: &mut H,
) -> Result<DeployResult, AppError> {
    let device = run.device.clone().ok_or_else(|| {
        AppError::InvalidInput(format!(
            "Run configuration '{}' has no device to run on.",
            run.name
        ))
    })?;
    let mut report = Reporter::new(hooks, &run, &device);
    let mut result = DeployResult {
        run: run.clone(),
        outcome: DeployOutcome::Done,
        build_id: None,
        device: device.clone(),
        apk: None,
        apk_sha256: None,
        package: None,
        launch: None,
        logcat_filter: run.logcat_filter.clone(),
    };

    report.enter(DeployPhase::Building, None).await;
    let built = match ensure_still_open(env, project).await {
        Ok(()) => report.hooks.build(&run.task).await,
        Err(e) => Err(e),
    };
    let built = match built {
        Ok(built) => built,
        Err(e) => {
            report.enter(DeployPhase::Failed, Some(reason(&e))).await;
            return Err(e);
        }
    };
    result.build_id = built.record_id;
    report.build_id = built.record_id;
    if built.cancelled {
        result.outcome = DeployOutcome::Cancelled;
        report.enter(DeployPhase::Cancelled, None).await;
        return Ok(result);
    }
    if !built.success {
        result.outcome = DeployOutcome::BuildFailed;
        let why = "The build failed; nothing was installed.".to_string();
        report.enter(DeployPhase::Failed, Some(why)).await;
        return Ok(result);
    }

    match install_and_launch(env, project, &run, &device, by, &mut report, &mut result).await {
        Ok(()) => {
            report.enter(DeployPhase::Done, None).await;
            Ok(result)
        }
        Err(e) => {
            report.enter(DeployPhase::Failed, Some(reason(&e))).await;
            Err(e)
        }
    }
}

async fn install_and_launch<H: DeployHooks>(
    env: &DeployEnv,
    project: &OpenProject,
    run: &ResolvedRun,
    device: &RunDevice,
    by: BuildActor,
    report: &mut Reporter<'_, H>,
    result: &mut DeployResult,
) -> Result<(), AppError> {
    ensure_still_open(env, project).await?;
    let apk = run_apk(env, project, report.build_id, run).await?;
    let apk_path =
        crate::utils::path::validate_apk_within_build_outputs(&project.gradle_root, &apk.path)?;
    report.step(describe_run_apk(&apk));
    report.step(format!(
        "Installing on: {}",
        device_label(&env.device_state, &device.serial).await
    ));
    report.step(format!("adb install {}", apk.path));
    result.apk = Some(apk);
    report.enter(DeployPhase::Installing, None).await;

    ensure_still_open(env, project).await?;
    let started = tokio::time::Instant::now();
    let installed = installed_builds::install_and_record(
        &env.adb,
        env.aapt2.as_deref(),
        &device.serial,
        &apk_path,
        &env.device_state,
        by.clone(),
    )
    .await
    .map_err(AppError::ProcessFailed)?;
    result.apk_sha256 = installed.sha256;
    report.step(format!(
        "Install: {} ({})",
        installed.output.trim(),
        format_duration(started.elapsed())
    ));
    record_device(project, &run.name, &device.serial).await;

    let activity = match &run.launch {
        RunLaunch::None => {
            report.step(format!(
                "Run configuration '{}' installs only — not launching.",
                run.name
            ));
            return Ok(());
        }
        RunLaunch::Activity { name } => Some(name.as_str()),
        RunLaunch::Default | RunLaunch::DeepLink { .. } => None,
    };
    // The installed APK's own package: the project's applicationId ignores
    // applicationIdSuffix and could start another app.
    let package = match apk_package(env.aapt2.as_deref(), &apk_path).await {
        Ok(package) => package,
        Err(e) => {
            report.step(format!("Could not read the APK's package name: {e}"));
            report.step(
                "APK installed. Could not determine the package name, so the app was not \
                 launched. Ensure aapt2 is available in your Android SDK (Settings → Android SDK).",
            );
            return Ok(());
        }
    };
    report.step(format!("Package (from APK): {package}"));
    result.package = Some(package.clone());
    let launched = match &run.launch {
        RunLaunch::DeepLink { uri } => {
            report.step(format!(
                "adb shell am start -a android.intent.action.VIEW -d {uri} -p {package}"
            ));
            report.enter(DeployPhase::Launching, None).await;
            open_deep_link(env, &device.serial, &package, uri, by).await?
        }
        _ => {
            report.step(match activity {
                Some(activity) => format!("adb shell am start -W ({package}/{activity})"),
                None => format!("adb shell am start -W (package: {package})"),
            });
            report.enter(DeployPhase::Launching, None).await;
            launch_app(env, &device.serial, &package, activity, report.build_id, by).await?
        }
    };
    result.launch = Some(launched);
    Ok(())
}

/// Collects a run's log lines and sends them with the next phase.
struct Reporter<'a, H> {
    hooks: &'a mut H,
    name: String,
    plan: String,
    device: RunDevice,
    build_id: Option<u32>,
    steps: Vec<String>,
}

impl<'a, H: DeployHooks> Reporter<'a, H> {
    fn new(hooks: &'a mut H, run: &ResolvedRun, device: &RunDevice) -> Self {
        Self {
            hooks,
            name: run.name.clone(),
            plan: run.plan.clone(),
            device: device.clone(),
            build_id: None,
            steps: Vec::new(),
        }
    }

    fn step(&mut self, line: impl Into<String>) {
        self.steps.push(line.into());
    }

    async fn enter(&mut self, phase: DeployPhase, error: Option<String>) {
        let event = DeployPhaseEvent {
            phase,
            name: self.name.clone(),
            plan: self.plan.clone(),
            device: self.device.clone(),
            build_id: self.build_id,
            steps: std::mem::take(&mut self.steps),
            error,
        };
        self.hooks.phase(event).await;
    }
}

/// Refuse to go on once another project is open: its APK must never be
/// installed for this run.
async fn ensure_still_open(env: &DeployEnv, project: &OpenProject) -> Result<(), AppError> {
    match OpenProject::of(&env.fs_state).await {
        Ok(open) if open == *project => Ok(()),
        _ => Err(AppError::InvalidInput(
            "The project changed during the run. Nothing was installed.".into(),
        )),
    }
}

/// The APK to install after build `build_id` (`installed_builds::run_apk`).
async fn run_apk(
    env: &DeployEnv,
    project: &OpenProject,
    build_id: Option<u32>,
    run: &ResolvedRun,
) -> Result<RunApk, AppError> {
    let history: Vec<BuildRecord> = {
        let bs = env.build_state.inner.lock().await;
        bs.history
            .iter()
            .filter(|r| r.project_root == project.history_root)
            .cloned()
            .collect()
    };
    let gradle_root = project.gradle_root.clone();
    let (module, variant) = (run.module.clone(), run.variant.clone());
    // Hashing an up-to-date APK reads it from disk.
    tokio::task::spawn_blocking(move || {
        installed_builds::run_apk(&gradle_root, &history, build_id, Some(&module), &variant)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
    .map_err(AppError::NotFound)
}

/// A Last used target prefers this device next time. Failing to save it does
/// not fail the run.
async fn record_device(project: &OpenProject, name: &str, serial: &str) {
    let (root, name, serial) = (
        project.registry_root.clone(),
        name.to_string(),
        serial.to_string(),
    );
    let saved = tokio::task::spawn_blocking(move || {
        run_configurations::record_last_device(&root, &name, &serial)
    })
    .await;
    match saved {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!("The run's device was not recorded: {e}"),
        Err(e) => tracing::warn!("The run's device was not recorded: {e}"),
    }
}

/// The package of `apk` (canonical): aapt2's answer, else the application ID
/// AGP recorded next to it, suffix included.
async fn apk_package(aapt2: Option<&Path>, apk: &Path) -> Result<String, String> {
    let from_aapt2 = match aapt2 {
        Some(aapt2) => adb_manager::get_package_name_from_apk(aapt2, apk).await,
        None => None,
    };
    from_aapt2
        .or_else(|| build_runner::application_id_from_output_metadata(apk))
        .filter(|package| validation::validate_package_name(package).is_ok())
        .ok_or_else(|| {
            "aapt2 failed or was not found in $ANDROID_HOME/build-tools, and the APK has no \
             output-metadata.json"
                .to_string()
        })
}

/// Launch `package` on `serial` with `am start -W` (its launcher activity,
/// or `activity`), as `by`. With `build_id`, the launch time is recorded on
/// that build's history entry: the build of this run.
///
/// While this process's logcat stream reads the device, the times to initial
/// and full display are read from it too: what arrived shortly after the
/// launch is returned, and what arrives later is added to the record and sent
/// as `build:launch_timing`. The launch is added to the device's debug session.
///
/// # Errors
/// `invalidInput` for an invalid serial, package, or activity, and
/// `processFailed` when the launch fails.
pub async fn launch_app(
    env: &DeployEnv,
    serial: &str,
    package: &str,
    activity: Option<&str>,
    build_id: Option<u32>,
    by: BuildActor,
) -> Result<LaunchResult, AppError> {
    validation::validate_device_serial(serial).map_err(AppError::InvalidInput)?;
    validation::validate_package_name(package).map_err(AppError::InvalidInput)?;
    if let Some(activity) = activity {
        validation::validate_activity_name(activity).map_err(AppError::InvalidInput)?;
    }
    let only_online_device = {
        let devices = &env.device_state.0.lock().await.devices;
        let mut online = devices
            .iter()
            .filter(|d| matches!(d.connection_state, DeviceConnectionState::Online));
        online.next().is_some_and(|d| d.serial == serial) && online.next().is_none()
    };
    let watch = LaunchWatch::start(
        &*env.logcat_state.lock().await,
        serial,
        package,
        only_online_device,
    );
    let started = tokio::time::Instant::now();
    let outcome = adb_manager::launch_app(&env.adb, serial, package, activity)
        .await
        .map_err(AppError::ProcessFailed)?;

    let mut timing = match outcome.timing {
        Some(measured) => {
            let device = env
                .device_state
                .0
                .lock()
                .await
                .devices
                .iter()
                .find(|d| d.serial == serial)
                .cloned();
            Some(launch_timing(measured, serial, device.as_ref()))
        }
        None => None,
    };
    if let (Some(timing), Some(watch)) = (&mut timing, &watch) {
        launch_display::add_display_times(timing, watch, &env.logcat_state).await;
    }
    if let (Some(id), Some(timing)) = (build_id, &timing) {
        // The app launched; failing to record its time must not fail the launch.
        if let Err(e) =
            build_runner::attach_launch_timing(&env.build_state, id, timing.clone()).await
        {
            tracing::warn!("Launch time not recorded on build #{id}: {e}");
        } else if let Some(watch) = watch.filter(|_| timing.fully_drawn_ms.is_none()) {
            let late = launch_display::record_late_display_times(
                env.build_state.clone(),
                env.logcat_state.clone(),
                env.app.clone(),
                id,
                timing.clone(),
                watch,
                started,
            );
            let (package, by) = (package.to_string(), by.clone());
            tokio::spawn(async move {
                if let Some(timing) = late.await {
                    debug_sessions::record_late_launch_timing(&package, timing, by);
                }
            });
        }
    }
    let launch = LaunchRecord {
        serial: serial.to_string(),
        package: package.to_string(),
        timing: timing.clone(),
        restart: false,
        by,
    };
    debug_sessions::record_launch(env.adb.clone(), env.device_state.clone(), launch);
    Ok(LaunchResult {
        output: outcome.description,
        timing,
    })
}

/// Open `uri` in `package` on `serial` (`am start -a VIEW -d <uri> -p
/// <package>`), as `by`. Android reports no launch time for it; the launch
/// is added to the device's debug session without one.
async fn open_deep_link(
    env: &DeployEnv,
    serial: &str,
    package: &str,
    uri: &str,
    by: BuildActor,
) -> Result<LaunchResult, AppError> {
    let output = ui_automation::adb_open_deep_link(&env.adb, serial, uri, Some(package))
        .await
        .map_err(AppError::ProcessFailed)?;
    let launch = LaunchRecord {
        serial: serial.to_string(),
        package: package.to_string(),
        timing: None,
        restart: false,
        by,
    };
    debug_sessions::record_launch(env.adb.clone(), env.device_state.clone(), launch);
    Ok(LaunchResult {
        output,
        timing: None,
    })
}

fn launch_timing(measured: AmStartTiming, serial: &str, device: Option<&Device>) -> LaunchTiming {
    LaunchTiming {
        total_ms: measured.total_ms,
        wait_ms: measured.wait_ms,
        launch_state: measured.launch_state,
        measured_at: chrono::Utc::now().to_rfc3339(),
        serial: serial.to_string(),
        avd_name: device.and_then(|d| d.avd_name.clone()),
        model: device.and_then(|d| d.model.clone()),
        displayed_ms: None,
        fully_drawn_ms: None,
    }
}

/// `Pixel 7 (API 35) [28151FDH2000Q4]`, or the serial of a device not listed.
async fn device_label(device_state: &DeviceState, serial: &str) -> String {
    let state = device_state.0.lock().await;
    match state.devices.iter().find(|d| d.serial == serial) {
        Some(device) => {
            let model = device.model.as_deref().unwrap_or(&device.name);
            let api = device
                .api_level
                .map(|level| format!(" (API {level})"))
                .unwrap_or_default();
            format!("{model}{api} [{serial}]")
        }
        None => serial.to_string(),
    }
}

/// Which build wrote the APK a run installs.
fn describe_run_apk(apk: &RunApk) -> String {
    match (apk.from_this_build, apk.build_id) {
        (true, Some(id)) => format!("APK (build #{id}): {}", apk.path),
        (true, None) => format!("APK (this build): {}", apk.path),
        (false, Some(id)) => format!("APK unchanged since build #{id}: {}", apk.path),
        (false, None) => format!(
            "APK unchanged by this build, and no build in the history wrote it: {}",
            apk.path
        ),
    }
}

fn format_duration(elapsed: Duration) -> String {
    let ms = elapsed.as_millis();
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1_000)
    }
}

/// The message of `e`, without its kind.
fn reason(e: &AppError) -> String {
    match e {
        AppError::NotFound(m)
        | AppError::PermissionDenied(m)
        | AppError::InvalidInput(m)
        | AppError::Io(m)
        | AppError::ProcessFailed(m)
        | AppError::SettingsError(m)
        | AppError::McpError(m)
        | AppError::Other(m)
        | AppError::ApprovalRequired(m) => m.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apk(from_this_build: bool, build_id: Option<u32>) -> RunApk {
        RunApk {
            path: "/p/app/build/outputs/apk/debug/app-debug.apk".into(),
            build_id,
            from_this_build,
        }
    }

    #[test]
    fn the_log_says_which_build_wrote_the_apk() {
        assert_eq!(
            describe_run_apk(&apk(true, Some(7))),
            "APK (build #7): /p/app/build/outputs/apk/debug/app-debug.apk"
        );
        assert_eq!(
            describe_run_apk(&apk(false, Some(3))),
            "APK unchanged since build #3: /p/app/build/outputs/apk/debug/app-debug.apk"
        );
        assert!(describe_run_apk(&apk(false, None))
            .starts_with("APK unchanged by this build, and no build in the history wrote it"));
    }

    #[test]
    fn durations_read_like_the_build_log() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0ms");
        assert_eq!(format_duration(Duration::from_millis(850)), "850ms");
        assert_eq!(format_duration(Duration::from_millis(1_234)), "1.2s");
        assert_eq!(format_duration(Duration::from_millis(125_000)), "2m 5s");
    }

    #[tokio::test]
    async fn the_open_project_is_the_project_root_else_the_gradle_root() {
        let fs = FsState::new();
        assert!(matches!(
            OpenProject::of(&fs).await,
            Err(AppError::NotFound(_))
        ));
        fs.0.lock().await.gradle_root = Some(PathBuf::from("/work/android"));
        let gradle_only = OpenProject::of(&fs).await.unwrap();
        assert_eq!(gradle_only.registry_root, "/work/android");
        assert_eq!(gradle_only.history_root, None);
        fs.0.lock().await.project_root = Some(PathBuf::from("/work"));
        let both = OpenProject::of(&fs).await.unwrap();
        assert_eq!(both.registry_root, "/work");
        assert_eq!(both.gradle_root, PathBuf::from("/work/android"));
        assert_eq!(both.trust_root, PathBuf::from("/work"));
        assert_eq!(both.history_root.as_deref(), Some("/work"));
        assert_ne!(both, gradle_only);
    }
}
