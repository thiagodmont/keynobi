//! What Run does with a run configuration: the task it builds, the device it
//! installs on, and what it launches, checked against the project and the
//! connected devices before anything runs (`resolve_run_configuration`).

use crate::models::device::{Device, DeviceConnectionState};
use crate::models::error::AppError;
use crate::models::run_configuration::{
    LocalRunState, ProjectRunConfigurations, ResolvedRun, RunConfiguration, RunDevice, RunLaunch,
    TargetPreference,
};
use crate::services::{gradle_modules, project_trust, run_configurations, settings_manager};
use std::path::Path;

/// The logcat filter a launch applies when its configuration names none.
pub const DEFAULT_LOGCAT_FILTER: &str = "package:mine";

/// What to resolve.
#[derive(Debug, Clone, Default)]
pub struct RunRequest {
    /// The configuration to run; the active one when `None`.
    pub name: Option<String>,
    /// Plan the build only: no device, install, or launch.
    pub build_only: bool,
}

/// The project a run belongs to.
#[derive(Debug, Clone, Copy)]
pub struct RunProject<'a> {
    /// The project's registry key (`list_run_configurations`).
    pub registry_root: &'a str,
    /// Where Gradle runs and application modules are found.
    pub gradle_root: &'a Path,
    /// The root whose trust decides whether Gradle may run.
    pub trust_root: &'a Path,
}

/// The connected devices, as the device poller last saw them.
#[derive(Debug, Clone, Copy)]
pub struct Devices<'a> {
    pub list: &'a [Device],
    /// The device selected in the app for this run (or picked for it), which
    /// a target of `ask`, and of `lastUsed` without its last device, runs on.
    pub selected: Option<&'a str>,
}

/// Resolve `request` in `project` with `devices`.
///
/// # Errors
/// `invalidInput` when no configuration is active or the configuration no
/// longer fits the project (module, variant, task, launch), `permissionDenied`
/// in Safe Mode, and `notFound` when the configuration does not exist or its
/// target finds no online device.
pub fn resolve(
    project: RunProject<'_>,
    request: &RunRequest,
    devices: Devices<'_>,
) -> Result<ResolvedRun, AppError> {
    resolve_at(
        &run_configurations::settings_path(),
        project,
        request,
        devices,
    )
}

pub fn resolve_at(
    settings_path: &Path,
    project: RunProject<'_>,
    request: &RunRequest,
    devices: Devices<'_>,
) -> Result<ResolvedRun, AppError> {
    let configurations = run_configurations::list_at(settings_path, project.registry_root)?;
    let config = choose(&configurations, request.name.as_deref())?;
    let task = check_configuration(config, project.gradle_root)?;
    let settings = settings_manager::load_settings_at_path(settings_path);
    project_trust::require_trusted(&settings, project.trust_root)
        .map_err(AppError::PermissionDenied)?;
    let device = if request.build_only {
        None
    } else {
        let local = configurations
            .local
            .get(&config.name)
            .cloned()
            .unwrap_or_default();
        Some(target(config, &local, devices)?)
    };
    let plan = describe(config, &task, device.as_ref());
    Ok(ResolvedRun {
        name: config.name.clone(),
        module: config.module.clone(),
        variant: config.variant.clone(),
        task,
        launch: config.launch.clone(),
        logcat_filter: config.logcat_filter.clone(),
        device,
        plan,
    })
}

/// The configuration named `name`, else the active one.
fn choose<'a>(
    configurations: &'a ProjectRunConfigurations,
    name: Option<&str>,
) -> Result<&'a RunConfiguration, AppError> {
    let listed = || {
        configurations
            .configurations
            .iter()
            .map(|c| format!("{} ({} {})", c.name, c.module, c.variant))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(name) = name.or(configurations.active.as_deref()) else {
        return Err(AppError::InvalidInput(
            if configurations.configurations.is_empty() {
                "This project has no run configurations. Add one to choose what Run builds and \
                 launches."
                    .to_string()
            } else {
                format!(
                    "No run configuration is active. Choose the one to run: {}.",
                    listed()
                )
            },
        ));
    };
    configurations
        .configurations
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| AppError::NotFound(format!("There is no run configuration named '{name}'.")))
}

