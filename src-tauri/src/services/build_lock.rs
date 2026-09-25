//! One Gradle build per project across processes.
//!
//! The app and every standalone `keynobi --mcp` server have their own build
//! slot. A lock file per Gradle root in the data directory keeps them from
//! building the same project at once: a build holds an exclusive lock on
//! `build-locks/<hash>.lock` from before Gradle spawns until it is recorded.
//! The file names the PID holding it, for the refusal message.
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
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let name: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    data_dir.join(BUILD_LOCKS_DIR).join(format!("{name}.lock"))
}

/// Take the lock for `gradle_root` without waiting.
pub fn try_acquire(data_dir: &Path, gradle_root: &Path) -> Result<BuildLock, LockError> {
    let path = lock_path(data_dir, gradle_root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| LockError::Io(format!("Failed to create {}: {e}", dir.display())))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
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
    Ok(BuildLock { _file: file })
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
        try_acquire(data.path(), project.path()).expect("released on drop");
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
