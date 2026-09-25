use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

/// Payload of `monitor://stats`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MonitorStats {
    #[ts(type = "number")]
    pub app_memory_bytes: u64,
    #[ts(type = "number")]
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
        if !name.starts_with(LOG_FILE_PREFIX) {
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

/// Prefix the daily appender gives every GUI log file (`app.log.YYYY-MM-DD`).
pub const LOG_FILE_PREFIX: &str = "app.log";

/// Name of the file the daily appender is writing to now. The appender rolls on UTC dates.
pub fn active_log_file_name() -> String {
    format!(
        "{LOG_FILE_PREFIX}.{}",
        chrono::Utc::now().format("%Y-%m-%d")
    )
}

/// Deletes oldest app.log* files, never `active_file_name`, until total_bytes <= limit.
/// Returns true if rotation was attempted (over limit).
pub fn rotate_logs(
    mut files: Vec<(PathBuf, SystemTime, u64)>,
    limit: u64,
    active_file_name: &str,
) -> bool {
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
        if path.file_name().and_then(|n| n.to_str()) == Some(active_file_name) {
            continue;
        }
        if std::fs::remove_file(path).is_ok() {
            tracing::info!("Size-based log rotation: removed {}", path.display());
            total = total.saturating_sub(*size);
        } else {
            tracing::warn!(
                "Size-based log rotation: could not remove {}, skipping",
                path.display()
            );
        }
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
            rotate_logs(files, log_max_size_bytes, &active_log_file_name())
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

    const ACTIVE: &str = "app.log.2026-04-09";

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
        let rotated = rotate_logs(files, 1000, ACTIVE);
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
        let rotated = rotate_logs(files, 400, ACTIVE);
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
        let rotated = rotate_logs(files, 250, ACTIVE);
        assert!(rotated);
        assert!(dir.path().join("app.log.2026-04-03").exists());
        assert!(!dir.path().join("app.log.2026-04-01").exists());
        assert!(!dir.path().join("app.log.2026-04-02").exists());
    }

    #[test]
    fn active_log_file_name_matches_daily_appender_naming() {
        let name = active_log_file_name();
        let date = name.strip_prefix("app.log.").unwrap();
        assert!(chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok());
    }

    #[test]
    fn rotate_never_deletes_the_active_log_even_when_it_is_oldest() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), ACTIVE, 300);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-02", 300);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-03", 300);

        let (_, files) = collect_log_files(dir.path());
        let rotated = rotate_logs(files, 100, ACTIVE);
        assert!(rotated);
        assert!(dir.path().join(ACTIVE).exists());
        assert!(!dir.path().join("app.log.2026-04-02").exists());
        assert!(!dir.path().join("app.log.2026-04-03").exists());
    }

    #[test]
    fn rotate_keeps_going_when_a_removal_fails() {
        let dir = tempdir().unwrap();
        // A directory cannot be removed with remove_file, so this removal always fails.
        fs::create_dir(dir.path().join("app.log.2026-04-01")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-02", 300);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), ACTIVE, 300);

        let (_, mut files) = collect_log_files(dir.path());
        // Give the directory a size that alone would bring the total under the limit.
        for (path, _, size) in &mut files {
            if path.ends_with("app.log.2026-04-01") {
                *size = 600;
            }
        }
        // 1200 bytes over a 700 limit: the failed 600-byte removal must not count as freed.
        let rotated = rotate_logs(files, 700, ACTIVE);
        assert!(rotated);
        assert!(!dir.path().join("app.log.2026-04-02").exists());
        assert!(dir.path().join(ACTIVE).exists());
    }
}
