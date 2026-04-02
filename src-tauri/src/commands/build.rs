use crate::models::build::{
    BuildError, BuildErrorSeverity, BuildLine, BuildLineKind, BuildRecord, BuildResult,
    BuildStatus,
};
use crate::services::build_runner::{self, build_env_vars, BuildState, find_output_apk};
use crate::services::process_manager::{self, ProcessManager, SpawnOptions};
use crate::services::settings_manager;
use crate::FsState;
use chrono::Utc;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter, State};
use tauri::ipc::Channel;

// ── Event payloads ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCompleteEvent {
    pub success: bool,
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub task: String,
}

// ── Validation helpers ─────────────────────────────────────────────────────────

/// Validate a Gradle task name against an allowlist.
/// Allowed: alphanumeric, colon, hyphen, underscore, dot. Max 256 chars.
fn validate_gradle_task(task: &str) -> Result<(), String> {
    if task.is_empty() {
        return Err("Gradle task name must not be empty".to_string());
    }
    if task.len() > 256 {
        return Err("Gradle task name is too long (max 256 characters)".to_string());
    }
    if !task.chars().all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.')) {
        return Err(format!(
            "Invalid Gradle task name '{task}': only alphanumeric, ':', '-', '_', '.' are allowed"
        ));
    }
    Ok(())
}

// ── Build commands ─────────────────────────────────────────────────────────────

/// Run a Gradle task, streaming output via a Tauri Channel.
///
/// Lines are emitted via `on_line` as they arrive. When the process exits,
/// a `build:complete` event is emitted on the AppHandle so the frontend can
/// call `finalize_build` to persist the result in history.
#[tauri::command]
pub async fn run_gradle_task(
    task: String,
    on_line: Channel<BuildLine>,
    app_handle: AppHandle,
    fs_state: State<'_, FsState>,
    build_state: State<'_, BuildState>,
    process_manager: State<'_, ProcessManager>,
) -> Result<u32, String> {
    validate_gradle_task(&task)?;

    let gradle_root: PathBuf = {
        let fs = fs_state.0.lock().await;
        fs.gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or("No project open")?
    };

    let settings = settings_manager::load_settings();

    let gradlew = build_runner::find_gradlew(&gradle_root)
        .ok_or_else(|| "gradlew not found at project root".to_string())?;

    let mut args = vec![task.clone(), "--console=plain".to_owned()];
    if settings.build.gradle_parallel {
        args.push("--parallel".into());
    }
    if settings.build.gradle_offline {
        args.push("--offline".into());
    }

    let env = build_env_vars(&settings, &gradle_root);
    let started_at = Utc::now().to_rfc3339();

    // Use std::sync::Mutex (not tokio) for these accumulators — they are
    // accessed only from sync callbacks (on_line / on_exit) and must never
    // use blocking_lock on a tokio mutex inside an async task.
    let errors_buf: Arc<StdMutex<Vec<BuildError>>> = Arc::new(StdMutex::new(vec![]));
    let duration_ms: Arc<StdMutex<u64>> = Arc::new(StdMutex::new(0));
    let success_flag: Arc<StdMutex<bool>> = Arc::new(StdMutex::new(false));
    // Share the BuildLog Arc so the on_line callback can push lines directly.
    let build_log = build_state.build_log.clone();

    let args_strs: Vec<String> = args;
    let args_refs: Vec<&str> = args_strs.iter().map(|s| s.as_str()).collect();

    let id = process_manager::spawn(
        &process_manager.0,
        gradlew.to_str().unwrap_or("./gradlew"),
        &args_refs,
        gradle_root.clone(),
        env,
        SpawnOptions {
            on_line: Box::new({
                let errors_buf = errors_buf.clone();
                let duration_ms = duration_ms.clone();
                let success_flag = success_flag.clone();
                let build_log = build_log.clone();
                move |proc_line| {
                    // Push every raw line into the persistent build log.
                    build_runner::push_build_log(&build_log, proc_line.text.clone());

                    let line = build_runner::parse_build_line(&proc_line.text);

                    // Collect structured errors / warnings (including those without file locations).
                    if matches!(line.kind, BuildLineKind::Error | BuildLineKind::Warning) {
                        let severity = if line.kind == BuildLineKind::Error {
                            BuildErrorSeverity::Error
                        } else {
                            BuildErrorSeverity::Warning
                        };
                        if let Ok(mut errs) = errors_buf.lock() {
                            errs.push(BuildError {
                                message: line.content.clone(),
                                file: line.file.clone(),
                                line: line.line,
                                col: line.col,
                                severity,
                            });
                        }
                    }

                    // Extract duration and success flag from the summary line.
                    if line.kind == BuildLineKind::Summary {
                        let dur = build_runner::parse_build_duration(&line.content);
                        if let Ok(mut d) = duration_ms.lock() {
                            *d = dur;
                        }
                        let is_success = line.content.contains("BUILD SUCCESSFUL");
                        if let Ok(mut s) = success_flag.lock() {
                            *s = is_success;
                        }
                    }

                    let _ = on_line.send(line);
                }
            }),
            on_exit: Box::new({
                let app = app_handle.clone();
                let task_name = task.clone();
                move |_pid, exit_code| {
                    // std::sync::Mutex::lock() — safe to call from any context.
                    let errs = errors_buf.lock().map(|g| g.clone()).unwrap_or_default();
                    let dur = duration_ms.lock().map(|g| *g).unwrap_or(0);
                    let flag = success_flag.lock().map(|g| *g).unwrap_or(false);
                    let success = flag || exit_code == Some(0);

                    let error_count = errs
                        .iter()
                        .filter(|e| e.severity == BuildErrorSeverity::Error)
                        .count() as u32;
                    let warn_count = errs
                        .iter()
                        .filter(|e| e.severity == BuildErrorSeverity::Warning)
                        .count() as u32;

                    let _ = app.emit(
                        "build:complete",
                        BuildCompleteEvent {
                            success,
                            duration_ms: dur,
                            error_count,
                            warning_count: warn_count,
                            task: task_name.clone(),
                        },
                    );
                }
            }),
        },
    )
    .await?;

    // Mark build as running in the managed state and clear the log for this run.
    {
        let mut bs = build_state.inner.lock().await;
        bs.status = BuildStatus::Running {
            task: task.clone(),
            started_at: started_at.clone(),
        };
        bs.current_build = Some(id);
        bs.current_errors.clear();
    }
    build_runner::clear_build_log(&build_state.build_log);

    Ok(id)
}

