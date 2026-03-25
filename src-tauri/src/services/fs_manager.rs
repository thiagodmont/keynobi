use crate::models::{FileEvent, FileEventKind, FileKind, FileNode, FsError};
use ignore::WalkBuilder;
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tracing::{debug, error, warn};

/// Maximum file size the editor will load (10 MiB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_FILE_SIZE_MB: u64 = 10;

const HARDCODED_EXCLUDES: &[&str] = &[
    "build",
    ".gradle",
    ".idea",
    ".git",
    "node_modules",
    ".DS_Store",
];

const EXCLUDED_EXTENSIONS: &[&str] = &["class", "dex", "apk", "aar"];

/// Convenience type alias for the debouncer we use throughout the codebase.
pub type FsWatcher = Debouncer<RecommendedWatcher, FileIdMap>;

/// Returns `true` if a file or directory name should never appear in the tree.
///
/// Checks both hard-coded directory names (build, .gradle, etc.) and
/// file extensions that represent compiled Android artefacts.
pub(crate) fn is_excluded(name: &str) -> bool {
    if HARDCODED_EXCLUDES.contains(&name) {
        return true;
    }
    // Extension check: look at the part after the last '.'
    if let Some(ext) = name.rsplit('.').next() {
        if name != ext && EXCLUDED_EXTENSIONS.contains(&ext) {
            // guard: `name != ext` avoids treating dotfiles with no extension (e.g.
            // ".gitignore") as having extension "gitignore"
            return true;
        }
    }
    false
}

/// Returns `true` if any path component matches the exclusion list.
fn is_path_excluded(path: &Path) -> bool {
    path.components()
        .any(|c| is_excluded(c.as_os_str().to_string_lossy().as_ref()))
}

// ── File tree ─────────────────────────────────────────────────────────────────

/// Build a one-level-deep `FileNode` tree rooted at `root`.
///
/// Uses `ignore::WalkBuilder` so `.gitignore` rules are respected at every
/// directory level. Children are sorted: directories first (alphabetical),
/// then files (alphabetical).
pub fn build_file_tree(root: &Path) -> FileNode {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    FileNode {
        name,
        path: root.to_string_lossy().to_string(),
        kind: FileKind::Directory,
        children: Some(collect_children(root)),
        extension: None,
    }
}

/// Return the immediate children of `dir`, respecting gitignore and exclusions.
/// Children are not recursed — the frontend expands lazily.
pub fn expand_directory(path: &Path) -> Vec<FileNode> {
    collect_children(path)
}

fn collect_children(dir: &Path) -> Vec<FileNode> {
    let mut dirs: Vec<FileNode> = Vec::new();
    let mut files: Vec<FileNode> = Vec::new();

    let walker = WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    for entry in walker.flatten() {
        let entry_path = entry.path();
        if entry_path == dir {
            continue;
        }

        let entry_name = match entry_path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        if entry_name.is_empty() || is_excluded(&entry_name) {
            continue;
        }

        if entry_path.is_dir() {
            dirs.push(FileNode {
                name: entry_name,
                path: entry_path.to_string_lossy().to_string(),
                kind: FileKind::Directory,
                children: Some(vec![]),
                extension: None,
            });
        } else {
            let extension = entry_path
                .extension()
                .map(|e| e.to_string_lossy().to_string());
            files.push(FileNode {
                name: entry_name,
                path: entry_path.to_string_lossy().to_string(),
                kind: FileKind::File,
                children: None,
                extension,
            });
        }
    }

    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut result = dirs;
    result.extend(files);
    result
}

// ── File CRUD ─────────────────────────────────────────────────────────────────

pub fn read_file(path: &Path) -> Result<String, FsError> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            FsError::NotFound(path.display().to_string())
        } else {
            FsError::io(path.display().to_string(), e)
        }
    })?;

    let size = metadata.len();
    if size > MAX_FILE_SIZE {
        return Err(FsError::TooLarge {
            path: path.display().to_string(),
            size_mb: size / 1_000_000,
            limit_mb: MAX_FILE_SIZE_MB,
        });
    }

    std::fs::read_to_string(path).map_err(|e| FsError::io(path.display().to_string(), e))
}

