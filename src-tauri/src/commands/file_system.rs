use crate::models::settings::{ProjectAppInfo, ProjectEntry, MAX_RECENT_PROJECTS};
use crate::services::{fs_manager, settings_manager};
use crate::FsState;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
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
    let mut settings = settings_manager::load_settings();

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
    path: String,
    state: State<'_, FsState>,
) -> Result<String, String> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {path}"));
    }

    let gradle_root = fs_manager::find_gradle_root(&root);
    if let Some(ref gr) = gradle_root {
        tracing::info!(
            "Gradle root detected: {} (opened: {})",
            gr.display(),
            root.display()
        );
    } else {
        tracing::info!(
            "No Gradle root found above {}; using it as workspace root",
            root.display()
        );
    }

    let project_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    // Persist before holding the state lock.
    upsert_project(&root, gradle_root.as_deref());

    let mut guard = state.0.lock().await;
    guard.project_root = Some(root);
    guard.gradle_root = gradle_root;

    Ok(project_name)
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard.project_root.as_ref().map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn get_gradle_root(state: State<'_, FsState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().await;
    Ok(guard.gradle_root.as_ref().map(|p| p.to_string_lossy().to_string()))
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

fn extract_application_id(content: &str) -> Option<String> {
    // Matches: applicationId "com.example" or applicationId = "com.example"
    let re = regex::Regex::new(r#"applicationId\s*=?\s*"([^"]+)""#).ok()?;
    let caps = re.captures(content)?;
    Some(caps.get(1)?.as_str().to_owned())
}

// ── Project registry commands ─────────────────────────────────────────────────

/// Return the full recent-projects list sorted: pinned first, then by
/// `last_opened` descending (most recent first).
#[tauri::command]
pub async fn list_projects() -> Result<Vec<ProjectEntry>, String> {
    let settings = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    let mut projects = settings.recent_projects;
    projects.sort_by(|a, b| {
        // Pinned entries come first; within each group sort by last_opened desc.
        b.pinned.cmp(&a.pinned)
            .then_with(|| b.last_opened.cmp(&a.last_opened))
    });

    Ok(projects)
}

/// Remove a project entry from the registry by its ID.
/// Does *not* delete the project from disk.
#[tauri::command]
pub async fn remove_project(id: String) -> Result<(), String> {
    let mut settings = tokio::task::spawn_blocking(settings_manager::load_settings)
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
    let mut settings = tokio::task::spawn_blocking(settings_manager::load_settings)
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
    let settings = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;
    Ok(settings.last_active_project)
}

// ── Project App Info ──────────────────────────────────────────────────────────

/// Read `applicationId`, `versionName`, and `versionCode` from the
/// app-level `build.gradle(.kts)`.
#[tauri::command]
pub async fn get_project_app_info(
    state: State<'_, FsState>,
) -> Result<ProjectAppInfo, String> {
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
            std::fs::rename(&tmp, path)
                .map_err(|e| format!("Failed to save gradle file: {e}"))?;

            return Ok(());
        }
    }

    Err("No build.gradle(.kts) file found in the project".to_string())
}

// ── Regex helpers ─────────────────────────────────────────────────────────────

fn extract_version_name(content: &str) -> Option<String> {
    let re = regex::Regex::new(r#"versionName\s*=?\s*"([^"]+)""#).ok()?;
    let caps = re.captures(content)?;
    Some(caps.get(1)?.as_str().to_owned())
}

fn extract_version_code(content: &str) -> Option<i64> {
    let re = regex::Regex::new(r"versionCode\s*=?\s*(\d+)").ok()?;
    let caps = re.captures(content)?;
    caps.get(1)?.as_str().parse().ok()
}

fn replace_version_name(content: &str, new_value: &str) -> String {
    let re = regex::Regex::new(r#"(versionName\s*=?\s*)"[^"]*""#).unwrap();
    re.replace(content, |caps: &regex::Captures| {
        format!("{}\"{}\"", &caps[1], new_value)
    })
    .to_string()
}

fn replace_version_code(content: &str, new_value: i64) -> String {
    let re = regex::Regex::new(r"(versionCode\s*=?\s*)\d+").unwrap();
    re.replace(content, |caps: &regex::Captures| {
        format!("{}{}", &caps[1], new_value)
    })
    .to_string()
}
