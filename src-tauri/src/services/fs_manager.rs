use std::path::{Path, PathBuf};

// ── Gradle root detection ─────────────────────────────────────────────────────

/// Walk upward from `start` looking for the nearest ancestor that contains
/// `settings.gradle` or `settings.gradle.kts` — the canonical marker for a
/// Gradle project root.  Returns `None` if no Gradle root is found within
/// `MAX_GRADLE_SEARCH_DEPTH` levels.
const MAX_GRADLE_SEARCH_DEPTH: usize = 10;

pub fn find_gradle_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };

    for _ in 0..MAX_GRADLE_SEARCH_DEPTH {
        if current.join("settings.gradle").is_file()
            || current.join("settings.gradle.kts").is_file()
        {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_temp_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("app/src/main/kotlin/com/example")).unwrap();
        fs::write(
            root.join("app/src/main/kotlin/com/example/Main.kt"),
            "fun main() {}",
        )
        .unwrap();
        fs::write(
            root.join("settings.gradle.kts"),
            "rootProject.name = \"example\"",
        )
        .unwrap();
        dir
    }

    #[test]
    fn finds_gradle_root_with_settings_gradle_kts() {
        let dir = make_temp_project();
        let result = find_gradle_root(dir.path());
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn finds_gradle_root_with_settings_gradle() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.gradle"), "rootProject.name = \"test\"").unwrap();
        let result = find_gradle_root(dir.path());
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn finds_gradle_root_from_subfolder() {
        let dir = make_temp_project();
        let module = dir.path().join("app");
        let result = find_gradle_root(&module);
        assert_eq!(
            result,
            Some(dir.path().to_path_buf()),
            "should walk up from app/ to find settings.gradle.kts at root"
        );
    }

    #[test]
    fn returns_none_for_non_gradle_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let result = find_gradle_root(dir.path());
        assert_eq!(result, None, "no settings.gradle means no Gradle root");
    }

    #[test]
    fn finds_nearest_gradle_root_not_parent() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.gradle.kts"), "outer").unwrap();
        let inner = dir.path().join("inner");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("settings.gradle.kts"), "inner").unwrap();
        let module = inner.join("app");
        fs::create_dir_all(&module).unwrap();

        let result = find_gradle_root(&module);
        assert_eq!(
            result,
            Some(inner.clone()),
            "should find the nearest (inner) Gradle root, not the outer one"
        );
    }
}
