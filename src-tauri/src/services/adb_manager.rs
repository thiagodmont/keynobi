use crate::models::device::{
    AvailableSystemImage, AvdInfo, Device, DeviceConnectionState, DeviceDefinition, DeviceKind,
    SdkDownloadProgress, SystemImageInfo,
};
use crate::models::settings::AppSettings;
use crate::utils::device_shell::quote_device_shell_arg;
use crate::utils::process::{
    describe_failure, output_with_timeout, timed_out, AAPT2_TIMEOUT, ADB_INSTALL_TIMEOUT,
    ADB_LAUNCH_TIMEOUT, ADB_QUERY_TIMEOUT, ADB_UNRESPONSIVE_HINT, AVDMANAGER_TIMEOUT,
    EMULATOR_STOP_TIMEOUT, SDKMANAGER_LIST_TIMEOUT, SDK_TOOL_HINT,
};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify};

// ── External tool argument validation ─────────────────────────────────────────

/// Validate an Android Virtual Device name before passing it to emulator/avdmanager.
pub fn validate_avd_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("AVD name must not be empty".to_string());
    }
    if name.len() > 128 {
        return Err("AVD name is too long (max 128 characters)".to_string());
    }
    if name.starts_with('-') {
        return Err("AVD name must not start with '-'".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
    {
        return Err(
            "AVD name may only contain letters, numbers, spaces, '_', '-', and '.'".to_string(),
        );
    }
    Ok(())
}

/// Validate an avdmanager hardware profile id.
pub fn validate_device_profile_id(device_id: &str) -> Result<(), String> {
    if device_id.is_empty() {
        return Err("Device profile id must not be empty".to_string());
    }
    if device_id.len() > 128 {
        return Err("Device profile id is too long (max 128 characters)".to_string());
    }
    if device_id.starts_with('-') {
        return Err("Device profile id must not start with '-'".to_string());
    }
    if !device_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(
            "Device profile id may only contain letters, numbers, '_', '-', and '.'".to_string(),
        );
    }
    Ok(())
}

/// Validate a sdkmanager system image package id.
pub fn validate_system_image_id(sdk_id: &str) -> Result<(), String> {
    let parts: Vec<&str> = sdk_id.split(';').collect();
    if parts.len() != 4 || parts[0] != "system-images" {
        return Err(
            "System image id must look like system-images;android-35;google_apis;x86_64"
                .to_string(),
        );
    }
    if !parts[1]
        .strip_prefix("android-")
        .is_some_and(|api| !api.is_empty() && api.chars().all(|c| c.is_ascii_digit()))
    {
        return Err("System image id must include an android API target".to_string());
    }
    for part in &parts[2..] {
        if part.is_empty()
            || part.starts_with('-')
            || !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(
                "System image id components may only contain letters, numbers, '_', '-', and '.'"
                    .to_string(),
            );
        }
    }
    Ok(())
}

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

// ── aapt2 utilities ────────────────────────────────────────────────────────────

/// Find the `aapt2` binary in `$ANDROID_HOME/build-tools/<latest>/aapt2`.
///
/// Scans build-tools subdirectories, parses their names as version tuples,
/// and returns the path inside the highest-versioned directory that actually
/// contains an `aapt2` executable.
pub fn find_aapt2(settings: &AppSettings) -> Option<PathBuf> {
    let sdk_path = settings.android.sdk_path.as_deref()?;
    let build_tools = expand_tilde(sdk_path).join("build-tools");

    let mut best: Option<(Vec<u32>, PathBuf)> = None;

    let entries = std::fs::read_dir(&build_tools).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("aapt2");
        if !candidate.is_file() {
            continue;
        }
        let dir_name = entry.file_name();
        let ver_str = dir_name.to_string_lossy();
        let parts: Vec<u32> = ver_str.split('.').filter_map(|p| p.parse().ok()).collect();
        if parts.is_empty() {
            continue;
        }
        if best.as_ref().is_none_or(|(prev, _)| parts > *prev) {
            best = Some((parts, candidate));
        }
    }

    best.map(|(_, path)| path)
}