/// Check `config` against the project as it is now and return its task.
fn check_configuration(config: &RunConfiguration, gradle_root: &Path) -> Result<String, AppError> {
    let cannot_run = |why: String| {
        AppError::InvalidInput(format!(
            "Run configuration '{}' cannot run: {why}",
            config.name
        ))
    };
    let modules = gradle_modules::application_modules(gradle_root);
    let paths: Vec<String> = modules.iter().map(|m| m.path.clone()).collect();
    run_configurations::validate_run_configuration(config, &paths, &[]).map_err(cannot_run)?;
    if let Some(module) = modules.iter().find(|m| m.path == config.module) {
        let (declared, _) = run_configurations::declared_variants(gradle_root, module);
        if !declared.is_empty() && !declared.contains(&config.variant) {
            return Err(cannot_run(format!(
                "{} has no variant '{}'. Its variants: {}.",
                config.module,
                config.variant,
                declared.join(", ")
            )));
        }
    }
    Ok(config
        .task
        .clone()
        .unwrap_or_else(|| assemble_task(&config.module, &config.variant)))
}

/// `:mobile:assembleFreeDebug`; the root project's task has no module path.
pub fn assemble_task(module: &str, variant: &str) -> String {
    let mut chars = variant.chars();
    let capitalized: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if module == ":" {
        format!("assemble{capitalized}")
    } else {
        format!("{module}:assemble{capitalized}")
    }
}

