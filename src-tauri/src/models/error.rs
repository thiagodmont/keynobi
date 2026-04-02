use serde::Serialize;
use thiserror::Error;
use ts_rs::TS;

/// Structured error type returned by Tauri command handlers at the IPC boundary.
///
/// Serializes as `{"kind": "notFound", "message": "..."}` so the TypeScript
/// frontend can match on the `kind` field and show context-appropriate messages.
#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Process failed: {0}")]
    ProcessFailed(String),

    #[error("Settings error: {0}")]
    SettingsError(String),

    #[error("MCP error: {0}")]
    McpError(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Convenience constructor for IO errors with path context.
    pub fn io(path: impl std::fmt::Display, source: impl std::fmt::Display) -> Self {
        AppError::Io(format!("'{path}': {source}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

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

#[cfg(test)]
mod app_error_tests {
    use super::AppError;

    #[test]
    fn app_error_serializes_with_kind_field() {
        let err = AppError::InvalidInput("bad task name".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"kind\""), "must have kind field: {json}");
        assert!(json.contains("invalidInput"), "kind must be camelCase: {json}");
        assert!(json.contains("bad task name"), "must contain message: {json}");
    }

    #[test]
    fn app_error_display_is_human_readable() {
        let err = AppError::NotFound("settings.json".to_string());
        assert!(err.to_string().contains("settings.json"));
    }

    #[test]
    fn io_convenience_constructor() {
        let err = AppError::io("/path/to/file", "permission denied");
        assert!(err.to_string().contains("/path/to/file"));
        assert!(err.to_string().contains("permission denied"));
    }
}
