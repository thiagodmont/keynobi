use crate::models::build::{BuildError, BuildLine, BuildRecord, BuildStatus};
use crate::models::error::AppError;
use crate::services::build_runner::{self, BuildActor, BuildState};
use crate::services::deploy::{self, OpenProject};
use crate::services::process_manager::ProcessManager;
use crate::services::settings_manager;
use crate::FsState;
use std::path::PathBuf;
use tauri::{AppHandle, State};

// Build finalization lives in `build_runner` so the Tauri command layer and the
// MCP server share one implementation. Re-exported for existing call sites.
pub use crate::services::build_runner::{
    finalize_completed_build, mark_build_spawn_failed, BuildCompleteEvent, BuildFinalization,
};

// ── Validation helpers ─────────────────────────────────────────────────────────

/// Thin wrapper over the shared validator (see utils::validation).
fn validate_gradle_task(task: &str) -> Result<(), AppError> {
    crate::utils::validation::validate_gradle_task(task).map_err(AppError::InvalidInput)
}

// ── Build commands ─────────────────────────────────────────────────────────────

/// Start a Gradle task and return its run ID once Gradle is running.
///
/// Output arrives as `build:lines` events and the result as `build:complete`,
/// the same events every build emits, whoever started it.
#[tauri::command]
pub async fn run_gradle_task(
    task: String,
    app_handle: AppHandle,
    fs_state: State<'_, FsState>,
    build_state: State<'_, BuildState>,
    process_manager: State<'_, ProcessManager>,
) -> Result<u32, AppError> {
    validate_gradle_task(&task)?;
    let project = OpenProject::of(&fs_state).await?;
    let request = deploy::build_request(&project, task, BuildActor::App)?;
    let handle =
        build_runner::start_build(&build_state, &process_manager, Some(&app_handle), request)
            .await
            .map_err(deploy::start_error)?;
    Ok(handle.run_id)
}

/// Cancel the running build, whoever started it. Recorded as cancelled in the app.
#[tauri::command]
pub async fn cancel_build(
    build_state: State<'_, BuildState>,
    process_manager: State<'_, ProcessManager>,
) -> Result<(), String> {
    build_runner::cancel_build(&build_state, &process_manager, BuildActor::App).await;
    Ok(())
}

/// Return the current build status.
#[tauri::command]
pub async fn get_build_status(build_state: State<'_, BuildState>) -> Result<BuildStatus, String> {
    Ok(build_state.inner.lock().await.status.clone())
}

/// Return the structured errors from the last build.
#[tauri::command]
pub async fn get_build_errors(
    build_state: State<'_, BuildState>,
) -> Result<Vec<BuildError>, String> {
    Ok(build_state.inner.lock().await.current_errors.clone())
}

/// Return the build history for the currently active project.
///
/// Records are filtered by `project_root` so builds from other projects are
/// not mixed in. Records with no `project_root` (persisted before this field
/// was added) are excluded.
#[tauri::command]
pub async fn get_build_history(
    build_state: State<'_, BuildState>,
    fs_state: State<'_, FsState>,
) -> Result<Vec<BuildRecord>, String> {
    let current_root: Option<String> = {
        let fs = fs_state.0.lock().await;
        fs.project_root
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
    };
    let bs = build_state.inner.lock().await;
    let records: Vec<BuildRecord> = bs
        .history
        .iter()
        .filter(|r| r.project_root.as_deref() == current_root.as_deref())
        .cloned()
        .collect();
    Ok(records)
}

/// Clear all build history (in-memory and on disk).
#[tauri::command]
pub async fn clear_build_history(build_state: State<'_, BuildState>) -> Result<(), String> {
    build_runner::clear_history(&build_state).await
}

/// Return the structured log entries for a specific completed build.
/// `NotFound` when log rotation removed the build's log.
#[tauri::command]
pub async fn get_build_log_entries(id: u32) -> Result<Vec<BuildLine>, AppError> {
    build_runner::read_build_log_in(&settings_manager::data_dir().join("build-logs"), id).await
}

