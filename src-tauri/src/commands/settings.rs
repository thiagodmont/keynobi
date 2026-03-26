use crate::models::settings::AppSettings;
use crate::services::settings_manager;

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    Ok(tokio::task::spawn_blocking(settings_manager::load_settings)
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?)
}

#[tauri::command]
pub async fn save_settings(settings: AppSettings) -> Result<(), String> {
    tokio::task::spawn_blocking(move || settings_manager::save_settings(&settings))
        .await
        .map_err(|e| format!("Failed to save settings: {e}"))?
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
    Ok(settings_manager::detect_android_sdk())
}

#[tauri::command]
pub async fn detect_java_path() -> Result<Option<String>, String> {
    Ok(settings_manager::detect_java_home())
}
