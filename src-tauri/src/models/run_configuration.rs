use crate::models::build::{LaunchResult, RunApk};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// What Run builds and launches for one application module and variant.
///
/// Portable: it holds no paths, serials, or environment, so it can be shared
/// with a project. Machine-specific state is in [`LocalRunState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RunConfiguration {
    /// Unique per project.
    pub name: String,
    /// Gradle path of an application module (`:app`; `:` for the root project).
    pub module: String,
    /// AGP variant name (`debug`, `freeRelease`).
    pub variant: String,
    /// The Gradle task to build; `None` for `:<module>:assemble<Variant>`.
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub launch: RunLaunch,
    /// Logcat query applied after launch, in the query bar's syntax.
    #[serde(default)]
    pub logcat_filter: Option<String>,
}

/// What Run starts after installing the APK.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum RunLaunch {
    /// The launcher activity.
    #[default]
    Default,
    /// An activity of the app (`.MainActivity`, `com.example.Main`).
    Activity { name: String },
    /// A deep link opened in the app.
    DeepLink { uri: String },
    /// Install only.
    None,
}

/// Which device Run installs on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum TargetPreference {
    /// Ask every time.
    Ask,
    /// A device by adb serial, when it is online.
    Serial { serial: String },
    /// An emulator by AVD name.
    Avd { name: String },
    /// The device this configuration last ran on.
    #[default]
    LastUsed,
}

/// Machine-specific state of one run configuration, never shared.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LocalRunState {
    pub target: TargetPreference,
    /// Serial of the device it last ran on.
    pub last_device: Option<String>,
    /// SHA-256 of the project's shared configuration file when the user last
    /// approved running this shared configuration.
    pub approved_project_file_sha256: Option<String>,
}

/// A project's run configurations, as `list_run_configurations` returns them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ProjectRunConfigurations {
    pub configurations: Vec<RunConfiguration>,
    /// Name of the active configuration; `None` until one is chosen.
    pub active: Option<String>,
    /// Local state by configuration name.
    pub local: BTreeMap<String, LocalRunState>,
    /// Names of the configurations read from the project's shared file.
    #[serde(default)]
    pub shared: Vec<String>,
    /// The project's shared file; `None` when the project has none.
    #[serde(default)]
    pub shared_file: Option<SharedRunConfigurationsFile>,
}

/// The project's shared run configuration file
/// (`<gradle root>/.keynobi/run-configurations.json`), as last read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SharedRunConfigurationsFile {
    /// Path relative to the Gradle root.
    pub path: String,
    /// SHA-256 of the file's bytes; `None` when it could not be read.
    pub sha256: Option<String>,
    /// Why none of the file's configurations are offered.
    pub error: Option<String>,
    /// Configurations of the file that are not offered, and why.
    pub problems: Vec<SharedRunConfigurationProblem>,
}

/// A configuration of the shared file that is not offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SharedRunConfigurationProblem {
    /// Its name, when the file gives one.
    pub name: Option<String>,
    pub message: String,
}

/// A run configuration checked against the project and the connected devices:
/// what Run builds, where it installs, and what it launches
/// (`resolve_run_configuration`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ResolvedRun {
    /// The configuration's name.
    pub name: String,
    pub module: String,
    pub variant: String,
    /// The Gradle task to build: the configuration's, else
    /// `:<module>:assemble<Variant>`.
    pub task: String,
    pub launch: RunLaunch,
    pub logcat_filter: Option<String>,
    /// The online device to install on; `None` for a build-only plan.
    pub device: Option<RunDevice>,
    /// The plan in one line, for the build log.
    pub plan: String,
}

/// The device a run installs on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RunDevice {
    pub serial: String,
    /// The AVD name, else the model, else the serial.
    pub label: String,
}

/// Where a run of a configuration is (`deploy:phase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DeployPhase {
    Building,
    Installing,
    Launching,
    Done,
    Failed,
    Cancelled,
}

/// Payload of `deploy:phase`: the app's run of a configuration entered `phase`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DeployPhaseEvent {
    pub phase: DeployPhase,
    /// The configuration's name.
    pub name: String,
    /// The plan in one line.
    pub plan: String,
    pub device: RunDevice,
    /// The run's build history record, once the build is recorded.
    pub build_id: Option<u32>,
    /// What the run did since the previous phase, one log line each.
    pub steps: Vec<String>,
    /// Why it failed (`failed`).
    pub error: Option<String>,
}

/// How a run of a configuration ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DeployOutcome {
    /// Built, installed, and launched as the configuration says.
    Done,
    /// The build failed; nothing was installed.
    BuildFailed,
    /// The build was cancelled; nothing was installed.
    Cancelled,
}

/// What a run of a configuration did (`run_run_configuration`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DeployResult {
    /// The plan it ran.
    pub run: ResolvedRun,
    pub outcome: DeployOutcome,
    /// The build history record of its build.
    pub build_id: Option<u32>,
    /// The device it installed on.
    pub device: RunDevice,
    /// The APK it installed.
    pub apk: Option<RunApk>,
    /// SHA-256 of that APK; `None` when it could not be hashed.
    pub apk_sha256: Option<String>,
    /// The installed APK's package; `None` when it could not be read.
    pub package: Option<String>,
    /// `None` when it did not launch: the launch is `none`, or the package is unknown.
    pub launch: Option<LaunchResult>,
    /// The configuration's logcat filter; `None` applies `package:mine`.
    pub logcat_filter: Option<String>,
}
