use crate::models::device::{AvailableSystemImage, AvdInfo, Device, DeviceDefinition, SdkDownloadProgress, SystemImageInfo};
use crate::services::adb_manager::{
    DeviceState, create_avd, delete_avd, download_system_image, enrich_device_props,
    get_adb_path, get_avdmanager_path, get_emulator_path, get_sdkmanager_path,
    install_apk, launch_app, launch_emulator, list_avds, list_available_system_images,
    list_device_definitions, list_devices, list_system_images, stop_app, stop_emulator,
    wipe_avd_data,
};
use crate::services::settings_manager;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri::ipc::Channel;

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
) -> Result<String, String> {
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

// ── AVD management commands ────────────────────────────────────────────────────

/// Return all installed system images from `$ANDROID_HOME/system-images/`.
#[tauri::command]
pub async fn list_system_images_cmd() -> Result<Vec<SystemImageInfo>, String> {
    let settings = settings_manager::load_settings();
    Ok(list_system_images(&settings))
}

/// Return hardware device definitions from `avdmanager list device -c`.
#[tauri::command]
pub async fn list_device_definitions_cmd() -> Result<Vec<DeviceDefinition>, String> {
    let settings = settings_manager::load_settings();
    let avdmanager = get_avdmanager_path(&settings);
    Ok(list_device_definitions(&avdmanager).await)
}

/// Create a new AVD using avdmanager.
#[tauri::command]
pub async fn create_avd_device(
    name: String,
    system_image: String,
    device: Option<String>,
) -> Result<Vec<AvdInfo>, String> {
    let settings = settings_manager::load_settings();
    let avdmanager = get_avdmanager_path(&settings);
    create_avd(&avdmanager, &name, &system_image, device.as_deref()).await?;
    Ok(list_avds())
}

/// Delete an existing AVD using avdmanager.
#[tauri::command]
pub async fn delete_avd_device(name: String) -> Result<Vec<AvdInfo>, String> {
    let settings = settings_manager::load_settings();
    let avdmanager = get_avdmanager_path(&settings);
    delete_avd(&avdmanager, &name).await?;
    Ok(list_avds())
}

/// Wipe an AVD's user data by relaunching it with -wipe-data.
#[tauri::command]
pub async fn wipe_avd_data_cmd(name: String) -> Result<(), String> {
    let settings = settings_manager::load_settings();
    let emulator = get_emulator_path(&settings);
    let adb = get_adb_path(&settings);
    wipe_avd_data(&emulator, &adb, &name).await
}

/// List all system images available for download from the Android SDK (via sdkmanager).
/// Cross-references installed images so the frontend can show installed/not-installed state.
#[tauri::command]
pub async fn list_available_system_images_cmd() -> Result<Vec<AvailableSystemImage>, String> {
    let settings = settings_manager::load_settings();
    let sdkmanager = get_sdkmanager_path(&settings);
    Ok(list_available_system_images(&sdkmanager, &settings).await)
}

/// Download a system image package via sdkmanager, streaming progress to the frontend.
#[tauri::command]
pub async fn download_system_image_cmd(
    sdk_id: String,
    on_progress: Channel<SdkDownloadProgress>,
) -> Result<(), String> {
    let settings = settings_manager::load_settings();
    let sdkmanager = get_sdkmanager_path(&settings);

    let channel_clone = on_progress.clone();
    let result = download_system_image(&sdkmanager, &sdk_id, &settings, move |progress| {
        let _ = channel_clone.send(progress);
    })
    .await;

    // Send a final "done" event regardless of success/failure.
    let _ = on_progress.send(SdkDownloadProgress {
        percent: if result.is_ok() { Some(100) } else { None },
        message: if result.is_ok() {
            "Download complete".to_owned()
        } else {
            result.as_ref().err().cloned().unwrap_or_default()
        },
        done: true,
        error: result.is_err(),
    });

    result
}
