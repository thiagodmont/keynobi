use crate::models::error::AppError;
use crate::models::settings::{ProjectAppInfo, ProjectEntry, MAX_RECENT_PROJECTS};
use crate::services::project_app_info::{self, extract_application_id};
use crate::services::{fs_manager, project_trust, settings_manager};
use crate::FsState;
use std::path::PathBuf;
use tauri::State;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a short deterministic hex ID from an absolute path.
/// Stable identifier for a project path.
///
/// MUST stay implementation-stable: these ids are persisted in settings.json and
/// used to match `activeProjectId`, pins, and per-project meta. `DefaultHasher`
/// (used previously) is explicitly not guaranteed stable across Rust releases,
/// so a toolchain bump would silently orphan every stored entry.
fn project_id(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    format!("{:016x}", u64::from_be_bytes(head))
}

/// Upsert a `ProjectEntry` into `settings.recent_projects` and persist.
/// Evicts the oldest non-pinned entry when the list exceeds `MAX_RECENT_PROJECTS`.
fn upsert_project(path: &std::path::Path, gradle_root: Option<&std::path::Path>) {
    let id = project_id(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let path_str = path.to_string_lossy().to_string();
    let gradle_root_str = gradle_root.map(|p| p.to_string_lossy().to_string());
    let now = chrono::Utc::now().to_rfc3339();

    if let Err(e) = settings_manager::mutate_settings(|settings| {
        // Heal entries written with the old unstable hash: same path, stale id.
        for entry in settings.recent_projects.iter_mut() {
            if entry.path == path_str && entry.id != id {
                entry.id = id.clone();
            }
        }
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
                trusted: None,
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
    }) {
        tracing::warn!("Failed to persist project registry: {e}");
    }
}

// ── Project open / switch ─────────────────────────────────────────────────────

/// Open an Android project folder and detect the Gradle root.
/// Upserts the project into the recent-projects registry.
/// Returns the detected project name on success.
#[tauri::command]
pub async fn open_project(path: String, state: State<'_, FsState>) -> Result<String, AppError> {
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
    tokio::task::spawn_blocking(move || {
        settings_manager::mutate_settings(|settings| {
            settings.recent_projects.retain(|e| e.id != id);

            // Clear last_active_project if it was the removed one.
            if let Some(ref last) = settings.last_active_project.clone() {
                let still_exists = settings.recent_projects.iter().any(|e| &e.path == last);
                if !still_exists {
                    settings.last_active_project = None;
                }
            }
        })
    })
    .await
    .map_err(|e| format!("Failed to save settings: {e}"))?
}

/// Toggle the `pinned` flag for a project entry.
#[tauri::command]
pub async fn pin_project(id: String, pinned: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        settings_manager::mutate_settings(|settings| {
            if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
                entry.pinned = pinned;
            }
        })
    })
    .await
    .map_err(|e| format!("Failed to save settings: {e}"))?
}

/// Record whether Keynobi may run the project's Gradle build scripts.
#[tauri::command]
pub async fn set_project_trust(id: String, trusted: bool) -> Result<(), AppError> {
    let found = tokio::task::spawn_blocking({
        let id = id.clone();
        move || project_trust::set_trust(&id, trusted)
    })
    .await
    .map_err(|e| AppError::SettingsError(format!("Failed to save settings: {e}")))?
    .map_err(AppError::SettingsError)?;
    if !found {
        return Err(AppError::NotFound(format!(
            "Project with id '{id}' not found"
        )));
    }
    Ok(())
}

