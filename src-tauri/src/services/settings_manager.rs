use crate::models::settings::AppSettings;
use std::path::PathBuf;

const KNOWN_SETTINGS_FIELDS: &[&str] = &[
    "appearance", "search", "android", "lsp", "java",
    "advanced", "build", "logcat", "mcp", "telemetry", "onboardingCompleted",
    "recentProjects", "lastActiveProject",
];

/// Log a warning for each top-level key in the JSON that isn't a known settings field.
/// This catches typos early (e.g. "fontSizee" silently ignored by serde(default)).
fn log_unknown_settings_fields(value: &serde_json::Value) {
    if let Some(obj) = value.as_object() {
        for key in obj.keys() {
            if !KNOWN_SETTINGS_FIELDS.contains(&key.as_str()) {
                tracing::warn!(
                    "Unknown key in settings.json ignored: '{}' (possible typo?)",
                    key
                );
            }
        }
    }
}

fn settings_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".keynobi")
}

/// Public accessor for the `~/.keynobi/` data directory.
/// Used by `mcp_activity` and other services that need to store files alongside settings.
pub fn data_dir() -> PathBuf {
    settings_dir()
}

fn settings_file() -> PathBuf {
    settings_dir().join("settings.json")
}

fn load_settings_from_path(path: &std::path::Path) -> (AppSettings, bool) {
    if !path.exists() {
        return (AppSettings::default(), false);
    }
    match std::fs::read_to_string(path) {
        Ok(content) => {
            // Detect and warn about unknown/misspelled top-level fields.
            if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&content) {
                log_unknown_settings_fields(&raw);
            }
            // Parse for actual use (serde(default) handles forward compat).
            match serde_json::from_str::<AppSettings>(&content) {
                Ok(mut settings) => {
                    crate::models::settings::normalize_logcat_section(&mut settings.logcat);
                    (settings, false)
                }
                Err(e) => {
                    tracing::warn!("Settings file is corrupted (using defaults): {e}");
                    (AppSettings::default(), true)
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to read settings file: {e}");
            (AppSettings::default(), false)
        }
    }
}

/// Returns `(settings, was_corrupted)`.
/// `was_corrupted` is true when the file existed but failed to parse —
/// the user's settings were lost and replaced with defaults.
pub fn load_settings() -> (AppSettings, bool) {
    load_settings_from_path(&settings_file())
}

/// Save settings to disk atomically (temp file + rename).
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let dir = settings_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create settings directory: {e}"))?;

    let mut normalized = settings.clone();
    crate::models::settings::normalize_logcat_section(&mut normalized.logcat);

    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    let path = settings_file();
    let tmp = path.with_extension("json.tmp");

    std::fs::write(&tmp, &json)
        .map_err(|e| format!("Failed to write settings: {e}"))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    Ok(())
}

/// Read `last_build_variant` for the given project path from a specific settings file.
/// Matches by `ProjectEntry.path` or `ProjectEntry.gradle_root`.
/// Used directly in unit tests; production code uses `get_active_variant_for_project`.
pub fn get_active_variant_for_project_path(
    settings_path: &std::path::Path,
    project_path: &str,
) -> Option<String> {
    let (settings, _) = load_settings_from_path(settings_path);
    settings
        .recent_projects
        .iter()
        .find(|e| {
            e.path == project_path
                || e.gradle_root.as_deref() == Some(project_path)
        })
        .and_then(|e| e.last_build_variant.clone())
}

/// Persist `variant` as `last_build_variant` for the given project path.
/// No-op (no error) if the project is not found in recent_projects.
pub fn set_active_variant_for_project_path(
    settings_path: &std::path::Path,
    project_path: &str,
    variant: &str,
) -> Result<(), String> {
    let (mut settings, _) = load_settings_from_path(settings_path);
    if let Some(entry) = settings.recent_projects.iter_mut().find(|e| {
        e.path == project_path || e.gradle_root.as_deref() == Some(project_path)
    }) {
        entry.last_build_variant = Some(variant.to_string());
        let json = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {e}"))?;
        std::fs::write(settings_path, json)
            .map_err(|e| format!("Failed to write settings: {e}"))?;
    }
    Ok(())
}

