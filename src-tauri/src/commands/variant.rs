use crate::models::error::AppError;
use crate::models::variant::VariantList;
use crate::services::{build_runner, settings_manager, variant_manager};
use crate::FsState;
use std::path::PathBuf;
use tauri::State;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Upper bound for a single `gradlew tasks` invocation. Unlike builds, variant
/// discovery has no user-facing progress UI, so a hung daemon must not block
/// this command indefinitely.
const GRADLE_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Resolve the gradle root for the active project, or `None` if no project
/// is open. Callers that want to treat "no project" as a normal outcome
/// (rather than an error) should use this instead of string-matching
/// [`resolve_gradle_root`]'s error.
async fn resolve_gradle_root_opt(fs_state: &State<'_, FsState>) -> Option<PathBuf> {
    let fs = fs_state.0.lock().await;
    fs.gradle_root
        .as_ref()
        .or(fs.project_root.as_ref())
        .cloned()
}

async fn resolve_gradle_root(fs_state: &State<'_, FsState>) -> Result<PathBuf, String> {
    resolve_gradle_root_opt(fs_state)
        .await
        .ok_or_else(|| "No project open".to_string())
}

fn restore_active(list: VariantList) -> VariantList {
    list
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Fast variant preview — parsed from the application module's
/// `build.gradle(.kts)` without running Gradle.  Returns only variants that
/// are **explicitly declared** in the build script; no hardcoded defaults are
/// injected. `module` names the application module; without it the project's
/// only one is used, and several are an error listing them.
///
/// This resolves instantly and is used to populate the UI while the
/// authoritative Gradle query runs in the background.
#[tauri::command]
pub async fn get_variants_preview(
    module: Option<String>,
    fs_state: State<'_, FsState>,
) -> Result<VariantList, AppError> {
    let gradle_root = resolve_gradle_root(&fs_state)
        .await
        .map_err(AppError::NotFound)?;
    variant_manager::preview_variants(&gradle_root, module.as_deref())
        .map(restore_active)
        .map_err(AppError::InvalidInput)
}

/// Authoritative variant list — obtained by running
/// `./gradlew <application module>:tasks --all --console=plain`.
///
/// Scoped to `module`, or without it to the project's only application module
/// (falling back to the whole build's `tasks` when there is none), and does
/// not use `--group` flags that vary by Gradle version.
/// The output includes all `assemble*` and `install*` tasks for every variant
/// the project defines, regardless of how complex its configuration is.
///
/// This is the source of truth; it discovers every variant the project
/// actually has.
#[tauri::command]
pub async fn get_variants_from_gradle(
    module: Option<String>,
    fs_state: State<'_, FsState>,
) -> Result<VariantList, AppError> {
    // Both roots from one lock, so a project switch cannot pair two projects.
    let (gradle_root, trust_root) = {
        let fs = fs_state.0.lock().await;
        let gradle_root = fs
            .gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or_else(|| AppError::NotFound("No project open".to_string()))?;
        let trust_root = fs
            .project_root
            .clone()
            .unwrap_or_else(|| gradle_root.clone());
        (gradle_root, trust_root)
    };

    let gradlew = gradle_root.join("gradlew");
    if !gradlew.is_file() {
        return Err(AppError::NotFound(
            "gradlew not found — cannot detect variants".to_string(),
        ));
    }

    let queries = variant_manager::variant_task_queries(&gradle_root, module.as_deref())
        .map_err(AppError::InvalidInput)?;
    let build_files =
        variant_manager::variant_build_files(&gradle_root, module.as_deref()).unwrap_or_default();

    let (settings, _) = settings_manager::load_settings();

    // Same trust check, JAVA_HOME, and SDK variables as builds; also makes
    // gradlew executable.
    let env = build_runner::trusted_gradle_env(&settings, &trust_root, &gradle_root)
        .map_err(AppError::PermissionDenied)?;

    // `<module>:tasks --all` first (module-scoped, lists every variant task).
    // `--all` is required because newer AGP versions mark individual variant tasks
    // (e.g. assembleDebug, assembleRelease) as "non-public" and they are hidden
    // from the plain `tasks` output without it.
    // Scoping to the application module keeps the output small and fast.
    for task_arg in &queries {
        let mut cmd = tokio::process::Command::new(&gradlew);
        cmd.args([task_arg.as_str(), "--all", "--console=plain"])
            .current_dir(&gradle_root)
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            // Kill the gradlew wrapper process when the timeout below drops
            // the output future. Note: the detached Gradle daemon JVM the
            // wrapper spawned may still survive; this only guarantees the
            // command itself unblocks and the launcher is reaped.
            .kill_on_drop(true);

        let output = match tokio::time::timeout(GRADLE_QUERY_TIMEOUT, cmd.output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                return Err(AppError::ProcessFailed(format!(
                    "Failed to run gradlew: {e}"
                )))
            }
            Err(_) => {
                return Err(AppError::ProcessFailed(format!(
                    "gradlew '{task_arg}' did not finish within {} seconds — \
                     is a Gradle daemon stuck? Try again after freeing Gradle.",
                    GRADLE_QUERY_TIMEOUT.as_secs()
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let combined = format!("{stdout}{stderr}");

        // If the command itself failed completely, try the next arg.
        if !output.status.success() && stdout.trim().is_empty() {
            continue;
        }

        let mut list = variant_manager::parse_variants_from_tasks_output(&combined);
        if !list.variants.is_empty() {
            list.default_variant = variant_manager::infer_default_variant_name_from(
                &gradle_root,
                &build_files,
                &list.variants,
            );
            return Ok(restore_active(list));
        }
    }

    Err(AppError::NotFound(
        "No build variants found after running 'gradlew tasks'. \
        Make sure JAVA_HOME is configured in Settings and the project builds correctly."
            .to_string(),
    ))
}

/// Persist the active build variant as `last_build_variant` for the current
/// project so it is restored on the next session — the same state the MCP
/// `set_active_variant` tool writes (keyed by gradle root). No-op success if
/// no project is open or the project is not yet in `recent_projects`.
#[tauri::command]
pub async fn set_active_variant(
    variant: String,
    fs_state: State<'_, FsState>,
) -> Result<(), String> {
    // No project open — nothing to persist, which matches the documented
    // no-op behavior rather than surfacing an error to the UI.
    let Some(gradle_root) = resolve_gradle_root_opt(&fs_state).await else {
        return Ok(());
    };
    let project_path = gradle_root.to_string_lossy().into_owned();
    settings_manager::set_active_variant_for_project(&project_path, &variant)
}
