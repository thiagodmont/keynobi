use crate::models::variant::VariantList;
use crate::services::{settings_manager, variant_manager};
use crate::FsState;
use crate::commands::treesitter::TreeSitterState;
use std::path::PathBuf;
use tauri::State;

/// Return the list of build variants discovered for the current project.
///
/// Uses Tree-sitter to parse `build.gradle.kts`; falls back to running
/// `./gradlew :app:tasks --all` if parsing yields no results.
#[tauri::command]
pub async fn get_build_variants(
    fs_state: State<'_, FsState>,
    ts_state: State<'_, TreeSitterState>,
) -> Result<VariantList, String> {
    let gradle_root: PathBuf = {
        let fs = fs_state.0.lock().await;
        fs.gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or("No project open")?
    };

    // Try the canonical locations for the app-level build script.
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

        let mut ts = ts_state.0.lock().await;
        let list = variant_manager::parse_variants_from_gradle(&mut ts, path, &content);
        drop(ts);

        if let Some(l) = list {
            if !l.variants.is_empty() {
                // Restore active variant from settings.
                let settings = settings_manager::load_settings();
                let active = settings.build.build_variant;
                return Ok(VariantList { active, ..l });
            }
        }
    }

    // Fallback: run `gradlew :app:tasks --all --console=plain` and parse.
    let gradlew = gradle_root.join("gradlew");
    if !gradlew.is_file() {
        return Ok(VariantList::default());
    }

    let output = tokio::process::Command::new(&gradlew)
        .args([":app:tasks", "--all", "--console=plain"])
        .current_dir(&gradle_root)
        .output()
        .await
        .map_err(|e| format!("Failed to run gradlew tasks: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut list = variant_manager::parse_variants_from_tasks_output(&stdout);

    // Restore active variant from settings.
    let settings = settings_manager::load_settings();
    list.active = settings.build.build_variant;

    Ok(list)
}

/// Persist the active build variant to settings.
#[tauri::command]
pub async fn set_active_variant(variant: String) -> Result<(), String> {
    let mut settings = settings_manager::load_settings();
    settings.build.build_variant = Some(variant);
    settings_manager::save_settings(&settings)
}