/// Atomic write: write to a temp file in the same directory, then rename.
/// This prevents file corruption if the process is killed mid-write.
/// If the target file already exists, its Unix permissions are preserved.
pub fn write_file(path: &Path, content: &str) -> Result<(), FsError> {
    let parent = path.parent().ok_or_else(|| {
        FsError::NoParentDir(path.display().to_string())
    })?;

    // Capture original permissions before overwriting so we can restore them.
    let original_permissions = if path.exists() {
        std::fs::metadata(path)
            .ok()
            .map(|m| m.permissions())
    } else {
        None
    };

    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| {
        FsError::io(parent.display().to_string(), e)
    })?;

    use std::io::Write;
    tmp.write_all(content.as_bytes())
        .map_err(|e| FsError::io(path.display().to_string(), e))?;

    // Restore original permissions on the temp file before the atomic rename.
    if let Some(perms) = original_permissions {
        if let Err(e) = std::fs::set_permissions(tmp.path(), perms) {
            warn!(path = %path.display(), error = %e, "Failed to preserve file permissions");
        }
    }

    tmp.persist(path)
        .map_err(|e| FsError::io(path.display().to_string(), e.error))?;

    debug!(path = %path.display(), "File written atomically");
    Ok(())
}

pub fn create_file(path: &Path) -> Result<(), FsError> {
    if path.exists() {
        return Err(FsError::AlreadyExists(path.display().to_string()));
    }
    std::fs::File::create(path).map_err(|e| FsError::io(path.display().to_string(), e))?;
    Ok(())
}

pub fn create_directory(path: &Path) -> Result<(), FsError> {
    std::fs::create_dir_all(path).map_err(|e| FsError::io(path.display().to_string(), e))
}

pub fn delete_path(path: &Path) -> Result<(), FsError> {
    trash::delete(path).map_err(|e| FsError::Other(format!("Failed to move '{}' to Trash: {e}", path.display())))
}

pub fn rename_path(old_path: &Path, new_path: &Path) -> Result<(), FsError> {
    if new_path.exists() {
        return Err(FsError::AlreadyExists(new_path.display().to_string()));
    }
    std::fs::rename(old_path, new_path).map_err(|e| {
        FsError::io(
            format!("{} → {}", old_path.display(), new_path.display()),
            e,
        )
    })
}

// ── File watcher ──────────────────────────────────────────────────────────────

