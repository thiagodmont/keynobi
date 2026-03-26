use crate::models::device::{AvdInfo, Device, DeviceConnectionState, DeviceKind};
use crate::models::settings::AppSettings;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;

// ── ADB path resolution ────────────────────────────────────────────────────────

/// Resolve the `adb` binary path from settings or fall back to PATH.
pub fn get_adb_path(settings: &AppSettings) -> PathBuf {
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        let candidate = expand_tilde(sdk).join("platform-tools").join("adb");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("adb")
}

/// Resolve the `emulator` binary path from settings or fall back to PATH.
pub fn get_emulator_path(settings: &AppSettings) -> PathBuf {
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        let candidate = expand_tilde(sdk).join("emulator").join("emulator");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("emulator")
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

// ── Device listing ─────────────────────────────────────────────────────────────

/// Parse the output of `adb devices -l` into a list of `Device`s.
///
/// The output format is:
/// ```
/// List of devices attached
/// emulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64x transport_id:1
/// ZX1G22ABCD             device usb:338690048X product:redfin model:Pixel_5 device:redfin transport_id:2
/// ```
pub fn parse_devices_output(output: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with("*")
        {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }
        let serial = parts[0].trim().to_owned();
        let rest = parts[1].trim();

        let state = if rest.starts_with("offline") {
            DeviceConnectionState::Offline
        } else if rest.starts_with("unauthorized") {
            DeviceConnectionState::Unauthorized
        } else if rest.starts_with("device") || rest.starts_with("online") {
            DeviceConnectionState::Online
        } else {
            DeviceConnectionState::Unknown
        };

        let device_kind = if serial.starts_with("emulator-") {
            DeviceKind::Emulator
        } else {
            DeviceKind::Physical
        };

        // Parse key=value pairs from the rest of the line.
        let model = extract_kv_pair(rest, "model")
            .map(|s| s.replace('_', " "));
        let name = model.clone().unwrap_or_else(|| serial.clone());

        devices.push(Device {
            serial,
            name,
            model,
            device_kind,
            connection_state: state,
            api_level: None,
            android_version: None,
        });
    }
    devices
}

/// Extract a value from a space-separated `key:value` pair.
fn extract_kv_pair(s: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    s.split_whitespace()
        .find(|t| t.starts_with(&prefix))
        .map(|t| t[prefix.len()..].to_owned())
}

/// Run `adb devices -l` and return parsed device list.
pub async fn list_devices(adb: &Path) -> Vec<Device> {
    let output = Command::new(adb)
        .args(["devices", "-l"])
        .output()
        .await;

    match output {
        Ok(out) => parse_devices_output(&String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            tracing::warn!("Failed to list ADB devices: {e}");
            vec![]
        }
    }
}

/// Enrich a device's API level and Android version by querying device props.
pub async fn enrich_device_props(adb: &Path, device: &mut Device) {
    if device.connection_state != DeviceConnectionState::Online {
        return;
    }
    let serial = device.serial.clone();

    let sdk_out = Command::new(adb)
        .args(["-s", &serial, "shell", "getprop", "ro.build.version.sdk"])
        .output()
        .await;
    if let Ok(out) = sdk_out {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        device.api_level = s.parse().ok();
    }

    let ver_out = Command::new(adb)
        .args(["-s", &serial, "shell", "getprop", "ro.build.version.release"])
        .output()
        .await;
    if let Ok(out) = ver_out {
        device.android_version = Some(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    }
}

// ── APK operations ─────────────────────────────────────────────────────────────

/// Install an APK on a device using `adb install -r -t`.
pub async fn install_apk(adb: &Path, serial: &str, apk_path: &str) -> Result<String, String> {
    let output = Command::new(adb)
        .args(["-s", serial, "install", "-r", "-t", apk_path])
        .output()
        .await
        .map_err(|e| format!("adb install failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let combined = format!("{stdout}{stderr}");

    if combined.contains("Success") || output.status.success() {
        Ok(combined)
    } else {
        Err(format!("APK install failed: {combined}"))
    }
}

/// Launch an app on a device.
///
/// If `activity` is provided, uses `am start -n package/activity`.
/// Otherwise uses `monkey -p package` to launch the default activity.
pub async fn launch_app(
    adb: &Path,
    serial: &str,
    package: &str,
    activity: Option<&str>,
) -> Result<(), String> {
    let output = if let Some(act) = activity {
        Command::new(adb)
            .args(["-s", serial, "shell", "am", "start", "-n", &format!("{package}/{act}")])
            .output()
            .await
    } else {
        Command::new(adb)
            .args([
                "-s", serial, "shell", "monkey", "-p", package,
                "-c", "android.intent.category.LAUNCHER", "1",
            ])
            .output()
            .await
    };

    let out = output.map_err(|e| format!("adb launch failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(format!("Launch failed: {stderr}"))
    }
}

/// Force-stop an app on a device.
pub async fn stop_app(adb: &Path, serial: &str, package: &str) -> Result<(), String> {
    Command::new(adb)
        .args(["-s", serial, "shell", "am", "force-stop", package])
        .output()
        .await
        .map(|_| ())
        .map_err(|e| format!("adb force-stop failed: {e}"))
}

// ── AVD management ─────────────────────────────────────────────────────────────

/// Scan `~/.android/avd/` for installed AVD definitions.
pub fn list_avds() -> Vec<AvdInfo> {
    let avd_dir = match dirs::home_dir() {
        Some(h) => h.join(".android").join("avd"),
        None => return vec![],
    };
    if !avd_dir.is_dir() {
        return vec![];
    }

    let mut avds = Vec::new();
    let entries = match std::fs::read_dir(&avd_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ini") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        if name.is_empty() {
            continue;
        }

        // Read the top-level .ini file to find the AVD directory path.
        let ini_content = std::fs::read_to_string(&path).unwrap_or_default();
        let avd_path = parse_ini_value(&ini_content, "path")
            .map(|p| p.to_owned())
            .unwrap_or_else(|| avd_dir.join(format!("{name}.avd")).to_string_lossy().into_owned());

        // Read the config.ini inside the AVD directory.
        let config_path = PathBuf::from(&avd_path).join("config.ini");
        let config = std::fs::read_to_string(&config_path).unwrap_or_default();
        let target = parse_ini_value(&config, "image.sysdir.1")
            .or_else(|| parse_ini_value(&ini_content, "target"))
            .map(str::to_owned);
        let abi = parse_ini_value(&config, "abi.type").map(str::to_owned);
        let api_level = target.as_deref().and_then(|t| {
            // "android-35" → 35
            t.split('-').last().and_then(|n| n.parse().ok())
        });
        let display_name = parse_ini_value(&config, "avd.ini.displayname")
            .map(str::to_owned)
            .unwrap_or_else(|| name.replace('_', " "));

        avds.push(AvdInfo {
            name,
            display_name,
            target,
            api_level,
            abi,
            path: avd_path,
        });
    }

    avds.sort_by(|a, b| a.name.cmp(&b.name));
    avds
}

/// Read a `key=value` line from a simple `.ini`-style file.
fn parse_ini_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{key}=")) {
            return Some(rest.trim());
        }
    }
    None
}