/// The Gradle path of the application module to build (`:app`; `:` for the
/// root project): `module` when it names one, else the project's only one.
///
/// Errors listing the application modules when the project has several and
/// none was named, or when `module` is not one of them.
#[tauri::command]
pub async fn get_application_module(
    module: Option<String>,
    fs_state: State<'_, FsState>,
) -> Result<String, AppError> {
    let gradle_root = open_gradle_root(&fs_state).await?;
    crate::services::gradle_modules::resolve_application_module(&gradle_root, module.as_deref())
        .map(|m| m.path)
        .map_err(AppError::InvalidInput)
}

async fn open_gradle_root(fs_state: &State<'_, FsState>) -> Result<PathBuf, AppError> {
    let fs = fs_state.0.lock().await;
    fs.gradle_root
        .as_ref()
        .or(fs.project_root.as_ref())
        .cloned()
        .ok_or_else(|| AppError::NotFound("No project open".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::build::BuildErrorSeverity;

    #[test]
    fn valid_gradle_tasks_pass() {
        assert!(validate_gradle_task(":app:assembleDebug").is_ok());
        assert!(validate_gradle_task("assembleRelease").is_ok());
        assert!(validate_gradle_task("test").is_ok());
        assert!(validate_gradle_task(":app:bundleRelease").is_ok());
        assert!(validate_gradle_task("lint-check").is_ok());
        assert!(validate_gradle_task("app.assemble").is_ok());
    }

    #[test]
    fn invalid_gradle_tasks_rejected() {
        assert!(validate_gradle_task("").is_err());
        assert!(validate_gradle_task(":app:assemble; rm -rf /").is_err());
        assert!(validate_gradle_task("assemble$(evil)").is_err());
        assert!(validate_gradle_task("assemble\necho pwned").is_err());
        assert!(validate_gradle_task(&"a".repeat(257)).is_err());
    }

    #[test]
    fn spawn_failure_preserves_cancelled_status() {
        let mut state = build_runner::BuildStateInner::new();
        state.starting = true;
        state.status = BuildStatus::Cancelled;

        mark_build_spawn_failed(&mut state);

        assert!(!state.starting);
        assert!(state.current_build.is_none());
        assert!(matches!(state.status, BuildStatus::Cancelled));
    }

    #[test]
    fn spawn_failure_marks_non_cancelled_build_failed() {
        let mut state = build_runner::BuildStateInner::new();
        state.starting = true;
        state.status = BuildStatus::Running {
            task: "assembleDebug".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
        };

        mark_build_spawn_failed(&mut state);

        assert!(!state.starting);
        assert!(state.current_build.is_none());
        assert!(matches!(state.status, BuildStatus::Failed(_)));
    }

    #[tokio::test]
    async fn process_exit_finalization_records_backend_history() {
        let build_state = BuildState::new();
        build_state.inner.lock().await.latest_run = Some(42);
        let errors = vec![BuildError {
            message: "compile failed".to_string(),
            file: Some("Main.kt".to_string()),
            line: Some(7),
            col: Some(3),
            severity: BuildErrorSeverity::Error,
        }];

        let event = finalize_completed_build(
            &build_state,
            BuildFinalization {
                run_id: 42,
                log: build_state.build_log.start_run(),
                task: "assembleDebug".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                project_root: Some("/tmp/project".to_string()),
                success: false,
                cancelled: false,
                duration_ms: 1234,
                errors,
                origin: Some(BuildActor::App),
                cancelled_by: None,
                mappings: None,
            },
        )
        .await;

        assert!(!event.success);
        assert!(!event.cancelled);
        assert_eq!(event.error_count, 1);

        let state = build_state.inner.lock().await;
        assert!(matches!(state.status, BuildStatus::Failed(_)));
        assert_eq!(
            state.history.back().map(|r| r.task.as_str()),
            Some("assembleDebug")
        );
    }
}