/// Start watching `root` recursively. Returns the debouncer handle; dropping
/// it stops the watcher cleanly.
///
/// Uses `notify-debouncer-full` which preserves full `EventKind` information
/// (Created / Modified / Deleted / Renamed), allowing the frontend to refresh
/// only the affected directory.
pub fn start_watching(root: PathBuf, app_handle: AppHandle) -> Result<FsWatcher, FsError> {
    let handler = move |result: DebounceEventResult| {
        let events = match result {
            Ok(e) => e,
            Err(errors) => {
                for e in errors {
                    error!(error = %e, "File watcher error");
                }
                return;
            }
        };

        for debounced in events {
            let event = &debounced.event;

            // Skip events for excluded paths.
            if event.paths.iter().any(|p| is_path_excluded(p)) {
                continue;
            }

            // Map notify's rich EventKind to our simpler FileEventKind.
            let (kind, new_path) = match &event.kind {
                EventKind::Create(CreateKind::File)
                | EventKind::Create(CreateKind::Folder)
                | EventKind::Create(_) => (FileEventKind::Created, None),

                EventKind::Remove(RemoveKind::File)
                | EventKind::Remove(RemoveKind::Folder)
                | EventKind::Remove(_) => (FileEventKind::Deleted, None),

                // RenameMode::Both means the debouncer correlated From + To.
                // paths[0] = old, paths[1] = new.
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                    let new = event.paths.get(1).map(|p| p.to_string_lossy().to_string());
                    (FileEventKind::Renamed, new)
                }

                // From-only rename: treat as delete of old name.
                EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                    (FileEventKind::Deleted, None)
                }

                // To-only rename: treat as creation of new name.
                EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                    (FileEventKind::Created, None)
                }

                // All other Modify variants (data, metadata, etc.) → Modified.
                EventKind::Modify(_) => (FileEventKind::Modified, None),

                // Access / Other events don't affect the tree or editor.
                _ => continue,
            };

            let path = match event.paths.first() {
                Some(p) => p.to_string_lossy().to_string(),
                None => continue,
            };

            let file_event = FileEvent {
                kind,
                path,
                new_path,
            };

            if let Err(e) = app_handle.emit("file:changed", file_event) {
                error!(error = %e, "Failed to emit file:changed event");
            }
        }
    };

    let mut debouncer = new_debouncer(Duration::from_millis(200), None, handler)
        .map_err(|e| FsError::Other(format!("Failed to create file watcher: {e}")))?;

    debouncer
        .watcher()
        .watch(&root, notify::RecursiveMode::Recursive)
        .map_err(|e| FsError::Other(format!("Failed to watch '{}': {e}", root.display())))?;

    Ok(debouncer)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_temp_project() -> TempDir {
        let dir = tempfile::tempdir().expect("create tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("app/src/main/kotlin/com/example")).unwrap();
        fs::write(
            root.join("app/src/main/kotlin/com/example/Main.kt"),
            "fun main() {}",
        )
        .unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("build/output.class"), b"deadbeef" as &[u8]).unwrap();
        fs::write(
            root.join("settings.gradle.kts"),
            "rootProject.name = \"example\"",
        )
        .unwrap();
        dir
    }

    // ── is_excluded ────────────────────────────────────────────────────────

    #[test]
    fn excludes_build_dir() {
        assert!(is_excluded("build"));
    }

    #[test]
    fn excludes_gradle_dir() {
        assert!(is_excluded(".gradle"));
    }

    #[test]
    fn excludes_class_files() {
        assert!(is_excluded("Foo.class"));
    }

    #[test]
    fn does_not_exclude_kt_files() {
        assert!(!is_excluded("Main.kt"));
    }

    #[test]
    fn does_not_exclude_gradle_kts() {
        assert!(!is_excluded("build.gradle.kts"));
    }

    #[test]
    fn does_not_exclude_dotfiles_without_extension() {
        // ".gitignore" has extension "gitignore", not "gitignore" from split on name
        // Verify dotfiles that happen to share a name with excluded dirs are not excluded.
        assert!(!is_excluded(".gitignore"));
        assert!(!is_excluded(".env"));
    }

    // ── is_path_excluded ────────────────────────────────────────────────────

    #[test]
    fn path_excluded_when_component_matches() {
        let path = Path::new("/project/build/outputs/debug.apk");
        assert!(is_path_excluded(path));
    }

    #[test]
    fn path_not_excluded_for_clean_path() {
        let path = Path::new("/project/app/src/main/Main.kt");
        assert!(!is_path_excluded(path));
    }

    // ── build_file_tree ────────────────────────────────────────────────────

    #[test]
    fn tree_excludes_build_directory() {
        let dir = make_temp_project();
        let tree = build_file_tree(dir.path());
        let children = tree.children.expect("root has children");
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(!names.contains(&"build"), "build/ must be excluded from tree");
    }

    #[test]
    fn tree_includes_gradle_kts() {
        let dir = make_temp_project();
        let tree = build_file_tree(dir.path());
        let children = tree.children.unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"settings.gradle.kts"));
    }

    #[test]
    fn dirs_sorted_before_files() {
        let dir = make_temp_project();
        let tree = build_file_tree(dir.path());
        let children = tree.children.unwrap();
        let mut saw_file = false;
        for node in &children {
            if node.kind == FileKind::File {
                saw_file = true;
            }
            if saw_file && node.kind == FileKind::Directory {
                panic!("Directory found after file in tree: {}", node.name);
            }
        }
    }

    // ── expand_directory ───────────────────────────────────────────────────

    #[test]
    fn expand_directory_returns_immediate_children_only() {
        let dir = make_temp_project();
        let children = expand_directory(dir.path());
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        // "app" dir should be present (immediate child)
        assert!(names.contains(&"app"), "app dir should appear: {:?}", names);
        // Nested files should NOT appear — only depth-1
        assert!(
            !names.contains(&"Main.kt"),
            "nested files must not appear at root level"
        );
    }

    #[test]
    fn expand_directory_excludes_build() {
        let dir = make_temp_project();
        let children = expand_directory(dir.path());
        assert!(!children.iter().any(|n| n.name == "build"));
    }

    // ── write_file / read_file ─────────────────────────────────────────────

    #[test]
    fn round_trip_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let content = "Hello, Android IDE!";
        write_file(&path, content).expect("write_file should succeed");
        let read_back = read_file(&path).expect("read_file should succeed");
        assert_eq!(read_back, content);
    }

    #[test]
    fn write_is_atomic_creates_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atomic.kt");
        write_file(&path, "val x = 1").unwrap();
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(files.len(), 1, "should contain exactly the written file");
    }

    #[test]
    fn read_file_rejects_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        let big: Vec<u8> = vec![0u8; 10 * 1024 * 1024 + 1];
        fs::write(&path, &big).unwrap();
        let err = read_file(&path).unwrap_err();
        assert!(
            err.to_string().contains("too large"),
            "error should mention size: {err}"
        );
    }

    #[test]
    fn read_file_returns_not_found_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.kt");
        let err = read_file(&path).unwrap_err();
        assert!(
            matches!(err, FsError::NotFound(_)),
            "expected NotFound, got {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_file_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.sh");
        // Create with executable bit set
        fs::write(&path, "#!/bin/sh\necho hi").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();

        write_file(&path, "#!/bin/sh\necho updated").unwrap();

        let after_perms = fs::metadata(&path).unwrap().permissions();
        assert_eq!(after_perms.mode() & 0o777, 0o755, "permissions should be preserved");
    }

    // ── create_file ────────────────────────────────────────────────────────

    #[test]
    fn create_file_creates_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.kt");
        create_file(&path).expect("create_file should succeed");
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn create_file_fails_if_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exists.kt");
        fs::write(&path, "existing").unwrap();
        let err = create_file(&path).unwrap_err();
        assert!(matches!(err, FsError::AlreadyExists(_)), "{err}");
    }

    // ── delete_path ────────────────────────────────────────────────────────

    #[test]
    fn delete_path_moves_file_to_trash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("to_delete.kt");
        fs::write(&path, "delete me").unwrap();
        assert!(path.exists());
        delete_path(&path).expect("delete should succeed");
        assert!(!path.exists(), "file should no longer exist after trash");
    }

    #[test]
    fn delete_path_moves_directory_to_trash() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("file.kt"), "content").unwrap();
        delete_path(&sub).expect("directory delete should succeed");
        assert!(!sub.exists());
    }

    // ── rename_path ────────────────────────────────────────────────────────

    #[test]
    fn rename_moves_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.kt");
        let dst = dir.path().join("b.kt");
        fs::write(&src, "content").unwrap();
        rename_path(&src, &dst).expect("rename should succeed");
        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "content");
    }

    #[test]
    fn rename_fails_if_target_exists() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.kt");
        let dst = dir.path().join("b.kt");
        fs::write(&src, "a").unwrap();
        fs::write(&dst, "b").unwrap();
        let err = rename_path(&src, &dst).unwrap_err();
        assert!(matches!(err, FsError::AlreadyExists(_)), "{err}");
    }
}
