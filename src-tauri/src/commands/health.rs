use crate::models::health::SystemHealthReport;
use crate::services::{lsp_downloader, settings_manager};
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

    // ── Gradle wrapper probe ──────────────────────────────────────────────────
    // Prefer the detected Gradle root (ancestor with settings.gradle) over
    // the user-opened folder, since `gradlew` lives at the Gradle project
    // root which may be an ancestor of the opened module directory.
    let gradle_wrapper_found = gradle_root
        .as_ref()
        .or(project_root.as_ref())
        .map(|root| root.join("gradlew").is_file() || root.join("gradlew.bat").is_file())
        .unwrap_or(false);

    // ── LSP system directory ──────────────────────────────────────────────────
    let lsp_system_dir = lsp_downloader::get_lsp_system_dir();
    let lsp_system_dir_ok = tokio::fs::create_dir_all(&lsp_system_dir)
        .await
        .is_ok();

    // ── Disk space (Unix only) ────────────────────────────────────────────────
    let disk_free_mb = get_disk_free_mb(
        lsp_system_dir
            .parent()
            .unwrap_or(&lsp_system_dir),
    );

    Ok(SystemHealthReport {
        java_executable_found,
        java_version,
        java_bin_used,
        android_sdk_valid,
        gradle_wrapper_found,
        lsp_system_dir_ok,
        disk_free_mb,
    })
}

// ── Disk space helper (Unix statvfs without extra deps) ───────────────────────

#[cfg(unix)]
fn get_disk_free_mb(path: &Path) -> Option<u32> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;

    #[repr(C)]
    struct Statvfs {
        f_bsize: u64,
        f_frsize: u64,
        f_blocks: u64,
        f_bfree: u64,
        f_bavail: u64,
        _pad: [u8; 48], // enough padding for both macOS and Linux structs
    }

    // Use the actual statvfs syscall via the libc convention.
    // On macOS the function is `statvfs`; on Linux it's also `statvfs`.
    extern "C" {
        fn statvfs(path: *const std::os::raw::c_char, buf: *mut Statvfs) -> std::os::raw::c_int;
    }

    let mut stat = Statvfs {
        f_bsize: 0, f_frsize: 0, f_blocks: 0, f_bfree: 0, f_bavail: 0, _pad: [0; 48],
    };

    let rc = unsafe { statvfs(c_path.as_ptr(), &mut stat) };
    if rc == 0 && stat.f_frsize > 0 {
        let free_bytes = stat.f_bavail.saturating_mul(stat.f_frsize);
        Some((free_bytes / (1024 * 1024)).min(u32::MAX as u64) as u32)
    } else {
        None
    }
}

#[cfg(not(unix))]
fn get_disk_free_mb(_path: &Path) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    #[test]
    fn disk_free_mb_returns_value_on_unix() {
        #[cfg(unix)]
        {
            let result = get_disk_free_mb(Path::new("/tmp"));
            // On any Unix system /tmp exists and has some free space.
            assert!(result.is_some(), "statvfs on /tmp should succeed");
            assert!(result.unwrap() > 0);
        }
    }
}