/// Extract the package name from an APK using `aapt2 dump packagename <apk>`.
///
/// This is the authoritative method — it reads the package name directly from
/// the APK binary, including any `applicationIdSuffix` from the build variant.
/// Returns `None` if `aapt2` fails or produces no parseable output.
pub async fn get_package_name_from_apk(aapt2: &Path, apk_path: &Path) -> Option<String> {
    let out = output_with_timeout(
        Command::new(aapt2).args(["dump", "packagename", apk_path.to_str()?]),
        AAPT2_TIMEOUT,
    )
    .await
    .map_err(|e| tracing::warn!("aapt2 dump packagename: {e}"))
    .ok()?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

// ── Device listing ─────────────────────────────────────────────────────────────

/// Parse the output of `adb devices -l` into a list of `Device`s.
///
/// The output format is:
/// ```text
/// List of devices attached
/// emulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64x transport_id:1
/// ZX1G22ABCD             device usb:338690048X product:redfin model:Pixel_5 device:redfin transport_id:2
/// ```
pub fn parse_devices_output(output: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") || line.starts_with("*") {
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
        let model = extract_kv_pair(rest, "model").map(|s| s.replace('_', " "));
        let name = model.clone().unwrap_or_else(|| serial.clone());

        devices.push(Device {
            serial,
            name,
            model,
            device_kind,
            connection_state: state,
            api_level: None,
            android_version: None,
            avd_name: None,
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
///
/// Returns an empty list (and logs) if adb fails or does not answer within
/// [`ADB_QUERY_TIMEOUT`], so the polling loop keeps running.
pub async fn list_devices(adb: &Path) -> Vec<Device> {
    list_devices_within(adb, ADB_QUERY_TIMEOUT).await
}

async fn list_devices_within(adb: &Path, timeout: Duration) -> Vec<Device> {
    try_list_devices_within(adb, timeout)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("{e}");
            vec![]
        })
}

/// `adb devices -l`, or why it could not be listed. Unlike [`list_devices`],
/// a failure here is not an empty list.
async fn try_list_devices_within(adb: &Path, timeout: Duration) -> Result<Vec<Device>, String> {
    let out = output_with_timeout(Command::new(adb).args(["devices", "-l"]), timeout)
        .await
        .map_err(|e| describe_failure("adb devices", &e, ADB_UNRESPONSIVE_HINT))?;
    if !out.status.success() {
        return Err(format!(
            "adb devices failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(parse_devices_output(&String::from_utf8_lossy(&out.stdout)))
}

/// Enrich an online device's API level and Android version from its
/// properties, and an emulator's AVD name (see [`resolve_avd_name`]).
pub async fn enrich_device_props(adb: &Path, device: &mut Device) {
    if device.connection_state != DeviceConnectionState::Online {
        return;
    }
    let serial = device.serial.clone();
    if device.device_kind == DeviceKind::Emulator {
        device.avd_name = resolve_avd_name(adb, &serial).await;
    }

    let sdk_out = output_with_timeout(
        Command::new(adb).args(["-s", &serial, "shell", "getprop", "ro.build.version.sdk"]),
        ADB_QUERY_TIMEOUT,
    )
    .await;
    match sdk_out {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            device.api_level = s.parse().ok();
        }
        // An unresponsive device must not stall the poll loop a second time.
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            tracing::warn!("adb getprop on {serial}: {e}");
            return;
        }
        Err(_) => {}
    }

    let ver_out = output_with_timeout(
        Command::new(adb).args([
            "-s",
            &serial,
            "shell",
            "getprop",
            "ro.build.version.release",
        ]),
        ADB_QUERY_TIMEOUT,
    )
    .await;
    if let Ok(out) = ver_out {
        device.android_version = Some(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    }
}

// ── APK operations ─────────────────────────────────────────────────────────────

/// Install an APK on a device using `adb install -r -t`.
pub async fn install_apk(adb: &Path, serial: &str, apk_path: &str) -> Result<String, String> {
    install_apk_within(adb, serial, apk_path, ADB_INSTALL_TIMEOUT).await
}

async fn install_apk_within(
    adb: &Path,
    serial: &str,
    apk_path: &str,
    timeout: Duration,
) -> Result<String, String> {
    let output = output_with_timeout(
        Command::new(adb).args(["-s", serial, "install", "-r", "-t", apk_path]),
        timeout,
    )
    .await
    .map_err(|e| {
        describe_failure(
            "adb install",
            &e,
            "check the device for an install or verification prompt, \
             then reconnect it or run `adb kill-server` and try again",
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let combined = format!("{stdout}{stderr}");

    if combined.contains("Success") || output.status.success() {
        Ok(combined)
    } else {
        Err(format!("APK install failed: {combined}"))
    }
}

/// Query the device for the launcher activity component of `package`.
///
/// Uses `adb shell cmd package resolve-activity --brief` which returns the
/// fully-qualified component (e.g. `com.example.app/.MainActivity`) on API 23+.
/// Returns `None` if the package is not installed or has no LAUNCHER activity.
async fn try_resolve_launcher(adb: &Path, serial: &str, package: &str) -> Option<String> {
    let out = output_with_timeout(
        Command::new(adb).args([
            "-s",
            serial,
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "-c",
            "android.intent.category.LAUNCHER",
            &quote_device_shell_arg(package),
        ]),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| tracing::warn!("adb resolve-activity on {serial}: {e}"))
    .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Output is two lines: priority integer then the component string.
    // Find the first line that contains '/' — that is the component.
    stdout
        .lines()
        .find(|l| l.contains('/'))
        .map(|l| l.trim().to_string())
}

/// Find the effective installed package name when the caller only knows the
/// base `applicationId` (without build-type / flavor suffixes).
///
/// Runs `adb shell pm list packages` and collects `base_package` and every
/// package extending it at a `.` or `:` boundary (`com.example.app.debug`, not
/// `com.example.apple`). Returns the match if there is exactly one,
/// so we can resolve the correct package for builds like `demoDebug` where the
/// installed package is `com.example.app.demo.debug` but the stored ID is
/// `com.example.app`.
async fn discover_effective_package(
    adb: &Path,
    serial: &str,
    base_package: &str,
) -> Option<String> {
    let out = output_with_timeout(
        Command::new(adb).args(["-s", serial, "shell", "pm", "list", "packages"]),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| tracing::warn!("adb pm list packages on {serial}: {e}"))
    .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let matches = variant_packages(&stdout, base_package);
    // If there is exactly one candidate (or an exact match), use it.
    if let [only] = matches.as_slice() {
        return Some(only.clone());
    }
    // If the base_package itself is among the candidates, prefer the exact match.
    if matches.iter().any(|m| m == base_package) {
        return Some(base_package.to_string());
    }
    None
}

/// Packages in `pm list packages` output (`package:<name>` lines) that are
/// `base_package` or extend it at a `.` or `:` boundary.
fn variant_packages(pm_list_output: &str, base_package: &str) -> Vec<String> {
    pm_list_output
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(str::trim)
        .filter(|name| {
            name.strip_prefix(base_package)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', ':']))
        })
        .map(str::to_string)
        .collect()
}

/// The line in `am start` output that reports a failure, if any.
///
/// `am start` usually exits 0 even when nothing started ("Error: Activity not
/// started, unable to resolve Intent"), so its output must be checked too.
pub fn am_start_failure(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| {
            line.get(..5)
                .is_some_and(|head| head.eq_ignore_ascii_case("error"))
                || line.starts_with("Security exception")
                || line.starts_with("java.lang.")
                || line.starts_with("Exception occurred while executing")
        })
        .map(str::to_string)
}

/// `Ok(combined output)` when `am start` succeeded, else `Err` with the failure.
fn check_am_start(out: &std::process::Output) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}").trim().to_owned();
    if out.status.success() && am_start_failure(&combined).is_none() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

/// Whether `serial` is an adb-over-network device (`host:port`, or an mDNS
/// name such as `adb-XXXX._adb-tls-connect._tcp`). Such a device is reached
/// through its own network connection.
pub fn is_wireless_adb_serial(serial: &str) -> bool {
    if serial.contains("._adb-tls-connect._tcp") || serial.contains("._adb._tcp") {
        return true;
    }
    serial.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Launch an app on a device.
///
/// Strategy:
///   1. Ask the device for the LAUNCHER component via `cmd package resolve-activity`.
///      This is the authoritative answer and handles `applicationIdSuffix` variants.
///   2. If the package isn't found under the given name, enumerate installed packages
///      to discover the effective package (e.g. `com.example.app.demo.debug` when
///      the stored ID is `com.example.app`), then repeat step 1 with the real name.
///   3. Fall back to `adb shell monkey` with the effective package name.
///   4. Last resort: `am start -a android.intent.action.MAIN` intent.
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
        let args = [
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "-n",
            &quote_device_shell_arg(&format!("{package}/{act}")),
        ];
        let out = output_with_timeout(Command::new(adb).args(args), ADB_LAUNCH_TIMEOUT)
            .await
            .map_err(|e| describe_failure("adb am start", &e, ADB_UNRESPONSIVE_HINT))?;
        return check_am_start(&out)
            .map(|combined| format!("am start OK: {combined}"))
            .map_err(|combined| format!("am start failed: {combined}"));
    }

    // Step 1: ask the device for the LAUNCHER activity of the given package name.
    if let Some(component) = try_resolve_launcher(adb, serial, package).await {
        let out = output_with_timeout(
            Command::new(adb).args([
                "-s",
                serial,
                "shell",
                "am",
                "start",
                "-n",
                &quote_device_shell_arg(&component),
            ]),
            ADB_LAUNCH_TIMEOUT,
        )
        .await
        .map_err(|e| describe_failure("adb am start", &e, ADB_UNRESPONSIVE_HINT))?;
        if let Ok(combined) = check_am_start(&out) {
            return Ok(format!("am start OK ({component}): {combined}"));
        }
    }

    // Step 2: package not found under the given name — try to discover the
    // effective package (e.g. the installed variant has a build-type/flavor suffix).
    let effective_package = discover_effective_package(adb, serial, package)
        .await
        .unwrap_or_else(|| package.to_string());

    if effective_package != package {
        if let Some(component) = try_resolve_launcher(adb, serial, &effective_package).await {
            let out = output_with_timeout(
                Command::new(adb).args([
                    "-s",
                    serial,
                    "shell",
                    "am",
                    "start",
                    "-n",
                    &quote_device_shell_arg(&component),
                ]),
                ADB_LAUNCH_TIMEOUT,
            )
            .await
            .map_err(|e| describe_failure("adb am start", &e, ADB_UNRESPONSIVE_HINT))?;
            if let Ok(combined) = check_am_start(&out) {
                return Ok(format!("am start OK ({component}): {combined}"));
            }
        }
    }

    // Step 3: fall back to monkey with the effective package name.
    let monkey_out = output_with_timeout(
        Command::new(adb).args([
            "-s",
            serial,
            "shell",
            "monkey",
            "-p",
            &quote_device_shell_arg(&effective_package),
            "-c",
            "android.intent.category.LAUNCHER",
            "1",
        ]),
        ADB_LAUNCH_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("adb monkey", &e, ADB_UNRESPONSIVE_HINT))?;

    let monkey_stdout = String::from_utf8_lossy(&monkey_out.stdout).into_owned();
    let monkey_stderr = String::from_utf8_lossy(&monkey_out.stderr).into_owned();
    let monkey_combined = format!("{monkey_stdout}{monkey_stderr}").trim().to_owned();

    if monkey_stdout.contains("Events injected: 1") {
        return Ok(format!("monkey OK: {monkey_combined}"));
    }

    // Step 4: last resort — fire the MAIN/LAUNCHER intent.
    let out = output_with_timeout(
        Command::new(adb).args([
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.LAUNCHER",
            &quote_device_shell_arg(&effective_package),
        ]),
        ADB_LAUNCH_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("adb am start", &e, ADB_UNRESPONSIVE_HINT))?;

    match check_am_start(&out) {
        Ok(combined) => Ok(format!("am start (intent) OK: {combined}")),
        Err(combined) => Err(format!(
            "processFailed: {monkey_combined} | intent: {combined}"
        )),
    }
}

/// Force-stop an app on a device.
pub async fn stop_app(adb: &Path, serial: &str, package: &str) -> Result<(), String> {
    let out = output_with_timeout(
        Command::new(adb).args([
            "-s",
            serial,
            "shell",
            "am",
            "force-stop",
            &quote_device_shell_arg(package),
        ]),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("adb force-stop", &e, ADB_UNRESPONSIVE_HINT))?;

    if out.status.success() {
        Ok(())
    } else {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .trim()
        .to_owned();
        Err(format!(
            "adb force-stop failed (exit {}): {combined}",
            out.status.code().unwrap_or(-1)
        ))
    }
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
            .unwrap_or_else(|| {
                avd_dir
                    .join(format!("{name}.avd"))
                    .to_string_lossy()
                    .into_owned()
            });

        // Read the config.ini inside the AVD directory.
        let config_path = PathBuf::from(&avd_path).join("config.ini");
        let config = std::fs::read_to_string(&config_path).unwrap_or_default();
        let target = parse_ini_value(&config, "image.sysdir.1")
            .or_else(|| parse_ini_value(&ini_content, "target"))
            .map(str::to_owned);
        let abi = parse_ini_value(&config, "abi.type").map(str::to_owned);
        let api_level = target.as_deref().and_then(|t| {
            // "android-35" → 35
            t.split('-').next_back().and_then(|n| n.parse().ok())
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

// ── Emulators ──────────────────────────────────────────────────────────────────

/// Upper bound for one `emu avd name` console query. Device enrichment and the
/// launch and wipe waits call it; a stuck console connection must not stall
/// them beyond this.
const EMU_AVD_NAME_TIMEOUT: Duration = Duration::from_secs(5);

/// System properties an emulator sets to its AVD name (newer images first).
const AVD_NAME_PROPS: [&str; 2] = ["ro.boot.qemu.avd_name", "ro.kernel.qemu.avd_name"];

/// How often and how long to poll `adb devices` while an emulator starts or stops.
#[derive(Debug, Clone, Copy)]
struct EmulatorWait {
    interval: Duration,
    limit: Duration,
}

/// Launch: the AVD's emulator must come online.
const LAUNCH_WAIT: EmulatorWait = EmulatorWait {
    interval: Duration::from_secs(2),
    limit: Duration::from_secs(60),
};
/// Wipe: the AVD's emulator, relaunched with `-wipe-data`, must come online.
const WIPE_WAIT: EmulatorWait = EmulatorWait {
    interval: Duration::from_secs(2),
    limit: Duration::from_secs(30),
};
/// Stop: after `emu kill`, the emulator must leave `adb devices`. It may save
/// a Quick Boot snapshot first.
const STOP_WAIT: EmulatorWait = EmulatorWait {
    interval: Duration::from_secs(1),
    limit: Duration::from_secs(30),
};

/// Resolve the AVD a running emulator was started from: the console's
/// `emu avd name`, else the AVD name properties. `None` while the emulator
/// cannot answer yet (still booting) or when the answer is not an AVD name.
pub async fn resolve_avd_name(adb: &Path, serial: &str) -> Option<String> {
    if let Some(name) = emulator_avd_name(adb, serial).await {
        return Some(name);
    }
    for prop in AVD_NAME_PROPS {
        match output_with_timeout(
            Command::new(adb).args(["-s", serial, "shell", "getprop", prop]),
            ADB_QUERY_TIMEOUT,
        )
        .await
        {
            Ok(out) if out.status.success() => {
                if let Some(name) = parse_avd_name(&String::from_utf8_lossy(&out.stdout)) {
                    return Some(name);
                }
            }
            Ok(_) => {}
            // An unresponsive device must not stall the caller once per property.
            Err(e) => {
                tracing::debug!("adb getprop {prop} on {serial}: {e}");
                return None;
            }
        }
    }
    None
}

/// `adb -s <serial> emu avd name`: the AVD name on the first line, then `OK`.
async fn emulator_avd_name(adb: &Path, serial: &str) -> Option<String> {
    let output = output_with_timeout(
        Command::new(adb).args(["-s", serial, "emu", "avd", "name"]),
        EMU_AVD_NAME_TIMEOUT,
    )
    .await
    .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_avd_name(&String::from_utf8_lossy(&output.stdout))
}

/// The AVD name on the first line of `output`. A console error such as
/// `KO: unknown command` or an empty property is not a name.
fn parse_avd_name(output: &str) -> Option<String> {
    let name = output.lines().next()?.trim();
    validate_avd_name(name).ok()?;
    Some(name.to_string())
}

/// The serial of a listed emulator running `avd_name`. `online_only` skips
/// emulators that are still booting or disconnected.
async fn avd_serial(
    adb: &Path,
    devices: &[Device],
    avd_name: &str,
    online_only: bool,
) -> Option<String> {
    for d in devices.iter().filter(|d| {
        d.device_kind == DeviceKind::Emulator
            && (!online_only || d.connection_state == DeviceConnectionState::Online)
    }) {
        if resolve_avd_name(adb, &d.serial).await.as_deref() == Some(avd_name) {
            return Some(d.serial.clone());
        }
    }
    None
}

/// AVDs this process is starting (launch or wipe). Starting an AVD that is
/// already starting would fail on the AVD's lock.
static STARTING_AVDS: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());

/// Marks an AVD as starting until dropped.
struct StartingAvd(String);

impl StartingAvd {
    /// `None` when another request in this process is already starting it.
    fn claim(avd_name: &str) -> Option<Self> {
        let mut starting = STARTING_AVDS.lock().unwrap_or_else(PoisonError::into_inner);
        starting
            .insert(avd_name.to_string())
            .then(|| StartingAvd(avd_name.to_string()))
    }
}

impl Drop for StartingAvd {
    fn drop(&mut self) {
        STARTING_AVDS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Spawn `emulator @<avd_name> <extra_args> -no-boot-anim -gpu auto`. The
/// emulator outlives the call; the handle only tells whether it exited early.
fn spawn_emulator(
    emulator_bin: &Path,
    avd_name: &str,
    extra_args: &[&str],
) -> Result<tokio::process::Child, String> {
    Command::new(emulator_bin)
        .arg(format!("@{avd_name}"))
        .args(extra_args)
        .args(["-no-boot-anim", "-gpu", "auto"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start emulator: {e}"))
}

/// Poll until an emulator running `avd_name` is online and return its serial.
///
/// Only emulators that were not online in `before` are candidates, and each is
/// confirmed by its AVD name, so another emulator coming online cannot satisfy
/// the wait. `child` is the emulator process this request started: if it exits
/// with a failure before its AVD is online (for example on the AVD's lock),
/// that is reported at once.
async fn wait_for_avd_online(
    adb: &Path,
    avd_name: &str,
    before: &[Device],
    mut child: Option<tokio::process::Child>,
    wait: EmulatorWait,
) -> Result<String, String> {
    let mut other_avds: HashSet<String> = HashSet::new();
    let deadline = std::time::Instant::now() + wait.limit;
    loop {
        tokio::time::sleep(wait.interval).await;
        let devices = list_devices(adb).await;
        for serial in newly_online_emulator_serials_since(before, &devices) {
            if other_avds.contains(&serial) {
                continue;
            }
            match resolve_avd_name(adb, &serial).await {
                Some(name) if name == avd_name => return Ok(serial),
                Some(_) => {
                    other_avds.insert(serial);
                }
                // Not answering yet (booting); ask again on the next poll.
                None => {}
            }
        }
        if let Some(status) = child.as_mut().and_then(|c| c.try_wait().ok().flatten()) {
            if !status.success() {
                return Err(format!(
                    "The emulator for '{avd_name}' exited ({status}) before it came online. \
                     Check that the AVD is not already running in another window or tool."
                ));
            }
            child = None;
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Emulator '{avd_name}' did not come online within {} s",
                wait.limit.as_secs()
            ));
        }
    }
}

/// An emulator returned by [`launch_emulator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedEmulator {
    pub serial: String,
    /// The AVD was already running, or another request in this process was
    /// starting it, so this call did not start another emulator.
    pub already_running: bool,
}

/// Launch an Android emulator for the given AVD name and return its serial
/// once it is online in `adb devices` (60 s limit).
///
/// An AVD that is already running is not started again; its serial is
/// returned. The serial is always that of the emulator running `avd_name`,
/// even while other emulators start at the same time.
pub async fn launch_emulator(
    emulator_bin: &Path,
    adb: &Path,
    avd_name: &str,
) -> Result<LaunchedEmulator, String> {
    launch_emulator_with(emulator_bin, adb, avd_name, LAUNCH_WAIT).await
}

async fn launch_emulator_with(
    emulator_bin: &Path,
    adb: &Path,
    avd_name: &str,
    wait: EmulatorWait,
) -> Result<LaunchedEmulator, String> {
    let Some(_starting) = StartingAvd::claim(avd_name) else {
        let serial = wait_for_avd_online(adb, avd_name, &[], None, wait).await?;
        return Ok(LaunchedEmulator {
            serial,
            already_running: true,
        });
    };
    let before = list_devices(adb).await;
    if let Some(serial) = avd_serial(adb, &before, avd_name, true).await {
        return Ok(LaunchedEmulator {
            serial,
            already_running: true,
        });
    }
    let child = spawn_emulator(emulator_bin, avd_name, &[])?;
    let serial = wait_for_avd_online(adb, avd_name, &before, Some(child), wait).await?;
    Ok(LaunchedEmulator {
        serial,
        already_running: false,
    })
}

/// Emulator serials online in `after` that were NOT online in `before`.
///
/// This also counts a serial that was listed (offline) before and came back
/// online: a relaunched emulator typically reacquires its previous console
/// port (`emulator-5554`).
///
/// Returns ALL matching serials — an unrelated emulator becoming online first
/// must not shadow the one being waited for.
fn newly_online_emulator_serials_since(before: &[Device], after: &[Device]) -> Vec<String> {
    let before_online: HashSet<&str> = before
        .iter()
        .filter(|d| {
            d.device_kind == DeviceKind::Emulator
                && d.connection_state == DeviceConnectionState::Online
        })
        .map(|d| d.serial.as_str())
        .collect();

    after
        .iter()
        .filter(|d| {
            d.device_kind == DeviceKind::Emulator
                && d.connection_state == DeviceConnectionState::Online
                && !before_online.contains(d.serial.as_str())
        })
        .map(|d| d.serial.clone())
        .collect()
}

/// Stop an emulator with `adb -s <serial> emu kill` and wait until it has left
/// `adb devices` (30 s limit). A refused or failed kill is an error, and so is
/// an emulator still listed at the limit.
pub async fn stop_emulator(adb: &Path, serial: &str) -> Result<(), String> {
    stop_emulator_with(adb, serial, STOP_WAIT).await
}

async fn stop_emulator_with(adb: &Path, serial: &str, wait: EmulatorWait) -> Result<(), String> {
    let out = output_with_timeout(
        Command::new(adb).args(["-s", serial, "emu", "kill"]),
        EMULATOR_STOP_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("adb emu kill", &e, ADB_UNRESPONSIVE_HINT))?;
    let output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if let Some(reason) = emu_kill_failure(out.status.success(), &output) {
        return Err(format!("Could not stop {serial}: {reason}"));
    }

    let deadline = std::time::Instant::now() + wait.limit;
    loop {
        tokio::time::sleep(wait.interval).await;
        match try_list_devices_within(adb, ADB_QUERY_TIMEOUT).await {
            Ok(devices) if !devices.iter().any(|d| d.serial == serial) => return Ok(()),
            Ok(_) => {}
            // adb not answering says nothing about the emulator; keep waiting.
            Err(e) => tracing::debug!("{e}"),
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "{serial} accepted the stop request but is still listed by adb after {} s",
                wait.limit.as_secs()
            ));
        }
    }
}

/// Why `adb emu kill` failed, if it did. adb reports some failures with exit
/// status 0, and the emulator console answers `KO: …` to a refused command.
fn emu_kill_failure(success: bool, output: &str) -> Option<String> {
    let error_line = output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("KO") || line.to_ascii_lowercase().starts_with("error"));
    match (success, error_line) {
        (_, Some(line)) => Some(line.to_string()),
        (true, None) => None,
        (false, None) if output.trim().is_empty() => Some("adb exited with an error".to_string()),
        (false, None) => Some(output.trim().to_string()),
    }
}

/// Wipe an emulator's user data by relaunching it with `-wipe-data`, and wait
/// until that AVD's emulator is online again (30 s limit).
///
/// Refused while the AVD is running or being started: the relaunch would fail
/// on the AVD's lock.
pub async fn wipe_avd_data(emulator_bin: &Path, adb: &Path, avd_name: &str) -> Result<(), String> {
    wipe_avd_data_with(emulator_bin, adb, avd_name, WIPE_WAIT).await
}

async fn wipe_avd_data_with(
    emulator_bin: &Path,
    adb: &Path,
    avd_name: &str,
    wait: EmulatorWait,
) -> Result<(), String> {
    let Some(_starting) = StartingAvd::claim(avd_name) else {
        return Err(format!(
            "'{avd_name}' is starting. Stop it once it is running, then wipe its data."
        ));
    };
    let before = list_devices(adb).await;
    if let Some(serial) = avd_serial(adb, &before, avd_name, false).await {
        return Err(format!(
            "'{avd_name}' is running as {serial}. Stop it before wiping its data."
        ));
    }
    let child = spawn_emulator(emulator_bin, avd_name, &["-wipe-data"])?;
    wait_for_avd_online(adb, avd_name, &before, Some(child), wait)
        .await
        .map(|_| ())
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
        let latest = sdk_root
            .join("cmdline-tools")
            .join("latest")
            .join("bin")
            .join("avdmanager");
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
        if !target_path.is_dir() {
            continue;
        }
        let target_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        // "android-35" → 35
        let api_level: u32 = target_name
            .strip_prefix("android-")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if api_level == 0 {
            continue;
        }

        let variants = match std::fs::read_dir(&target_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for variant_entry in variants.flatten() {
            let variant_path = variant_entry.path();
            if !variant_path.is_dir() {
                continue;
            }
            let variant = variant_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_owned();

            let abis = match std::fs::read_dir(&variant_path) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for abi_entry in abis.flatten() {
                let abi_path = abi_entry.path();
                if !abi_path.is_dir() {
                    continue;
                }
                let abi = abi_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned();
                if abi.is_empty() {
                    continue;
                }

                let sdk_id = format!("system-images;{target_name};{variant};{abi}");
                let variant_label = match variant.as_str() {
                    "google_apis" => "Google APIs".to_owned(),
                    "google_apis_playstore" => "Google Play".to_owned(),
                    "default" => "AOSP".to_owned(),
                    other => other.replace('_', " "),
                };
                let android_ver = api_to_android_version(api_level);
                let display_name =
                    format!("Android {android_ver} (API {api_level}) · {variant_label} · {abi}");

                images.push(SystemImageInfo {
                    sdk_id,
                    api_level,
                    variant: variant.clone(),
                    abi,
                    display_name,
                });
            }
        }
    }

    // Sort: highest API first, then by variant preference, then ABI.
    images.sort_by(|a, b| {
        b.api_level
            .cmp(&a.api_level)
            .then(variant_sort_key(&a.variant).cmp(&variant_sort_key(&b.variant)))
            .then(a.abi.cmp(&b.abi))
    });
    images
}

fn variant_sort_key(v: &str) -> u8 {
    match v {
        "google_apis_playstore" => 0,
        "google_apis" => 1,
        "default" => 2,
        _ => 3,
    }
}

fn api_to_android_version(api: u32) -> &'static str {
    match api {
        36 => "16.0",
        35 => "15.0",
        34 => "14.0",
        33 => "13.0",
        32 => "12L",
        31 => "12.0",
        30 => "11.0",
        29 => "10.0",
        28 => "9.0",
        27 => "8.1",
        26 => "8.0",
        25 => "7.1",
        24 => "7.0",
        _ => "?",
    }
}

/// Run `avdmanager list device -c` and return phone/tablet hardware profiles.
pub async fn list_device_definitions(avdmanager: &Path) -> Vec<DeviceDefinition> {
    let output = output_with_timeout(
        Command::new(avdmanager).args(["list", "device", "-c"]),
        AVDMANAGER_TIMEOUT,
    )
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
/// ```text
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
            if !id.is_empty()
                && !name.is_empty()
                && !matches!(
                    tag.as_str(),
                    "android-tv" | "android-automotive" | "wear" | "chromeos"
                )
            {
                devices.push(DeviceDefinition {
                    id: id.clone(),
                    name: name.clone(),
                    manufacturer: manufacturer.clone(),
                });
            }
            id.clear();
            name.clear();
            manufacturer.clear();
            tag.clear();
        }
    }
    // Handle last entry without trailing separator.
    if !id.is_empty()
        && !name.is_empty()
        && !matches!(
            tag.as_str(),
            "android-tv" | "android-automotive" | "wear" | "chromeos"
        )
    {
        devices.push(DeviceDefinition {
            id,
            name,
            manufacturer,
        });
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
    let mut args = vec!["create", "avd", "--name", name, "-k", sdk_id, "--force"];
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
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start avdmanager: {e}"))?;

    let run = async move {
        if let Some(stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            let _ = stdin.write_all(b"no\n").await;
        }
        child.wait_with_output().await
    };
    let output = tokio::time::timeout(AVDMANAGER_TIMEOUT, run)
        .await
        .unwrap_or_else(|_| Err(timed_out(AVDMANAGER_TIMEOUT)))
        .map_err(|e| describe_failure("avdmanager create", &e, SDK_TOOL_HINT))?;

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
    let output = output_with_timeout(
        tokio::process::Command::new(avdmanager).args(["delete", "avd", "--name", name]),
        AVDMANAGER_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("avdmanager delete", &e, SDK_TOOL_HINT))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("AVD deletion failed: {stderr}"))
    }
}

