use crate::models::build::{LaunchResult, LaunchTiming};
use crate::models::device::{
    AvailableSystemImage, AvdInfo, Device, DeviceConnectionState, DeviceDefinition, DeviceKind,
    SdkDownloadProgress, SystemImageInfo,
};
use crate::models::error::AppError;
use crate::services::adb_manager::{
    create_avd, delete_avd, download_system_image, enrich_device_props, get_adb_path,
    get_avdmanager_path, get_emulator_path, get_sdkmanager_path, install_apk, launch_app,
    launch_emulator, list_available_system_images, list_avds, list_device_definitions,
    list_devices, list_system_images, stop_app, stop_emulator, validate_avd_name,
    validate_device_profile_id, validate_system_image_id, wipe_avd_data, AmStartTiming,
    DeviceState, DeviceStateInner,
};
use crate::services::build_runner::{attach_launch_timing, BuildState};
use crate::services::settings_manager;
use crate::FsState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{Mutex, Notify};
use ts_rs::TS;

// ── Event payloads ─────────────────────────────────────────────────────────────

/// Payload of `device:list_changed`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DeviceListChangedEvent {
    pub devices: Vec<Device>,
}

fn record_polled_devices(
    state: &mut crate::services::adb_manager::DeviceStateInner,
    devices: Vec<Device>,
) -> DeviceListChangedEvent {
    state.devices = devices.clone();
    DeviceListChangedEvent { devices }
}

/// Serial and connection state of each polled device, in `adb devices` order.
/// A state change on the same serial (e.g. unauthorized → online) counts as a change.
fn device_snapshot(devices: &[Device]) -> Vec<(String, DeviceConnectionState)> {
    devices
        .iter()
        .map(|d| (d.serial.clone(), d.connection_state.clone()))
        .collect()
}

// ── Validation helpers ─────────────────────────────────────────────────────────

/// Thin wrappers over the shared validators (see utils::validation).
pub(crate) fn validate_device_serial(serial: &str) -> Result<(), AppError> {
    crate::utils::validation::validate_device_serial(serial).map_err(AppError::InvalidInput)
}

fn validate_package_name(package: &str) -> Result<(), AppError> {
    crate::utils::validation::validate_package_name(package).map_err(AppError::InvalidInput)
}

fn validate_activity_name(activity: &str) -> Result<(), AppError> {
    crate::utils::validation::validate_activity_name(activity).map_err(AppError::InvalidInput)
}

// ── Device commands ────────────────────────────────────────────────────────────

/// Return the current list of connected ADB devices (physical + emulators).
#[tauri::command]
pub async fn list_adb_devices(device_state: State<'_, DeviceState>) -> Result<Vec<Device>, String> {
    Ok(device_state.0.lock().await.devices.clone())
}

/// Force-refresh the device list from ADB.
#[tauri::command]
pub async fn refresh_devices(device_state: State<'_, DeviceState>) -> Result<Vec<Device>, String> {
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    let mut devices = list_devices(&adb).await;
    for d in &mut devices {
        enrich_device_props(&adb, d).await;
    }
    device_state.0.lock().await.devices = devices.clone();
    Ok(devices)
}

/// Persist the selected device serial to device state.
/// Device selection is managed by the device store, not settings.
#[tauri::command]
pub async fn select_device(
    serial: String,
    device_state: State<'_, DeviceState>,
) -> Result<(), AppError> {
    validate_device_serial(&serial)?;
    device_state.0.lock().await.selected_serial = Some(serial);
    Ok(())
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
    fs_state: State<'_, FsState>,
) -> Result<String, AppError> {
    validate_device_serial(&serial)?;
    let root = {
        let fs = fs_state.0.lock().await;
        fs.gradle_root
            .as_ref()
            .or(fs.project_root.as_ref())
            .cloned()
            .ok_or_else(|| AppError::NotFound("No project is open".into()))?
    };
    let apk = crate::utils::path::validate_apk_within_build_outputs(&root, &apk_path)?;
    let apk_path = apk.to_string_lossy().into_owned();
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    install_apk(&adb, &serial, &apk_path)
        .await
        .map_err(AppError::Io)
}

