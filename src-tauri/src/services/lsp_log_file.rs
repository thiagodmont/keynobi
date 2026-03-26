use crate::models::log_entry::{LogEntry, LogLevel};
use chrono::Local;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncWriteExt, BufWriter};

/// Keep at most this many session log files on disk.
const MAX_LOG_FILES: usize = 5;

fn get_log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".androidide")
        .join("logs")
        .join("lsp")
}

/// Buffered, append-only session log file written to `~/.androidide/logs/lsp/`.
///
/// A new file is created for each LSP session with a timestamp in the name
/// so past sessions are always accessible.  Old files beyond [`MAX_LOG_FILES`]
/// are pruned automatically on creation.
pub struct LspLogFile {
    writer: BufWriter<tokio::fs::File>,
    /// Absolute path to the current session log — emitted to the Output panel
    /// so developers know exactly where to look when reporting issues.
    pub path: PathBuf,
}

impl LspLogFile {
    /// Create a new timestamped session log file.
    ///
    /// This is a best-effort operation: if the directory can't be created or
    /// the file can't be opened, the error is returned and the caller should
    /// continue without file logging rather than failing the LSP start.
    pub async fn create() -> Result<Self, String> {
        let log_dir = get_log_dir();
        tokio::fs::create_dir_all(&log_dir)
            .await
            .map_err(|e| format!("Failed to create log directory {log_dir:?}: {e}"))?;

        // Prune old session logs before opening a new one.
        rotate_log_files(&log_dir).await;

        let timestamp = Local::now().format("%Y-%m-%dT%H-%M-%S");
        let path = log_dir.join(format!("session-{timestamp}.log"));

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .await
            .map_err(|e| format!("Failed to open log file {path:?}: {e}"))?;

        Ok(Self {
            writer: BufWriter::new(file),
            path,
        })
    }

    /// Append a structured log entry as a human-readable line.
    ///
    /// Format: `[timestamp] [LEVEL] [source] message\n`
    ///
    /// Errors and warnings are flushed to disk immediately so they survive
    /// a crash.  Other levels are buffered for throughput.
    pub async fn write_entry(&mut self, entry: &LogEntry) {
        let level_str = match entry.level {
            LogLevel::Error => "ERROR",
            LogLevel::Warn  => "WARN ",
            LogLevel::Info  => "INFO ",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        };
        let line = format!(
            "[{}] [{}] [{}] {}\n",
            entry.timestamp, level_str, entry.source, entry.message
        );
        if self.writer.write_all(line.as_bytes()).await.is_ok() {
            // Flush eagerly on errors and warnings so they're not lost on crash.
            if matches!(entry.level, LogLevel::Error | LogLevel::Warn) {
                let _ = self.writer.flush().await;
            }
        }
    }

    /// Write a plain separator line (used for session start/end markers).
    pub async fn write_separator(&mut self, text: &str) {
        let line = format!(
            "\n=== {} [{}] ===\n\n",
            text,
            Local::now().to_rfc3339()
        );
        let _ = self.writer.write_all(line.as_bytes()).await;
        let _ = self.writer.flush().await;
    }

    /// Flush any buffered output.  Call before dropping when the session ends.
    pub async fn flush(mut self) {
        let _ = self.writer.flush().await;
    }
}

/// Remove the oldest log files so at most `MAX_LOG_FILES - 1` remain,
/// leaving room for the new session that's about to be created.
async fn rotate_log_files(log_dir: &Path) {
    let mut read_dir = match tokio::fs::read_dir(log_dir).await {
        Ok(rd) => rd,
        Err(_) => return,
    };

    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("log") {
            if let Ok(meta) = entry.metadata().await {
                if let Ok(mtime) = meta.modified() {
                    files.push((mtime, path));
                }
            }
        }
    }

    // Sort newest first; drop anything beyond the slots we want to keep.
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in files.iter().skip(MAX_LOG_FILES.saturating_sub(1)) {
        let _ = tokio::fs::remove_file(path).await;
        tracing::debug!("Rotated old LSP log: {:?}", path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_is_under_androidide() {
        let dir = get_log_dir();
        let home = dirs::home_dir().unwrap();
        assert!(dir.starts_with(&home));
        let s = dir.to_string_lossy();
        assert!(s.contains(".androidide"), "log dir not inside .androidide: {s}");
        assert!(s.ends_with("logs/lsp"), "unexpected log dir path: {s}");
    }

    #[tokio::test]
    async fn create_writes_file_and_separator() {
        let tmp = tempfile::tempdir().unwrap();

        // Patch the log dir to our temp dir by creating the log file directly.
        let path = tmp.path().join("session-test.log");
        let file = tokio::fs::File::create(&path).await.unwrap();
        let mut lf = LspLogFile {
            writer: BufWriter::new(file),
            path: path.clone(),
        };

        lf.write_separator("Session started").await;

        let entry = crate::models::log_entry::LogEntry::new(
            LogLevel::Info,
            "lsp:test",
            "hello world",
        );
        lf.write_entry(&entry).await;
        lf.flush().await;

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("=== Session started"), "separator missing");
        assert!(content.contains("[INFO ]"), "level missing");
        assert!(content.contains("[lsp:test]"), "source missing");
        assert!(content.contains("hello world"), "message missing");
    }

    #[tokio::test]
    async fn rotate_keeps_at_most_max_files() {
        let tmp = tempfile::tempdir().unwrap();

        // Create MAX_LOG_FILES + 2 dummy log files.
        for i in 0..(MAX_LOG_FILES + 2) {
            let path = tmp.path().join(format!("session-{i:03}.log"));
            tokio::fs::write(&path, b"dummy").await.unwrap();
            // Sleep briefly so modification times differ.
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        rotate_log_files(tmp.path()).await;

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Ok(Some(_)) = entries.next_entry().await {
            count += 1;
        }
        assert!(
            count <= MAX_LOG_FILES - 1,
            "rotation left {count} files, expected at most {}",
            MAX_LOG_FILES - 1
        );
    }
}