// ── sdkmanager operations ──────────────────────────────────────────────────────

/// Resolve the `sdkmanager` binary path, mirroring `get_avdmanager_path`.
pub fn get_sdkmanager_path(settings: &AppSettings) -> PathBuf {
    if let Some(sdk) = settings.android.sdk_path.as_deref() {
        let sdk_root = expand_tilde(sdk);
        let latest = sdk_root
            .join("cmdline-tools")
            .join("latest")
            .join("bin")
            .join("sdkmanager");
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
    let output = output_with_timeout(
        tokio::process::Command::new(sdkmanager)
            .args(["--list", "--include_obsolete"])
            .env("JAVA_HOME", get_java_home(settings))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null()),
        SDKMANAGER_LIST_TIMEOUT,
    )
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
    crate::services::jdk::java_home_for_gradle(settings, None).unwrap_or_default()
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
        let api_level: u32 = target
            .strip_prefix("android-")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if api_level == 0 {
            continue;
        }

        let installed = installed_ids.contains(&sdk_id);
        let variant_label = match variant.as_str() {
            "google_apis" => "Google APIs",
            "google_apis_playstore" => "Google Play",
            "default" => "AOSP",
            other => other,
        };
        let android_ver = api_to_android_version(api_level);
        let display_name =
            format!("Android {android_ver} (API {api_level}) · {variant_label} · {abi}");

        images.push(AvailableSystemImage {
            sdk_id,
            api_level,
            variant,
            abi,
            display_name,
            installed,
        });
    }

    // Sort: highest API first, then variant preference, then ABI.
    images.sort_by(|a, b| {
        b.api_level
            .cmp(&a.api_level)
            .then(variant_sort_key(&a.variant).cmp(&variant_sort_key(&b.variant)))
            .then(a.abi.cmp(&b.abi))
    });
    images
}