/// Launch an app on the given device. With `build_id`, the launch time is
/// recorded on that build's history entry: the build whose APK was installed.
#[tauri::command]
pub async fn launch_app_on_device(
    serial: String,
    package: String,
    activity: Option<String>,
    build_id: Option<u32>,
    device_state: State<'_, DeviceState>,
    build_state: State<'_, BuildState>,
) -> Result<LaunchResult, AppError> {
    validate_device_serial(&serial)?;
    validate_package_name(&package)?;
    if let Some(ref activity_name) = activity {
        validate_activity_name(activity_name)?;
    }
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    let outcome = launch_app(&adb, &serial, &package, activity.as_deref())
        .await
        .map_err(AppError::ProcessFailed)?;

    let timing = match outcome.timing {
        Some(measured) => {
            let device = device_state
                .0
                .lock()
                .await
                .devices
                .iter()
                .find(|d| d.serial == serial)
                .cloned();
            Some(launch_timing(measured, &serial, device.as_ref()))
        }
        None => None,
    };
    if let (Some(id), Some(timing)) = (build_id, &timing) {
        // The app launched; failing to record its time must not fail the launch.
        if let Err(e) = attach_launch_timing(&build_state, id, timing.clone()).await {
            tracing::warn!("Launch time not recorded on build #{id}: {e}");
        }
    }
    Ok(LaunchResult {
        output: outcome.description,
        timing,
    })
}

fn launch_timing(measured: AmStartTiming, serial: &str, device: Option<&Device>) -> LaunchTiming {
    LaunchTiming {
        total_ms: measured.total_ms,
        wait_ms: measured.wait_ms,
        launch_state: measured.launch_state,
        measured_at: chrono::Utc::now().to_rfc3339(),
        serial: serial.to_string(),
        avd_name: device.and_then(|d| d.avd_name.clone()),
        model: device.and_then(|d| d.model.clone()),
    }
}

/// Force-stop an app on the given device.
#[tauri::command]
pub async fn stop_app_on_device(serial: String, package: String) -> Result<(), AppError> {
    validate_device_serial(&serial)?;
    validate_package_name(&package)?;
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    stop_app(&adb, &serial, &package)
        .await
        .map_err(AppError::ProcessFailed)
}

/// Return the list of installed AVDs.
#[tauri::command]
pub async fn list_avd_devices() -> Result<Vec<AvdInfo>, String> {
    Ok(list_avds())
}

/// Launch an emulator and wait for it to come online. Returns the serial of
/// the emulator running `avd_name`, which may have been running already.
///
/// Emits `device:list_changed` once the emulator appears in `adb devices`.
#[tauri::command]
pub async fn launch_avd(
    avd_name: String,
    app_handle: AppHandle,
    device_state: State<'_, DeviceState>,
) -> Result<String, String> {
    validate_avd_name(&avd_name)?;

    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    let emulator = get_emulator_path(&settings);

    let serial = launch_emulator(&emulator, &adb, &avd_name).await?.serial;

    // Refresh device list and notify frontend.
    let mut devices = list_devices(&adb).await;
    for d in &mut devices {
        enrich_device_props(&adb, d).await;
    }
    device_state.0.lock().await.devices = devices.clone();
    let _ = app_handle.emit("device:list_changed", DeviceListChangedEvent { devices });

    Ok(serial)
}

/// Kill an emulator and wait until adb no longer lists it.
#[tauri::command]
pub async fn stop_avd(serial: String) -> Result<(), AppError> {
    validate_device_serial(&serial)?;
    let (settings, _) = settings_manager::load_settings();
    let adb = get_adb_path(&settings);
    stop_emulator(&adb, &serial)
        .await
        .map_err(AppError::ProcessFailed)
}

/// Start background polling for device connections (every 3 seconds).
///
/// Emits `device:list_changed` whenever the device list changes.
#[tauri::command]
pub async fn start_device_polling(
    app_handle: AppHandle,
    device_state: State<'_, DeviceState>,
) -> Result<(), String> {
    start_polling_loop(
        &device_state.0,
        DEVICE_POLL_INTERVAL,
        // Resolved on every tick so an Android SDK path change takes effect.
        || get_adb_path(&settings_manager::load_settings().0),
        move |event| {
            let _ = app_handle.emit("device:list_changed", event);
        },
    )
    .await;
    Ok(())
}

