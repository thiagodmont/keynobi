use crate::models::health::SystemHealthReport;
use crate::services::settings_manager;
use crate::FsState;
use std::path::PathBuf;

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
    let settings = settings_manager::load_settings();

    let (project_root, gradle_root): (Option<PathBuf>, Option<PathBuf>) = {
        let fs = fs_state.0.lock().await;
        (fs.project_root.clone(), fs.gradle_root.clone())
    };

    // ── Java probe ────────────────────────────────────────────────────────────
    // Prefer the user-configured Java home; fall back to whatever `java` is on
    // PATH (the bundled JBR handles running the LSP itself, but Gradle tasks
    // need a separate JDK for compilation).
    let java_bin: PathBuf = settings
        .java
        .home
        .as_deref()
        .map(|h| expand_tilde(h).join("bin").join("java"))
        .unwrap_or_else(|| PathBuf::from("java"));

    let java_bin_used = java_bin.to_string_lossy().into_owned();

    let java_output = tokio::process::Command::new(&java_bin)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await;

    let (java_executable_found, java_version) = match java_output {
        Ok(out) if out.status.success() || !out.stderr.is_empty() => {
            // `java -version` prints to stderr — grab the first non-empty line.
            let ver = String::from_utf8_lossy(&out.stderr)
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(str::to_owned);
            (true, ver)
        }
        _ => (false, None),
    };

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

    let adb_output = tokio::process::Command::new(&adb_bin)
        .arg("version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
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
        .map(|sdk| expand_tilde(sdk).join("emulator").join("emulator").is_file())
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
    let app_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".keynobi");
    let lsp_system_dir_ok = tokio::fs::create_dir_all(&app_dir)
        .await
        .is_ok();

    // ── Android Studio CLI probe ──────────────────────────────────────────────
    // Uses a login shell so macOS users who set PATH in .zshrc / .zprofile
    // have the `studio` command resolved correctly.
    let studio_command_found = tokio::process::Command::new("sh")
        .args(["-lc", "which studio"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    Ok(SystemHealthReport {
        java_executable_found,
        java_version,
        java_bin_used,
        android_sdk_valid,
        adb_found,
        adb_version,
        emulator_found,
        gradle_wrapper_found,
        lsp_system_dir_ok,
        studio_command_found,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn java_bin_defaults_to_java_when_no_home() {
        // Without a configured home the binary is just "java".
        let bin: PathBuf = None::<&str>
            .map(|h| Path::new(h).join("bin").join("java"))
            .unwrap_or_else(|| PathBuf::from("java"));
        assert_eq!(bin, PathBuf::from("java"));
    }

    #[test]
    fn java_bin_uses_home_when_configured() {
        let home = "/usr/lib/jvm/java-17";
        let bin: PathBuf = Some(home)
            .map(|h| Path::new(h).join("bin").join("java"))
            .unwrap_or_else(|| PathBuf::from("java"));
        assert_eq!(bin, PathBuf::from("/usr/lib/jvm/java-17/bin/java"));
    }

    #[test]
    fn android_sdk_invalid_when_path_missing() {
        let valid = Some("/nonexistent/sdk/path")
            .map(|p| {
                let root = expand_tilde(p);
                root.exists()
                    && (root.join("platforms").is_dir()
                        || root.join("platform-tools").is_dir())
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
