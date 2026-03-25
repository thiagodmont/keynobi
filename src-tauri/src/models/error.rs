use thiserror::Error;

/// Structured error type for all file-system operations.
///
/// Service functions (`fs_manager`) return `Result<_, FsError>`.
/// Tauri command handlers convert to `String` at the IPC boundary via
/// `.map_err(|e| e.to_string())`, which keeps the frontend API stable while
/// allowing internal callers to match on specific error variants.
#[derive(Debug, Error)]
pub enum FsError {
    #[error("Not found: '{0}'")]
    NotFound(String),

    #[error("Permission denied: '{0}'")]
    PermissionDenied(String),

    #[error("Already exists: '{0}'")]
    AlreadyExists(String),

    /// File exceeds the configured size limit.
    #[error("'{path}' is too large ({size_mb} MB). Maximum is {limit_mb} MB.")]
    TooLarge {
        path: String,
        size_mb: u64,
        limit_mb: u64,
    },

    /// A crafted path escaped the open project directory.
    #[error("Access denied: '{0}' is outside the open project directory")]
    PathTraversal(String),

    /// Parent directory does not exist when creating a file.
    #[error("Parent directory does not exist: '{0}'")]
    NoParentDir(String),

    /// Invalid filename (e.g. contains path separators or null bytes).
    #[error("Invalid path: '{0}'")]
    InvalidPath(String),

    /// Transparent wrapper for `std::io::Error` — preserves the OS error kind.
    #[error("IO error on '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Catch-all for errors that don't fit a specific variant.
    #[error("{0}")]
    Other(String),
}

impl FsError {
    /// Convenience constructor for `Io` variant.
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        FsError::Io {
            path: path.into(),
            source,
        }
    }
}
