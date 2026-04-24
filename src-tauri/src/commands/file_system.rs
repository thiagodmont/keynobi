use crate::models::error::AppError;
use crate::models::settings::{ProjectAppInfo, ProjectEntry, MAX_RECENT_PROJECTS};
use crate::services::{fs_manager, settings_manager};
use crate::FsState;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::LazyLock;
use tauri::State;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a short deterministic hex ID from an absolute path.
fn project_id(path: &std::path::Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Upsert a `ProjectEntry` into `settings.recent_projects` and persist.
/// Evicts the oldest non-pinned entry when the list exceeds `MAX_RECENT_PROJECTS`.
fn upsert_project(path: &std::path::Path, gradle_root: Option<&std::path::Path>) {
    let (mut settings, _) = settings_manager::load_settings();

    let id = project_id(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let path_str = path.to_string_lossy().to_string();
    let gradle_root_str = gradle_root.map(|p| p.to_string_lossy().to_string());
    let now = chrono::Utc::now().to_rfc3339();

    // Update existing entry or insert new one.
    if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
        entry.last_opened = now;
        entry.gradle_root = gradle_root_str;
        entry.name = name;
    } else {
        settings.recent_projects.push(ProjectEntry {
            id,
            path: path_str.clone(),
            name,
            gradle_root: gradle_root_str,
            last_opened: now,
            pinned: false,
            last_build_variant: None,
            last_device: None,
        });

        // Evict oldest non-pinned entries when over the cap.
        while settings.recent_projects.len() > MAX_RECENT_PROJECTS {
            // Find the index of the oldest non-pinned entry.
            let evict_idx = settings
                .recent_projects
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.pinned)
                .min_by_key(|(_, e)| e.last_opened.clone())
                .map(|(i, _)| i);
            if let Some(idx) = evict_idx {
                settings.recent_projects.remove(idx);
            } else {
                break; // All are pinned — keep them all.
            }
        }
    }

    settings.last_active_project = Some(path_str);

    if let Err(e) = settings_manager::save_settings(&settings) {
        tracing::warn!("Failed to persist project registry: {e}");
    }
}

// ── Project open / switch ─────────────────────────────────────────────────────

/// Open an Android project folder and detect the Gradle root.
/// Upserts the project into the recent-projects registry.
/// Returns the detected project name on success.
#[tauri::command]
pub async fn open_project(
    app_handle: tauri::AppHandle,
    path: String,
    state: State<'_, FsState>,
) -> Result<String, AppError> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(AppError::NotFound(format!(
            "Directory does not exist: {path}"
        )));
    }
    if !root.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "Path is not a directory: {path}"
        )));
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::Io(format!("Failed to canonicalize path: {e}")))?;

    let gradle_root = fs_manager::find_gradle_root(&canonical_root);
    if let Some(ref gr) = gradle_root {
        tracing::info!(
            "Gradle root detected: {} (opened: {})",
            gr.display(),
            canonical_root.display()
        );
    } else {
        tracing::info!(
            "No Gradle root found above {}; using it as workspace root",
            canonical_root.display()
        );
    }

    let project_name = canonical_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    // Persist before holding the state lock.
    upsert_project(&canonical_root, gradle_root.as_deref());

    // Register the opened project directory as an allowed fs scope so the
    // frontend can read project files. This is tighter than home-recursive.
    use tauri_plugin_fs::FsExt;
    if let Some(scope) = app_handle.try_fs_scope() {
        if let Err(e) = scope.allow_directory(&canonical_root, true) {
            tracing::warn!("Failed to register project fs scope: {e}");
        }
    }

    let mut guard = state.0.lock().await;
    guard.project_root = Some(canonical_root);
    guard.gradle_root = gradle_root;

    Ok(project_name)
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard
        .project_root
        .as_ref()
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn get_gradle_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard
        .gradle_root
        .as_ref()
        .map(|p| p.to_string_lossy().to_string()))
}

