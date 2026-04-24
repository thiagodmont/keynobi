use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
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

/// Deletes oldest app.log* files until total_bytes <= limit. Returns true if rotation was attempted (over limit).
pub fn rotate_logs(mut files: Vec<(PathBuf, SystemTime, u64)>, limit: u64) -> bool {
    // Sort oldest-first
    files.sort_by_key(|(_, modified, _)| *modified);
    let mut total: u64 = files.iter().map(|(_, _, size)| size).sum();
    if total <= limit {
        return false;
    }
    // Over the limit — attempt rotation.
    for (path, _, size) in &files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            tracing::info!("Size-based log rotation: removed {}", path.display());
        } else {
            tracing::warn!(
                "Size-based log rotation: could not remove {}, skipping",
                path.display()
            );
        }
        // Subtract either way so we don't retry the same file on the next tick.
        total = total.saturating_sub(*size);
    }
    // Return true to signal that rotation was triggered (regardless of deletion success).
    true
}

pub async fn run_monitor(app_handle: AppHandle, log_dir: PathBuf, log_max_size_bytes: u64) {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let pid = Pid::from(std::process::id() as usize);
    let mut sys = System::new();

    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        // 1. Read app process RSS memory
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
        let app_memory_bytes = sys.process(pid).map(|p| p.memory()).unwrap_or(0);

        // 2. Scan log folder
        let (log_folder_bytes, files) = collect_log_files(&log_dir);

        // 3. Rotate if needed
        let rotation_triggered = if log_folder_bytes > log_max_size_bytes {
            rotate_logs(files, log_max_size_bytes)
        } else {
            false
        };

        // 4. Emit stats to frontend
        let stats = MonitorStats {
            app_memory_bytes,
            log_folder_bytes,
            rotation_triggered,
        };
        let _ = app_handle.emit("monitor://stats", stats);
    }
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
