use crate::models::error::AppError;
use std::path::{Path, PathBuf};

/// Resolve `untrusted` relative to `root` and verify it stays within `root`.
///
/// Returns the canonical absolute path on success.
/// Returns `AppError::PermissionDenied` if the path escapes the root.
/// Returns `AppError::NotFound` if the path doesn't exist.
pub fn validate_within_root(root: &Path, untrusted: &str) -> Result<PathBuf, AppError> {
    use std::path::Component;

    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::io(root.display(), e))?;

    // Perform a lexical traversal check before hitting the filesystem.
    // Walk each component of the untrusted string: if we ever see a `..`
    // that would pop us above the root (depth == 0), reject immediately.
    let mut depth: i64 = 0;
    for component in Path::new(untrusted).components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(AppError::PermissionDenied(format!(
                        "'{untrusted}' is outside the project root"
                    )));
                }
            }
            Component::Normal(_) => depth += 1,
            Component::RootDir | Component::Prefix(_) => {
                // Absolute paths are unconditionally rejected — they bypass
                // the root entirely.
                return Err(AppError::PermissionDenied(format!(
                    "'{untrusted}' is outside the project root"
                )));
            }
            Component::CurDir => {}
        }
    }

    let candidate = canonical_root.join(untrusted);
    let canonical_file = candidate
        .canonicalize()
        .map_err(|_| AppError::NotFound(format!("Path not found: {untrusted}")))?;

    // Double-check with canonical paths to catch symlink escapes.
    if !canonical_file.starts_with(&canonical_root) {
        return Err(AppError::PermissionDenied(format!(
            "'{}' is outside the project root",
            canonical_file.display()
        )));
    }

    Ok(canonical_file)
}

/// Resolve a fixed project file, such as `app/build.gradle.kts`, and require
/// the canonical file to be a regular file inside the canonical `root`.
///
/// Symlinks inside the project are followed; one that leads outside it (at the
/// file or at any directory above it) is `PermissionDenied`.
pub fn resolve_project_file(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let file = validate_within_root(root, relative)?;
    if !file.is_file() {
        return Err(AppError::NotFound(format!("Not a file: {relative}")));
    }
    Ok(file)
}