/// Download a system image package using `sdkmanager`, streaming progress
/// via a Tauri `Channel<SdkDownloadProgress>`.
///
/// sdkmanager outputs lines like:
/// ```text
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
                if trimmed.is_empty() {
                    continue;
                }
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

    let status = child
        .wait()
        .await
        .map_err(|e| format!("sdkmanager wait failed: {e}"))?;

    let _ = progress_task.await;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "sdkmanager exited with status {}",
            status.code().unwrap_or(-1)
        ))
    }
}

fn parse_sdkmanager_progress(line: &str) -> Option<u32> {
    // Match "[====...] 73% ..." or "73%"
    if let Some(pct_start) = line.find('%') {
        let before = &line[..pct_start];
        // Walk backwards to find digits.
        let digits: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let digits_rev: String = digits.chars().rev().collect();
        if let Ok(n) = digits_rev.parse::<u32>() {
            if n <= 100 {
                return Some(n);
            }
        }
    }
    None
}

// ── State management ───────────────────────────────────────────────────────────

pub struct DeviceStateInner {
    pub devices: Vec<Device>,
    pub selected_serial: Option<String>,
    polling: bool,
    /// Bumped by every start and stop; a polling loop runs only while its
    /// generation is current, so a stop followed by a start never leaves two.
    polling_generation: u64,
    /// Wakes a sleeping polling loop so a stop takes effect at once.
    polling_wake: Arc<Notify>,
}

