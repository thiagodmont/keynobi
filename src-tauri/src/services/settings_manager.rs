use crate::models::settings::AppSettings;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex as StdMutex};

const KNOWN_SETTINGS_FIELDS: &[&str] = &[
    "appearance",
    "search",
    "android",
    "lsp",
    "java",
    "advanced",
    "build",
    "logcat",
    "mcp",
    "telemetry",
    "onboardingCompleted",
    "recentProjects",
    "lastActiveProject",
];

static SETTINGS_MUTATION_LOCK: LazyLock<StdMutex<()>> = LazyLock::new(|| StdMutex::new(()));

/// Process-wide cache for the default settings file.
///
/// `load_settings()` has ~67 call sites, several on hot async paths
/// (`run_gradle_task`, `start_logcat`, `record_build_result`), and each one was
/// doing a synchronous read + full JSON parse on a tokio worker thread. Every
/// mutation path invalidates this.
static SETTINGS_CACHE: LazyLock<std::sync::RwLock<Option<AppSettings>>> =
    LazyLock::new(|| std::sync::RwLock::new(None));

/// Drop the cached settings so the next `load_settings()` re-reads from disk.
/// Must be called by every path that writes the default settings file.
pub fn invalidate_settings_cache() {
    match SETTINGS_CACHE.write() {
        Ok(mut g) => *g = None,
        Err(poisoned) => *poisoned.into_inner() = None,
    }
}

