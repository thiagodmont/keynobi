use crate::FsState;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Open the given source file at a specific line in Android Studio.
///
/// Parameters
/// ----------
/// - `class_path`  – fully-qualified Java/Kotlin package, e.g. `com.example.app`
/// - `filename`    – source file name extracted from a stack frame, e.g. `MainActivity.kt`
/// - `line`        – 1-based line number
///
/// The command resolves the absolute path by searching the open project directory for a file
/// whose path ends with `<class_dir>/<filename>` (preferred) or just `<filename>` as a
/// fallback.  Once found it invokes `studio --line <line> <abs_path>` via a login shell so
/// that macOS users whose PATH is set in `.zshrc` / `.zprofile` have the `studio` binary
/// resolved correctly.
#[tauri::command]
pub async fn open_in_studio(
    fs_state: tauri::State<'_, FsState>,
    class_path: String,
    filename: String,
    line: u32,
) -> Result<String, String> {
    // ── Resolve project root ───────────────────────────────────────────────────
    let project_root: PathBuf = {
        let fs = fs_state.0.lock().await;
        fs.project_root
            .clone()
            .ok_or_else(|| "No project is open".to_string())?
    };

    // ── Validate inputs ────────────────────────────────────────────────────────
    // Filename must not contain path separators — it comes from a stack frame
    // like `(MainActivity.kt:42)` and should always be a bare filename.
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err(format!("Invalid filename: {filename}"));
    }
    if filename.is_empty() {
        return Err("filename must not be empty".to_string());
    }

    // ── Build the expected directory suffix from the class path ────────────────
    // `com.example.app` → `com/example/app`
    let class_dir_suffix: PathBuf = class_path
        .split('.')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(std::path::MAIN_SEPARATOR_STR)
        .into();

    // ── Search the project for the source file ─────────────────────────────────
    let found_path = find_source_file(&project_root, &class_dir_suffix, &filename)?;

    // ── Security: ensure the resolved path is inside the project root ──────────
    let canonical_root = project_root
        .canonicalize()
        .map_err(|e| format!("Cannot canonicalize project root: {e}"))?;
    let canonical_file = found_path
        .canonicalize()
        .map_err(|e| format!("Cannot canonicalize resolved path: {e}"))?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(format!(
            "Resolved path is outside the project root: {}",
            found_path.display()
        ));
    }

    let abs_path = canonical_file.to_string_lossy().into_owned();

    // ── Invoke studio ──────────────────────────────────────────────────────────
    // Use a login shell so macOS users' custom PATH (set in .zshrc / .zprofile)
    // is available.  The command is: studio --line <line> <path>
    let shell_cmd = format!("studio --line {line} '{abs_path}'");
    let status = tokio::process::Command::new("sh")
        .args(["-lc", &shell_cmd])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| format!("Failed to launch studio: {e}"))?;

    if !status.success() {
        return Err(format!(
            "studio exited with status {status}. Make sure the `studio` command is on your PATH \
             (see the Health Panel for setup instructions)."
        ));
    }

    Ok(abs_path)
}

/// Walk the project tree to find a source file.
///
/// Strategy (in priority order):
/// 1. Match an entry whose path ends with `<class_dir>/<filename>`.
/// 2. If nothing is found, match any entry whose name equals `filename`.
///
/// Returns the first match or an error.
fn find_source_file(
    project_root: &PathBuf,
    class_dir_suffix: &PathBuf,
    filename: &str,
) -> Result<PathBuf, String> {
    // Build the suffix we expect: "com/example/app/MainActivity.kt"
    let ideal_suffix = class_dir_suffix.join(filename);

    let mut fallback: Option<PathBuf> = None;

    for entry in WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        // Use filter_entry to prune build/hidden *directories* before descending.
        // This checks only the entry's own name (last component), not the full
        // absolute path, so it never accidentally matches parent directories
        // that might be named similarly.
        // Depth 0 is the root entry itself — never prune it because its name is
        // outside our control (e.g. macOS TempDir creates dirs like `.tmpXXXXXX`).
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                return name != "build"
                    && name != ".gradle"
                    && name != ".idea"
                    && name != ".git"
                    && !name.starts_with('.');
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        if path.ends_with(&ideal_suffix) {
            return Ok(path.to_path_buf());
        }

        if fallback.is_none() {
            if let Some(name) = path.file_name() {
                if name.to_string_lossy() == filename {
                    fallback = Some(path.to_path_buf());
                }
            }
        }
    }

    fallback.ok_or_else(|| {
        format!(
            "Source file `{filename}` not found in the project. \
             Make sure the project is open."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_file(dir: &std::path::Path, rel: &str) -> PathBuf {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"// test").unwrap();
        p
    }

    #[test]
    fn finds_file_by_class_path() {
        let tmp = TempDir::new().unwrap();
        make_file(
            tmp.path(),
            "app/src/main/java/com/example/app/MainActivity.kt",
        );
        let root = tmp.path().to_path_buf();
        let suffix: PathBuf = "com/example/app".into();
        let found = find_source_file(&root, &suffix, "MainActivity.kt").unwrap();
        assert!(found.ends_with("com/example/app/MainActivity.kt"));
    }

    #[test]
    fn falls_back_to_filename_only() {
        let tmp = TempDir::new().unwrap();
        // File is in an unexpected location — class path won't match.
        make_file(tmp.path(), "app/src/main/java/other/place/MainActivity.kt");
        let root = tmp.path().to_path_buf();
        let suffix: PathBuf = "com/example/app".into();
        let found = find_source_file(&root, &suffix, "MainActivity.kt").unwrap();
        assert!(found.to_string_lossy().ends_with("MainActivity.kt"));
    }

    #[test]
    fn returns_error_when_not_found() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let suffix: PathBuf = "com/example/app".into();
        let result = find_source_file(&root, &suffix, "Missing.kt");
        assert!(result.is_err());
    }

    #[test]
    fn skips_build_directories() {
        let tmp = TempDir::new().unwrap();
        // The "correct" file is in build/ — should be pruned.
        make_file(
            tmp.path(),
            "app/build/intermediates/com/example/app/MainActivity.kt",
        );
        // The real source file is in src/
        make_file(
            tmp.path(),
            "app/src/main/java/com/example/app/MainActivity.kt",
        );
        let root = tmp.path().to_path_buf();
        let suffix: PathBuf = "com/example/app".into();
        let found = find_source_file(&root, &suffix, "MainActivity.kt").unwrap();
        let found_str = found.to_string_lossy();
        assert!(
            !found_str.contains("/build/"),
            "Should not return build path, got: {found_str}"
        );
        assert!(
            found_str.ends_with("com/example/app/MainActivity.kt"),
            "Expected src path, got: {found_str}"
        );
    }
}
