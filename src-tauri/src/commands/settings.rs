use crate::models::error::AppError;
use crate::models::settings::{self, AppSettings};
use crate::services::logcat::LogcatState;
use crate::services::settings_manager;
use tauri::State;

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    let (settings, _) = tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;
    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    app_handle: tauri::AppHandle,
    logcat_state: State<'_, LogcatState>,
    settings: AppSettings,
) -> Result<(), AppError> {
    // Register the Android SDK directory as an accessible fs scope.
    if let Some(ref sdk_path) = settings.android.sdk_path {
        if let Ok(canonical_sdk) = std::path::PathBuf::from(sdk_path).canonicalize() {
            if canonical_sdk.is_dir() {
                use tauri_plugin_fs::FsExt;
                if let Some(scope) = app_handle.try_fs_scope() {
                    let _ = scope.allow_directory(&canonical_sdk, true);
                }
            }
        }
    }

    let ring_cap = settings::clamp_logcat_ring_capacity_usize({
        let mut tmp = settings.clone();
        settings::normalize_logcat_section(&mut tmp.logcat);
        tmp.logcat.ring_max_entries
    });

    tokio::task::spawn_blocking(move || settings_manager::save_settings(&settings))
        .await
        .map_err(|e| AppError::SettingsError(format!("Failed to save settings: {e}")))?
        .map_err(AppError::SettingsError)?;

    {
        let mut state = logcat_state.lock().await;
        state.store.set_capacity(ring_cap);
    }

    Ok(())
}

#[tauri::command]
pub async fn get_default_settings() -> Result<AppSettings, String> {
    Ok(AppSettings::default())
}

#[tauri::command]
pub async fn reset_settings() -> Result<AppSettings, String> {
    tokio::task::spawn_blocking(settings_manager::reset_settings)
        .await
        .map_err(|e| format!("Failed to reset settings: {e}"))?
}

#[tauri::command]
pub async fn detect_sdk_path() -> Result<Option<String>, String> {
    // 1. Fast path: process environment (works when launched from terminal).
    if let Some(path) = settings_manager::detect_android_sdk() {
        return Ok(Some(path));
    }
    // 2. Slow path: login shell (needed when launched from Finder/Dock on macOS,
    //    because GUI apps don't inherit the user's shell environment).
    Ok(settings_manager::detect_android_sdk_from_shell().await)
}

#[tauri::command]
pub async fn detect_java_path() -> Result<Option<String>, String> {
    // 1. Fast path: process environment and known JVM directories.
    if let Some(path) = settings_manager::detect_java_home() {
        return Ok(Some(path));
    }
    // 2. Slow path: login shell.
    Ok(settings_manager::detect_java_home_from_shell().await)
}
