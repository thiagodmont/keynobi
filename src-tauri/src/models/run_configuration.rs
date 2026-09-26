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
    /// SHA-256 of the shared configuration file the user last approved.
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
}
