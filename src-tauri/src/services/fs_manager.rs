use crate::models::{FileEvent, FileEventKind, FileKind, FileNode};
use ignore::WalkBuilder;
use notify::RecommendedWatcher;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
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

fn is_excluded(name: &str) -> bool {
    if HARDCODED_EXCLUDES.contains(&name) {
        return true;
    }
    // Check extensions
    if let Some(ext) = name.rsplit('.').next() {
        if EXCLUDED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }
    false
}

pub fn build_file_tree(root: &Path) -> FileNode {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    let children = collect_children(root);

    FileNode {
        name,
        path: root.to_string_lossy().to_string(),
        kind: FileKind::Directory,
        children: Some(children),
        extension: None,
    }
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

        // Skip the directory itself
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

pub fn expand_directory(path: &Path) -> Vec<FileNode> {
    collect_children(path)
}

pub fn read_file(path: &Path) -> Result<String, String> {
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("Cannot read file metadata: {e}"))?;

    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File is too large ({} MB). Maximum is 10 MB.",
            metadata.len() / 1_000_000
        ));
    }

    std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))
}

pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("File has no parent directory")?;

    let mut tmp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("Temp file error: {e}"))?;

    use std::io::Write;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("Write error: {e}"))?;

    tmp.persist(path)
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(())
}

pub fn create_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("Already exists: {}", path.display()));
    }
    std::fs::File::create(path).map_err(|e| format!("Failed to create: {e}"))?;
    Ok(())
}

pub fn create_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("Failed to create directory: {e}"))
}

pub fn delete_path(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| format!("Failed to move to trash: {e}"))
}

pub fn rename_path(old_path: &Path, new_path: &Path) -> Result<(), String> {
    if new_path.exists() {
        return Err(format!("Target already exists: {}", new_path.display()));
    }
    std::fs::rename(old_path, new_path).map_err(|e| format!("Failed to rename: {e}"))
}

pub fn start_watching(
    root: PathBuf,
    app_handle: AppHandle,
) -> Result<Debouncer<RecommendedWatcher>, String> {
    let excluded: HashSet<String> = HARDCODED_EXCLUDES
        .iter()
        .map(|s| s.to_string())
        .collect();

    let handler = move |result: DebounceEventResult| match result {
        Ok(events) => {
            for event in events {
                let path = &event.path;

                // Skip excluded paths
                let skip = path.components().any(|c| {
                    let name = c.as_os_str().to_string_lossy();
                    excluded.contains(name.as_ref())
                });
                if skip {
                    continue;
                }

                let file_event = FileEvent {
                    kind: FileEventKind::Modified,
                    path: path.to_string_lossy().to_string(),
                    new_path: None,
                };
                let _ = app_handle.emit("file:changed", file_event);
            }
        }
        Err(e) => {
            eprintln!("File watcher error: {e:?}");
        }
    };

    let mut debouncer = new_debouncer(Duration::from_millis(200), handler)
        .map_err(|e| format!("Failed to create watcher: {e}"))?;

    debouncer
        .watcher()
        .watch(&root, notify::RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch: {e}"))?;

    Ok(debouncer)
}