/// Try to read the `applicationId` from the app-level build.gradle(.kts).
/// Called once on project open so the frontend can resolve `package:mine`.
#[tauri::command]
pub async fn get_application_id(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    let root = guard
        .gradle_root
        .as_ref()
        .or(guard.project_root.as_ref())
        .cloned();
    drop(guard);

    let Some(root) = root else { return Ok(None) };

    let candidates = [
        root.join("app").join("build.gradle.kts"),
        root.join("app").join("build.gradle"),
        root.join("build.gradle.kts"),
        root.join("build.gradle"),
    ];

    for path in &candidates {
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Some(id) = extract_application_id(&content) {
                    return Ok(Some(id));
                }
            }
        }
    }
    Ok(None)
}

static RE_APPLICATION_ID: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"applicationId\s*=?\s*"([^"]+)""#)
        .expect("RE_APPLICATION_ID: invalid regex")
});

fn extract_application_id(content: &str) -> Option<String> {
    // Matches: applicationId "com.example" or applicationId = "com.example"
    let caps = RE_APPLICATION_ID.captures(content)?;
    Some(caps.get(1)?.as_str().to_owned())
}

// ── Project registry commands ─────────────────────────────────────────────────

/// Return the full recent-projects list sorted: pinned first, then by
/// `last_opened` descending (most recent first).
#[tauri::command]
pub async fn list_projects() -> Result<Vec<ProjectEntry>, String> {
    let (settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    let mut projects = settings.recent_projects;
    projects.sort_by(|a, b| {
        // Pinned entries come first; within each group sort by last_opened desc.
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.last_opened.cmp(&a.last_opened))
    });

    Ok(projects)
}

/// Remove a project entry from the registry by its ID.
/// Does *not* delete the project from disk.
#[tauri::command]
pub async fn remove_project(id: String) -> Result<(), String> {
    let (mut settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    settings.recent_projects.retain(|e| e.id != id);

    // Clear last_active_project if it was the removed one.
    if let Some(ref last) = settings.last_active_project.clone() {
        let still_exists = settings.recent_projects.iter().any(|e| &e.path == last);
        if !still_exists {
            settings.last_active_project = None;
        }
    }

    tokio::task::spawn_blocking(move || settings_manager::save_settings(&settings))
        .await
        .map_err(|e| format!("Failed to save settings: {e}"))?
}

/// Toggle the `pinned` flag for a project entry.
#[tauri::command]
pub async fn pin_project(id: String, pinned: bool) -> Result<(), String> {
    let (mut settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
        entry.pinned = pinned;
    }

    tokio::task::spawn_blocking(move || settings_manager::save_settings(&settings))
        .await
        .map_err(|e| format!("Failed to save settings: {e}"))?
}

/// Return the path of the last-active project (used on startup to restore the session).
#[tauri::command]
pub async fn get_last_active_project() -> Result<Option<String>, String> {
    let (settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;
    Ok(settings.last_active_project)
}

// ── Project App Info ──────────────────────────────────────────────────────────

/// Read `applicationId`, `versionName`, and `versionCode` from the
/// app-level `build.gradle(.kts)`.
#[tauri::command]
pub async fn get_project_app_info(state: State<'_, FsState>) -> Result<ProjectAppInfo, String> {
    let guard = state.0.lock().await;
    let root = guard
        .gradle_root
        .as_ref()
        .or(guard.project_root.as_ref())
        .cloned();
    drop(guard);

    let Some(root) = root else {
        return Ok(ProjectAppInfo {
            application_id: None,
            version_name: None,
            version_code: None,
        });
    };

    let candidates = [
        root.join("app").join("build.gradle.kts"),
        root.join("app").join("build.gradle"),
        root.join("build.gradle.kts"),
        root.join("build.gradle"),
    ];

    for path in &candidates {
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(path) {
                return Ok(ProjectAppInfo {
                    application_id: extract_application_id(&content),
                    version_name: extract_version_name(&content),
                    version_code: extract_version_code(&content),
                });
            }
        }
    }

    Ok(ProjectAppInfo {
        application_id: None,
        version_name: None,
        version_code: None,
    })
}

/// Write `versionName` and `versionCode` back to the app-level
/// `build.gradle(.kts)` using regex replacement.
#[tauri::command]
pub async fn save_project_app_info(
    version_name: String,
    version_code: i64,
    state: State<'_, FsState>,
) -> Result<(), String> {
    let guard = state.0.lock().await;
    let root = guard
        .gradle_root
        .as_ref()
        .or(guard.project_root.as_ref())
        .cloned();
    drop(guard);

    let root = root.ok_or_else(|| "No project is open".to_string())?;

    let candidates = [
        root.join("app").join("build.gradle.kts"),
        root.join("app").join("build.gradle"),
        root.join("build.gradle.kts"),
        root.join("build.gradle"),
    ];

    for path in &candidates {
        if path.is_file() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

            let updated = replace_version_name(&content, &version_name);
            let updated = replace_version_code(&updated, version_code);

            // Atomic write: write to temp file then rename.
            let tmp = path.with_extension("gradle.tmp");
            std::fs::write(&tmp, &updated)
                .map_err(|e| format!("Failed to write temp file: {e}"))?;
            std::fs::rename(&tmp, path).map_err(|e| format!("Failed to save gradle file: {e}"))?;

            return Ok(());
        }
    }

    Err("No build.gradle(.kts) file found in the project".to_string())
}