/// Production wrapper: get active variant from the default settings file.
pub fn get_active_variant_for_project(project_path: &str) -> Option<String> {
    get_active_variant_for_project_path(&settings_file(), project_path)
}

/// Production wrapper: set active variant in the default settings file.
pub fn set_active_variant_for_project(project_path: &str, variant: &str) -> Result<(), String> {
    set_active_variant_for_project_path(&settings_file(), project_path, variant)
}

/// Delete the settings file and return defaults.
pub fn reset_settings() -> Result<AppSettings, String> {
    let path = settings_file();
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete settings file: {e}"))?;
    }
    Ok(AppSettings::default())
}

/// Try to find the Android SDK on this machine.
/// Checks process environment first (fast), then falls back to the macOS
/// default Android Studio location.
pub fn detect_android_sdk() -> Option<String> {
    if let Ok(sdk) = std::env::var("ANDROID_HOME") {
        let p = PathBuf::from(&sdk);
        if p.is_dir() {
            return Some(sdk);
        }
    }
    if let Ok(sdk) = std::env::var("ANDROID_SDK_ROOT") {
        let p = PathBuf::from(&sdk);
        if p.is_dir() {
            return Some(sdk);
        }
    }
    let home = dirs::home_dir()?;
    let default_path = home.join("Library/Android/sdk");
    if default_path.is_dir() {
        return Some(default_path.to_string_lossy().to_string());
    }
    None
}

