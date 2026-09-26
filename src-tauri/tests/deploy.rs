//! Run App in the backend (`services/deploy.rs`): a resolved run
//! configuration is built through the build service, its APK installed, and
//! the app launched, against fake `gradlew` and `adb` scripts.

use keynobi_lib::models::build::{BuildActor, BuildStatus};
use keynobi_lib::models::debug_session::DebugSessionEventData;
use keynobi_lib::models::device::{Device, DeviceConnectionState, DeviceKind};
use keynobi_lib::models::error::AppError;
use keynobi_lib::models::run_configuration::{
    DeployOutcome, DeployPhase, DeployPhaseEvent, LocalRunState, ResolvedRun, RunConfiguration,
    RunLaunch, TargetPreference,
};
use keynobi_lib::models::settings::ProjectEntry;
use keynobi_lib::services::adb_manager::DeviceState;
use keynobi_lib::services::build_runner::{self, BuildOutcome, BuildState};
use keynobi_lib::services::deploy::{self, DeployEnv, DeployHooks, OpenProject};
use keynobi_lib::services::process_manager::ProcessManager;
use keynobi_lib::services::run_plan::{self, Devices, RunRequest};
use keynobi_lib::services::{debug_sessions, settings_manager};
use keynobi_lib::FsState;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod common;

const SERIAL: &str = "emulator-5554";
const PACKAGE: &str = "com.example.sandbox.debug";
const WAIT: Duration = Duration::from_secs(30);

/// A trusted project with the application module `:app`, registered with
/// `configurations` (the first active, each on the emulator), a fake SDK
/// `adb`, and the state a run uses.
struct Fixture {
    _dir: tempfile::TempDir,
    project: PathBuf,
    calls: PathBuf,
    env: DeployEnv,
    open: OpenProject,
    process_manager: ProcessManager,
}

impl Fixture {
    fn new(configurations: Vec<RunConfiguration>) -> Self {
        common::isolate_data_dir();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let project = root.join("project");
        std::fs::create_dir_all(project.join("app")).unwrap();
        std::fs::write(
            project.join("settings.gradle.kts"),
            "rootProject.name = \"sandbox\"\ninclude(\":app\")\n",
        )
        .unwrap();
        std::fs::write(
            project.join("app/build.gradle.kts"),
            "plugins { id(\"com.android.application\") }\nandroid {\n    defaultConfig { \
             applicationId = \"com.example.sandbox\" }\n    buildTypes {\n        release {\n        \
             }\n    }\n}\n",
        )
        .unwrap();
        let calls = root.join("adb-calls.txt");
        let adb = root.join("adb");
        write_script(
            &adb,
            &format!(
                r#"echo "$*" >> '{calls}'
case "$*" in
  *"emu avd name"*) printf 'Pixel_7\nOK\n' ;;
  *" install "*) echo Success ;;
  *"resolve-activity"*) printf 'priority=0\ncom.example.sandbox.debug/com.example.sandbox.MainActivity\n' ;;
  *"am start -W -n"*)
    echo 'Status: ok'
    echo 'LaunchState: COLD'
    echo 'TotalTime: 812'
    echo 'WaitTime: 815'
    echo 'Complete'
    ;;
  *"android.intent.action.VIEW"*) echo 'Starting: Intent {{ act=android.intent.action.VIEW }}' ;;