// ── Regex helpers ─────────────────────────────────────────────────────────────

static RE_VERSION_NAME: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(versionName\s*=?\s*)"[^"]*""#).expect("RE_VERSION_NAME: invalid regex")
});

static RE_VERSION_CODE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(versionCode\s*=?\s*)\d+").expect("RE_VERSION_CODE: invalid regex")
});

static RE_EXTRACT_VERSION_NAME: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"versionName\s*=?\s*"([^"]+)""#)
        .expect("RE_EXTRACT_VERSION_NAME: invalid regex")
});

fn extract_version_name(content: &str) -> Option<String> {
    let caps = RE_EXTRACT_VERSION_NAME.captures(content)?;
    Some(caps.get(1)?.as_str().to_owned())
}

static RE_EXTRACT_VERSION_CODE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"versionCode\s*=?\s*(\d+)").expect("RE_EXTRACT_VERSION_CODE: invalid regex")
});

fn extract_version_code(content: &str) -> Option<i64> {
    let caps = RE_EXTRACT_VERSION_CODE.captures(content)?;
    caps.get(1)?.as_str().parse().ok()
}

fn replace_version_name(content: &str, new_value: &str) -> String {
    RE_VERSION_NAME
        .replace(content, |caps: &regex::Captures| {
            format!("{}\"{}\"", &caps[1], new_value)
        })
        .to_string()
}

fn replace_version_code(content: &str, new_value: i64) -> String {
    RE_VERSION_CODE
        .replace(content, |caps: &regex::Captures| {
            format!("{}{}", &caps[1], new_value)
        })
        .to_string()
}

// ── Per-project meta persistence ──────────────────────────────────────────────

/// Persist per-project variant and device selections back into the registry.
/// Called from the frontend whenever the user changes variant or device.
#[tauri::command]
pub async fn update_project_meta(
    id: String,
    last_build_variant: Option<String>,
    last_device: Option<String>,
) -> Result<(), String> {
    let (mut settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
        entry.last_build_variant = last_build_variant;
        entry.last_device = last_device;
    }

    tokio::task::spawn_blocking(move || settings_manager::save_settings(&settings))
        .await
        .map_err(|e| format!("Failed to save settings: {e}"))?
}

/// Rename the display name of a project in the registry.
/// Does NOT rename the directory on disk.
#[tauri::command]
pub async fn rename_project(id: String, new_name: String) -> Result<(), String> {
    let new_name = new_name.trim().to_owned();
    if new_name.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }

    let (mut settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
        entry.name = new_name;
    } else {
        return Err(format!("Project with id '{id}' not found"));
    }

    tokio::task::spawn_blocking(move || settings_manager::save_settings(&settings))
        .await
        .map_err(|e| format!("Failed to save settings: {e}"))?
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn empty_path_would_fail_is_dir_check() {
        // Verify that an empty path doesn't exist as a directory
        let path = PathBuf::from("");
        assert!(!path.is_dir(), "empty path should not be a directory");
    }

    #[test]
    fn nonexistent_path_would_fail_exists_check() {
        let path = PathBuf::from("/this/path/definitely/does/not/exist/on/any/machine/12345");
        assert!(!path.exists(), "clearly nonexistent path should not exist");
    }

    #[test]
    fn valid_temp_dir_would_pass_checks() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();
        assert!(path.exists(), "tempdir must exist");
        assert!(path.is_dir(), "tempdir must be a directory");
    }
}