/// Persist the final build result into state and history.
///
/// The frontend calls this after receiving the `build:complete` event.
#[tauri::command]
pub async fn finalize_build(
    success: bool,
    duration_ms: u64,
    errors: Vec<BuildError>,
    task: String,
    started_at: String,
    build_state: State<'_, BuildState>,
) -> Result<(), String> {
    let error_count = errors
        .iter()
        .filter(|e| e.severity == BuildErrorSeverity::Error)
        .count() as u32;
    let warn_count = errors
        .iter()
        .filter(|e| e.severity == BuildErrorSeverity::Warning)
        .count() as u32;
    let result = BuildResult {
        success,
        duration_ms,
        error_count,
        warning_count: warn_count,
    };
    build_runner::record_build_result(&build_state, task, started_at, result, errors).await;
    Ok(())
}

/// Cancel the currently running build.
#[tauri::command]
pub async fn cancel_build(
    build_state: State<'_, BuildState>,
    process_manager: State<'_, ProcessManager>,
) -> Result<(), String> {
    build_runner::cancel_build(&build_state, &process_manager).await;
    Ok(())
}

/// Return the current build status.
#[tauri::command]
pub async fn get_build_status(
    build_state: State<'_, BuildState>,
) -> Result<BuildStatus, String> {
    Ok(build_state.inner.lock().await.status.clone())
}

/// Return the structured errors from the last build.
#[tauri::command]
pub async fn get_build_errors(
    build_state: State<'_, BuildState>,
) -> Result<Vec<BuildError>, String> {
    Ok(build_state.inner.lock().await.current_errors.clone())
}

/// Return a summary of the last N builds (up to MAX_HISTORY).
#[tauri::command]
pub async fn get_build_history(
    build_state: State<'_, BuildState>,
) -> Result<Vec<BuildRecord>, String> {
    let bs = build_state.inner.lock().await;
    Ok(bs.history.iter().cloned().collect())
}

/// Find the output APK path for the given variant after a successful build.
#[tauri::command]
pub async fn find_apk_path(
    variant: String,
    fs_state: State<'_, FsState>,
) -> Result<Option<String>, String> {
    let gradle_root: PathBuf = {
        let fs = fs_state.0.lock().await;
        fs.gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or("No project open")?
    };
    Ok(find_output_apk(&gradle_root, &variant)
        .map(|p| p.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

