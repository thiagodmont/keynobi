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

/// Validate that an APK path resolves inside `{root}/app/build/outputs`.
///
/// Unlike [`validate_within_root`], this accepts absolute paths because APK
/// paths returned by build discovery are absolute. Canonicalization still
/// enforces the project/build-output boundary and catches symlink escapes.
pub fn validate_apk_within_build_outputs(
    root: &Path,
    untrusted: impl AsRef<Path>,
) -> Result<PathBuf, AppError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::io(root.display(), e))?;
    let build_outputs = canonical_root.join("app").join("build").join("outputs");
    let canonical_outputs = build_outputs
        .canonicalize()
        .map_err(|_| AppError::NotFound("Build outputs directory not found".to_string()))?;

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
}
