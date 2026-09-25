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
    /// Whether `java -version` exits successfully and reports a version.
    pub java_executable_found: bool,
    /// The version line of `java -version`, e.g. `openjdk version "17.0.8" …`
    pub java_version: Option<String>,
    /// The Java binary that was probed.
    pub java_bin_used: String,
    /// Major version parsed from `java -version`, e.g. `21`.
    pub java_major_version: Option<u32>,
    /// The JDK home Gradle builds use, or `None` when none was resolved and
    /// `java` on `PATH` was probed instead.
    pub java_home: Option<String>,
    /// Where `java_home` came from.
    pub java_source: Option<JdkSource>,
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
    /// Why the app runs from a temporary location (a disk image or App
    /// Translocation) that AI clients cannot rely on, or `None`.
    pub app_location_problem: Option<String>,
}

/// Where the JDK used for Gradle builds was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum JdkSource {
    /// `org.gradle.java.home` in the Gradle user home `gradle.properties`.
    UserGradleProperties,
    /// `org.gradle.java.home` in the project's `gradle.properties`.
    ProjectGradleProperties,
    /// The `java.home` setting.
    Settings,
    /// The JetBrains Runtime bundled with Android Studio.
    AndroidStudio,
    /// The newest installed JDK 17 or later.
    InstalledJdk,
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
            java_major_version: Some(17),
            java_home: Some("/jdk".into()),
            java_source: Some(JdkSource::AndroidStudio),
            android_sdk_valid: true,
            adb_found: true,
            adb_version: Some("Android Debug Bridge version 1.0.41".into()),
            emulator_found: true,
            gradle_wrapper_found: true,
            lsp_system_dir_ok: true,
            studio_command_found: false,
            app_location_problem: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("javaExecutableFound"));
        assert!(json.contains("openjdk 17.0.8"));
        assert!(json.contains("adbFound"));
        assert!(json.contains(r#""javaSource":"androidStudio""#));
        assert!(json.contains(r#""javaMajorVersion":17"#));
    }
}