impl Default for DeviceStateInner {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceStateInner {
    pub fn new() -> Self {
        Self {
            devices: vec![],
            selected_serial: None,
            polling: false,
            polling_generation: 0,
            polling_wake: Arc::new(Notify::new()),
        }
    }

    /// Mark polling as running and return the new loop's generation and wake
    /// signal, or `None` when a loop is already running.
    pub fn begin_polling(&mut self) -> Option<(u64, Arc<Notify>)> {
        if self.polling {
            return None;
        }
        self.polling = true;
        self.polling_generation += 1;
        Some((self.polling_generation, self.polling_wake.clone()))
    }

    /// Stop the running polling loop and wake it so it exits now.
    pub fn stop_polling(&mut self) {
        self.polling = false;
        self.polling_generation += 1;
        self.polling_wake.notify_waiters();
    }

    /// Whether the polling loop of `generation` should keep running.
    pub fn is_current_polling(&self, generation: u64) -> bool {
        self.polling && self.polling_generation == generation
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

/// Resolve a device serial: use the provided one, or fall back to the first
/// online device reported by `adb devices`.
pub async fn resolve_device_serial(
    adb: &std::path::PathBuf,
    requested: Option<&str>,
) -> Option<String> {
    if let Some(s) = requested {
        return Some(s.to_string());
    }
    let output = output_with_timeout(
        tokio::process::Command::new(adb).arg("devices"),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            "{}",
            describe_failure("adb devices", &e, ADB_UNRESPONSIVE_HINT)
        )
    })
    .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .skip(1)
        .find(|l| l.contains("\tdevice"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use crate::utils::device_shell::test_support::{fake_adb, recorded_calls};

    #[tokio::test]
    async fn launch_app_passes_inner_class_activity_intact() {
        let dir = tempfile::tempdir().unwrap();
        let (adb, record) = fake_adb(dir.path());

        launch_app(
            &adb,
            "emulator-5554",
            "com.example.app",
            Some(".Main$Inner"),
        )
        .await
        .unwrap();

        assert_eq!(
            recorded_calls(&record),
            vec![vec!["am", "start", "-n", "com.example.app/.Main$Inner"]]
        );
    }

    use super::*;

    /// An `adb` that never answers, like a wedged adb server.
    fn hanging_adb(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let adb = dir.join("adb");
        std::fs::write(&adb, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        adb
    }

    /// Longer than the deadlines below, much shorter than the hung adb.
    const TEST_GUARD: Duration = Duration::from_secs(10);

    #[test]
    fn variant_packages_match_only_at_a_package_boundary() {
        let pm = "package:com.example.app.debug\npackage:com.example.apple\n\
                  package:com.example.app\npackage:com.example.application.demo\n";
        assert_eq!(
            variant_packages(pm, "com.example.app"),
            vec!["com.example.app.debug", "com.example.app"]
        );
        assert!(variant_packages("package:com.example.apple\n", "com.example.app").is_empty());
    }

    #[test]
    fn am_start_failure_detects_errors_despite_exit_zero() {
        for output in [
            "Starting: Intent { act=android.intent.action.VIEW dat=myapp://x }\n\
             Error: Activity not started, unable to resolve Intent { act=android.intent.action.VIEW }",
            "Error type 3\nError: Activity class {com.example.app/.Missing} does not exist.",
            "Security exception: Permission Denial: starting Intent",
            "Exception occurred while executing 'start':\njava.lang.SecurityException: Permission Denial",
            "error: device offline",
        ] {
            assert!(am_start_failure(output).is_some(), "{output}");
        }
    }

    #[test]
    fn am_start_failure_accepts_successful_starts() {
        for output in [
            "Starting: Intent { act=android.intent.action.VIEW dat=myapp://Error/42 }",
            "Starting: Intent { cmp=com.example.app/.Main }\n\
             Warning: Activity not started, intent has been delivered to currently running top-most instance.",
            "",
        ] {
            assert_eq!(am_start_failure(output), None, "{output}");
        }
    }

    #[test]
    fn wireless_serials_are_detected() {
        for serial in [
            "192.168.1.5:5555",
            "localhost:5555",
            "[fe80::1]:37000",
            "adb-R58M12ABCDE-a1b2c3._adb-tls-connect._tcp",
            "adb-R58M12ABCDE-a1b2c3._adb-tls-connect._tcp.",
            "adb-R58M12ABCDE._adb._tcp",
        ] {
            assert!(is_wireless_adb_serial(serial), "{serial}");
        }
        for serial in [
            "emulator-5554",
            "R58M12ABCDE",
            "0123456789ABCDEF",
            "device:",
            ":5555",
        ] {
            assert!(!is_wireless_adb_serial(serial), "{serial}");
        }
    }

    /// An `adb` that answers every call with `stdout` and exit status 0.
    fn adb_printing(dir: &Path, stdout: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let adb = dir.join("adb");
        std::fs::write(&adb, format!("#!/bin/sh\necho '{stdout}'\n")).unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        adb
    }

    #[tokio::test]
    async fn launch_app_reports_an_unresolved_activity_as_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let adb = adb_printing(
            dir.path(),
            "Error: Activity class {com.example.app/.Missing} does not exist.",
        );

        let err = launch_app(&adb, "emulator-5554", "com.example.app", Some(".Missing"))
            .await
            .unwrap_err();

        assert!(err.contains("does not exist"), "{err}");
    }

    #[tokio::test]
    async fn list_devices_gives_up_on_a_hung_adb() {
        let dir = tempfile::tempdir().unwrap();
        let adb = hanging_adb(dir.path());

        let start = std::time::Instant::now();
        let devices = tokio::time::timeout(
            TEST_GUARD,
            list_devices_within(&adb, Duration::from_millis(300)),
        )
        .await
        .expect("list_devices blocked on a hung adb");

        assert!(devices.is_empty());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn install_apk_reports_a_hung_install() {
        let dir = tempfile::tempdir().unwrap();
        let adb = hanging_adb(dir.path());

        let err = tokio::time::timeout(
            TEST_GUARD,
            install_apk_within(
                &adb,
                "emulator-5554",
                "/nonexistent/app.apk",
                Duration::from_millis(300),
            ),
        )
        .await
        .expect("install_apk blocked on a hung adb")
        .unwrap_err();

        assert!(
            err.starts_with("adb install timed out after 300 ms"),
            "{err}"
        );
        assert!(err.contains("adb kill-server"), "{err}");
    }

    fn test_device(
        serial: &str,
        device_kind: DeviceKind,
        connection_state: DeviceConnectionState,
    ) -> Device {
        Device {
            serial: serial.to_string(),
            name: serial.to_string(),
            model: None,
            device_kind,
            connection_state,
            api_level: None,
            android_version: None,
            avd_name: None,
        }
    }

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
        assert_eq!(
            devices[0].connection_state,
            DeviceConnectionState::Unauthorized
        );
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

    #[test]
    fn avd_argument_validators_accept_sdk_tool_ids() {
        assert!(validate_avd_name("Pixel 8_API_35").is_ok());
        assert!(validate_device_profile_id("pixel_8").is_ok());
        assert!(validate_system_image_id("system-images;android-35;google_apis;x86_64").is_ok());
    }

    #[test]
    fn avd_argument_validators_reject_shell_sensitive_input() {
        assert!(validate_avd_name("bad;rm").is_err());
        assert!(validate_avd_name("-bad").is_err());
        assert!(validate_device_profile_id("pixel 8").is_err());
        assert!(validate_system_image_id("system-images;android-35;google_apis;$(bad)").is_err());
    }

    #[test]
    fn newly_online_emulator_since_detects_brand_new_serial() {
        let before = vec![test_device(
            "emulator-5554",
            DeviceKind::Emulator,
            DeviceConnectionState::Online,
        )];
        let after = vec![
            test_device(
                "emulator-5554",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
            test_device(
                "emulator-5556",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
        ];

        assert_eq!(
            newly_online_emulator_serials_since(&before, &after),
            vec!["emulator-5556".to_string()]
        );
    }

    #[test]
    fn newly_online_emulator_since_counts_serial_that_returned_from_offline() {
        // Wiped AVD relaunch: the emulator reacquires its previous console
        // port, so the serial exists in both snapshots but was offline before.
        let before = vec![
            test_device(
                "emulator-5554",
                DeviceKind::Emulator,
                DeviceConnectionState::Offline,
            ),
            test_device(
                "emulator-5556",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
        ];
        let after = vec![
            test_device(
                "emulator-5554",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
            test_device(
                "emulator-5556",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
        ];

        assert_eq!(
            newly_online_emulator_serials_since(&before, &after),
            vec!["emulator-5554".to_string()]
        );
    }

    #[test]
    fn newly_online_emulator_since_returns_all_candidates_in_order() {
        // An unrelated emulator becoming online before the wiped AVD must not
        // shadow it — every candidate is returned so the caller can check each
        // one's AVD identity.
        let before: Vec<Device> = Vec::new();
        let after = vec![
            test_device(
                "emulator-5554",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
            test_device(
                "emulator-5556",
                DeviceKind::Emulator,
                DeviceConnectionState::Online,
            ),
            test_device(
                "ABC123",
                DeviceKind::Physical,
                DeviceConnectionState::Online,
            ),
        ];

        assert_eq!(
            newly_online_emulator_serials_since(&before, &after),
            vec!["emulator-5554".to_string(), "emulator-5556".to_string()]
        );
    }

    #[test]
    fn newly_online_emulator_since_ignores_already_online_emulators() {
        // The bug this guards against: another emulator that was already
        // online must not satisfy the wipe wait.
        let before = vec![test_device(
            "emulator-5554",
            DeviceKind::Emulator,
            DeviceConnectionState::Online,
        )];
        let after = before.clone();

        assert!(newly_online_emulator_serials_since(&before, &after).is_empty());
    }

    #[test]
    fn newly_online_emulator_since_ignores_non_emulator_devices() {
        let before: Vec<Device> = Vec::new();
        let after = vec![test_device(
            "ABC123",
            DeviceKind::Physical,
            DeviceConnectionState::Online,
        )];

        assert!(newly_online_emulator_serials_since(&before, &after).is_empty());
    }

    // ── Emulator lifecycle against a fake SDK ─────────────────────────────────

    /// A fake `adb` and `emulator` sharing a state directory. A running
    /// emulator is a file named after its serial holding its AVD name. Every
    /// `adb devices` line reports the Google image's model, never the AVD name.
    ///
    /// Markers in the state directory: `<serial>.offline` (listed offline),
    /// `<serial>.noconsole` (`emu` commands fail), `<serial>.kill-error`
    /// (`emu kill` fails), `<serial>.kill-ko` (the console refuses it with exit
    /// status 0), `<serial>.kill-ignored` (accepted but the emulator keeps
    /// running), `delay-<avd>` (seconds before the emulator comes online).
    struct FakeSdk {
        _dir: tempfile::TempDir,
        state: PathBuf,
        adb: PathBuf,
        emulator: PathBuf,
    }

    const FAKE_ADB: &str = r#"#!/bin/sh
cd "$STATE" || exit 1
if [ "$1" = devices ]; then
  echo "List of devices attached"
  for f in emulator-*; do
    case "$f" in *.*|'emulator-*'|emulator-calls) continue ;; esac
    st=device; [ -f "$f.offline" ] && st=offline
    printf '%s\t%s product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a\n' "$f" "$st"
  done
  exit 0
fi
serial="$2"; shift 2
if [ ! -f "$serial" ]; then echo "error: device '$serial' not found" >&2; exit 1; fi
if [ "$1" = emu ]; then
  if [ -f "$serial.noconsole" ]; then echo "error: could not connect to TCP port" >&2; exit 1; fi
  if [ "$2" = avd ]; then cat "$serial"; echo; echo OK; exit 0; fi
  if [ "$2" = kill ]; then
    if [ -f "$serial.kill-error" ]; then echo "error: could not connect to TCP port 5554: Connection refused" >&2; exit 1; fi
    if [ -f "$serial.kill-ko" ]; then echo "KO: permission denied"; exit 0; fi
    echo "OK: killing emulator, bye bye"; echo OK
    [ -f "$serial.kill-ignored" ] || (sleep 1; rm -f "$serial") >/dev/null 2>&1 &
    exit 0
  fi
fi
if [ "$1" = shell ] && [ "$2" = getprop ]; then
  case "$3" in
    ro.boot.qemu.avd_name) cat "$serial"; echo ;;
    ro.kernel.qemu.avd_name) echo ;;
    *) echo 34 ;;
  esac
  exit 0
fi
exit 0
"#;

    const FAKE_EMULATOR: &str = r#"#!/bin/sh
cd "$STATE" || exit 1
avd="${1#@}"
echo "$@" >> emulator-calls
for f in emulator-*; do
  case "$f" in *.*|'emulator-*'|emulator-calls) continue ;; esac
  if [ "$(cat "$f")" = "$avd" ]; then echo "ERROR | the AVD is already running" >&2; exit 1; fi
done
[ -f "fail-$avd" ] && exit 1
port=5554
while ! mkdir "port-$port" 2>/dev/null; do port=$((port + 2)); done
[ -f "delay-$avd" ] && sleep "$(cat "delay-$avd")"
printf '%s' "$avd" > "tmp-$port" && mv "tmp-$port" "emulator-$port"
"#;

    impl FakeSdk {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let state = dir.path().join("state");
            std::fs::create_dir(&state).unwrap();
            let write = |name: &str, body: &str| {
                let path = dir.path().join(name);
                let script = body.replacen(
                    "#!/bin/sh\n",
                    &format!("#!/bin/sh\nSTATE='{}'\n", state.display()),
                    1,
                );
                std::fs::write(&path, script).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
                path
            };
            let adb = write("adb", FAKE_ADB);
            let emulator = write("emulator", FAKE_EMULATOR);
            FakeSdk {
                state,
                adb,
                emulator,
                _dir: dir,
            }
        }

        /// An emulator already running `avd` on `serial`.
        fn running(&self, serial: &str, avd: &str) -> &Self {
            std::fs::write(self.state.join(serial), avd).unwrap();
            let port = serial.trim_start_matches("emulator-");
            std::fs::create_dir_all(self.state.join(format!("port-{port}"))).unwrap();
            self
        }

        fn mark(&self, name: &str, contents: &str) -> &Self {
            std::fs::write(self.state.join(name), contents).unwrap();
            self
        }

        fn is_listed(&self, serial: &str) -> bool {
            self.state.join(serial).exists()
        }

        fn emulator_calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.state.join("emulator-calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    /// Polls quickly; the limit is generous because a loaded machine can slow
    /// the fake scripts, and waits that succeed return as soon as they can.
    const QUICK: EmulatorWait = EmulatorWait {
        interval: Duration::from_millis(50),
        limit: Duration::from_secs(20),
    };

    #[tokio::test]
    async fn enrichment_names_the_avd_even_when_the_model_differs() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "Pixel_7_Pro");

        let mut devices = list_devices(&sdk.adb).await;
        assert_eq!(devices[0].model.as_deref(), Some("sdk gphone64 arm64"));
        enrich_device_props(&sdk.adb, &mut devices[0]).await;

        assert_eq!(devices[0].avd_name.as_deref(), Some("Pixel_7_Pro"));
    }

    #[tokio::test]
    async fn avd_name_falls_back_to_the_boot_property_without_a_console() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "Pixel_7")
            .mark("emulator-5554.noconsole", "");

        assert_eq!(
            resolve_avd_name(&sdk.adb, "emulator-5554").await.as_deref(),
            Some("Pixel_7")
        );
    }

    #[test]
    fn console_errors_are_not_avd_names() {
        assert_eq!(parse_avd_name("KO: unknown command, try 'help'\n"), None);
        assert_eq!(parse_avd_name("\n"), None);
        assert_eq!(
            parse_avd_name("Pixel_7\r\nOK\r\n").as_deref(),
            Some("Pixel_7")
        );
    }

    #[tokio::test]
    async fn physical_devices_get_no_avd_name() {
        let dir = tempfile::tempdir().unwrap();
        let adb = adb_printing(dir.path(), "Pixel_7");
        let mut device = test_device(
            "ZX1G22ABCD",
            DeviceKind::Physical,
            DeviceConnectionState::Online,
        );

        enrich_device_props(&adb, &mut device).await;

        assert_eq!(device.avd_name, None);
    }

    #[tokio::test]
    async fn a_failed_stop_is_reported() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "StopFails")
            .mark("emulator-5554.kill-error", "");

