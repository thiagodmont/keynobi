use crate::models::device::{AvailableSystemImage, AvdInfo, Device, DeviceConnectionState, DeviceDefinition, DeviceKind, SdkDownloadProgress, SystemImageInfo};
use crate::models::settings::AppSettings;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
/// Strategy (per Android docs):
///   1. Try `adb shell monkey -p <package> 1` — easiest, no activity name needed.
///      Monkey prints "Events injected: 1" to stdout on success.
///   2. If monkey fails (or stdout doesn't confirm injection), fall back to
///      `adb shell am start -n <package>/<activity>` using `aapt dump badging`
///      to discover the main activity.
///
/// Returns a human-readable description of what happened (for build log display).
pub async fn launch_app(
    adb: &Path,
    serial: &str,
    package: &str,
    activity: Option<&str>,
) -> Result<String, String> {
    // If caller already knows the activity, use am start directly.
    if let Some(act) = activity {
        let args = ["-s", serial, "shell", "am", "start", "-n", &format!("{package}/{act}")];
        let out = Command::new(adb)
            .args(args)
            .output()
            .await
            .map_err(|e| format!("adb am start failed: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let combined = format!("{stdout}{stderr}").trim().to_owned();
        if out.status.success() && !stdout.contains("Error") && !stdout.contains("error") {
            return Ok(format!("am start OK: {combined}"));
        }
        return Err(format!("am start failed: {combined}"));
    }

    // Step 1: try monkey.
    let monkey_out = Command::new(adb)
        .args([
            "-s", serial, "shell", "monkey",
            "-p", package,
            "-c", "android.intent.category.LAUNCHER",
            "1",
        ])
        .output()
        .await
        .map_err(|e| format!("adb monkey failed: {e}"))?;

    let monkey_stdout = String::from_utf8_lossy(&monkey_out.stdout).into_owned();
    let monkey_stderr = String::from_utf8_lossy(&monkey_out.stderr).into_owned();
    let monkey_combined = format!("{monkey_stdout}{monkey_stderr}").trim().to_owned();

    // Monkey prints "Events injected: 1" on success. If it printed that, we're done.
    if monkey_stdout.contains("Events injected: 1") {
        return Ok(format!("monkey OK: {monkey_combined}"));
    }

    // Step 2: monkey didn't confirm success — fall back to am start.
    // Use am start -n with the default activity (try common entry points).
    let fallback_result = am_start_fallback(adb, serial, package).await;
    match fallback_result {
        Ok(msg) => Ok(msg),
        Err(am_err) => Err(format!(
            "monkey: {monkey_combined} | am start fallback: {am_err}"
        )),
    }
}

/// Try to launch using `am start` with common activity name patterns,
/// falling back to a `.MainActivity` convention if all else fails.
async fn am_start_fallback(adb: &Path, serial: &str, package: &str) -> Result<String, String> {
    // Try the most common activity name patterns.
    let candidates = [
        format!("{package}/.MainActivity"),
        format!("{package}/.main.MainActivity"),
        format!("{package}/com.google.android.apps.internal.Main"),
    ];

    let mut last_err = String::new();
    for activity in &candidates {
        let out = Command::new(adb)
            .args(["-s", serial, "shell", "am", "start", "-n", activity])
            .output()
            .await
            .map_err(|e| format!("adb failed: {e}"))?;

        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let combined = format!("{stdout}{stderr}").trim().to_owned();

        if out.status.success() && !stdout.contains("Error:") && !stdout.contains("does not exist") {
            return Ok(format!("am start OK ({activity}): {combined}"));
        }
        last_err = combined;
    }

    // Last resort: am start without activity (launches default).
    let out = Command::new(adb)
        .args([
            "-s", serial, "shell", "am", "start",
            "-a", "android.intent.action.MAIN",
            "-c", "android.intent.category.LAUNCHER",
            package,
        ])
        .output()
        .await
        .map_err(|e| format!("adb am start failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let combined = format!("{stdout}{stderr}").trim().to_owned();

    if out.status.success() && !stdout.contains("Error:") {
        Ok(format!("am start (intent) OK: {combined}"))
    } else {
        Err(format!("{last_err} | intent: {combined}"))
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

// ── avdmanager operations ──────────────────────────────────────────────────────

/// Resolve the `avdmanager` binary path.
///
/// Checks `$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager` and versioned
/// paths (`cmdline-tools/*/bin/avdmanager`) before falling back to PATH.
pub fn get_avdmanager_path(settings: &AppSettings) -> PathBuf {
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        let sdk_root = expand_tilde(sdk);

        // Try the canonical "latest" path first.
        let latest = sdk_root.join("cmdline-tools").join("latest").join("bin").join("avdmanager");
        if latest.is_file() {
            return latest;
        }

        // Try versioned paths (e.g. cmdline-tools/12.0/bin/avdmanager).
        if let Ok(entries) = std::fs::read_dir(sdk_root.join("cmdline-tools")) {
            let mut versioned: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path().join("bin").join("avdmanager"))
                .filter(|p| p.is_file())
                .collect();
            versioned.sort();
            if let Some(found) = versioned.into_iter().last() {
                return found;
            }
        }
    }
    PathBuf::from("avdmanager")
}

/// Scan `$ANDROID_HOME/system-images/` for installed system images.
///
/// Directory layout: `system-images/<target>/<variant>/<abi>/`.
pub fn list_system_images(settings: &AppSettings) -> Vec<SystemImageInfo> {
    let sdk = match settings.android.sdk_path.as_deref() {
        Some(s) => expand_tilde(s),
        None => return vec![],
    };
    let images_dir = sdk.join("system-images");
    if !images_dir.is_dir() {
        return vec![];
    }

    let mut images = Vec::new();
    let targets = match std::fs::read_dir(&images_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for target_entry in targets.flatten() {
        let target_path = target_entry.path();
        if !target_path.is_dir() { continue; }
        let target_name = target_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();
        // "android-35" → 35
        let api_level: u32 = target_name.strip_prefix("android-")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if api_level == 0 { continue; }

        let variants = match std::fs::read_dir(&target_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for variant_entry in variants.flatten() {
            let variant_path = variant_entry.path();
            if !variant_path.is_dir() { continue; }
            let variant = variant_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();

            let abis = match std::fs::read_dir(&variant_path) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for abi_entry in abis.flatten() {
                let abi_path = abi_entry.path();
                if !abi_path.is_dir() { continue; }
                let abi = abi_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();
                if abi.is_empty() { continue; }

                let sdk_id = format!("system-images;{target_name};{variant};{abi}");
                let variant_label = match variant.as_str() {
                    "google_apis" => "Google APIs".to_owned(),
                    "google_apis_playstore" => "Google Play".to_owned(),
                    "default" => "AOSP".to_owned(),
                    other => other.replace('_', " "),
                };
                let android_ver = api_to_android_version(api_level);
                let display_name = format!("Android {android_ver} (API {api_level}) · {variant_label} · {abi}");

                images.push(SystemImageInfo { sdk_id, api_level, variant: variant.clone(), abi, display_name });
            }
        }
    }

    // Sort: highest API first, then by variant preference, then ABI.
    images.sort_by(|a, b| {
        b.api_level.cmp(&a.api_level)
            .then(variant_sort_key(&a.variant).cmp(&variant_sort_key(&b.variant)))
            .then(a.abi.cmp(&b.abi))
    });
    images
}

fn variant_sort_key(v: &str) -> u8 {
    match v {
        "google_apis_playstore" => 0,
        "google_apis"           => 1,
        "default"               => 2,
        _                       => 3,
    }
}

fn api_to_android_version(api: u32) -> &'static str {
    match api {
        36 => "16.0", 35 => "15.0", 34 => "14.0", 33 => "13.0",
        32 => "12L",  31 => "12.0", 30 => "11.0", 29 => "10.0",
        28 => "9.0",  27 => "8.1",  26 => "8.0",  25 => "7.1",
        24 => "7.0",  _  => "?",
    }
}

/// Run `avdmanager list device -c` and return phone/tablet hardware profiles.
pub async fn list_device_definitions(avdmanager: &Path) -> Vec<DeviceDefinition> {
    let output = Command::new(avdmanager)
        .args(["list", "device", "-c"])
        .output()
        .await;

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("avdmanager list device failed: {e}");
            return vec![];
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_device_definitions(&stdout)
}

/// Parse `avdmanager list device -c` compact output.
///
/// Format (one device per line):
/// ```
/// id: 0 or "pixel_7"
/// Name: Pixel 7
/// OEM : Google
/// Tag : default
/// ---------
/// ```
fn parse_device_definitions(output: &str) -> Vec<DeviceDefinition> {
    let mut devices = Vec::new();
    let mut id = String::new();
    let mut name = String::new();
    let mut manufacturer = String::new();
    let mut tag = String::new();

    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("id:") {
            // Extract the quoted id or numeric id.
            let rest = line.trim_start_matches("id:").trim();
            // "id: 0 or \"pixel_7\"" → extract the quoted part if present.
            if let Some(start) = rest.find('"') {
                if let Some(end) = rest.rfind('"') {
                    if end > start {
                        id = rest[start + 1..end].to_owned();
                    }
                }
            } else {
                id = rest.to_owned();
            }
        } else if line.starts_with("Name:") {
            name = line.trim_start_matches("Name:").trim().to_owned();
        } else if line.starts_with("OEM") {
            manufacturer = line.split(':').nth(1).unwrap_or("").trim().to_owned();
        } else if line.starts_with("Tag") {
            tag = line.split(':').nth(1).unwrap_or("").trim().to_owned();
        } else if line.starts_with("---") {
            // End of a device block — include only phone/tablet profiles.
            if !id.is_empty() && !name.is_empty()
                && !matches!(tag.as_str(), "android-tv" | "android-automotive" | "wear" | "chromeos")
            {
                devices.push(DeviceDefinition {
                    id: id.clone(),
                    name: name.clone(),
                    manufacturer: manufacturer.clone(),
                });
            }
            id.clear(); name.clear(); manufacturer.clear(); tag.clear();
        }
    }
    // Handle last entry without trailing separator.
    if !id.is_empty() && !name.is_empty()
        && !matches!(tag.as_str(), "android-tv" | "android-automotive" | "wear" | "chromeos")
    {
        devices.push(DeviceDefinition { id, name, manufacturer });
    }

    devices
}

/// Create a new AVD using `avdmanager`.
///
/// `name` must contain only alphanumeric characters, underscores, hyphens, dots, and spaces.
/// `sdk_id` is the full SDK package string (e.g. `"system-images;android-35;google_apis;arm64-v8a"`).
pub async fn create_avd(
    avdmanager: &Path,
    name: &str,
    sdk_id: &str,
    device_id: Option<&str>,
) -> Result<(), String> {
    // Build argument list.
    let mut args = vec![
        "create", "avd",
        "--name", name,
        "-k", sdk_id,
        "--force",
    ];
    if let Some(dev) = device_id {
        args.push("--device");
        args.push(dev);
    }

    // avdmanager prompts "Do you wish to create a custom hardware profile? [no]"
    // Pipe "no\n" to stdin to accept the default.
    let mut child = tokio::process::Command::new(avdmanager)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start avdmanager: {e}"))?;

    if let Some(stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let mut stdin = stdin;
        let _ = stdin.write_all(b"no\n").await;
    }

    let output = child.wait_with_output().await
        .map_err(|e| format!("avdmanager create failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        Ok(())
    } else {
        Err(format!("AVD creation failed: {stderr}{stdout}"))
    }
}

/// Delete an existing AVD using `avdmanager delete avd -n <name>`.
pub async fn delete_avd(avdmanager: &Path, name: &str) -> Result<(), String> {
    let output = tokio::process::Command::new(avdmanager)
        .args(["delete", "avd", "--name", name])
        .output()
        .await
        .map_err(|e| format!("Failed to start avdmanager: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("AVD deletion failed: {stderr}"))
    }
}

/// Wipe an emulator's user data by relaunching it with `-wipe-data`.
pub async fn wipe_avd_data(
    emulator_bin: &Path,
    adb: &Path,
    avd_name: &str,
) -> Result<(), String> {
    tokio::process::Command::new(emulator_bin)
        .args([&format!("@{avd_name}"), "-wipe-data", "-no-boot-anim", "-gpu", "auto"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start emulator: {e}"))?;

    // Wait up to 30s for it to come online.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let devices = list_devices(adb).await;
        if devices.iter().any(|d| {
            d.device_kind == DeviceKind::Emulator && d.connection_state == DeviceConnectionState::Online
        }) {
            return Ok(());
        }
    }
    Ok(())
}

// ── sdkmanager operations ──────────────────────────────────────────────────────

/// Resolve the `sdkmanager` binary path, mirroring `get_avdmanager_path`.
pub fn get_sdkmanager_path(settings: &AppSettings) -> PathBuf {
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        let sdk_root = expand_tilde(sdk);
        let latest = sdk_root.join("cmdline-tools").join("latest").join("bin").join("sdkmanager");
        if latest.is_file() {
            return latest;
        }
        if let Ok(entries) = std::fs::read_dir(sdk_root.join("cmdline-tools")) {
            let mut versioned: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path().join("bin").join("sdkmanager"))
                .filter(|p| p.is_file())
                .collect();
            versioned.sort();
            if let Some(found) = versioned.into_iter().last() {
                return found;
            }
        }
    }
    PathBuf::from("sdkmanager")
}

/// Query `sdkmanager --list` and return all available system images.
///
/// Cross-references with locally installed images (from `list_system_images`)
/// to set the `installed` flag.
pub async fn list_available_system_images(
    sdkmanager: &Path,
    settings: &AppSettings,
) -> Vec<AvailableSystemImage> {
    let output = tokio::process::Command::new(sdkmanager)
        .args(["--list", "--include_obsolete"])
        .env("JAVA_HOME", get_java_home(settings))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("sdkmanager --list failed: {e}");
            return vec![];
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let installed_ids: std::collections::HashSet<String> = list_system_images(settings)
        .into_iter()
        .map(|i| i.sdk_id)
        .collect();

    parse_sdkmanager_list(&stdout, &installed_ids)
}

fn get_java_home(settings: &AppSettings) -> String {
    settings.java.home.clone().unwrap_or_default()
}

fn parse_sdkmanager_list(
    output: &str,
    installed_ids: &std::collections::HashSet<String>,
) -> Vec<AvailableSystemImage> {
    let mut images = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        // Lines look like: "  system-images;android-35;google_apis;arm64-v8a | 1           | Android SDK System Image"
        if !trimmed.starts_with("system-images;") {
            continue;
        }
        let sdk_id = trimmed.split('|').next().unwrap_or("").trim().to_owned();
        let parts: Vec<&str> = sdk_id.split(';').collect();
        if parts.len() < 4 {
            continue;
        }
        // parts: ["system-images", "android-35", "google_apis", "arm64-v8a"]
        let target = parts[1]; // "android-35"
        let variant = parts[2].to_owned();
        let abi = parts[3].to_owned();
        let api_level: u32 = target.strip_prefix("android-")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if api_level == 0 { continue; }

        let installed = installed_ids.contains(&sdk_id);
        let variant_label = match variant.as_str() {
            "google_apis" => "Google APIs",
            "google_apis_playstore" => "Google Play",
            "default" => "AOSP",
            other => other,
        };
        let android_ver = api_to_android_version(api_level);
        let display_name = format!("Android {android_ver} (API {api_level}) · {variant_label} · {abi}");

        images.push(AvailableSystemImage { sdk_id, api_level, variant, abi, display_name, installed });
    }

    // Sort: highest API first, then variant preference, then ABI.
    images.sort_by(|a, b| {
        b.api_level.cmp(&a.api_level)
            .then(variant_sort_key(&a.variant).cmp(&variant_sort_key(&b.variant)))
            .then(a.abi.cmp(&b.abi))
    });
    images
}

/// Download a system image package using `sdkmanager`, streaming progress
/// via a Tauri `Channel<SdkDownloadProgress>`.
///
/// sdkmanager outputs lines like:
/// ```
/// [=====                                 ] 14% Downloading sdk-tools-linux-...
/// [=======================================] 100% Computing updates...
/// ```
pub async fn download_system_image(
    sdkmanager: &Path,
    sdk_id: &str,
    settings: &AppSettings,
    on_progress: impl Fn(SdkDownloadProgress) + Send + 'static,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = tokio::process::Command::new(sdkmanager)
        .arg(sdk_id)
        .env("JAVA_HOME", get_java_home(settings))
        // Accept the license agreement automatically.
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start sdkmanager: {e}"))?;

    // Auto-accept license prompts (sdkmanager may ask "y/n").
    if let Some(stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let mut stdin = stdin;
        // Send 'y' for each potential prompt (max 10).
        let _ = stdin.write_all(b"y\ny\ny\ny\ny\ny\ny\ny\ny\ny\n").await;
    }

    let stderr = child.stderr.take();
    let stdout = child.stdout.take();

    // Read stderr for progress (sdkmanager writes progress to stderr).
    let progress_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim().to_owned();
                if trimmed.is_empty() { continue; }
                let percent = parse_sdkmanager_progress(&trimmed);
                on_progress(SdkDownloadProgress {
                    percent,
                    message: trimmed,
                    done: false,
                    error: false,
                });
            }
        }
        // Drain stdout silently.
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        }
    });

    let status = child.wait().await
        .map_err(|e| format!("sdkmanager wait failed: {e}"))?;

    let _ = progress_task.await;

    if status.success() {
        Ok(())
    } else {
        Err(format!("sdkmanager exited with status {}", status.code().unwrap_or(-1)))
    }
}

fn parse_sdkmanager_progress(line: &str) -> Option<u32> {
    // Match "[====...] 73% ..." or "73%"
    if let Some(pct_start) = line.find('%') {
        let before = &line[..pct_start];
        // Walk backwards to find digits.
        let digits: String = before.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
        let digits_rev: String = digits.chars().rev().collect();
        if let Ok(n) = digits_rev.parse::<u32>() {
            if n <= 100 { return Some(n); }
        }
    }
    None
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

pub struct DeviceState(pub Arc<Mutex<DeviceStateInner>>);

impl DeviceState {
    pub fn new() -> Self {
        DeviceState(Arc::new(Mutex::new(DeviceStateInner::new())))
    }
}

impl Clone for DeviceState {
    fn clone(&self) -> Self {
        DeviceState(self.0.clone())
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
