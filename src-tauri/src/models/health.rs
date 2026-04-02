use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The outcome of running system-level health probes from Rust.
/// Frontend-observable store checks (LSP status, project open, settings) are
/// computed in TypeScript from existing stores; only checks that require
/// process execution or filesystem introspection are done here.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SystemHealthReport {
    /// Whether `java -version` exits successfully.
    pub java_executable_found: bool,
    /// First line of `java -version` stderr output, e.g. `openjdk version "17.0.8" …`
    pub java_version: Option<String>,
    /// The Java binary that was probed.
    pub java_bin_used: String,
    /// Whether the Android SDK path has recognisable SDK structure.
    pub android_sdk_valid: bool,
    /// Whether `adb` was found in `$ANDROID_HOME/platform-tools/` or on PATH.
    pub adb_found: bool,
    /// First line of `adb version` output.
    pub adb_version: Option<String>,
    /// Whether the Android emulator binary was found in `$ANDROID_HOME/emulator/`.
    pub emulator_found: bool,
    /// Whether `gradlew` exists at the project root.
    pub gradle_wrapper_found: bool,
    /// Whether the `.keynobi` app directory is writable.
    pub lsp_system_dir_ok: bool,
    /// Whether the `studio` command is available on PATH (Android Studio CLI).
    pub studio_command_found: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_health_report_serialises() {
        let r = SystemHealthReport {
            java_executable_found: true,
            java_version: Some("openjdk 17.0.8".into()),
            java_bin_used: "/usr/bin/java".into(),
            android_sdk_valid: true,
            adb_found: true,
            adb_version: Some("Android Debug Bridge version 1.0.41".into()),
            emulator_found: true,
            gradle_wrapper_found: true,
            lsp_system_dir_ok: true,
            studio_command_found: false,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("javaExecutableFound"));
        assert!(json.contains("openjdk 17.0.8"));
        assert!(json.contains("adbFound"));
    }
}
