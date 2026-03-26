use crate::models::build::{
    BuildError, BuildErrorSeverity, BuildLine, BuildLineKind, BuildRecord, BuildResult,
    BuildStatus,
};
use crate::services::build_runner::{self, BuildState, find_output_apk};
use crate::services::process_manager::{self, ProcessManager, SpawnOptions};
use crate::services::settings_manager;
use crate::FsState;
use chrono::Utc;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri::ipc::Channel;
use tokio::sync::Mutex;

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

    // Shared buffers for accumulating build info across async line callbacks.
    let errors_buf: Arc<Mutex<Vec<BuildError>>> = Arc::new(Mutex::new(vec![]));
    let duration_ms: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let success_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

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
                move |proc_line| {
                    let line = build_runner::parse_build_line(&proc_line.text);

                    // Collect structured errors / warnings.
                    if matches!(line.kind, BuildLineKind::Error | BuildLineKind::Warning) {
                        if let (Some(file), Some(ln)) = (line.file.clone(), line.line) {
                            let severity = if line.kind == BuildLineKind::Error {
                                BuildErrorSeverity::Error
                            } else {
                                BuildErrorSeverity::Warning
                            };
                            if let Ok(mut errs) = errors_buf.try_lock() {
                                errs.push(BuildError {
                                    message: line.content.clone(),
                                    file,
                                    line: ln,
                                    col: line.col,
                                    severity,
                                });
                            }
                        }
                    }

                    // Extract duration and success flag from the summary line.
                    if line.kind == BuildLineKind::Summary {
                        let dur = build_runner::parse_build_duration(&line.content);
                        if let Ok(mut d) = duration_ms.try_lock() {
                            *d = dur;
                        }
                        let is_success = line.content.contains("BUILD SUCCESSFUL");
                        if let Ok(mut s) = success_flag.try_lock() {
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
                    let errs = errors_buf.blocking_lock().clone();
                    let dur = *duration_ms.blocking_lock();
                    let success = *success_flag.blocking_lock() || exit_code == Some(0);

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

    // Mark build as running in the managed state.
    {
        let mut bs = build_state.0.lock().await;
        bs.status = BuildStatus::Running {
            task: task.clone(),
            started_at: started_at.clone(),
        };
        bs.current_build = Some(id);
        bs.current_errors.clear();
    }

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
    build_runner::record_build_result(&build_state.0, task, started_at, result, errors).await;
    Ok(())
}

/// Cancel the currently running build.
#[tauri::command]
pub async fn cancel_build(
    build_state: State<'_, BuildState>,
    process_manager: State<'_, ProcessManager>,
) -> Result<(), String> {
    build_runner::cancel_build(&build_state.0, &process_manager).await;
    Ok(())
}

/// Return the current build status.
#[tauri::command]
pub async fn get_build_status(
    build_state: State<'_, BuildState>,
) -> Result<BuildStatus, String> {
    Ok(build_state.0.lock().await.status.clone())
}

/// Return the structured errors from the last build.
#[tauri::command]
pub async fn get_build_errors(
    build_state: State<'_, BuildState>,
) -> Result<Vec<BuildError>, String> {
    Ok(build_state.0.lock().await.current_errors.clone())
}

/// Return a summary of the last N builds (up to MAX_HISTORY).
#[tauri::command]
pub async fn get_build_history(
    build_state: State<'_, BuildState>,
) -> Result<Vec<BuildRecord>, String> {
    let bs = build_state.0.lock().await;
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

// ── Helpers ────────────────────────────────────────────────────────────────────

fn build_env_vars(
    settings: &crate::models::settings::AppSettings,
    gradle_root: &std::path::Path,
) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(java_home) = settings.java.home.as_deref() {
        env.push(("JAVA_HOME".into(), java_home.into()));
    }
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        env.push(("ANDROID_HOME".into(), sdk.into()));
        env.push(("ANDROID_SDK_ROOT".into(), sdk.into()));
    }
    if let Some(jvm_args) = settings.build.gradle_jvm_args.as_deref() {
        env.push(("GRADLE_OPTS".into(), jvm_args.into()));
    }
    // Ensure gradlew has executable permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let gradlew = gradle_root.join("gradlew");
        if let Ok(meta) = std::fs::metadata(&gradlew) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(&gradlew, perms);
        }
    }
    env
}