/// Validate that an APK path resolves inside `{root}/app/build/outputs`, and
/// that the outputs directory itself resolves inside `root`.
///
/// Unlike [`validate_within_root`], this accepts absolute paths because APK
/// paths returned by build discovery are absolute. Canonicalization still
/// enforces the project/build-output boundary and catches symlink escapes.
/// Install the returned canonical path, not `untrusted`.
pub fn validate_apk_within_build_outputs(
    root: &Path,
    untrusted: impl AsRef<Path>,
) -> Result<PathBuf, AppError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::io(root.display(), e))?;
    let build_outputs = canonical_root.join("app").join("build").join("outputs");
    let canonical_outputs = build_outputs.canonicalize().map_err(|_| {
        AppError::NotFound(
            "Build outputs directory (app/build/outputs) not found. Run a build first.".to_string(),
        )
    })?;
    // A symlinked `app`, `app/build`, or `app/build/outputs` must not move the
    // boundary outside the project.
    if !canonical_outputs.starts_with(&canonical_root) {
        return Err(AppError::PermissionDenied(
            "app/build/outputs resolves outside the project".to_string(),
        ));
    }

    let untrusted = untrusted.as_ref();
    let canonical_apk = untrusted
        .canonicalize()
        .map_err(|_| AppError::NotFound(format!("APK path not found: {}", untrusted.display())))?;

    if !canonical_apk.starts_with(&canonical_outputs) {
        return Err(AppError::PermissionDenied(
            "APK path must be within app/build/outputs".to_string(),
        ));
    }

    if canonical_apk.extension().and_then(|e| e.to_str()) != Some("apk") {
        return Err(AppError::InvalidInput(
            "Path must point to a .apk file".to_string(),
        ));
    }

    Ok(canonical_apk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rejects_traversal_outside_root() {
        let tmp = TempDir::new().unwrap();
        let result = validate_within_root(tmp.path(), "../etc/passwd");
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::PermissionDenied(_) => {}
            e => panic!("expected PermissionDenied, got {e:?}"),
        }
    }

    #[test]
    fn accepts_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Main.kt"), b"// test").unwrap();
        let result = validate_within_root(tmp.path(), "Main.kt");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("Main.kt"));
    }

    #[test]
    fn rejects_nonexistent_file() {
        let tmp = TempDir::new().unwrap();
        let result = validate_within_root(tmp.path(), "nonexistent.kt");
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(_) => {}
            e => panic!("expected NotFound for missing file, got {e:?}"),
        }
    }

    #[test]
    fn accepts_nested_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/main")).unwrap();
        std::fs::write(tmp.path().join("src/main/Foo.kt"), b"// test").unwrap();
        let result = validate_within_root(tmp.path(), "src/main/Foo.kt");
        assert!(result.is_ok());
    }

    #[test]
    fn apk_validation_accepts_apk_inside_build_outputs() {
        let tmp = TempDir::new().unwrap();
        let apk = tmp.path().join("app/build/outputs/apk/debug/app-debug.apk");
        std::fs::create_dir_all(apk.parent().unwrap()).unwrap();
        std::fs::write(&apk, b"apk").unwrap();

        let result = validate_apk_within_build_outputs(tmp.path(), &apk).unwrap();

        assert_eq!(result, apk.canonicalize().unwrap());
    }

    #[test]
    fn apk_validation_rejects_path_outside_build_outputs() {
        let tmp = TempDir::new().unwrap();
        let apk = tmp.path().join("outside.apk");
        std::fs::write(&apk, b"apk").unwrap();
        std::fs::create_dir_all(tmp.path().join("app/build/outputs")).unwrap();

        let result = validate_apk_within_build_outputs(tmp.path(), &apk);

        assert!(matches!(result, Err(AppError::PermissionDenied(_))));
    }

    #[test]
    fn apk_validation_rejects_non_apk_extension() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("app/build/outputs/apk/debug/app-debug.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"not apk").unwrap();

        let result = validate_apk_within_build_outputs(tmp.path(), &file);

        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }

    #[cfg(unix)]
    #[test]
    fn apk_validation_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let tmp = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();
        let outside_apk = outside_dir.path().join("outside.apk");
        std::fs::write(&outside_apk, b"apk").unwrap();
        let link = tmp.path().join("app/build/outputs/apk/debug/link.apk");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&outside_apk, &link).unwrap();

        let result = validate_apk_within_build_outputs(tmp.path(), &link);

        assert!(matches!(result, Err(AppError::PermissionDenied(_))));
    }

    fn write_file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    /// For each directory above the APK, a project whose directory is a
    /// symlink to an outside tree holding the rest of the outputs layout.
    #[cfg(unix)]
    #[test]
    fn apk_validation_rejects_an_ancestor_linked_outside_the_project() {
        use std::os::unix::fs::symlink;

        for (linked, rest) in [
            ("app", "build/outputs/apk/debug/app-debug.apk"),
            ("app/build", "outputs/apk/debug/app-debug.apk"),
            ("app/build/outputs", "apk/debug/app-debug.apk"),
        ] {
            let project = TempDir::new().unwrap();
            let outside = TempDir::new().unwrap();
            write_file(&outside.path().join(rest));
            let link = project.path().join(linked);
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            symlink(outside.path(), &link).unwrap();
            let apk = project
                .path()
                .join("app/build/outputs/apk/debug/app-debug.apk");
            assert!(apk.is_file(), "{linked}: setup");

            let result = validate_apk_within_build_outputs(project.path(), &apk);

            assert!(
                matches!(result, Err(AppError::PermissionDenied(_))),
                "{linked}: {result:?}"
            );
        }
    }

    #[test]
    fn apk_validation_rejects_dot_dot_traversal() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("app/build/outputs/apk")).unwrap();
        write_file(&tmp.path().join("outside.apk"));
        let apk = tmp
            .path()
            .join("app/build/outputs/apk/../../../../outside.apk");

        let result = validate_apk_within_build_outputs(tmp.path(), &apk);

        assert!(matches!(result, Err(AppError::PermissionDenied(_))));
    }

    #[cfg(unix)]
    #[test]
    fn apk_validation_follows_symlinks_that_stay_inside_the_project() {
        use std::os::unix::fs::symlink;

        let tmp = TempDir::new().unwrap();
        let real_build = tmp.path().join("build-cache/app");
        let apk = real_build.join("outputs/apk/debug/app-debug.apk");
        write_file(&apk);
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();
        symlink(&real_build, tmp.path().join("app/build")).unwrap();
        let latest = tmp.path().join("app/build/outputs/apk/debug/latest.apk");
        symlink("app-debug.apk", &latest).unwrap();

        let result = validate_apk_within_build_outputs(tmp.path(), &latest).unwrap();

        assert_eq!(result, apk.canonicalize().unwrap());
    }

    #[test]
    fn project_file_resolves_to_the_canonical_file() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("app/build.gradle.kts"));

        let file = resolve_project_file(tmp.path(), "app/build.gradle.kts").unwrap();

        assert_eq!(
            file,
            tmp.path()
                .join("app/build.gradle.kts")
                .canonicalize()
                .unwrap()
        );
    }

    #[test]
    fn project_file_rejects_traversal_and_directories() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();

        assert!(matches!(
            resolve_project_file(tmp.path(), "../outside.kts"),
            Err(AppError::PermissionDenied(_))
        ));
        assert!(matches!(
            resolve_project_file(tmp.path(), "app"),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            resolve_project_file(tmp.path(), "missing.kts"),
            Err(AppError::NotFound(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn project_file_rejects_a_symlink_outside_the_project() {
        use std::os::unix::fs::symlink;

        let project = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        write_file(&outside.path().join("secret"));
        write_file(&outside.path().join("app/build.gradle.kts"));
        symlink(
            outside.path().join("secret"),
            project.path().join("build.gradle.kts"),
        )
        .unwrap();
        symlink(outside.path().join("app"), project.path().join("app")).unwrap();

        for relative in ["build.gradle.kts", "app/build.gradle.kts"] {
            let result = resolve_project_file(project.path(), relative);
            assert!(
                matches!(result, Err(AppError::PermissionDenied(_))),
                "{relative}: {result:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn project_file_follows_a_symlink_inside_the_project() {
        use std::os::unix::fs::symlink;

        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("gradle/app.gradle.kts"));
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();
        symlink(
            "../gradle/app.gradle.kts",
            tmp.path().join("app/build.gradle.kts"),
        )
        .unwrap();

        let file = resolve_project_file(tmp.path(), "app/build.gradle.kts").unwrap();

        assert_eq!(
            file,
            tmp.path()
                .join("gradle/app.gradle.kts")
                .canonicalize()
                .unwrap()
        );
    }
}