esac"#,
                calls = calls.display()
            ),
        );

        let registry_root = project.to_string_lossy().into_owned();
        let local = configurations
            .iter()
            .map(|c| {
                let state = LocalRunState {
                    target: TargetPreference::Serial {
                        serial: SERIAL.into(),
                    },
                    ..Default::default()
                };
                (c.name.clone(), state)
            })
            .collect();
        settings_manager::mutate_settings(|settings| {
            settings.recent_projects.push(ProjectEntry {
                id: registry_root.clone(),
                path: registry_root.clone(),
                name: "sandbox".into(),
                gradle_root: Some(registry_root.clone()),
                trusted: Some(true),
                active_run_configuration: configurations.first().map(|c| c.name.clone()),
                run_configurations: Some(configurations.clone()),
                run_local: local,
                ..Default::default()
            });
        })
        .unwrap();

        let fs_state = FsState::new();
        {
            let mut fs = fs_state.0.try_lock().unwrap();
            fs.project_root = Some(project.clone());
            fs.gradle_root = Some(project.clone());
        }
        let device_state = DeviceState::new();
        device_state.0.try_lock().unwrap().devices = vec![emulator()];
        let open = OpenProject {
            registry_root,
            gradle_root: project.clone(),
            trust_root: project.clone(),
            history_root: Some(project.to_string_lossy().into_owned()),
        };
        let env = DeployEnv {
            fs_state,
            build_state: common::isolated_build_state(),
            device_state,
            logcat_state: keynobi_lib::commands::logcat::new_logcat_state(),
            app: None,
            adb,
            aapt2: None,
        };
        Self {
            _dir: dir,
            project,
            calls,
            env,
            open,
            process_manager: ProcessManager::new(),
        }
    }

    /// A `gradlew` that writes `:app`'s debug APK (`contents`) and succeeds.
    fn gradlew_writing_the_debug_apk(&self, contents: &str) {
        let debug = self.project.join("app/build/outputs/apk/debug");
        self.write_gradlew(&format!(
            "mkdir -p '{debug}'\n\
             printf '{contents}' > '{debug}/app-debug.apk'\n\
             printf '%s' '{{\"applicationId\":\"{PACKAGE}\",\"variantName\":\"debug\",\
             \"elements\":[{{\"versionCode\":1,\"outputFile\":\"app-debug.apk\"}}]}}' \
             > '{debug}/output-metadata.json'\n\
             echo 'BUILD SUCCESSFUL in 1s'",
            debug = debug.display(),
        ));
    }

    fn write_gradlew(&self, body: &str) {
        write_script(&self.project.join("gradlew"), body);
    }

    async fn resolve(&self, name: &str) -> ResolvedRun {
        let devices = self.env.device_state.0.lock().await.devices.clone();
        run_plan::resolve(
            self.open.run_project(),
            &RunRequest {
                name: Some(name.into()),
                ..RunRequest::default()
            },
            Devices {
                list: &devices,
                selected: None,
            },
        )
        .expect("the configuration resolves")
    }

    fn adb_calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn installs(&self) -> Vec<String> {
        self.adb_calls()
            .into_iter()
            .filter(|c| c.contains(" install "))
            .collect()
    }

    fn debug_apk(&self) -> String {
        self.project
            .join("app/build/outputs/apk/debug/app-debug.apk")
            .to_string_lossy()
            .into_owned()
    }
}

fn emulator() -> Device {
    Device {
        serial: SERIAL.into(),
        name: "sdk_gphone64_arm64".into(),
        model: Some("sdk_gphone64_arm64".into()),
        device_kind: DeviceKind::Emulator,
        connection_state: DeviceConnectionState::Online,
        api_level: Some(35),
        android_version: Some("15".into()),
        avd_name: Some("Pixel_7".into()),
    }
}

fn configuration(name: &str, launch: RunLaunch) -> RunConfiguration {
    RunConfiguration {
        name: name.into(),
        module: ":app".into(),
        variant: "debug".into(),
        task: None,
        launch,
        logcat_filter: None,
    }
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Something to do while a run's build runs.
type DuringBuild<'a> = Box<dyn FnOnce(&Fixture) + Send + 'a>;

/// The app's hooks, recording the phases; `during_build` runs while the
/// build runs.
struct Hooks<'a> {
    fixture: &'a Fixture,
    phases: Vec<DeployPhaseEvent>,
    during_build: Option<DuringBuild<'a>>,
}

impl<'a> Hooks<'a> {
    fn new(fixture: &'a Fixture) -> Self {
        Self {
            fixture,
            phases: vec![],
            during_build: None,
        }
    }

    fn phase_names(&self) -> Vec<DeployPhase> {
        self.phases.iter().map(|e| e.phase).collect()
    }

    fn steps(&self) -> Vec<String> {
        self.phases.iter().flat_map(|e| e.steps.clone()).collect()
    }
}