/// Spawn a login shell to read `ANDROID_HOME` / `ANDROID_SDK_ROOT` from the
/// user's profile (`.zshrc`, `.bash_profile`, etc.).
///
/// On macOS, GUI apps launched from Finder or the Dock do **not** inherit
/// the user's shell environment, so `std::env::var` returns nothing for
/// variables set only in shell config files.  Spawning a login shell is the
/// standard workaround.
pub async fn detect_android_sdk_from_shell() -> Option<String> {
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            let cmd = format!("echo ${var}");
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(4),
                tokio::process::Command::new(&shell)
                    .args(["-l", "-c", &cmd])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output(),
            )
            .await;

            if let Ok(Ok(out)) = result {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path.is_empty() && PathBuf::from(&path).is_dir() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Try to find a JDK installation on this machine.
/// Checks process environment first, then macOS `/Library/Java/`.
pub fn detect_java_home() -> Option<String> {
    if let Ok(java) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(&java);
        if p.is_dir() {
            return Some(java);
        }
    }
    let jvm_dir = PathBuf::from("/Library/Java/JavaVirtualMachines");
    if jvm_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&jvm_dir) {
            for entry in entries.flatten() {
                let home = entry.path().join("Contents/Home");
                if home.is_dir() {
                    return Some(home.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Spawn a login shell to read `JAVA_HOME` from the user's profile.
/// Same rationale as `detect_android_sdk_from_shell`.
pub async fn detect_java_home_from_shell() -> Option<String> {
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            tokio::process::Command::new(&shell)
                .args(["-l", "-c", "echo $JAVA_HOME"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output(),
        )
        .await;

        if let Ok(Ok(out)) = result {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() && PathBuf::from(&path).is_dir() {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_defaults_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        // File does not exist — must return defaults.
        let (settings, _) = load_settings_from_path(&path);
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");

        let mut settings = AppSettings::default();
        settings.appearance.ui_font_size = 14;
        settings.search.context_lines = 5;

        let json = serde_json::to_string_pretty(&settings).unwrap();
        std::fs::write(&path, &json).unwrap();

        let loaded: AppSettings = serde_json::from_str(
            &std::fs::read_to_string(&path).unwrap()
        ).unwrap();
        assert_eq!(loaded.appearance.ui_font_size, 14);
        assert_eq!(loaded.search.context_lines, 5);
    }

    #[test]
    fn corrupt_json_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "not valid json!!!").unwrap();

        let result: AppSettings = serde_json::from_str(
            &std::fs::read_to_string(&path).unwrap()
        ).unwrap_or_default();
        assert_eq!(result, AppSettings::default());
    }

    #[test]
    fn load_settings_from_path_detects_corruption() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not valid json !!!").unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(corrupted, "should report corruption");
        assert_eq!(settings, AppSettings::default(), "should return defaults on corruption");
    }

    #[test]
    fn load_settings_from_path_no_corruption_flag_on_valid_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        std::fs::write(&path, &json).unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "valid file should not report corruption");
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn load_settings_from_path_no_corruption_flag_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "missing file is not corruption");
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn detect_sdk_returns_option() {
        let result = detect_android_sdk();
        // Can't guarantee SDK is installed in CI, but verify it doesn't panic
        if let Some(path) = result {
            assert!(!path.is_empty());
        }
    }

    #[test]
    fn detect_java_returns_option() {
        let result = detect_java_home();
        if let Some(path) = result {
            assert!(!path.is_empty());
        }
    }

    #[test]
    fn unknown_top_level_field_triggers_no_corruption_flag() {
        // Writing a file with an unknown key should:
        // 1. Not be treated as corruption (bool = false)
        // 2. Still load the valid fields correctly
        // The warning is logged via tracing::warn — not directly assertable here,
        // but the return value must be correct.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"appearance": {"uiFontSize": 14}, "unknownKey": true}"#).unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "unknown field must not be treated as corruption");
        assert_eq!(settings.appearance.ui_font_size, 14, "valid field must be loaded despite unknown key");
    }

    #[test]
    fn all_known_fields_pass_validation() {
        let known = ["appearance", "search", "android", "lsp", "java",
                     "advanced", "build", "logcat", "mcp", "telemetry", "onboardingCompleted",
                     "recentProjects", "lastActiveProject"];
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = value.as_object().unwrap();
        let unknown: Vec<&str> = obj.keys()
            .filter(|k| !known.contains(&k.as_str()))
            .map(|k| k.as_str())
            .collect();
        assert!(unknown.is_empty(), "default settings should have no unknown fields, got: {unknown:?}");
    }

    #[test]
    fn load_settings_from_path_with_unknown_field_still_loads_valid_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        // Write settings with a typo field + a valid field
        std::fs::write(&path, r#"{"appearance": {"uiFontSize": 14}, "uiFontSizee": 20}"#).unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "misspelled field is not corruption");
        assert_eq!(settings.appearance.ui_font_size, 14, "valid field must be loaded");
        // The typo "fontSizee" is silently ignored by serde(default) —
        // our helper only logs a warning, it doesn't change the return value.
    }
}

#[cfg(test)]
mod variant_tests {
    use super::*;
    use std::fs;

    fn write_settings(path: &std::path::Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn set_and_get_active_variant_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let initial = serde_json::json!({
            "lastActiveProject": "/proj/myapp",
            "recentProjects": [{
                "id": "abc123",
                "path": "/proj/myapp",
                "name": "myapp",
                "gradleRoot": null,
                "lastOpened": "2026-01-01T00:00:00Z",
                "pinned": false,
                "lastBuildVariant": null,
                "lastDevice": null
            }]
        });
        write_settings(&settings_path, &initial.to_string());

        set_active_variant_for_project_path(&settings_path, "/proj/myapp", "demoDebug").unwrap();
        let result = get_active_variant_for_project_path(&settings_path, "/proj/myapp");
        assert_eq!(result, Some("demoDebug".to_string()));
    }

    #[test]
    fn get_active_variant_returns_none_for_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        write_settings(&settings_path, "{}");
        let result = get_active_variant_for_project_path(&settings_path, "/proj/unknown");
        assert_eq!(result, None);
    }

    #[test]
    fn set_active_variant_is_noop_for_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        write_settings(&settings_path, "{}");
        let result = set_active_variant_for_project_path(&settings_path, "/proj/unknown", "debug");
        assert!(result.is_ok());
    }
}
