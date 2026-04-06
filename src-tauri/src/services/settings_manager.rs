use crate::models::settings::AppSettings;
use std::path::PathBuf;

const KNOWN_SETTINGS_FIELDS: &[&str] = &[
    "editor", "appearance", "search", "android", "lsp", "java",
    "advanced", "build", "logcat", "mcp", "recentProjects", "lastActiveProject",
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
                Ok(settings) => (settings, false),
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

    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    let path = settings_file();
    let tmp = path.with_extension("json.tmp");

    std::fs::write(&tmp, &json)
        .map_err(|e| format!("Failed to write settings: {e}"))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    Ok(())
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
        settings.editor.font_size = 18;
        settings.editor.tab_size = 2;

        let json = serde_json::to_string_pretty(&settings).unwrap();
        std::fs::write(&path, &json).unwrap();

        let loaded: AppSettings = serde_json::from_str(
            &std::fs::read_to_string(&path).unwrap()
        ).unwrap();
        assert_eq!(loaded.editor.font_size, 18);
        assert_eq!(loaded.editor.tab_size, 2);
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
        std::fs::write(&path, r#"{"editor": {"fontSize": 20}, "unknownKey": true}"#).unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "unknown field must not be treated as corruption");
        assert_eq!(settings.editor.font_size, 20, "valid field must be loaded despite unknown key");
    }

    #[test]
    fn all_known_fields_pass_validation() {
        let known = ["editor", "appearance", "search", "android", "lsp", "java",
                     "advanced", "build", "logcat", "mcp", "recentProjects", "lastActiveProject"];
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
        std::fs::write(&path, r#"{"editor": {"fontSize": 18}, "fontSizee": 20}"#).unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "misspelled field is not corruption");
        assert_eq!(settings.editor.font_size, 18, "valid field must be loaded");
        // The typo "fontSizee" is silently ignored by serde(default) —
        // our helper only logs a warning, it doesn't change the return value.
    }
}