impl DeployHooks for Hooks<'_> {
    async fn build(&mut self, task: &str) -> Result<BuildOutcome, AppError> {
        let f = self.fixture;
        let request = deploy::build_request(&f.open, task.to_string(), BuildActor::App)?;
        let build =
            deploy::build_and_wait(&f.env.build_state, &f.process_manager, None, request, WAIT);
        match self.during_build.take() {
            Some(during) => {
                let (outcome, ()) = tokio::join!(build, async {
                    wait_until_building(&f.env.build_state).await;
                    during(f);
                });
                outcome
            }
            None => build.await,
        }
    }

    async fn phase(&mut self, event: DeployPhaseEvent) {
        self.phases.push(event);
    }
}

async fn wait_until_building(build_state: &BuildState) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while build_state.inner.lock().await.current_build.is_none() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the build never started"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn run(
    fixture: &Fixture,
    hooks: &mut Hooks<'_>,
    name: &str,
) -> Result<keynobi_lib::models::run_configuration::DeployResult, AppError> {
    let resolved = fixture.resolve(name).await;
    deploy::run_configuration(
        &fixture.env,
        &fixture.open,
        resolved,
        BuildActor::App,
        hooks,
    )
    .await
}

/// The debug session of the APK `sha256` once it has `launches` launches.
async fn session_with_launches(
    sha256: &str,
    launches: u32,
) -> keynobi_lib::models::debug_session::DebugSessionDetail {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let found = debug_sessions::list_sessions()
            .into_iter()
            .find(|s| s.apk_sha256.as_deref() == Some(sha256) && s.counts.launches >= launches);
        if let Some(summary) = found {
            return debug_sessions::get_session(&summary.id).unwrap();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no session of {sha256} with {launches} launch(es)"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn history_record(
    build_state: &BuildState,
    id: u32,
) -> keynobi_lib::models::build::BuildRecord {
    build_state
        .inner
        .lock()
        .await
        .history
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .expect("the run's build is in the history")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_builds_installs_the_apk_its_build_wrote_and_launches_the_app() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Default", RunLaunch::Default)]);
    f.gradlew_writing_the_debug_apk("first apk");
    let mut hooks = Hooks::new(&f);

    let result = run(&f, &mut hooks, "Default").await.unwrap();

    assert_eq!(result.outcome, DeployOutcome::Done);
    let build_id = result.build_id.expect("the build is recorded");
    let record = history_record(&f.env.build_state, build_id).await;
    assert_eq!(record.task, ":app:assembleDebug");
    assert!(matches!(record.status, BuildStatus::Success(_)));
    assert_eq!(record.origin, Some(BuildActor::App));
    let apk = result.apk.clone().unwrap();
    assert!(apk.from_this_build, "{apk:?}");
    assert_eq!(apk.build_id, Some(build_id));
    assert_eq!(apk.path, f.debug_apk());
    assert_eq!(
        result.apk_sha256.as_deref(),
        Some(record.apks[0].sha256.as_str())
    );
    assert_eq!(result.package.as_deref(), Some(PACKAGE));
    assert_eq!(result.device.serial, SERIAL);
    assert_eq!(result.logcat_filter, None);
    let launch = result.launch.clone().unwrap();
    assert_eq!(launch.timing.as_ref().map(|t| t.total_ms), Some(812));
    // The launch time is on this run's build.
    let record = history_record(&f.env.build_state, build_id).await;
    assert_eq!(record.launch.map(|t| t.total_ms), Some(812));

    assert_eq!(
        f.installs(),
        vec![format!("-s {SERIAL} install -r -t {}", f.debug_apk())]
    );
    assert!(
        f.adb_calls().iter().any(|c| c.contains("am start -W -n")),
        "{:?}",
        f.adb_calls()
    );
    assert_eq!(
        hooks.phase_names(),
        [
            DeployPhase::Building,
            DeployPhase::Installing,
            DeployPhase::Launching,
            DeployPhase::Done
        ]
    );
    assert_eq!(hooks.phases[1].build_id, Some(build_id));
    let steps = hooks.steps();
    assert!(
        steps.contains(&format!("APK (build #{build_id}): {}", f.debug_apk())),
        "{steps:?}"
    );
    assert!(
        steps.contains(&format!(
            "Installing on: sdk_gphone64_arm64 (API 35) [{SERIAL}]"
        )),
        "{steps:?}"
    );
    assert!(
        steps.contains(&format!("Package (from APK): {PACKAGE}")),
        "{steps:?}"
    );
    assert!(
        steps.contains(&format!("adb shell am start -W (package: {PACKAGE})")),
        "{steps:?}"
    );

    // A Last used target prefers this device next time.
    let local = keynobi_lib::services::run_configurations::list(&f.open.registry_root)
        .unwrap()
        .local;
    assert_eq!(local["Default"].last_device.as_deref(), Some(SERIAL));
    // The install opened a session, and the launch is on it.
    let session = session_with_launches(result.apk_sha256.as_deref().unwrap(), 1).await;
    assert_eq!(session.session.package, PACKAGE);
    assert!(session.events.iter().any(|e| matches!(
        &e.event,
        DebugSessionEventData::Launch(l) if l.timing.as_ref().map(|t| t.total_ms) == Some(812)
    )));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_launches_the_configurations_activity() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration(
        "Settings",
        RunLaunch::Activity {
            name: ".SettingsActivity".into(),
        },
    )]);
    f.gradlew_writing_the_debug_apk("settings apk");
    let mut hooks = Hooks::new(&f);

    let result = run(&f, &mut hooks, "Settings").await.unwrap();

    assert_eq!(result.outcome, DeployOutcome::Done);
    assert!(
        f.adb_calls()
            .iter()
            .any(|c| c.contains(&format!("am start -W -n {PACKAGE}/.SettingsActivity"))),
        "{:?}",
        f.adb_calls()
    );
    assert!(!f.adb_calls().iter().any(|c| c.contains("resolve-activity")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_link_run_opens_the_link_in_the_package_and_records_the_launch() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration(
        "Home",
        RunLaunch::DeepLink {
            uri: "myapp://home".into(),
        },
    )]);
    f.gradlew_writing_the_debug_apk("deep link apk");
    let mut hooks = Hooks::new(&f);

    let result = run(&f, &mut hooks, "Home").await.unwrap();

    assert_eq!(result.outcome, DeployOutcome::Done);
    let view = f
        .adb_calls()
        .into_iter()
        .find(|c| c.contains("android.intent.action.VIEW"))
        .expect("the deep link was opened");
    assert!(view.contains("myapp://home"), "{view}");
    assert!(view.contains(PACKAGE), "{view}");
    assert!(!f.adb_calls().iter().any(|c| c.contains("am start -W")));
    let launch = result.launch.unwrap();
    assert!(launch.timing.is_none());
    // The deep-link launch is on the session, without a launch time.
    let session = session_with_launches(result.apk_sha256.as_deref().unwrap(), 1).await;
    assert!(session.events.iter().any(|e| matches!(
        &e.event,
        DebugSessionEventData::Launch(l) if l.timing.is_none()
    )));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_with_no_launch_installs_only() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Install", RunLaunch::None)]);
    f.gradlew_writing_the_debug_apk("install only apk");
    let mut hooks = Hooks::new(&f);

    let result = run(&f, &mut hooks, "Install").await.unwrap();

    assert_eq!(result.outcome, DeployOutcome::Done);
    assert_eq!(f.installs().len(), 1);
    assert!(result.launch.is_none());
    assert!(!f.adb_calls().iter().any(|c| c.contains("am start")));
    assert_eq!(
        hooks.phase_names(),
        [
            DeployPhase::Building,
            DeployPhase::Installing,
            DeployPhase::Done
        ]
    );
    assert!(hooks
        .steps()
        .contains(&"Run configuration 'Install' installs only — not launching.".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_build_gradle_found_up_to_date_installs_the_apk_an_earlier_build_wrote() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Default", RunLaunch::None)]);
    f.gradlew_writing_the_debug_apk("up to date apk");
    let first = run(&f, &mut Hooks::new(&f), "Default").await.unwrap();
    let first_build = first.build_id.unwrap();
    // Gradle finds the APK up to date and writes nothing.
    f.write_gradlew("echo 'BUILD SUCCESSFUL in 1s'");
    let mut hooks = Hooks::new(&f);

    let second = run(&f, &mut hooks, "Default").await.unwrap();

    assert_eq!(second.outcome, DeployOutcome::Done);
    assert_ne!(second.build_id, Some(first_build));
    let apk = second.apk.unwrap();
    assert!(!apk.from_this_build, "{apk:?}");
    assert_eq!(apk.build_id, Some(first_build));
    assert_eq!(apk.path, f.debug_apk());
    assert_eq!(second.apk_sha256, first.apk_sha256);
    assert!(hooks.steps().contains(&format!(
        "APK unchanged since build #{first_build}: {}",
        f.debug_apk()
    )));
    assert_eq!(f.installs().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_build_installs_nothing() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Default", RunLaunch::Default)]);
    f.write_gradlew("echo 'e: file:///x/Main.kt:1:1: Unresolved reference: foo'\necho 'BUILD FAILED in 1s'\nexit 1");
    let mut hooks = Hooks::new(&f);

    let result = run(&f, &mut hooks, "Default").await.unwrap();

    assert_eq!(result.outcome, DeployOutcome::BuildFailed);
    assert!(result.build_id.is_some());
    assert!(result.apk.is_none());
    assert!(f.installs().is_empty(), "{:?}", f.adb_calls());
    assert_eq!(
        hooks.phase_names(),
        [DeployPhase::Building, DeployPhase::Failed]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_build_installs_nothing() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Default", RunLaunch::Default)]);
    f.write_gradlew("sleep 30\necho 'BUILD SUCCESSFUL in 30s'");
    let mut hooks = Hooks::new(&f);
    hooks.during_build = Some(Box::new(|f: &Fixture| {
        let (state, pm) = (f.env.build_state.clone(), f.process_manager.clone());
        tokio::spawn(async move {
            build_runner::cancel_build(&state, &pm, BuildActor::App).await;
        });
    }));

    let result = run(&f, &mut hooks, "Default").await.unwrap();

    assert_eq!(result.outcome, DeployOutcome::Cancelled);
    assert!(f.installs().is_empty(), "{:?}", f.adb_calls());
    assert_eq!(
        hooks.phase_names(),
        [DeployPhase::Building, DeployPhase::Cancelled]
    );
    let record = history_record(&f.env.build_state, result.build_id.unwrap()).await;
    assert!(matches!(record.status, BuildStatus::Cancelled));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_never_installs_after_another_project_was_opened() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Default", RunLaunch::Default)]);
    f.write_gradlew(&format!(
        "sleep 1\nmkdir -p '{debug}'\nprintf 'apk' > '{debug}/app-debug.apk'\necho 'BUILD SUCCESSFUL in 1s'",
        debug = f.project.join("app/build/outputs/apk/debug").display()
    ));
    let other = f.project.parent().unwrap().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let mut hooks = Hooks::new(&f);
    hooks.during_build = Some(Box::new(move |f: &Fixture| {
        // The user opens another project while the build runs.
        let fs_state = f.env.fs_state.clone();
        tokio::spawn(async move {
            let mut fs = fs_state.0.lock().await;
            fs.project_root = Some(other.clone());
            fs.gradle_root = Some(other);
        });
    }));

    let error = run(&f, &mut hooks, "Default").await.unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidInput(m) if m.contains("The project changed")),
        "{error:?}"
    );
    assert!(f.installs().is_empty(), "{:?}", f.adb_calls());
    assert_eq!(
        hooks.phase_names(),
        [DeployPhase::Building, DeployPhase::Failed]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_build_past_its_timeout_is_stopped_and_installs_nothing() {
    let _history = common::lock_history().await;
    let f = Fixture::new(vec![configuration("Default", RunLaunch::Default)]);
    f.write_gradlew("sleep 30\necho 'BUILD SUCCESSFUL in 30s'");
    let request =
        deploy::build_request(&f.open, ":app:assembleDebug".into(), BuildActor::App).unwrap();

    let error = deploy::build_and_wait(
        &f.env.build_state,
        &f.process_manager,
        None,
        request,
        Duration::from_millis(500),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(&error, AppError::ProcessFailed(m) if m.contains("timed out")),
        "{error:?}"
    );
    assert!(f.installs().is_empty());
    assert!(f.env.build_state.inner.lock().await.current_build.is_none());
}