fn cached_settings() -> Option<AppSettings> {
    match SETTINGS_CACHE.read() {
        Ok(g) => g.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn store_settings_in_cache(settings: &AppSettings) {
    match SETTINGS_CACHE.write() {
        Ok(mut g) => *g = Some(settings.clone()),
        Err(poisoned) => *poisoned.into_inner() = Some(settings.clone()),
    }
}

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

fn repair_corrupt_settings_file(path: &std::path::Path, defaults: &AppSettings) {
    let backup = path.with_extension("json.corrupt");
    if let Err(e) = std::fs::rename(path, &backup) {
        tracing::warn!("Failed to move corrupted settings file aside: {e}");
    }
    if let Err(e) = save_settings_to_path(path, defaults, None) {
        tracing::warn!("Failed to write default settings after corruption: {e}");
    }
    invalidate_settings_cache();
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
                    let defaults = AppSettings::default();
                    repair_corrupt_settings_file(path, &defaults);
                    (defaults, true)
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
    // `was_corrupted` is deliberately NOT cached: it is a one-shot startup
    // signal, and a corrupt file is repaired on the first load, so subsequent
    // reads are legitimately "not corrupted".
    if let Some(settings) = cached_settings() {
        return (settings, false);
    }
    let (settings, corrupted) = load_settings_from_path(&settings_file());
    store_settings_in_cache(&settings);
    (settings, corrupted)
}

/// Save settings to disk atomically (temp file + rename).
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let dir = settings_dir();
    let path = settings_file();
    let _guard = SETTINGS_MUTATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = save_settings_to_path(&path, settings, Some(&dir));
    invalidate_settings_cache();
    result
}

fn save_settings_to_path(
    path: &std::path::Path,
    settings: &AppSettings,
    dir_override: Option<&std::path::Path>,
) -> Result<(), String> {
    let dir = match dir_override {
        Some(dir) => dir.to_path_buf(),
        None => path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create settings directory: {e}"))?;

    let mut normalized = settings.clone();
    crate::models::settings::normalize_logcat_section(&mut normalized.logcat);

    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    let tmp = path.with_extension("json.tmp");

    std::fs::write(&tmp, &json).map_err(|e| format!("Failed to write settings: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Failed to save settings: {e}"))?;

    Ok(())
}

pub fn mutate_settings_at_path(
    settings_path: &std::path::Path,
    mutate: impl FnOnce(&mut AppSettings),
) -> Result<(), String> {
    mutate_settings_at_path_with_result(settings_path, |settings| {
        mutate(settings);
        Ok(())
    })
}

pub fn mutate_settings_at_path_with_result<R>(
    settings_path: &std::path::Path,
    mutate: impl FnOnce(&mut AppSettings) -> Result<R, String>,
) -> Result<R, String> {
    let _guard = SETTINGS_MUTATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (mut settings, _) = load_settings_from_path(settings_path);
    let result = mutate(&mut settings)?;
    save_settings_to_path(settings_path, &settings, None)?;
    // Invalidate unconditionally: this helper is also used with explicit paths
    // in tests, and over-invalidating only costs one re-read.
    invalidate_settings_cache();
    Ok(result)
}

pub fn mutate_settings(mutate: impl FnOnce(&mut AppSettings)) -> Result<(), String> {
    mutate_settings_at_path(&settings_file(), mutate)
}

pub fn mutate_settings_with_result<R>(
    mutate: impl FnOnce(&mut AppSettings) -> Result<R, String>,
) -> Result<R, String> {
    mutate_settings_at_path_with_result(&settings_file(), mutate)
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
        .find(|e| e.path == project_path || e.gradle_root.as_deref() == Some(project_path))
        .and_then(|e| e.last_build_variant.clone())
}

/// Persist `variant` as `last_build_variant` for the given project path.
/// No-op (no error) if the project is not found in recent_projects.
pub fn set_active_variant_for_project_path(
    settings_path: &std::path::Path,
    project_path: &str,
    variant: &str,
) -> Result<(), String> {
    mutate_settings_at_path(settings_path, |settings| {
        if let Some(entry) = settings
            .recent_projects
            .iter_mut()
            .find(|e| e.path == project_path || e.gradle_root.as_deref() == Some(project_path))
        {
            entry.last_build_variant = Some(variant.to_string());
        }
    })
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
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete settings file: {e}"))?;
    }
    invalidate_settings_cache();
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

        let loaded: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.appearance.ui_font_size, 14);
        assert_eq!(loaded.search.context_lines, 5);
    }

    #[test]
    fn corrupt_json_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "not valid json!!!").unwrap();

        let result: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap_or_default();
        assert_eq!(result, AppSettings::default());
    }

    #[test]
    fn load_settings_from_path_detects_corruption() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not valid json !!!").unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(corrupted, "should report corruption");
        assert_eq!(
            settings,
            AppSettings::default(),
            "should return defaults on corruption"
        );
    }

    #[test]
    fn load_settings_from_path_repairs_corrupt_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not valid json !!!").unwrap();

        let (_, corrupted) = load_settings_from_path(&path);

        assert!(corrupted, "first load should report corruption");
        assert!(
            path.with_extension("json.corrupt").exists(),
            "corrupt settings file should be moved aside"
        );
        let repaired: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(repaired, AppSettings::default());

        let (_, corrupted_again) = load_settings_from_path(&path);
        assert!(
            !corrupted_again,
            "repaired settings file should load cleanly next time"
        );
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
    fn load_settings_from_path_normalizes_extreme_logcat_values() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"logcat":{"ringMaxEntries":500000,"maxUiLines":200000,"autoStart":true}}"#,
        )
        .unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted);
        assert_eq!(
            settings.logcat.ring_max_entries,
            crate::models::settings::LOGCAT_RING_ABS_MAX
        );
        assert_eq!(
            settings.logcat.max_ui_lines,
            crate::models::settings::LOGCAT_RING_ABS_MAX
        );
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
        std::fs::write(
            &path,
            r#"{"appearance": {"uiFontSize": 14}, "unknownKey": true}"#,
        )
        .unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(
            !corrupted,
            "unknown field must not be treated as corruption"
        );
        assert_eq!(
            settings.appearance.ui_font_size, 14,
            "valid field must be loaded despite unknown key"
        );
    }

    #[test]
    fn all_known_fields_pass_validation() {
        let known = [
            "appearance",
            "search",
            "android",
            "lsp",
            "java",
            "advanced",
            "build",
            "logcat",
            "mcp",
            "telemetry",
            "onboardingCompleted",
            "recentProjects",
            "lastActiveProject",
        ];
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = value.as_object().unwrap();
        let unknown: Vec<&str> = obj
            .keys()
            .filter(|k| !known.contains(&k.as_str()))
            .map(|k| k.as_str())
            .collect();
        assert!(
            unknown.is_empty(),
            "default settings should have no unknown fields, got: {unknown:?}"
        );
    }

    #[test]
    fn load_settings_from_path_with_unknown_field_still_loads_valid_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        // Write settings with a typo field + a valid field
        std::fs::write(
            &path,
            r#"{"appearance": {"uiFontSize": 14}, "uiFontSizee": 20}"#,
        )
        .unwrap();
        let (settings, corrupted) = load_settings_from_path(&path);
        assert!(!corrupted, "misspelled field is not corruption");
        assert_eq!(
            settings.appearance.ui_font_size, 14,
            "valid field must be loaded"
        );
        // The typo "fontSizee" is silently ignored by serde(default) —
        // our helper only logs a warning, it doesn't change the return value.
    }
}

