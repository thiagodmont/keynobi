//! Tauri commands for UI Automator hierarchy capture.

use super::device::validate_device_serial;
use crate::models::error::AppError;
use crate::models::ui_hierarchy::UiHierarchySnapshot;
use crate::services::adb_manager::{DeviceState, get_adb_path};
use crate::services::settings_manager;
use crate::services::ui_hierarchy::capture_ui_hierarchy_snapshot;
use tauri::State;

/// Dump the focused window's accessibility hierarchy from the given device (or the selected device).
#[tauri::command]
pub async fn dump_ui_hierarchy(
    device_serial: Option<String>,
    device_state: State<'_, DeviceState>,
) -> Result<UiHierarchySnapshot, AppError> {
    let serial = match device_serial {
        Some(s) => {
            validate_device_serial(&s)?;
            s
        }
        None => {
            let sel = device_state
                .0
                .lock()
                .await
                .selected_serial
                .clone()
                .ok_or_else(|| {
                    AppError::InvalidInput(
                        "No device selected — pick a device in the sidebar or pass deviceSerial."
                            .to_string(),
                    )
                })?;
            validate_device_serial(&sel)?;
            sel
        }
    };

    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);

    capture_ui_hierarchy_snapshot(&adb, &serial)
        .await
        .map_err(AppError::ProcessFailed)
}