/// Stop the background device polling.
#[tauri::command]
pub async fn stop_device_polling(device_state: State<'_, DeviceState>) -> Result<(), String> {
    device_state.0.lock().await.stop_polling();
    Ok(())
}

/// Time between two device polls.
const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Polls on which an online emulator whose AVD name did not resolve (its
/// console still starting) is asked again while the device list is unchanged.
const AVD_NAME_RETRY_POLLS: u32 = 10;

fn has_unnamed_emulator(devices: &[Device]) -> bool {
    devices.iter().any(|d| {
        d.device_kind == DeviceKind::Emulator
            && d.connection_state == DeviceConnectionState::Online
            && d.avd_name.is_none()
    })
}

/// Spawn a polling loop unless one is already running.
async fn start_polling_loop<A, E>(
    state: &Arc<Mutex<DeviceStateInner>>,
    interval: Duration,
    resolve_adb: A,
    emit: E,
) -> Option<tokio::task::JoinHandle<()>>
where
    A: Fn() -> PathBuf + Send + 'static,
    E: Fn(DeviceListChangedEvent) + Send + 'static,
{
    let (generation, wake) = state.lock().await.begin_polling()?;
    Some(tokio::spawn(poll_devices(
        state.clone(),
        generation,
        wake,
        interval,
        resolve_adb,
        emit,
    )))
}

/// Poll `adb devices` every `interval` and emit the list when it changes,
/// until `generation` is no longer current (stop, restart, or shutdown).
async fn poll_devices<A, E>(
    state: Arc<Mutex<DeviceStateInner>>,
    generation: u64,
    wake: Arc<Notify>,
    interval: Duration,
    resolve_adb: A,
    emit: E,
) where
    A: Fn() -> PathBuf,
    E: Fn(DeviceListChangedEvent),
{
    let mut last_snapshot: Vec<(String, DeviceConnectionState)> = vec![];
    let mut last_avd_names: Vec<Option<String>> = vec![];
    let mut avd_name_retries = 0;
    loop {
        // Registered before the check, so a stop from here on wakes the sleep.
        let woken = wake.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();
        if !state.lock().await.is_current_polling(generation) {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = &mut woken => {}
        }
        if !state.lock().await.is_current_polling(generation) {
            break;
        }

        let adb = resolve_adb();
        let mut current = list_devices(&adb).await;
        let current_snapshot = device_snapshot(&current);

        let changed = current_snapshot != last_snapshot;
        if changed || avd_name_retries > 0 {
            // Enrich online devices with API level / version / AVD name.
            for d in &mut current {
                enrich_device_props(&adb, d).await;
            }
            let avd_names: Vec<Option<String>> =
                current.iter().map(|d| d.avd_name.clone()).collect();
            avd_name_retries = if !has_unnamed_emulator(&current) {
                0
            } else if changed {
                AVD_NAME_RETRY_POLLS
            } else {
                avd_name_retries - 1
            };
            if !changed && avd_names == last_avd_names {
                continue;
            }
            last_snapshot = current_snapshot;
            last_avd_names = avd_names;
            let event = {
                let mut state = state.lock().await;
                if !state.is_current_polling(generation) {
                    break;
                }
                record_polled_devices(&mut state, current)
            };
            emit(event);
        }
    }
    tracing::debug!("Device polling stopped");
}

// ── AVD management commands ────────────────────────────────────────────────────

/// Return all installed system images from `$ANDROID_HOME/system-images/`.
#[tauri::command]
pub async fn list_system_images_cmd() -> Result<Vec<SystemImageInfo>, String> {
    let (settings, _) = settings_manager::load_settings();
    Ok(list_system_images(&settings))
}

/// Return hardware device definitions from `avdmanager list device -c`.
#[tauri::command]
pub async fn list_device_definitions_cmd() -> Result<Vec<DeviceDefinition>, String> {
    let (settings, _) = settings_manager::load_settings();
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
    validate_avd_name(&name)?;
    validate_system_image_id(&system_image)?;
    if let Some(device_id) = device.as_deref() {
        validate_device_profile_id(device_id)?;
    }

    let (settings, _) = settings_manager::load_settings();
    let avdmanager = get_avdmanager_path(&settings);
    create_avd(&avdmanager, &name, &system_image, device.as_deref()).await?;
    Ok(list_avds())
}

/// Delete an existing AVD using avdmanager.
#[tauri::command]
pub async fn delete_avd_device(name: String) -> Result<Vec<AvdInfo>, String> {
    validate_avd_name(&name)?;

    let (settings, _) = settings_manager::load_settings();
    let avdmanager = get_avdmanager_path(&settings);
    delete_avd(&avdmanager, &name).await?;
    Ok(list_avds())
}

/// Wipe an AVD's user data by relaunching it with -wipe-data.
#[tauri::command]
pub async fn wipe_avd_data_cmd(name: String) -> Result<(), String> {
    validate_avd_name(&name)?;

    let (settings, _) = settings_manager::load_settings();
    let emulator = get_emulator_path(&settings);
    let adb = get_adb_path(&settings);
    wipe_avd_data(&emulator, &adb, &name).await
}

/// List all system images available for download from the Android SDK (via sdkmanager).
/// Cross-references installed images so the frontend can show installed/not-installed state.
#[tauri::command]
pub async fn list_available_system_images_cmd() -> Result<Vec<AvailableSystemImage>, String> {
    let (settings, _) = settings_manager::load_settings();
    let sdkmanager = get_sdkmanager_path(&settings);
    Ok(list_available_system_images(&sdkmanager, &settings).await)
}

/// Download a system image package via sdkmanager, streaming progress to the frontend.
#[tauri::command]
pub async fn download_system_image_cmd(
    sdk_id: String,
    on_progress: Channel<SdkDownloadProgress>,
) -> Result<(), String> {
    validate_system_image_id(&sdk_id)?;

    let (settings, _) = settings_manager::load_settings();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::device::DeviceKind;
    use crate::services::adb_manager::DeviceStateInner;

    #[test]
    fn valid_serials_pass() {
        assert!(validate_device_serial("emulator-5554").is_ok());
        assert!(validate_device_serial("192.168.1.100:5555").is_ok());
        assert!(validate_device_serial("R5CNA0ZVXXX").is_ok());
        assert!(validate_device_serial("ce121507d0e5de0e03").is_ok());
    }

    #[test]
    fn invalid_serials_rejected() {
        assert!(validate_device_serial("").is_err());
        assert!(validate_device_serial("serial; rm -rf /").is_err());
        assert!(validate_device_serial("$(evil)").is_err());
        assert!(validate_device_serial(&"a".repeat(65)).is_err());
        assert!(validate_device_serial("emulator 5554").is_err()); // space
        assert!(validate_device_serial("emulator\n5554").is_err());
    }

    #[test]
    fn serial_exactly_64_chars_passes() {
        let serial = "a".repeat(64);
        assert!(validate_device_serial(&serial).is_ok());
    }

    #[test]
    fn serial_tab_char_is_rejected() {
        assert!(validate_device_serial("emulator\t5554").is_err());
    }

    #[test]
    fn serial_carriage_return_is_rejected() {
        assert!(validate_device_serial("emulator\r5554").is_err());
    }

    #[test]
    fn record_polled_devices_updates_state_and_event_payload() {
        let device = Device {
            serial: "emulator-5554".to_string(),
            name: "Pixel".to_string(),
            model: None,
            device_kind: DeviceKind::Emulator,
            connection_state: DeviceConnectionState::Online,
            api_level: Some(35),
            android_version: Some("15".to_string()),
            avd_name: None,
        };
        let mut state = DeviceStateInner::new();

        let event = record_polled_devices(&mut state, vec![device.clone()]);

        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].serial, "emulator-5554");
        assert_eq!(event.devices.len(), 1);
        assert_eq!(event.devices[0].serial, device.serial);
    }

    fn polled_device(serial: &str, connection_state: DeviceConnectionState) -> Device {
        Device {
            serial: serial.to_string(),
            name: serial.to_string(),
            model: None,
            device_kind: DeviceKind::Physical,
            connection_state,
            api_level: None,
            android_version: None,
            avd_name: None,
        }
    }

    #[test]
    fn device_snapshot_changes_when_state_changes_on_same_serial() {
        let before = [polled_device(
            "R5CNA0ZVXXX",
            DeviceConnectionState::Unauthorized,
        )];
        let after = [polled_device("R5CNA0ZVXXX", DeviceConnectionState::Online)];

        assert_ne!(device_snapshot(&before), device_snapshot(&after));
    }

    #[test]
    fn device_snapshot_is_equal_for_identical_polls() {
        let poll = || {
            [
                polled_device("emulator-5554", DeviceConnectionState::Online),
                polled_device("R5CNA0ZVXXX", DeviceConnectionState::Offline),
            ]
        };

        assert_eq!(device_snapshot(&poll()), device_snapshot(&poll()));
    }

    #[test]
    fn device_snapshot_keeps_adb_order() {
        let a = polled_device("emulator-5554", DeviceConnectionState::Online);
        let b = polled_device("R5CNA0ZVXXX", DeviceConnectionState::Online);

        assert_ne!(
            device_snapshot(&[a.clone(), b.clone()]),
            device_snapshot(&[b, a])
        );
    }

    #[test]
    fn valid_package_names_pass() {
        assert!(validate_package_name("com.example.app").is_ok());
        assert!(validate_package_name("com.example.my_app").is_ok());
    }

    #[test]
    fn invalid_package_names_are_rejected() {
        assert!(validate_package_name("").is_err());
        assert!(validate_package_name("notapackage").is_err());
        assert!(validate_package_name("com.example; rm -rf").is_err());
        assert!(validate_package_name("com.example app").is_err());
        assert!(validate_package_name("com.example\napp").is_err());
    }

    #[test]
    fn valid_activity_names_pass() {
        assert!(validate_activity_name(".MainActivity").is_ok());
        assert!(validate_activity_name("com.example.app.MainActivity").is_ok());
        assert!(validate_activity_name("com.example.app.MainActivity$Inner").is_ok());
    }

    #[test]
    fn invalid_activity_names_are_rejected() {
        assert!(validate_activity_name("").is_err());
        assert!(validate_activity_name("Main Activity").is_err());
        assert!(validate_activity_name(".MainActivity; rm -rf").is_err());
        assert!(validate_activity_name(".MainActivity\nOther").is_err());
        assert!(validate_activity_name(&format!(".{}", "A".repeat(256))).is_err());
        assert!(validate_activity_name(&format!(".{}", "A".repeat(255))).is_ok());
    }

    // ── Device polling loop ───────────────────────────────────────────────────

    /// An `adb` named `name` that appends `name` to `calls` on every call and
    /// reports one online emulator.
    fn fake_adb(dir: &std::path::Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let adb = dir.join(name);
        std::fs::write(
            &adb,
            format!(
                "#!/bin/sh\necho {name} >> '{}'\ncase \"$1\" in\n\
                 devices) printf 'List of devices attached\\nemulator-5554\\tdevice\\n' ;;\n\
                 *) echo 34 ;;\nesac\n",
                dir.join("calls").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        // So the first poll does not wait on the first-run check of a new file.
        crate::utils::process::test_support::run_once(&adb);
        let _ = std::fs::remove_file(dir.join("calls"));
        adb
    }

    fn calls(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Bounds a wait that only fails on a hang. Polls spawn the fake `adb`,
    /// which a loaded machine can slow down by seconds.
    const HANG_GUARD: Duration = Duration::from_secs(30);

    async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + HANG_GUARD;
        while !done() {
            assert!(std::time::Instant::now() < deadline, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn polling_state() -> Arc<Mutex<DeviceStateInner>> {
        Arc::new(Mutex::new(DeviceStateInner::new()))
    }

    #[tokio::test]
    async fn stop_then_start_leaves_exactly_one_loop() {
        let dir = tempfile::tempdir().unwrap();
        let adb = fake_adb(dir.path(), "adb");
        let state = polling_state();
        let resolver = move || adb.clone();
        // Longer than the test: a loop only polls when woken, so no poll is
        // in flight when it is stopped.
        let interval = Duration::from_secs(60);

        let first = start_polling_loop(&state, interval, resolver.clone(), |_| {})
            .await
            .expect("first start spawns a loop");
        assert!(
            start_polling_loop(&state, interval, resolver.clone(), |_| {})
                .await
                .is_none(),
            "a start while polling must not spawn a second loop"
        );
        // Let the first loop reach its sleep.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Restart inside one interval: the old loop wakes to a polling flag
        // that is true again, but for a newer generation.
        state.lock().await.stop_polling();
        let second = start_polling_loop(&state, interval, resolver, |_| {})
            .await
            .expect("restart spawns a loop");

        tokio::time::timeout(HANG_GUARD, first)
            .await
            .expect("the stale loop keeps running after a restart")
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!second.is_finished(), "the current loop must keep running");

        state.lock().await.stop_polling();
        tokio::time::timeout(HANG_GUARD, second)
            .await
            .expect("the loop outlived its stop")
            .unwrap();
        assert!(calls(dir.path()).is_empty(), "no loop polled");
    }

    #[tokio::test]
    async fn stop_wakes_a_sleeping_loop_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let adb = fake_adb(dir.path(), "adb");
        let state = polling_state();
        let task = start_polling_loop(&state, Duration::from_secs(60), move || adb.clone(), |_| {})
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        state.lock().await.stop_polling();
        tokio::time::timeout(HANG_GUARD, task)
            .await
            .expect("stop waited for the 60 s interval")
            .unwrap();
        assert!(calls(dir.path()).is_empty(), "a stopped loop must not poll");
    }

    #[tokio::test]
    async fn every_tick_uses_the_current_adb_path() {
        let dir = tempfile::tempdir().unwrap();
        let old_adb = fake_adb(dir.path(), "old-sdk-adb");
        let new_adb = fake_adb(dir.path(), "new-sdk-adb");
        let configured = Arc::new(std::sync::Mutex::new(old_adb));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = polling_state();
        let task = start_polling_loop(
            &state,
            Duration::from_millis(30),
            {
                let configured = configured.clone();
                move || configured.lock().unwrap().clone()
            },
            move |event: DeviceListChangedEvent| {
                let _ = tx.send(event);
            },
        )
        .await
        .unwrap();

        let event = tokio::time::timeout(HANG_GUARD, rx.recv())
            .await
            .expect("no device:list_changed")
            .unwrap();
        assert_eq!(event.devices[0].serial, "emulator-5554");
        assert_eq!(state.lock().await.devices.len(), 1);

        // The SDK path changes in settings.
        *configured.lock().unwrap() = new_adb;
        wait_until("a poll with the new adb", || {
            calls(dir.path()).iter().any(|c| c == "new-sdk-adb")
        })
        .await;

        state.lock().await.stop_polling();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn an_emulator_named_late_is_reported_without_a_list_change() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let adb = dir.path().join("adb");
        // The console does not answer the first two AVD name queries (booting).
        std::fs::write(
            &adb,
            format!(
                "#!/bin/sh\ncd '{}'\n\
                 if [ \"$1\" = devices ]; then printf 'List of devices attached\\nemulator-5554\\tdevice\\n'; exit 0; fi\n\
                 if [ \"$3\" = emu ]; then echo x >> queries; \
                 [ \"$(wc -l < queries)\" -gt 2 ] && {{ echo Pixel_7; echo OK; exit 0; }}; exit 1; fi\n\
                 case \"$5\" in *avd_name) echo ;; *) echo 34 ;; esac\n",
                dir.path().display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = polling_state();
        let task = start_polling_loop(
            &state,
            Duration::from_millis(30),
            move || adb.clone(),
            move |event: DeviceListChangedEvent| {
                let _ = tx.send(event);
            },
        )
        .await
        .unwrap();

        async fn next(
            rx: &mut tokio::sync::mpsc::UnboundedReceiver<DeviceListChangedEvent>,
        ) -> DeviceListChangedEvent {
            tokio::time::timeout(Duration::from_secs(10), rx.recv())
                .await
                .expect("no device:list_changed")
                .unwrap()
        }
        assert_eq!(next(&mut rx).await.devices[0].avd_name, None);
        let named = next(&mut rx).await;
        assert_eq!(named.devices[0].avd_name.as_deref(), Some("Pixel_7"));
        assert_eq!(
            state.lock().await.devices[0].avd_name.as_deref(),
            Some("Pixel_7")
        );

        state.lock().await.stop_polling();
        task.await.unwrap();
    }
}