#[cfg(test)]
mod variant_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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

    #[test]
    fn settings_mutations_are_serialized() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        write_settings(&settings_path, "{}");

        let first_path = settings_path.clone();
        let second_path = settings_path.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (second_entered_tx, second_entered_rx) = mpsc::channel();

        let first = std::thread::spawn(move || {
            mutate_settings_at_path(&first_path, |settings| {
                settings.onboarding_completed = true;
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .unwrap();
        });

        entered_rx.recv().unwrap();

        let second = std::thread::spawn(move || {
            mutate_settings_at_path(&second_path, |settings| {
                settings.last_active_project = Some("/proj/second".to_string());
                second_entered_tx.send(()).unwrap();
            })
            .unwrap();
        });

        assert!(
            second_entered_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "second mutation must wait for the first mutation to finish"
        );

        release_tx.send(()).unwrap();
        second_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        first.join().unwrap();
        second.join().unwrap();

        let (settings, _) = load_settings_from_path(&settings_path);
        assert!(settings.onboarding_completed);
        assert_eq!(
            settings.last_active_project.as_deref(),
            Some("/proj/second")
        );
    }

    // ── Settings cache (M13) ─────────────────────────────────────────────────

    #[test]
    fn invalidate_settings_cache_forces_a_reread() {
        // The cache keys off the default settings file, so exercise it through
        // the public API and assert the invalidation contract directly.
        invalidate_settings_cache();
        assert!(
            cached_settings().is_none(),
            "cache must be empty after invalidation"
        );

        let sample = AppSettings::default();
        store_settings_in_cache(&sample);
        assert!(
            cached_settings().is_some(),
            "cache must hold the stored value"
        );

        invalidate_settings_cache();
        assert!(
            cached_settings().is_none(),
            "every mutation path must clear the cache"
        );
    }

    #[test]
    fn mutate_settings_at_path_invalidates_the_cache() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        store_settings_in_cache(&AppSettings::default());
        mutate_settings_at_path(&path, |s| {
            s.onboarding_completed = true;
        })
        .unwrap();

        assert!(
            cached_settings().is_none(),
            "a mutation must not leave a stale cached copy behind"
        );
    }

    #[test]
    fn load_settings_from_path_is_unaffected_by_the_cache() {
        // Explicit-path loads must always hit disk — tests and the corruption
        // repair path depend on it.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let mut written = AppSettings::default();
        written.onboarding_completed = true;
        save_settings_to_path(&path, &written, None).unwrap();

        store_settings_in_cache(&AppSettings::default());

        let (loaded, _) = load_settings_from_path(&path);
        assert!(
            loaded.onboarding_completed,
            "load_settings_from_path must read the file, not the cache"
        );
    }
}
