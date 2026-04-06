use crate::models::variant::VariantList;
use crate::services::{settings_manager, variant_manager};
use crate::FsState;
use std::path::PathBuf;
use tauri::State;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn resolve_gradle_root(fs_state: &State<'_, FsState>) -> Result<PathBuf, String> {
    let fs = fs_state.0.lock().await;
    fs.gradle_root
        .as_ref()
        .or(fs.project_root.as_ref())
        .cloned()
        .ok_or_else(|| "No project open".to_string())
}

fn restore_active(mut list: VariantList) -> VariantList {
    let (settings, _) = settings_manager::load_settings();
    list.active = settings.build.build_variant;
    list
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Fast variant preview — parsed from `app/build.gradle(.kts)` without
/// running Gradle.  Returns only variants that are **explicitly declared**
/// in the build script; no hardcoded defaults are injected.
///
/// This resolves instantly and is used to populate the UI while the
/// authoritative Gradle query runs in the background.
#[tauri::command]
pub async fn get_variants_preview(
    fs_state: State<'_, FsState>,
) -> Result<VariantList, String> {
    let gradle_root = resolve_gradle_root(&fs_state).await?;

    let candidates = [
        gradle_root.join("app").join("build.gradle.kts"),
        gradle_root.join("app").join("build.gradle"),
        gradle_root.join("build.gradle.kts"),
        gradle_root.join("build.gradle"),
    ];

    for path in &candidates {
        if !path.is_file() {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Some(list) = variant_manager::parse_variants_from_gradle(path, &content) {
            if !list.variants.is_empty() {
                return Ok(restore_active(list));
            }
        }
    }

    // No explicitly declared variants found — return an empty list.
    // The caller should use get_variants_from_gradle for the full picture.
    Ok(restore_active(VariantList::default()))
}

/// Authoritative variant list — obtained by running
/// `./gradlew :app:tasks --console=plain`.
///
/// Scoped to the `:app` module (where all build variants live) and does not
/// use `--all` or `--group` flags that vary by Gradle version.
/// The output includes all `assemble*` and `install*` tasks for every variant
/// the project defines, regardless of how complex its configuration is.
///
/// This is the source of truth; it discovers every variant the project
/// actually has.
#[tauri::command]
pub async fn get_variants_from_gradle(
    fs_state: State<'_, FsState>,
) -> Result<VariantList, String> {
    let gradle_root = resolve_gradle_root(&fs_state).await?;

    let gradlew = gradle_root.join("gradlew");
    if !gradlew.is_file() {
        return Err("gradlew not found — cannot detect variants".to_string());
    }

    let (settings, _) = settings_manager::load_settings();

    // Ensure gradlew is executable before spawning it (on macOS/Linux the file
    // may not have execute permission set, causing Permission denied errors).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&gradlew) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(&gradlew, perms);
        }
    }

    // Pass JAVA_HOME and Android SDK paths so gradlew can start even when
    // they are not on the system PATH.
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(java_home) = settings.java.home.as_deref() {
        env.push(("JAVA_HOME".into(), java_home.into()));
    }
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        env.push(("ANDROID_HOME".into(), sdk.into()));
        env.push(("ANDROID_SDK_ROOT".into(), sdk.into()));
    }

    // Try `:app:tasks --all` first (module-scoped, lists every variant task).
    // `--all` is required because newer AGP versions mark individual variant tasks
    // (e.g. assembleDebug, assembleRelease) as "non-public" and they are hidden
    // from the plain `tasks` output without it.
    // Scoping to `:app` keeps the output small and fast.
    for task_arg in [":app:tasks", "tasks"] {
        let mut cmd = tokio::process::Command::new(&gradlew);
        cmd.args([task_arg, "--all", "--console=plain"])
            .current_dir(&gradle_root)
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));

        let output = match cmd.output().await {
            Ok(o) => o,
            Err(e) => return Err(format!("Failed to run gradlew: {e}")),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let combined = format!("{stdout}{stderr}");

        // If the command itself failed completely, try the next arg.
        if !output.status.success() && stdout.trim().is_empty() {
            continue;
        }

        let list = variant_manager::parse_variants_from_tasks_output(&combined);
        if !list.variants.is_empty() {
            return Ok(restore_active(list));
        }
    }

    Err(
        "No build variants found after running 'gradlew tasks'. \
        Make sure JAVA_HOME is configured in Settings and the project builds correctly."
            .to_string(),
    )
}

/// Persist the active build variant to settings.
#[tauri::command]
pub async fn set_active_variant(variant: String) -> Result<(), String> {
    let (mut settings, _) = settings_manager::load_settings();
    settings.build.build_variant = Some(variant);
    settings_manager::save_settings(&settings)
}