/// Launch an Android emulator for the given AVD name.
///
/// Spawns `emulator @avd_name -no-boot-anim -gpu auto` and then polls
/// `adb devices` until the emulator appears as online (30s timeout).
pub async fn launch_emulator(
    emulator_bin: &Path,
    adb: &Path,
    avd_name: &str,
) -> Result<String, String> {
    // Spawn detached — we don't wait for the emulator process to exit.
    tokio::process::Command::new(emulator_bin)
        .args([&format!("@{avd_name}"), "-no-boot-anim", "-gpu", "auto"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start emulator: {e}"))?;

    // Poll `adb devices` until the new emulator appears (up to 60 seconds).
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let devices = list_devices(adb).await;
        if let Some(d) = devices.iter().find(|d| {
            d.device_kind == DeviceKind::Emulator
                && d.connection_state == DeviceConnectionState::Online
        }) {
            return Ok(d.serial.clone());
        }
    }
    Err(format!("Emulator '{avd_name}' did not come online within 60 seconds"))
}

/// Kill an emulator via `adb -s <serial> emu kill`.
pub async fn stop_emulator(adb: &Path, serial: &str) -> Result<(), String> {
    Command::new(adb)
        .args(["-s", serial, "emu", "kill"])
        .output()
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to stop emulator: {e}"))
}

// ── State management ───────────────────────────────────────────────────────────

pub struct DeviceStateInner {
    pub devices: Vec<Device>,
    pub selected_serial: Option<String>,
    pub polling: bool,
}

impl DeviceStateInner {
    pub fn new() -> Self {
        Self { devices: vec![], selected_serial: None, polling: false }
    }
}

pub struct DeviceState(pub Mutex<DeviceStateInner>);

impl DeviceState {
    pub fn new() -> Self {
        DeviceState(Mutex::new(DeviceStateInner::new()))
    }
}

impl Default for DeviceState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_online_physical_device() {
        let output = "List of devices attached\n\
ZX1G22ABCD             device usb:338X product:redfin model:Pixel_5 device:redfin transport_id:2\n";
        let devices = parse_devices_output(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "ZX1G22ABCD");
        assert_eq!(devices[0].device_kind, DeviceKind::Physical);
        assert_eq!(devices[0].connection_state, DeviceConnectionState::Online);
        assert_eq!(devices[0].model.as_deref(), Some("Pixel 5"));
    }

    #[test]
    fn parses_emulator_device() {
        let output = "List of devices attached\n\
emulator-5554          device product:sdk model:sdk_gphone transport_id:1\n";
        let devices = parse_devices_output(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_kind, DeviceKind::Emulator);
        assert_eq!(devices[0].serial, "emulator-5554");
    }

    #[test]
    fn parses_offline_device() {
        let output = "List of devices attached\nSOME123    offline\n";
        let devices = parse_devices_output(output);
        assert_eq!(devices[0].connection_state, DeviceConnectionState::Offline);
    }

    #[test]
    fn parses_unauthorized_device() {
        let output = "List of devices attached\nSOME123    unauthorized\n";
        let devices = parse_devices_output(output);
        assert_eq!(devices[0].connection_state, DeviceConnectionState::Unauthorized);
    }

    #[test]
    fn empty_output_returns_empty() {
        let devices = parse_devices_output("List of devices attached\n");
        assert!(devices.is_empty());
    }

    #[test]
    fn parse_ini_value_finds_key() {
        let content = "path=/home/user/.android/avd/Pixel_7.avd\ntarget=android-34\n";
        assert_eq!(parse_ini_value(content, "target"), Some("android-34"));
    }
}
