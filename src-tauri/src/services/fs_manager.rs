use crate::models::{FileEvent, FileEventKind, FileKind, FileNode};
use ignore::WalkBuilder;
use notify::RecommendedWatcher;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, DebouncedEventKind, Debouncer};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

const HARDCODED_EXCLUDES: &[&str] = &[
    "build",
    ".gradle",
    ".idea",
    ".git",
    "node_modules",
    ".DS_Store",
];

const EXCLUDED_EXTENSIONS: &[&str] = &["class", "dex", "apk", "aar"];

pub(crate) fn is_excluded(name: &str) -> bool {
    if HARDCODED_EXCLUDES.contains(&name) {
        return true;
    }
    if let Some(ext) = name.rsplit('.').next() {
        if EXCLUDED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }
    false
}

fn is_path_excluded(path: &Path) -> bool {
    path.components()
        .any(|c| is_excluded(c.as_os_str().to_string_lossy().as_ref()))
}

// ── File tree ─────────────────────────────────────────────────────────────────

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

pub fn read_file(path: &Path) -> Result<String, String> {
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("Cannot read metadata for '{}': {e}", path.display()))?;

    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "'{}' is too large ({} MB). Maximum is 10 MB.",
            path.display(),
            metadata.len() / 1_000_000
        ));
    }

    std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read '{}': {e}", path.display()))
}

/// Atomic write: write to a temp file in the same directory, then rename.
/// This prevents file corruption if the process is killed mid-write.
pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("'{}' has no parent directory", path.display()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Failed to create temp file in '{}': {e}", parent.display()))?;

    use std::io::Write;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write to temp file: {e}"))?;

    tmp.persist(path)
        .map_err(|e| format!("Failed to replace '{}': {e}", path.display()))?;

    Ok(())
}

pub fn create_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("'{}' already exists", path.display()));
    }
    std::fs::File::create(path)
        .map_err(|e| format!("Failed to create '{}': {e}", path.display()))?;
    Ok(())
}

pub fn create_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|e| format!("Failed to create directory '{}': {e}", path.display()))
}

pub fn delete_path(path: &Path) -> Result<(), String> {
    trash::delete(path)
        .map_err(|e| format!("Failed to move '{}' to Trash: {e}", path.display()))
}

pub fn rename_path(old_path: &Path, new_path: &Path) -> Result<(), String> {
    if new_path.exists() {
        return Err(format!("Target '{}' already exists", new_path.display()));
    }
    std::fs::rename(old_path, new_path).map_err(|e| {
        format!(
            "Failed to rename '{}' → '{}': {e}",
            old_path.display(),
            new_path.display()
        )
    })
}

// ── File watcher ──────────────────────────────────────────────────────────────

pub fn start_watching(
    root: PathBuf,
    app_handle: AppHandle,
) -> Result<Debouncer<RecommendedWatcher>, String> {
    let excluded: HashSet<String> = HARDCODED_EXCLUDES
        .iter()
        .map(|s| s.to_string())
        .collect();

    let handler = move |result: DebounceEventResult| {
        let events = match result {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[fs_watcher] error: {e:?}");
                return;
            }
        };

        for event in events {
            let path = &event.path;
            if is_path_excluded(path) {
                continue;
            }

            // notify-debouncer-mini v0.4 abstracts away specific event kinds
            // into `Any` (single event) and `AnyContinuous` (ongoing, e.g. saves).
            // We skip AnyContinuous to avoid flooding the frontend during large writes.
            let kind = match event.kind {
                DebouncedEventKind::Any => FileEventKind::Modified,
                DebouncedEventKind::AnyContinuous => continue,
                // DebouncedEventKind is non-exhaustive; handle future variants gracefully.
                _ => FileEventKind::Modified,
            };

            let file_event = FileEvent {
                kind,
                path: path.to_string_lossy().to_string(),
                new_path: None,
            };
            let _ = app_handle.emit("file:changed", file_event);
        }
    };

    let mut debouncer = new_debouncer(Duration::from_millis(200), handler)
        .map_err(|e| format!("Failed to create file watcher: {e}"))?;

    debouncer
        .watcher()
        .watch(&root, notify::RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch '{}': {e}", root.display()))?;

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
        // Mimic a minimal Android project layout
        fs::create_dir_all(root.join("app/src/main/kotlin/com/example")).unwrap();
        fs::write(root.join("app/src/main/kotlin/com/example/Main.kt"), "fun main() {}").unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("build/output.class"), b"deadbeef" as &[u8]).unwrap();
        fs::write(root.join("settings.gradle.kts"), "rootProject.name = \"example\"").unwrap();
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
        // The file itself is not excluded — only directories like ".gradle" are
        assert!(!is_excluded("build.gradle.kts"));
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
        // All directory entries should precede all file entries.
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
        // Only the target file should exist; no leftover temp files.
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(files.len(), 1, "should contain exactly the written file");
    }

    #[test]
    fn read_file_rejects_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        // Write slightly over the 10 MB limit.
        let big: Vec<u8> = vec![0u8; 10 * 1024 * 1024 + 1];
        fs::write(&path, &big).unwrap();
        let err = read_file(&path).unwrap_err();
        assert!(err.contains("too large"), "error should mention size: {err}");
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
        assert!(err.contains("already exists"), "{err}");
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
        assert!(err.contains("already exists"), "{err}");
    }
}
