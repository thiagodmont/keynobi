//! One Gradle build per project across processes.
//!
//! The app and every standalone `keynobi --mcp` server have their own build
//! slot. A lock file per Gradle root in the data directory keeps them from
//! building the same project at once: a build holds an exclusive lock on
//! `build-locks/<hash>.lock` from before Gradle spawns until it is recorded.
//! The file names the PID holding it, for the refusal message.
//!
//! The per-device UI Automator lock (`ui_automator_lock`) uses the same
//! file lock.
//!
//! This lock is separate from the data lock (`with_data_lock`), which guards
//! short file writes; never take one while waiting on the other.
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Folder under the data directory holding one lock file per Gradle root.
pub const BUILD_LOCKS_DIR: &str = "build-locks";

/// An exclusive lock on one Gradle root, released when dropped.
#[derive(Debug)]
pub struct BuildLock {
    _file: File,
}

/// Why a lock was not taken.
#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    /// Another process holds it; carries that process's PID when readable.
    Held { pid: Option<u32> },
    /// The lock file could not be created or locked.
    Io(String),
}

/// The lock file for `gradle_root` under `data_dir`.
pub fn lock_path(data_dir: &Path, gradle_root: &Path) -> PathBuf {
    let root = gradle_root
        .canonicalize()
        .unwrap_or_else(|_| gradle_root.to_path_buf());
    data_dir
        .join(BUILD_LOCKS_DIR)
        .join(lock_file_name(&root.to_string_lossy()))
}

/// `<hash of key>.lock`, a file name for a key whatever characters it holds.
pub fn lock_file_name(key: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    let name: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("{name}.lock")
}

/// Take the lock for `gradle_root` without waiting.
pub fn try_acquire(data_dir: &Path, gradle_root: &Path) -> Result<BuildLock, LockError> {
    try_lock_file(&lock_path(data_dir, gradle_root)).map(|file| BuildLock { _file: file })
}

/// Take an exclusive advisory lock on `path` without waiting, creating the
/// file and its folder, and write this process's PID into it. The lock is
/// held while the returned file is open, and dies with the process.
pub fn try_lock_file(path: &Path) -> Result<File, LockError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| LockError::Io(format!("Failed to create {}: {e}", dir.display())))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| LockError::Io(format!("Failed to open {}: {e}", path.display())))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            let mut holder = String::new();
            let _ = file.read_to_string(&mut holder);
            return Err(LockError::Held {
                pid: holder.trim().parse().ok(),
            });
        }
        Err(std::fs::TryLockError::Error(e)) => {
            return Err(LockError::Io(format!(
                "Failed to lock {}: {e}",
                path.display()
            )))
        }
    }
    // Best effort: the PID only improves the other side's message.
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    let _ = write!(file, "{}", std::process::id());
    Ok(file)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Lock `path` once nothing holds it. A child that another test thread
    /// forks inherits the lock until it execs, so a released lock can stay
    /// taken for a moment.
    pub fn lock_once_free(path: &Path) -> Result<File, LockError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match try_lock_file(path) {
                Err(LockError::Held { .. }) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_is_refused_until_the_first_releases() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();

        let first = try_acquire(data.path(), project.path()).expect("free lock");
        assert_eq!(
            try_acquire(data.path(), project.path()).unwrap_err(),
            LockError::Held {
                pid: Some(std::process::id())
            },
        );

        drop(first);
        test_support::lock_once_free(&lock_path(data.path(), project.path()))
            .expect("released on drop");
    }

    #[test]
    fn different_projects_do_not_share_a_lock() {
        let data = tempfile::tempdir().unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();

        let _a = try_acquire(data.path(), a.path()).unwrap();
        let _b = try_acquire(data.path(), b.path()).expect("another project builds freely");
    }

    #[test]
    fn the_same_project_through_another_path_shares_the_lock() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("app")).unwrap();

        let _held = try_acquire(data.path(), project.path()).unwrap();
        let other_spelling = project.path().join("app").join("..");
        assert!(matches!(
            try_acquire(data.path(), &other_spelling),
            Err(LockError::Held { .. })
        ));
    }
}
