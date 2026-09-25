use crate::models::settings::AppSettings;
use crate::services::jdk::{self, JavaCheck, JdkSearchRoots};
use crate::utils::process::{output_with_timeout, TOOL_PROBE_TIMEOUT};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct HealthReport {
    pub all_ok: bool,
    pub java: JavaCheck,
    pub sdk_ok: bool,
    pub adb_ok: bool,
    pub gradlew_ok: bool,
    pub project_open: bool,
    pub detected_sdk: Option<String>,
    pub project_path: Option<PathBuf>,
}

pub async fn run_health_check(
    settings: &AppSettings,
    project_root: Option<&Path>,
    gradle_root: Option<&Path>,
    jdk_roots: &JdkSearchRoots,
) -> HealthReport {
    let adb = crate::services::adb_manager::get_adb_path(settings);

    let (java, adb_status) = tokio::join!(
        jdk::check_project_java(settings, project_root, gradle_root, jdk_roots),
        async {
            output_with_timeout(
                tokio::process::Command::new(&adb).arg("version"),
                TOOL_PROBE_TIMEOUT,
            )
            .await
        },
    );
    let java_ok = java.found;
    let adb_ok = adb_status.map(|o| o.status.success()).unwrap_or(false);

    let detected_sdk = detected_sdk_for(settings, project_root);
    let sdk_ok = detected_sdk.is_some();

    if sdk_ok {
        if let Some(ref sdk_path) = detected_sdk {
            if settings.android.sdk_path.as_deref() != Some(sdk_path.as_str()) {
                let mut updated = settings.clone();
                updated.android.sdk_path = Some(sdk_path.clone());
                let _ = crate::services::settings_manager::save_settings(&updated);
            }
        }
    }

    // Check both gradlew (Unix) and gradlew.bat (Windows)
    let gradlew_ok = gradle_root
        .or(project_root)
        .map(|r| r.join("gradlew").is_file() || r.join("gradlew.bat").is_file())
        .unwrap_or(false);

    HealthReport {
        all_ok: java_ok && sdk_ok && adb_ok && gradlew_ok && project_root.is_some(),
        java,
        sdk_ok,
        adb_ok,
        gradlew_ok,
        project_open: project_root.is_some(),
        detected_sdk,
        project_path: project_root.map(|p| p.to_path_buf()),
    }
}

/// The SDK to report and save. An untrusted project's `local.properties` is
/// not consulted, so it cannot choose the SDK whose tools Keynobi runs.
fn detected_sdk_for(settings: &AppSettings, project_root: Option<&Path>) -> Option<String> {
    let trusted_root =
        project_root.filter(|root| crate::services::project_trust::is_trusted(settings, root));
    detect_sdk_path(settings.android.sdk_path.as_deref(), trusted_root)
}

pub fn detect_sdk_path(configured: Option<&str>, project_root: Option<&Path>) -> Option<String> {
    fn is_valid_sdk(path: &Path) -> bool {
        path.exists() && (path.join("platforms").is_dir() || path.join("platform-tools").is_dir())
    }

    fn expand(s: &str) -> PathBuf {
        if let Some(rest) = s.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(s)
    }

    if let Some(p) = configured {
        let path = expand(p);
        if is_valid_sdk(&path) {
            return Some(path.to_string_lossy().to_string());
        }
    }

    if let Some(root) = project_root {
        let lp = root.join("local.properties");
        if let Ok(content) = std::fs::read_to_string(&lp) {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("sdk.dir=") {
                    let path = expand(val.trim());
                    if is_valid_sdk(&path) {
                        return Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(val) = std::env::var(var) {
            let path = PathBuf::from(&val);
            if is_valid_sdk(&path) {
                return Some(val);
            }
        }
    }

    for candidate in &[
        "~/Library/Android/sdk",
        "~/Android/Sdk",
        "~/AppData/Local/Android/Sdk",
    ] {
        let path = expand(candidate);
        if is_valid_sdk(&path) {
            return Some(path.to_string_lossy().to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_trusted_project_can_choose_the_sdk_through_local_properties() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let sdk = root.join("project-sdk");
        std::fs::create_dir_all(sdk.join("platform-tools")).unwrap();
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("local.properties"),
            format!("sdk.dir={}\n", sdk.display()),
        )
        .unwrap();
        let project_sdk = Some(sdk.to_string_lossy().into_owned());

        let mut settings = AppSettings::default();
        assert_ne!(detected_sdk_for(&settings, Some(&project)), project_sdk);

        settings
            .recent_projects
            .push(crate::models::settings::ProjectEntry {
                path: project.to_string_lossy().into_owned(),
                trusted: Some(true),
                ..Default::default()
            });
        assert_eq!(detected_sdk_for(&settings, Some(&project)), project_sdk);
    }

    #[test]
    fn detect_sdk_path_accepts_valid_configured_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("platform-tools")).unwrap();
        let path = dir.path().to_str().unwrap();
        let result = detect_sdk_path(Some(path), None);
        assert_eq!(result.as_deref(), Some(path));
    }

    #[test]
    fn detect_sdk_path_ignores_invalid_configured_path() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_sdk_path(Some(dir.path().to_str().unwrap()), None);
        assert_ne!(result.as_deref(), Some(dir.path().to_str().unwrap()));
    }

    #[test]
    fn detect_sdk_path_falls_back_to_local_properties() {
        let sdk_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(sdk_dir.path().join("platforms")).unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let props = format!("sdk.dir={}\n", sdk_dir.path().to_str().unwrap());
        std::fs::write(project_dir.path().join("local.properties"), props).unwrap();
        let result = detect_sdk_path(None, Some(project_dir.path()));
        assert_eq!(result.as_deref(), Some(sdk_dir.path().to_str().unwrap()));
    }

    #[test]
    fn detect_sdk_path_skips_missing_local_properties() {
        let project_dir = tempfile::tempdir().unwrap();
        let _ = detect_sdk_path(None, Some(project_dir.path()));
        // must not panic
    }

    #[test]
    fn detect_sdk_path_does_not_return_invalid_configured_path() {
        let result = detect_sdk_path(Some("/nonexistent/sdk/path_xyz_unique"), None);
        assert_ne!(result.as_deref(), Some("/nonexistent/sdk/path_xyz_unique"));
    }
}
