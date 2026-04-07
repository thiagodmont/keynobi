use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
// Duration, AppHandle, Emitter are used by run_monitor (implemented in Task 3)
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStats {
    pub app_memory_bytes: u64,
    pub log_folder_bytes: u64,
    pub rotation_triggered: bool,
}

/// Returns (total_bytes, list of (path, modified, size)) for app.log* files in log_dir.
pub fn collect_log_files(log_dir: &Path) -> (u64, Vec<(PathBuf, SystemTime, u64)>) {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return (0, vec![]);
    };
    let mut total = 0u64;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("app.log") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            let size = meta.len();
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            total += size;
            files.push((path, modified, size));
        }
    }
    (total, files)
}

/// Deletes oldest app.log* files until total_bytes <= limit. Returns true if any files deleted.
pub fn rotate_logs(mut files: Vec<(PathBuf, SystemTime, u64)>, limit: u64) -> bool {
    // Sort oldest-first
    files.sort_by_key(|(_, modified, _)| *modified);
    let mut total: u64 = files.iter().map(|(_, _, size)| size).sum();
    let mut rotated = false;
    for (path, _, size) in &files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            tracing::info!("Size-based log rotation: removed {}", path.display());
            total = total.saturating_sub(*size);
            rotated = true;
        }
    }
    rotated
}

pub async fn run_monitor(_app_handle: AppHandle, _log_dir: PathBuf, _log_max_size_bytes: u64) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_file(dir: &Path, name: &str, size: usize) {
        fs::write(dir.join(name), vec![0u8; size]).unwrap();
    }

    #[test]
    fn collect_returns_zero_for_empty_dir() {
        let dir = tempdir().unwrap();
        let (total, files) = collect_log_files(dir.path());
        assert_eq!(total, 0);
        assert!(files.is_empty());
    }

    #[test]
    fn collect_sums_only_app_log_files() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "app.log", 1000);
        write_file(dir.path(), "app.log.2026-04-01", 2000);
        write_file(dir.path(), "other.log", 500); // should be ignored
        let (total, files) = collect_log_files(dir.path());
        assert_eq!(total, 3000);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_returns_zero_for_missing_dir() {
        let (total, files) = collect_log_files(Path::new("/tmp/keynobi-nonexistent-dir-xyz"));
        assert_eq!(total, 0);
        assert!(files.is_empty());
    }

    #[test]
    fn rotate_does_nothing_when_under_limit() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "app.log.2026-04-01", 100);
        let (_, files) = collect_log_files(dir.path());
        let rotated = rotate_logs(files, 1000);
        assert!(!rotated);
        assert!(dir.path().join("app.log.2026-04-01").exists());
    }

    #[test]
    fn rotate_deletes_oldest_file_first() {
        let dir = tempdir().unwrap();
        // Write two files; ensure older one has an earlier modified time
        write_file(dir.path(), "app.log.2026-04-01", 300);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-02", 300);

        let (_, files) = collect_log_files(dir.path());
        // Limit of 400 → must delete oldest (300 bytes) to get to 300 ≤ 400
        let rotated = rotate_logs(files, 400);
        assert!(rotated);
        // Newer file must survive
        assert!(dir.path().join("app.log.2026-04-02").exists());
        // Older file must be gone
        assert!(!dir.path().join("app.log.2026-04-01").exists());
    }

    #[test]
    fn rotate_deletes_multiple_files_until_under_limit() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "app.log.2026-04-01", 200);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-02", 200);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-03", 200);

        let (_, files) = collect_log_files(dir.path());
        // Limit of 250 → must delete first two files (400 bytes) to reach 200 ≤ 250
        let rotated = rotate_logs(files, 250);
        assert!(rotated);
        assert!(dir.path().join("app.log.2026-04-03").exists());
        assert!(!dir.path().join("app.log.2026-04-01").exists());
        assert!(!dir.path().join("app.log.2026-04-02").exists());
    }
}
