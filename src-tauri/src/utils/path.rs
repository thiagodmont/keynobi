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
    }

    #[test]
    fn accepts_nested_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/main")).unwrap();
        std::fs::write(tmp.path().join("src/main/Foo.kt"), b"// test").unwrap();
        let result = validate_within_root(tmp.path(), "src/main/Foo.kt");
        assert!(result.is_ok());
    }
}