/// The one online device `config`'s target names.
fn target(
    config: &RunConfiguration,
    local: &LocalRunState,
    devices: Devices<'_>,
) -> Result<RunDevice, AppError> {
    let online = |serial: &str| {
        devices.list.iter().find(|d| {
            d.serial == serial && matches!(d.connection_state, DeviceConnectionState::Online)
        })
    };
    let name = &config.name;
    let pick_one = || {
        AppError::NotFound(format!(
            "Run configuration '{name}' runs on the selected device, and no device is selected. \
             Pick a device in the Devices sidebar, or launch an AVD, then run again."
        ))
    };
    let selected = || devices.selected.and_then(online);
    match &local.target {
        TargetPreference::Serial { serial } => online(serial).map(run_device).ok_or_else(|| {
            AppError::NotFound(format!(
                "Run configuration '{name}' runs on device {serial}, which is not online. \
                 Connect it, or change the configuration's target."
            ))
        }),
        TargetPreference::Avd { name: avd } => {
            let running: Vec<&Device> = devices
                .list
                .iter()
                .filter(|d| {
                    d.avd_name.as_deref() == Some(avd.as_str())
                        && matches!(d.connection_state, DeviceConnectionState::Online)
                })
                .collect();
            match running.as_slice() {
                [only] => Ok(run_device(only)),
                [] => Err(AppError::NotFound(format!(
                    "Run configuration '{name}' runs on the AVD {avd}, which is not running. \
                     Launch it, then run again."
                ))),
                several => Err(AppError::InvalidInput(format!(
                    "Several emulators run the AVD {avd} ({}). Stop all but one, then run again.",
                    several
                        .iter()
                        .map(|d| d.serial.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))),
            }
        }
        TargetPreference::LastUsed => local
            .last_device
            .as_deref()
            .and_then(online)
            .or_else(selected)
            .map(run_device)
            .ok_or_else(pick_one),
        TargetPreference::Ask => selected().map(run_device).ok_or_else(pick_one),
    }
}

fn run_device(device: &Device) -> RunDevice {
    RunDevice {
        serial: device.serial.clone(),
        label: device
            .avd_name
            .clone()
            .or_else(|| device.model.clone())
            .unwrap_or_else(|| device.serial.clone()),
    }
}

/// The plan in one line: `Run 'Default': build :app:assembleDebug → install
/// this build's APK → launch the app on Pixel_7 → filter package:mine`.
fn describe(config: &RunConfiguration, task: &str, device: Option<&RunDevice>) -> String {
    let mut steps = vec![format!("build {task}")];
    if let Some(device) = device {
        let on = &device.label;
        let launch = match &config.launch {
            RunLaunch::Default => Some(format!("launch the app on {on}")),
            RunLaunch::Activity { name } => Some(format!("launch {name} on {on}")),
            RunLaunch::DeepLink { uri } => Some(format!("open {uri} on {on}")),
            RunLaunch::None => None,
        };
        match launch {
            Some(launch) => {
                steps.push("install this build's APK".into());
                steps.push(launch);
                steps.push(format!(
                    "filter {}",
                    config
                        .logcat_filter
                        .as_deref()
                        .unwrap_or(DEFAULT_LOGCAT_FILTER)
                ));
            }
            None => steps.push(format!("install this build's APK on {on} (no launch)")),
        }
    }
    let verb = if device.is_some() { "Run" } else { "Build" };
    format!("{verb} '{}': {}", config.name, steps.join(" → "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::device::DeviceKind;
    use crate::models::settings::ProjectEntry;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const APP: &str = "plugins { id(\"com.android.application\") }\n";
    const WITH_STAGING: &str = "plugins { id(\"com.android.application\") }\nandroid {\n    buildTypes {\n        staging {\n        }\n    }\n}\n";

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A project with application modules `modules`, and a settings file
    /// whose registry holds it (trusted or not).
    struct Fixture {
        project: TempDir,
        _settings: TempDir,
        path: PathBuf,
    }

    impl Fixture {
        fn new(modules: &[&str], trusted: Option<bool>) -> Self {
            let project = TempDir::new().unwrap();
            let includes: Vec<String> = modules.iter().map(|m| format!("\"{m}\"")).collect();
            write(
                project.path(),
                "settings.gradle.kts",
                &format!("include({})\n", includes.join(", ")),
            );
            for module in modules {
                let rel = module.trim_start_matches(':').replace(':', "/");
                write(project.path(), &format!("{rel}/build.gradle.kts"), APP);
            }
            let settings = TempDir::new().unwrap();
            let path = settings.path().join("settings.json");
            let root = project.path().to_string_lossy().into_owned();
            settings_manager::mutate_settings_at_path(&path, |s| {
                s.recent_projects = vec![ProjectEntry {
                    id: "p".into(),
                    path: root.clone(),
                    name: "p".into(),
                    gradle_root: Some(root.clone()),
                    trusted,
                    last_device: Some("emulator-5554".into()),
                    ..Default::default()
                }];
            })
            .unwrap();
            Fixture {
                project,
                _settings: settings,
                path,
            }
        }

        fn root(&self) -> String {
            self.project.path().to_string_lossy().into_owned()
        }

        fn save(&self, config: RunConfiguration) {
            run_configurations::save_at(&self.path, &self.root(), config).unwrap();
        }

        fn set_active(&self, name: &str) {
            run_configurations::set_active_at(&self.path, &self.root(), name).unwrap();
        }

        /// Set the target of the configuration named `name`.
        fn target(&self, name: &str, target: TargetPreference) {
            let root = self.root();
            // Migrated first, so the target is not replaced by the migrated one.
            run_configurations::list_at(&self.path, &root).unwrap();
            settings_manager::mutate_settings_at_path(&self.path, |s| {
                let entry = s
                    .recent_projects
                    .iter_mut()
                    .find(|e| e.path == root)
                    .unwrap();
                entry.run_local.entry(name.to_string()).or_default().target = target;
            })
            .unwrap();
        }

        fn resolve(
            &self,
            request: RunRequest,
            devices: Devices<'_>,
        ) -> Result<ResolvedRun, AppError> {
            resolve_at(
                &self.path,
                RunProject {
                    registry_root: &self.root(),
                    gradle_root: self.project.path(),
                    trust_root: self.project.path(),
                },
                &request,
                devices,
            )
        }
    }

    fn device(serial: &str, avd: Option<&str>, state: DeviceConnectionState) -> Device {
        Device {
            serial: serial.into(),
            name: serial.into(),
            model: Some(format!("Model of {serial}")),
            device_kind: if avd.is_some() {
                DeviceKind::Emulator
            } else {
                DeviceKind::Physical
            },
            connection_state: state,
            api_level: Some(34),
            android_version: Some("14".into()),
            avd_name: avd.map(Into::into),
        }
    }

    fn pixel() -> Device {
        device(
            "emulator-5554",
            Some("Pixel_7"),
            DeviceConnectionState::Online,
        )
    }

    fn phone() -> Device {
        device("28151FDH2000Q4", None, DeviceConnectionState::Online)
    }

    fn config(name: &str, module: &str) -> RunConfiguration {
        RunConfiguration {
            name: name.into(),
            module: module.into(),
            variant: "debug".into(),
            task: None,
            launch: RunLaunch::Default,
            logcat_filter: None,
        }
    }

    fn run() -> RunRequest {
        RunRequest::default()
    }

    fn on<'a>(list: &'a [Device], selected: Option<&'static str>) -> Devices<'a> {
        Devices { list, selected }
    }

    fn message(err: AppError) -> String {
        err.to_string()
    }

    // ── The plan ─────────────────────────────────────────────────────────────

    #[test]
    fn the_default_configuration_builds_installs_and_launches_on_the_last_device() {
        let f = Fixture::new(&[":app"], Some(true));
        let devices = [pixel(), phone()];

        let resolved = f
            .resolve(run(), on(&devices, Some("28151FDH2000Q4")))
            .unwrap();

        assert_eq!(
            resolved,
            ResolvedRun {
                name: "Default".into(),
                module: ":app".into(),
                variant: "debug".into(),
                task: ":app:assembleDebug".into(),
                launch: RunLaunch::Default,
                logcat_filter: None,
                device: Some(RunDevice {
                    serial: "emulator-5554".into(),
                    label: "Pixel_7".into(),
                }),
                plan: "Run 'Default': build :app:assembleDebug → install this build's APK → \
                       launch the app on Pixel_7 → filter package:mine"
                    .into(),
            }
        );
    }

    #[test]
    fn the_plan_names_the_task_launch_and_filter_of_the_configuration() {
        let f = Fixture::new(&[":app"], Some(true));
        write(f.project.path(), "app/build.gradle.kts", WITH_STAGING);
        f.save(RunConfiguration {
            variant: "staging".into(),
            task: Some(":app:bundleStaging".into()),
            launch: RunLaunch::Activity {
                name: ".SettingsActivity".into(),
            },
            logcat_filter: Some("package:mine level:warn".into()),
            ..config("Settings", ":app")
        });
        f.save(RunConfiguration {
            launch: RunLaunch::DeepLink {
                uri: "myapp://home".into(),
            },
            ..config("Link", ":app")
        });
        f.save(RunConfiguration {
            launch: RunLaunch::None,
            ..config("Install", ":app")
        });
        let devices = [phone()];
        let plan = |name: &str| {
            f.resolve(
                RunRequest {
                    name: Some(name.into()),
                    ..run()
                },
                on(&devices, Some("28151FDH2000Q4")),
            )
            .unwrap()
        };

        let settings = plan("Settings");
        assert_eq!(settings.task, ":app:bundleStaging");
        assert_eq!(settings.variant, "staging");
        assert_eq!(
            settings.plan,
            "Run 'Settings': build :app:bundleStaging → install this build's APK → launch \
             .SettingsActivity on Model of 28151FDH2000Q4 → filter package:mine level:warn"
        );
        assert_eq!(
            plan("Link").plan,
            "Run 'Link': build :app:assembleDebug → install this build's APK → open myapp://home \
             on Model of 28151FDH2000Q4 → filter package:mine"
        );
        assert_eq!(
            plan("Install").plan,
            "Run 'Install': build :app:assembleDebug → install this build's APK on Model of \
             28151FDH2000Q4 (no launch)"
        );
    }

    #[test]
    fn a_build_only_plan_needs_no_device() {
        let f = Fixture::new(&[":app"], Some(true));

        let resolved = f
            .resolve(
                RunRequest {
                    build_only: true,
                    ..run()
                },
                on(&[], None),
            )
            .unwrap();

        assert_eq!(resolved.device, None);
        assert_eq!(resolved.plan, "Build 'Default': build :app:assembleDebug");
    }

    #[test]
    fn the_root_project_builds_without_a_module_path() {
        let f = Fixture::new(&[], Some(true));
        write(
            f.project.path(),
            "settings.gradle.kts",
            "rootProject.name = \"x\"\n",
        );
        write(f.project.path(), "build.gradle.kts", APP);

        let resolved = f
            .resolve(
                RunRequest {
                    build_only: true,
                    ..run()
                },
                on(&[], None),
            )
            .unwrap();

        assert_eq!(resolved.module, ":");
        assert_eq!(resolved.task, "assembleDebug");
        assert_eq!(
            assemble_task(":wear", "freeRelease"),
            ":wear:assembleFreeRelease"
        );
    }

    // ── Which configuration ──────────────────────────────────────────────────

    #[test]
    fn several_modules_with_none_active_ask_to_choose_listing_the_configurations() {
        let f = Fixture::new(&[":mobile", ":wear"], Some(true));

        let err = f.resolve(run(), on(&[pixel()], None)).unwrap_err();

        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert!(message(err).contains(
            "No run configuration is active. Choose the one to run: mobile (:mobile debug), \
                 wear (:wear debug)."
        ),);
        // Naming one runs it.
        let wear = f
            .resolve(
                RunRequest {
                    name: Some("wear".into()),
                    ..run()
                },
                on(&[pixel()], None),
            )
            .unwrap();
        assert_eq!(wear.task, ":wear:assembleDebug");
    }

    #[test]
    fn an_unknown_or_missing_configuration_is_an_error() {
        let f = Fixture::new(&[":app"], Some(true));
        let err = f
            .resolve(
                RunRequest {
                    name: Some("Nope".into()),
                    ..run()
                },
                on(&[pixel()], None),
            )
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");

        run_configurations::delete_at(&f.path, &f.root(), "Default").unwrap();
        let err = f.resolve(run(), on(&[pixel()], None)).unwrap_err();
        assert!(message(err).contains("no run configurations"));
    }

    // ── The configuration against the project ────────────────────────────────

    #[test]
    fn a_module_that_is_no_longer_an_application_is_refused() {
        let f = Fixture::new(&[":app"], Some(true));
        f.resolve(run(), on(&[pixel()], None)).unwrap();
        write(
            f.project.path(),
            "app/build.gradle.kts",
            "plugins { id(\"com.android.library\") }\n",
        );

        let err = f.resolve(run(), on(&[pixel()], None)).unwrap_err();

        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert!(message(err).contains(
            "Run configuration 'Default' cannot run: ':app' is not an application module"
        ));
    }

    #[test]
    fn a_variant_the_module_does_not_declare_is_refused() {
        let f = Fixture::new(&[":app"], Some(true));
        write(f.project.path(), "app/build.gradle.kts", WITH_STAGING);
        f.save(RunConfiguration {
            variant: "paidDebug".into(),
            ..config("Paid", ":app")
        });
        f.set_active("Paid");

        let err = f.resolve(run(), on(&[pixel()], None)).unwrap_err();

        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert!(message(err)
            .contains(":app has no variant 'paidDebug'. Its variants: debug, staging, release."),);
    }

    #[test]
    fn a_variant_is_not_checked_when_the_module_declares_none() {
        let f = Fixture::new(&[":app"], Some(true));
        f.save(RunConfiguration {
            variant: "freeDebug".into(),
            ..config("Free", ":app")
        });

        let resolved = f
            .resolve(
                RunRequest {
                    name: Some("Free".into()),
                    build_only: true,
                },
                on(&[], None),
            )
            .unwrap();

        assert_eq!(resolved.task, ":app:assembleFreeDebug");
    }

    /// Store `config` as is, bypassing the save checks, as an older or
    /// hand-edited registry could.
    fn store_raw(f: &Fixture, config: RunConfiguration) {
        let root = f.root();
        run_configurations::list_at(&f.path, &root).unwrap();
        settings_manager::mutate_settings_at_path(&f.path, |s| {
            let entry = s
                .recent_projects
                .iter_mut()
                .find(|e| e.path == root)
                .unwrap();
            entry.run_configurations = Some(vec![config]);
        })
        .unwrap();
    }

    #[test]
    fn a_task_outside_the_module_or_a_bad_launch_is_refused() {
        let f = Fixture::new(&[":app", ":wear"], Some(true));
        let cases = [
            (
                RunConfiguration {
                    task: Some(":wear:assembleDebug".into()),
                    ..config("Default", ":app")
                },
                "is not a task of :app",
            ),
            (
                RunConfiguration {
                    task: Some("assembleDebug; rm -rf /".into()),
                    ..config("Default", ":app")
                },
                "cannot run",
            ),
            (
                RunConfiguration {
                    launch: RunLaunch::Activity {
                        name: ".Main; reboot".into(),
                    },
                    ..config("Default", ":app")
                },
                "Invalid activity name",
            ),
            (
                RunConfiguration {
                    launch: RunLaunch::DeepLink {
                        uri: "not a uri".into(),
                    },
                    ..config("Default", ":app")
                },
                "Invalid deep link",
            ),
        ];
        for (bad, needle) in cases {
            store_raw(&f, bad);
            let err = f
                .resolve(
                    RunRequest {
                        name: Some("Default".into()),
                        ..run()
                    },
                    on(&[pixel()], None),
                )
                .unwrap_err();
            assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
            assert!(message(err).contains(needle), "{needle}");
        }
    }

    #[test]
    fn safe_mode_refuses_to_run() {
        for trusted in [Some(false), None] {
            let f = Fixture::new(&[":app"], trusted);

            let err = f.resolve(run(), on(&[pixel()], None)).unwrap_err();

            assert!(matches!(err, AppError::PermissionDenied(_)), "{err:?}");
            assert!(message(err).contains("not trusted"));
        }
    }

    // ── The target ───────────────────────────────────────────────────────────

    #[test]
    fn a_serial_target_runs_only_on_that_device_when_it_is_online() {
        let f = Fixture::new(&[":app"], Some(true));
        f.target(
            "Default",
            TargetPreference::Serial {
                serial: "28151FDH2000Q4".into(),
            },
        );

        let resolved = f
            .resolve(run(), on(&[pixel(), phone()], Some("emulator-5554")))
            .unwrap();
        assert_eq!(resolved.device.unwrap().serial, "28151FDH2000Q4");

        let offline = [
            pixel(),
            device("28151FDH2000Q4", None, DeviceConnectionState::Offline),
        ];
        let err = f
            .resolve(run(), on(&offline, Some("emulator-5554")))
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
        assert!(message(err).contains("runs on device 28151FDH2000Q4, which is not online"));
    }

    #[test]
    fn an_avd_target_runs_on_the_emulator_running_that_avd_exactly() {
        let f = Fixture::new(&[":app"], Some(true));
        f.target(
            "Default",
            TargetPreference::Avd {
                name: "Pixel_7".into(),
            },
        );
        let devices = [
            device(
                "emulator-5556",
                Some("Pixel_7_Pro"),
                DeviceConnectionState::Online,
            ),
            phone(),
            pixel(),
        ];

        let resolved = f
            .resolve(run(), on(&devices, Some("28151FDH2000Q4")))
            .unwrap();

        assert_eq!(
            resolved.device,
            Some(RunDevice {
                serial: "emulator-5554".into(),
                label: "Pixel_7".into(),
            })
        );
    }

    #[test]
    fn an_avd_that_is_not_running_is_reported_so_it_can_be_launched() {
        let f = Fixture::new(&[":app"], Some(true));
        f.target(
            "Default",
            TargetPreference::Avd {
                name: "Pixel_7".into(),
            },
        );
        let booting = [
            device(
                "emulator-5554",
                Some("Pixel_7"),
                DeviceConnectionState::Offline,
            ),
            phone(),
        ];

        let err = f
            .resolve(run(), on(&booting, Some("28151FDH2000Q4")))
            .unwrap_err();

        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
        assert_eq!(
            message(err),
            "Not found: Run configuration 'Default' runs on the AVD Pixel_7, which is not \
             running. Launch it, then run again."
        );
    }

    #[test]
    fn two_emulators_of_one_avd_are_refused() {
        let f = Fixture::new(&[":app"], Some(true));
        f.target(
            "Default",
            TargetPreference::Avd {
                name: "Pixel_7".into(),
            },
        );
        let devices = [
            pixel(),
            device(
                "emulator-5556",
                Some("Pixel_7"),
                DeviceConnectionState::Online,
            ),
        ];

        let err = f.resolve(run(), on(&devices, None)).unwrap_err();

        assert!(message(err).contains("Several emulators run the AVD Pixel_7"));
    }

    #[test]
    fn last_used_prefers_the_last_device_then_the_selected_one() {
        let f = Fixture::new(&[":app"], Some(true));
        // Migration took the project's last device, emulator-5554.
        let both = [pixel(), phone()];
        let resolved = f.resolve(run(), on(&both, Some("28151FDH2000Q4"))).unwrap();
        assert_eq!(resolved.device.unwrap().serial, "emulator-5554");

        let only_phone = [phone()];
        let resolved = f
            .resolve(run(), on(&only_phone, Some("28151FDH2000Q4")))
            .unwrap();
        assert_eq!(resolved.device.unwrap().serial, "28151FDH2000Q4");

        let err = f.resolve(run(), on(&only_phone, None)).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
        assert!(message(err).contains("no device is selected"));
    }

    #[test]
    fn ask_runs_on_the_selected_device_else_asks_to_pick_one() {
        let f = Fixture::new(&[":app"], Some(true));
        f.target("Default", TargetPreference::Ask);
        let both = [pixel(), phone()];

        let resolved = f.resolve(run(), on(&both, Some("28151FDH2000Q4"))).unwrap();
        assert_eq!(resolved.device.unwrap().serial, "28151FDH2000Q4");

        let offline_selection = [
            pixel(),
            device("28151FDH2000Q4", None, DeviceConnectionState::Unauthorized),
        ];
        let err = f
            .resolve(run(), on(&offline_selection, Some("28151FDH2000Q4")))
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
        assert!(message(err).contains("Pick a device"));
    }

    #[test]
    fn a_device_picked_for_the_run_is_the_selected_device() {
        let f = Fixture::new(&[":app"], Some(true));
        f.target("Default", TargetPreference::Ask);
        let both = [pixel(), phone()];

        let resolved = f.resolve(run(), on(&both, Some("28151FDH2000Q4"))).unwrap();
        assert_eq!(resolved.device.unwrap().serial, "28151FDH2000Q4");

        // An explicit target ignores the selection.
        f.target(
            "Default",
            TargetPreference::Avd {
                name: "Pixel_7".into(),
            },
        );
        let resolved = f.resolve(run(), on(&both, Some("28151FDH2000Q4"))).unwrap();
        assert_eq!(resolved.device.unwrap().serial, "emulator-5554");
    }
}
