use crate::models::device::{AvdInfo, Device};
use crate::services::adb_manager::{
    DeviceState, enrich_device_props, get_adb_path, get_emulator_path,
    install_apk, launch_app, launch_emulator, list_avds, list_devices, stop_app, stop_emulator,
};
use crate::services::settings_manager;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

// ── Event payloads ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListChangedEvent {
    pub devices: Vec<Device>,
}

// ── Device commands ────────────────────────────────────────────────────────────

/// Return the current list of connected ADB devices (physical + emulators).
#[tauri::command]
pub async fn list_adb_devices(
    device_state: State<'_, DeviceState>,
) -> Result<Vec<Device>, String> {
    Ok(device_state.0.lock().await.devices.clone())
}

/// Force-refresh the device list from ADB.
#[tauri::command]
pub async fn refresh_devices(
    device_state: State<'_, DeviceState>,
) -> Result<Vec<Device>, String> {
    let settings = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    let mut devices = list_devices(&adb).await;
    for d in &mut devices {
        enrich_device_props(&adb, d).await;
    }
    device_state.0.lock().await.devices = devices.clone();
    Ok(devices)
}

/// Persist the selected device serial to settings and device state.
#[tauri::command]
pub async fn select_device(
    serial: String,
    device_state: State<'_, DeviceState>,
) -> Result<(), String> {
    device_state.0.lock().await.selected_serial = Some(serial.clone());
    let mut settings = settings_manager::load_settings();
    settings.build.selected_device = Some(serial);
    settings_manager::save_settings(&settings)
}

/// Return the currently selected device serial.
#[tauri::command]
pub async fn get_selected_device(
    device_state: State<'_, DeviceState>,
) -> Result<Option<String>, String> {
    Ok(device_state.0.lock().await.selected_serial.clone())
}

/// Install an APK on the given device.
#[tauri::command]
pub async fn install_apk_on_device(
    serial: String,
    apk_path: String,
) -> Result<String, String> {
    let settings = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    install_apk(&adb, &serial, &apk_path).await
}

/// Launch an app on the given device.
#[tauri::command]
pub async fn launch_app_on_device(
    serial: String,
    package: String,
    activity: Option<String>,
) -> Result<(), String> {
    let settings = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    launch_app(&adb, &serial, &package, activity.as_deref()).await
}

/// Force-stop an app on the given device.
#[tauri::command]
pub async fn stop_app_on_device(
    serial: String,
    package: String,
) -> Result<(), String> {
    let settings = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    stop_app(&adb, &serial, &package).await
}

/// Return the list of installed AVDs.
#[tauri::command]
pub async fn list_avd_devices() -> Result<Vec<AvdInfo>, String> {
    Ok(list_avds())
}

/// Launch an emulator and wait for it to come online.
///
/// Emits `device:list_changed` once the emulator appears in `adb devices`.
#[tauri::command]
pub async fn launch_avd(
    avd_name: String,
    app_handle: AppHandle,
    device_state: State<'_, DeviceState>,
) -> Result<String, String> {
    let settings = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    let emulator = get_emulator_path(&settings);

    let serial = launch_emulator(&emulator, &adb, &avd_name).await?;

    // Refresh device list and notify frontend.
    let mut devices = list_devices(&adb).await;
    for d in &mut devices {
        enrich_device_props(&adb, d).await;
    }
    device_state.0.lock().await.devices = devices.clone();
    let _ = app_handle.emit("device:list_changed", DeviceListChangedEvent { devices });

    Ok(serial)
}

/// Kill an emulator.
#[tauri::command]
pub async fn stop_avd(serial: String) -> Result<(), String> {
    let settings = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    stop_emulator(&adb, &serial).await
}

/// Start background polling for device connections (every 3 seconds).
///
/// Emits `device:list_changed` whenever the device list changes.
#[tauri::command]
pub async fn start_device_polling(
    app_handle: AppHandle,
    device_state: State<'_, DeviceState>,
) -> Result<(), String> {
    let already_polling = {
        let mut ds = device_state.0.lock().await;
        if ds.polling {
            true
        } else {
            ds.polling = true;
            false
        }
    };
    if already_polling {
        return Ok(());
    }

    let app = app_handle.clone();
    tokio::spawn(async move {
        let settings = settings_manager::load_settings();
        let adb = get_adb_path(&settings);
        let mut last_serials: Vec<String> = vec![];

        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            let mut current = list_devices(&adb).await;
            let current_serials: Vec<String> = current.iter().map(|d| d.serial.clone()).collect();

            if current_serials != last_serials {
                // Enrich online devices with API level / version.
                for d in &mut current {
                    enrich_device_props(&adb, d).await;
                }
                last_serials = current_serials;
                let _ = app.emit(
                    "device:list_changed",
                    DeviceListChangedEvent { devices: current },
                );
            }
        }
    });

    Ok(())
}

/// Stop the background device polling.
#[tauri::command]
pub async fn stop_device_polling(
    device_state: State<'_, DeviceState>,
) -> Result<(), String> {
    device_state.0.lock().await.polling = false;
    // Note: the background task will continue for one more interval before
    // stopping on the next iteration check. A full cancellation token is
    // left as a future enhancement (Phase 4 cleanup).
    Ok(())
}