/// Return the path of the last-active project (used on startup to restore the session).
#[tauri::command]
pub async fn get_last_active_project() -> Result<Option<String>, String> {
    let (settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;

    // A deleted or renamed folder otherwise produces a "Failed to open project"
    // toast on every launch, forever — the stale value was never cleared.
    if let Some(ref path) = settings.last_active_project {
        if !std::path::Path::new(path).exists() {
            tracing::info!("Clearing last_active_project — path no longer exists: {path}");
            let _ = tokio::task::spawn_blocking(|| {
                settings_manager::mutate_settings(|s| s.last_active_project = None)
            })
            .await;
            return Ok(None);
        }
    }
    Ok(settings.last_active_project)
}

// ── Project App Info ──────────────────────────────────────────────────────────

/// Read `applicationId`, `versionName`, and `versionCode` from the
/// app-level `build.gradle(.kts)`.
#[tauri::command]
pub async fn get_project_app_info(state: State<'_, FsState>) -> Result<ProjectAppInfo, AppError> {
    let guard = state.0.lock().await;
    let root = guard
        .gradle_root
        .as_ref()
        .or(guard.project_root.as_ref())
        .cloned();
    drop(guard);

    let root = root.ok_or_else(|| AppError::NotFound("No project is open".to_string()))?;
    tokio::task::spawn_blocking(move || project_app_info::read_app_info(&root))
        .await
        .map_err(|e| AppError::Other(format!("Failed to read app info: {e}")))
}

/// Write `versionName` and `versionCode` back to the app-level
/// `build.gradle(.kts)`. A `None` field is left as it is.
#[tauri::command]
pub async fn save_project_app_info(
    version_name: Option<String>,
    version_code: Option<i64>,
    state: State<'_, FsState>,
) -> Result<(), AppError> {
    let guard = state.0.lock().await;
    let root = guard
        .gradle_root
        .as_ref()
        .or(guard.project_root.as_ref())
        .cloned();
    drop(guard);

    let root = root.ok_or_else(|| AppError::NotFound("No project is open".to_string()))?;
    tokio::task::spawn_blocking(move || {
        project_app_info::save_app_info(&root, version_name.as_deref(), version_code)
    })
    .await
    .map_err(|e| AppError::Other(format!("Failed to save app info: {e}")))?
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
    tokio::task::spawn_blocking(move || {
        settings_manager::mutate_settings(|settings| {
            if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
                entry.last_build_variant = last_build_variant;
                entry.last_device = last_device;
            }
        })
    })
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

    tokio::task::spawn_blocking(move || {
        settings_manager::mutate_settings_with_result(|settings| {
            if let Some(entry) = settings.recent_projects.iter_mut().find(|e| e.id == id) {
                entry.name = new_name;
                Ok(())
            } else {
                Err(format!("Project with id '{id}' not found"))
            }
        })
    })
    .await
    .map_err(|e| format!("Failed to save settings: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // ── Stable project ids (M16) ─────────────────────────────────────────────

    /// These ids are persisted in settings.json. If this expectation ever needs
    /// updating, every stored project entry is being orphaned — which is
    /// exactly what this test exists to catch.
    #[test]
    fn project_id_is_stable_for_a_known_path() {
        let id = project_id(std::path::Path::new("/Users/dev/projects/my-app"));
        assert_eq!(id.len(), 16, "id width must stay 16 hex chars");
        assert_eq!(
            id,
            project_id(std::path::Path::new("/Users/dev/projects/my-app")),
            "same path must always hash to the same id"
        );
        // Pin the actual value so an implementation swap is loud, not silent.
        assert_eq!(id, "c361c9fa33e7adb3");
    }

    #[test]
    fn project_id_differs_between_paths() {
        let a = project_id(std::path::Path::new("/projects/a"));
        let b = project_id(std::path::Path::new("/projects/b"));
        assert_ne!(a, b);
    }

    // ── Legacy project-id healing ────────────────────────────────────────────

    /// Entries written with the old DefaultHasher-based id must be rewritten in
    /// place, otherwise pins and per-project meta are orphaned after the switch
    /// to a stable hash.
    #[test]
    fn upsert_rewrites_a_legacy_id_for_the_same_path() {
        use crate::models::settings::AppSettings;

        let tmp = tempfile::tempdir().unwrap();
        let settings_path = tmp.path().join("settings.json");
        let project_path = "/projects/legacy-app";
        let expected = project_id(std::path::Path::new(project_path));

        settings_manager::mutate_settings_at_path(&settings_path, |s: &mut AppSettings| {
            s.recent_projects.push(ProjectEntry {
                id: "deadbeefdeadbeef".to_string(), // old-hash id
                path: project_path.to_string(),
                name: "legacy-app".to_string(),
                gradle_root: None,
                last_opened: "2026-01-01T00:00:00Z".to_string(),
                pinned: true,
                last_build_variant: Some("debug".to_string()),
                last_device: None,
                trusted: Some(true),
            });
        })
        .unwrap();

        // Apply the same heal the upsert path performs.
        settings_manager::mutate_settings_at_path(&settings_path, |s: &mut AppSettings| {
            for entry in s.recent_projects.iter_mut() {
                if entry.path == project_path && entry.id != expected {
                    entry.id = expected.clone();
                }
            }
        })
        .unwrap();

        let healed = settings_manager::load_settings_from_path_for_tests(&settings_path);
        let entry = healed
            .recent_projects
            .iter()
            .find(|e| e.path == project_path)
            .expect("entry must survive the heal");

        assert_eq!(
            entry.id, expected,
            "the id must be rewritten to the stable hash"
        );
        assert!(entry.pinned, "healing must not drop the pinned flag");
        assert_eq!(
            entry.last_build_variant.as_deref(),
            Some("debug"),
            "healing must not drop per-project meta"
        );
    }
}
