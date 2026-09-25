use crate::models::health::SystemHealthReport;
use crate::models::settings::AppSettings;
use crate::services::jdk::{self, JdkSearchRoots};
use crate::services::settings_manager;
use crate::utils::process::{output_with_timeout, TOOL_PROBE_TIMEOUT};
use crate::FsState;
use std::path::{Path, PathBuf};

/// Expand a leading `~/` to the real home directory.
/// Rust's `Path::new` does NOT interpret `~` — it's a shell shorthand only.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[tauri::command]
pub async fn run_health_checks(
    fs_state: tauri::State<'_, FsState>,
) -> Result<SystemHealthReport, String> {
    let (settings, _) = settings_manager::load_settings();

    let (project_root, gradle_root): (Option<PathBuf>, Option<PathBuf>) = {
        let fs = fs_state.0.lock().await;
        (fs.project_root.clone(), fs.gradle_root.clone())
    };

    Ok(system_report(
        &settings,
        project_root.as_deref(),
        gradle_root.as_deref(),
        &JdkSearchRoots::system(),
    )
    .await)
}

async fn system_report(
    settings: &AppSettings,
    project_root: Option<&Path>,
    gradle_root: Option<&Path>,
    jdk_roots: &JdkSearchRoots,
) -> SystemHealthReport {
    // ── Java probe ────────────────────────────────────────────────────────────
    // The same JDK resolution and probe as Gradle builds and MCP health.
    let java = jdk::check_project_java(settings, project_root, gradle_root, jdk_roots).await;

    // ── Android SDK probe ─────────────────────────────────────────────────────
    // Expand `~/` before any filesystem check — Rust does NOT expand the tilde
    // shorthand; `Path::new("~/…").exists()` always returns false.
    let android_sdk_valid = settings
        .android
        .sdk_path
        .as_deref()
        .map(|p| {
            let root = expand_tilde(p);
            root.exists()
                && (root.join("platforms").is_dir() || root.join("platform-tools").is_dir())
        })
        .unwrap_or(false);

    // ── ADB probe ─────────────────────────────────────────────────────────────
    let adb_bin = settings
        .android
        .sdk_path
        .as_deref()
        .map(|sdk| expand_tilde(sdk).join("platform-tools").join("adb"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("adb"));

    let adb_output = output_with_timeout(
        tokio::process::Command::new(&adb_bin)
            .arg("version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null()),
        TOOL_PROBE_TIMEOUT,
    )
    .await;

    let (adb_found, adb_version) = match adb_output {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(str::to_owned);
            (true, ver)
        }
        _ => (false, None),
    };

    // ── Emulator probe ────────────────────────────────────────────────────────
    let emulator_found = settings
        .android
        .sdk_path
        .as_deref()
        .map(|sdk| {
            expand_tilde(sdk)
                .join("emulator")
                .join("emulator")
                .is_file()
        })
        .unwrap_or(false);

    // ── Gradle wrapper probe ──────────────────────────────────────────────────
    // Prefer the detected Gradle root (ancestor with settings.gradle) over
    // the user-opened folder, since `gradlew` lives at the Gradle project
    // root which may be an ancestor of the opened module directory.
    let gradle_wrapper_found = gradle_root
        .as_ref()
        .or(project_root.as_ref())
        .map(|root| root.join("gradlew").is_file() || root.join("gradlew.bat").is_file())
        .unwrap_or(false);

    // ── App directory probe ──────────────────────────────────────────────────
    let app_dir = crate::services::settings_manager::data_dir();
    let lsp_system_dir_ok = tokio::fs::create_dir_all(&app_dir).await.is_ok();

    // ── Android Studio CLI probe ──────────────────────────────────────────────
    // Uses a login shell so macOS users who set PATH in .zshrc / .zprofile
    // have the `studio` command resolved correctly.
    let studio_command_found = output_with_timeout(
        tokio::process::Command::new("sh").args(["-lc", "which studio"]),
        TOOL_PROBE_TIMEOUT,
    )
    .await
    .map(|o| o.status.success())
    .unwrap_or(false);

    SystemHealthReport {
        java_executable_found: java.found,
        java_version: java.version_line,
        java_bin_used: java.bin.to_string_lossy().into_owned(),
        java_major_version: java.major,
        java_home: java
            .jdk
            .as_ref()
            .map(|j| j.home.to_string_lossy().into_owned()),
        java_source: java.jdk.map(|j| j.source),
        android_sdk_valid,
        adb_found,
        adb_version,
        emulator_found,
        gradle_wrapper_found,
        lsp_system_dir_ok,
        studio_command_found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::health::JdkSource;
    use crate::services::health_inspector;
    use crate::services::jdk::test_support::*;

    /// GUI and MCP health, run on the same inputs, against a fake SDK so no
    /// real `adb` runs.
    async fn gui_and_mcp_reports(
        root: &Path,
        settings: &mut AppSettings,
        project: &Path,
        roots: &JdkSearchRoots,
    ) -> (SystemHealthReport, health_inspector::HealthReport) {
        let sdk = root.join("sdk");
        write_script(
            &sdk.join("platform-tools").join("adb"),
            "echo 'Android Debug Bridge version 1.0.41'",
        );
        settings.android.sdk_path = Some(sdk.to_string_lossy().into_owned());
        let gui = system_report(settings, Some(project), Some(project), roots).await;
        let mcp =
            health_inspector::run_health_check(settings, Some(project), Some(project), roots).await;
        (gui, mcp)
    }

    fn assert_same_java(gui: &SystemHealthReport, mcp: &health_inspector::HealthReport) {
        let java = &mcp.java;
        assert_eq!(gui.java_executable_found, java.found);
        assert_eq!(gui.java_version, java.version_line);
        assert_eq!(gui.java_major_version, java.major);
        assert_eq!(gui.java_bin_used, java.bin.to_string_lossy());
        assert_eq!(
            gui.java_home.as_deref().map(PathBuf::from),
            java.jdk.as_ref().map(|j| j.home.clone())
        );
        assert_eq!(gui.java_source, java.jdk.as_ref().map(|j| j.source));
    }

    #[tokio::test]
    async fn gui_and_mcp_health_choose_the_same_jdk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let roots = JdkSearchRoots {
            gradle_user_home: None,
            application_dirs: vec![root.join("Applications")],
            jvm_dir: Some(root.join("jvms")),
        };
        fake_installed_jdk(&root.join("jvms"), "jdk-11.jdk", "11.0.21");
        let jbr = fake_jbr(&root.join("Applications"), "Android Studio.app", "21.0.8");
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();

        let (gui, mcp) =
            gui_and_mcp_reports(&root, &mut AppSettings::default(), &project, &roots).await;

        assert_same_java(&gui, &mcp);
        assert!(gui.java_executable_found);
        assert_eq!(gui.java_major_version, Some(21));
        assert_eq!(gui.java_source, Some(JdkSource::AndroidStudio));
        assert_eq!(
            gui.java_home.as_deref(),
            Some(jbr.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn gui_and_mcp_health_both_report_a_java_stub_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let stub_home = root.join("stub");
        write_script(&stub_home.join("bin").join("java"), STUB_JAVA);
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let mut settings = AppSettings::default();
        settings.java.home = Some(stub_home.to_string_lossy().into_owned());

        let (gui, mcp) =
            gui_and_mcp_reports(&root, &mut settings, &project, &JdkSearchRoots::default()).await;

        assert_same_java(&gui, &mcp);
        assert!(!gui.java_executable_found);
        assert!(!mcp.all_ok);
        assert_eq!(gui.java_version, None);
    }

    #[tokio::test]
    async fn health_runs_a_project_chosen_java_only_for_a_trusted_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let project = root.join("project");
        let marker = root.join("project-java-ran");
        write_script(
            &project.join("tools").join("bin").join("java"),
            &format!("touch '{}'", marker.display()),
        );
        std::fs::write(
            project.join("gradle.properties"),
            format!("org.gradle.java.home={}\n", project.join("tools").display()),
        )
        .unwrap();
        let roots = JdkSearchRoots::default();

        let mut settings = AppSettings::default();
        let (gui, mcp) = gui_and_mcp_reports(&root, &mut settings, &project, &roots).await;
        assert!(!marker.exists(), "an untrusted project's java must not run");
        assert_ne!(gui.java_source, Some(JdkSource::ProjectGradleProperties));
        assert_ne!(
            mcp.java.jdk.map(|j| j.source),
            Some(JdkSource::ProjectGradleProperties)
        );

        settings
            .recent_projects
            .push(crate::models::settings::ProjectEntry {
                path: project.to_string_lossy().into_owned(),
                trusted: Some(true),
                ..Default::default()
            });
        let (gui, _) = gui_and_mcp_reports(&root, &mut settings, &project, &roots).await;
        assert!(marker.exists(), "a trusted project's java is probed");
        assert_eq!(gui.java_source, Some(JdkSource::ProjectGradleProperties));
    }

    #[test]
    fn android_sdk_invalid_when_path_missing() {
        let valid = Some("/nonexistent/sdk/path")
            .map(|p| {
                let root = expand_tilde(p);
                root.exists()
                    && (root.join("platforms").is_dir() || root.join("platform-tools").is_dir())
            })
            .unwrap_or(false);
        assert!(!valid);
    }

    #[test]
    fn expand_tilde_replaces_home_prefix() {
        let home = dirs::home_dir().unwrap();
        let result = expand_tilde("~/Documents/test");
        assert_eq!(result, home.join("Documents/test"));
    }

    #[test]
    fn expand_tilde_leaves_absolute_paths_unchanged() {
        let result = expand_tilde("/absolute/path/to/sdk");
        assert_eq!(result, PathBuf::from("/absolute/path/to/sdk"));
    }

    #[test]
    fn expand_tilde_leaves_relative_paths_unchanged() {
        let result = expand_tilde("relative/path");
        assert_eq!(result, PathBuf::from("relative/path"));
    }

    #[test]
    fn expand_tilde_handles_tilde_only() {
        let home = dirs::home_dir().unwrap();
        // "~" alone (no slash) is NOT expanded — only "~/" prefix is.
        let result = expand_tilde("~");
        assert_eq!(result, PathBuf::from("~"));
        // But "~/" is expanded to the home directory itself.
        let result2 = expand_tilde("~/");
        assert_eq!(result2, home.join(""));
    }
}