        let err = stop_emulator_with(&sdk.adb, "emulator-5554", QUICK)
            .await
            .unwrap_err();

        assert!(err.contains("Connection refused"), "{err}");
    }

    #[tokio::test]
    async fn a_stop_the_console_refuses_is_reported_despite_exit_zero() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "StopRefused")
            .mark("emulator-5554.kill-ko", "");

        let err = stop_emulator_with(&sdk.adb, "emulator-5554", QUICK)
            .await
            .unwrap_err();

        assert!(err.contains("KO: permission denied"), "{err}");
        assert!(sdk.is_listed("emulator-5554"));
    }

    #[tokio::test]
    async fn stop_waits_until_the_emulator_leaves_the_device_list() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "StopWaits");

        stop_emulator_with(&sdk.adb, "emulator-5554", QUICK)
            .await
            .unwrap();

        assert!(
            !sdk.is_listed("emulator-5554"),
            "reported stopped while adb still lists the emulator"
        );
    }

    #[tokio::test]
    async fn an_emulator_that_keeps_running_is_not_reported_stopped() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "StopIgnored")
            .mark("emulator-5554.kill-ignored", "");
        let wait = EmulatorWait {
            interval: Duration::from_millis(50),
            limit: Duration::from_millis(500),
        };

        let err = stop_emulator_with(&sdk.adb, "emulator-5554", wait)
            .await
            .unwrap_err();

        assert!(err.contains("still listed"), "{err}");
    }

    #[test]
    fn emu_kill_output_is_checked() {
        assert_eq!(
            emu_kill_failure(true, "OK: killing emulator, bye bye\nOK\n"),
            None
        );
        assert!(emu_kill_failure(true, "KO: bad command\n").is_some());
        assert!(emu_kill_failure(true, "error: device offline\n").is_some());
        assert!(emu_kill_failure(false, "\n").is_some());
    }

    #[tokio::test]
    async fn wiping_a_running_avd_is_refused_without_relaunching_it() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "WipeRunning");

        let err = wipe_avd_data_with(&sdk.emulator, &sdk.adb, "WipeRunning", QUICK)
            .await
            .unwrap_err();

        assert!(err.contains("is running as emulator-5554"), "{err}");
        assert!(
            sdk.emulator_calls().is_empty(),
            "the emulator was relaunched"
        );
    }

    #[tokio::test]
    async fn wipe_waits_for_its_own_avd_while_another_emulator_runs() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "WipeBystander")
            .mark("delay-WipeTarget", "0.3");

        wipe_avd_data_with(&sdk.emulator, &sdk.adb, "WipeTarget", QUICK)
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(sdk.state.join("emulator-5556")).unwrap(),
            "WipeTarget",
            "wipe returned before the wiped AVD came online"
        );
        assert_eq!(
            sdk.emulator_calls(),
            vec!["@WipeTarget -wipe-data -no-boot-anim -gpu auto"]
        );
    }

    #[tokio::test]
    async fn wipe_fails_at_once_when_the_emulator_exits_with_an_error() {
        let sdk = FakeSdk::new();
        sdk.mark("fail-WipeBroken", "");
        let wait = EmulatorWait {
            interval: Duration::from_millis(50),
            limit: Duration::from_secs(120),
        };

        let start = std::time::Instant::now();
        let err = wipe_avd_data_with(&sdk.emulator, &sdk.adb, "WipeBroken", wait)
            .await
            .unwrap_err();

        assert!(err.contains("exited"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(60));
    }

    #[tokio::test]
    async fn simultaneous_launches_each_return_their_own_emulator() {
        let sdk = FakeSdk::new();
        // The first AVD comes online after the second one.
        sdk.mark("delay-LaunchSlow", "0.6")
            .mark("delay-LaunchFast", "0.1");

        let (slow, fast) = tokio::join!(
            launch_emulator_with(&sdk.emulator, &sdk.adb, "LaunchSlow", QUICK),
            launch_emulator_with(&sdk.emulator, &sdk.adb, "LaunchFast", QUICK),
        );
        let (slow, fast) = (slow.unwrap(), fast.unwrap());

        assert_ne!(slow.serial, fast.serial);
        for (launched, avd) in [(&slow, "LaunchSlow"), (&fast, "LaunchFast")] {
            assert_eq!(
                std::fs::read_to_string(sdk.state.join(&launched.serial)).unwrap(),
                avd
            );
            assert!(!launched.already_running);
        }
    }

    #[tokio::test]
    async fn launching_the_same_avd_twice_at_once_starts_one_emulator() {
        let sdk = FakeSdk::new();
        sdk.mark("delay-LaunchTwice", "0.3");

        let (first, second) = tokio::join!(
            launch_emulator_with(&sdk.emulator, &sdk.adb, "LaunchTwice", QUICK),
            launch_emulator_with(&sdk.emulator, &sdk.adb, "LaunchTwice", QUICK),
        );

        assert_eq!(first.unwrap().serial, second.unwrap().serial);
        assert_eq!(sdk.emulator_calls().len(), 1);
    }

    #[tokio::test]
    async fn launching_a_running_avd_returns_its_serial() {
        let sdk = FakeSdk::new();
        sdk.running("emulator-5554", "LaunchOther")
            .running("emulator-5556", "LaunchRunning");

        let launched = launch_emulator_with(&sdk.emulator, &sdk.adb, "LaunchRunning", QUICK)
            .await
            .unwrap();

        assert_eq!(
            launched,
            LaunchedEmulator {
                serial: "emulator-5556".into(),
                already_running: true
            }
        );
        assert!(sdk.emulator_calls().is_empty());
    }

    #[tokio::test]
    async fn another_emulator_coming_online_does_not_satisfy_a_launch() {
        let sdk = FakeSdk::new();
        // Its own emulator is still booting when the wait ends.
        sdk.mark("delay-LaunchNever", "3");
        // An unrelated emulator that finishes booting during the launch.
        sdk.running("emulator-5554", "LaunchBystander")
            .mark("emulator-5554.offline", "");
        let state = sdk.state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = std::fs::remove_file(state.join("emulator-5554.offline"));
        });
        let wait = EmulatorWait {
            interval: Duration::from_millis(50),
            limit: Duration::from_millis(600),
        };

        let result = launch_emulator_with(&sdk.emulator, &sdk.adb, "LaunchNever", wait).await;

        assert!(result.is_err(), "{result:?}");
    }
}
