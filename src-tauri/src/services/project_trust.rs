//! Whether Keynobi may run a project's own build code.
//!
//! Running `gradlew` executes the project's wrapper and build scripts, so a
//! freshly cloned repository must not run them until the user trusts it. Trust
//! is stored on the project's registry entry, keyed by its canonical root.
//! Every Gradle spawn path (GUI builds, variant discovery, MCP builds) checks it
//! through [`require_trusted`].

use crate::models::settings::AppSettings;
use crate::services::settings_manager;
use std::path::{Path, PathBuf};

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The trust decision for the project at `project_root`: `Some(true)` trusted,
/// `Some(false)` Safe Mode, `None` unknown or never asked.
///
/// A registry entry for the same folder decides. Otherwise the folder is the
/// Gradle root of registered projects (an app opened from a subfolder, or a
/// headless server started at the build root): any Safe Mode entry wins, then
/// any trusted one, since they all run this folder's `gradlew`.
pub fn trust_in(settings: &AppSettings, project_root: &Path) -> Option<bool> {
    let root = canonical(project_root);
    if let Some(entry) = settings
        .recent_projects
        .iter()
        .find(|e| canonical(Path::new(&e.path)) == root)
    {
        return entry.trusted;
    }
    let builds_here: Vec<Option<bool>> = settings
        .recent_projects
        .iter()
        .filter(|e| {
            e.gradle_root
                .as_deref()
                .is_some_and(|g| canonical(Path::new(g)) == root)
        })
        .map(|e| e.trusted)
        .collect();
    if builds_here.contains(&Some(false)) {
        Some(false)
    } else if builds_here.contains(&Some(true)) {
        Some(true)
    } else {
        None
    }
}

pub fn is_trusted(settings: &AppSettings, project_root: &Path) -> bool {
    trust_in(settings, project_root) == Some(true)
}

/// Refuse to run the project's build code unless the user trusted it.
pub fn require_trusted(settings: &AppSettings, project_root: &Path) -> Result<(), String> {
    if is_trusted(settings, project_root) {
        return Ok(());
    }
    Err(format!(
        "This project is not trusted, so Keynobi will not run its Gradle build scripts ({}). \
         Open the project in the Keynobi app and choose Trust, or right-click it in the \
         Projects sidebar and choose Trust Project, then try again.",
        project_root.display()
    ))
}

/// Record the user's trust decision for the registry entry `id`. Returns
/// `false` when no entry has that id.
pub fn set_trust(id: &str, trusted: bool) -> Result<bool, String> {
    settings_manager::mutate_settings_with_result(|settings| {
        let entry = settings.recent_projects.iter_mut().find(|e| e.id == id);
        let found = entry.is_some();
        if let Some(entry) = entry {
            entry.trusted = Some(trusted);
        }
        Ok(found)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::ProjectEntry;

    fn entry(path: &Path, gradle_root: Option<&Path>, trusted: Option<bool>) -> ProjectEntry {
        ProjectEntry {
            id: path.to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            gradle_root: gradle_root.map(|g| g.to_string_lossy().into_owned()),
            trusted,
            ..ProjectEntry::default()
        }
    }

    fn settings_with(entries: Vec<ProjectEntry>) -> AppSettings {
        AppSettings {
            recent_projects: entries,
            ..AppSettings::default()
        }
    }

    #[test]
    fn an_unknown_project_is_not_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let settings = AppSettings::default();
        assert_eq!(trust_in(&settings, dir.path()), None);
        let err = require_trusted(&settings, dir.path()).unwrap_err();
        assert!(err.contains("choose Trust"), "{err}");
    }

    #[test]
    fn the_registry_entry_for_the_folder_decides() {
        let dir = tempfile::tempdir().unwrap();
        for trusted in [None, Some(false), Some(true)] {
            let settings = settings_with(vec![entry(dir.path(), None, trusted)]);
            assert_eq!(trust_in(&settings, dir.path()), trusted);
            assert_eq!(
                require_trusted(&settings, dir.path()).is_ok(),
                trusted == Some(true)
            );
        }
    }

    #[test]
    fn paths_are_compared_canonically() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(project.join("app")).unwrap();
        let settings = settings_with(vec![entry(&project, None, Some(true))]);

        let dotted = project.join("app").join("..");
        assert!(is_trusted(&settings, &dotted));

        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&project, &link).unwrap();
        assert!(is_trusted(&settings, &link));
    }

    #[test]
    fn a_symlink_to_another_folder_is_not_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let trusted = dir.path().join("trusted");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&trusted).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let link = trusted.join("link");
        std::os::unix::fs::symlink(&other, &link).unwrap();
        let settings = settings_with(vec![entry(&trusted, None, Some(true))]);

        assert!(!is_trusted(&settings, &link));
        assert!(!is_trusted(&settings, &other));
    }

    #[test]
    fn a_gradle_root_follows_the_projects_that_build_from_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let app = root.join("app");
        let lib = root.join("lib");

        let settings = settings_with(vec![entry(&app, Some(&root), Some(true))]);
        assert!(is_trusted(&settings, &root));

        let settings = settings_with(vec![
            entry(&app, Some(&root), Some(true)),
            entry(&lib, Some(&root), Some(false)),
        ]);
        assert_eq!(trust_in(&settings, &root), Some(false), "a revoke wins");

        let settings = settings_with(vec![
            entry(&root, Some(&root), Some(false)),
            entry(&app, Some(&root), Some(true)),
        ]);
        assert_eq!(
            trust_in(&settings, &root),
            Some(false),
            "an entry for the folder itself decides"
        );
    }
}
