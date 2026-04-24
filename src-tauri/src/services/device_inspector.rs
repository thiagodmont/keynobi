use std::path::PathBuf;

#[derive(Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub build_version_sdk: Option<String>,
    pub build_version_release: Option<String>,
    pub product_manufacturer: Option<String>,
    pub product_model: Option<String>,
    pub product_name: Option<String>,
    pub build_fingerprint: Option<String>,
    pub build_id: Option<String>,
    pub display_size: Option<String>,
    pub battery: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct DumpedAppInfo {
    pub package: String,
    pub install_path: Option<String>,
    pub data_dir: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub first_install: Option<String>,
    pub raw_dump_excerpt: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryInfo {
    pub package: String,
    pub total_pss: Option<String>,
    pub java_heap_pss: Option<String>,
    pub java_heap_rss: Option<String>,
    pub native_heap_pss: Option<String>,
    pub native_heap_rss: Option<String>,
    pub graphics_pss: Option<String>,
    pub graphics_rss: Option<String>,
    pub raw: String,
}

#[allow(clippy::ptr_arg)]
pub async fn get_device_info(adb: &PathBuf, serial: &str) -> Result<DeviceInfo, String> {
    let mk_getprop = |prop: &'static str| {
        tokio::process::Command::new(adb.clone())
            .args(["-s", serial, "shell", "getprop", prop])
            .output()
    };

    let (sdk, release, manufacturer, model, name, fingerprint, build_id, wm_size, battery) = tokio::join!(
        mk_getprop("ro.build.version.sdk"),
        mk_getprop("ro.build.version.release"),
        mk_getprop("ro.product.manufacturer"),
        mk_getprop("ro.product.model"),
        mk_getprop("ro.product.name"),
        mk_getprop("ro.build.fingerprint"),
        mk_getprop("ro.build.id"),
        tokio::process::Command::new(adb.clone())
            .args(["-s", serial, "shell", "wm", "size"])
            .output(),
        tokio::process::Command::new(adb.clone())
            .args(["-s", serial, "shell", "dumpsys", "battery"])
            .output(),
    );

    let prop_val = |res: Result<std::process::Output, _>| -> Option<String> {
        let s = res
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };

    let battery_str = battery
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let battery_level = battery_str
        .lines()
        .find(|l| l.trim_start().starts_with("level:"))
        .map(|l| l.trim().to_string());

    let size_str = wm_size
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty());

    Ok(DeviceInfo {
        build_version_sdk: prop_val(sdk),
        build_version_release: prop_val(release),
        product_manufacturer: prop_val(manufacturer),
        product_model: prop_val(model),
        product_name: prop_val(name),
        build_fingerprint: prop_val(fingerprint),
        build_id: prop_val(build_id),
        display_size: size_str,
        battery: battery_level,
    })
}

#[allow(clippy::ptr_arg)]
pub async fn dump_app_info(
    adb: &PathBuf,
    serial: &str,
    package: &str,
) -> Result<DumpedAppInfo, String> {
    let (path_res, dump_res) = tokio::join!(
        tokio::process::Command::new(adb.clone())
            .args(["-s", serial, "shell", "pm", "path", package])
            .output(),
        tokio::process::Command::new(adb.clone())
            .args(["-s", serial, "shell", "dumpsys", "package", package])
            .output(),
    );

    let path_out = path_res
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    let dump_out = dump_res
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if dump_out.is_empty() && path_out.is_empty() {
        return Err(format!(
            "Package '{package}' not found on device {serial}. Is it installed?"
        ));
    }

    let raw = path_out
        .strip_prefix("package:")
        .unwrap_or(&path_out)
        .trim();
    let install_path = if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    };

    Ok(DumpedAppInfo {
        package: package.to_string(),
        install_path,
        data_dir: extract_dump_value(&dump_out, "dataDir="),
        version_name: extract_dump_value(&dump_out, "versionName="),
        version_code: extract_dump_value(&dump_out, "versionCode="),
        first_install: extract_dump_value(&dump_out, "firstInstallTime="),
        raw_dump_excerpt: dump_out.lines().take(40).collect::<Vec<_>>().join("\n"),
    })
}

pub async fn get_memory_info(
    adb: &PathBuf,
    serial: &str,
    package: &str,
) -> Result<MemoryInfo, String> {
    let output = tokio::process::Command::new(adb)
        .args(["-s", serial, "shell", "dumpsys", "meminfo", package])
        .output()
        .await
        .map_err(|e| format!("adb dumpsys meminfo failed: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();

    if text.trim().is_empty() || text.contains("No process found") {
        return Err(format!(
            "No memory info for '{package}' — is the app running?"
        ));
    }

    let (java_heap_pss, java_heap_rss) = extract_dump_two_values(&text, "Java Heap:");
    let (native_heap_pss, native_heap_rss) = extract_dump_two_values(&text, "Native Heap:");
    let (graphics_pss, graphics_rss) = extract_dump_two_values(&text, "Graphics:");

    Ok(MemoryInfo {
        package: package.to_string(),
        total_pss: extract_dump_value(&text, "TOTAL PSS:"),
        java_heap_pss,
        java_heap_rss,
        native_heap_pss,
        native_heap_rss,
        graphics_pss,
        graphics_rss,
        raw: text.lines().take(50).collect::<Vec<_>>().join("\n"),
    })
}

pub async fn take_screenshot(adb: &PathBuf, serial: &str) -> Result<Vec<u8>, String> {
    let output = tokio::process::Command::new(adb)
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output()
        .await
        .map_err(|e| format!("adb exec-out screencap failed: {e}"))?;

    if !output.status.success() || output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = if stderr.trim().is_empty() {
            "Screenshot failed (no error output from adb)".to_string()
        } else {
            stderr.to_string()
        };
        return Err(msg);
    }
    Ok(output.stdout)
}

fn extract_dump_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find(|l| l.contains(key))
        .and_then(|l| l.split(key).nth(1))
        .map(|v| v.split_whitespace().next().unwrap_or(v).trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn extract_dump_two_values(text: &str, key: &str) -> (Option<String>, Option<String>) {
    let after = text
        .lines()
        .find(|l| l.contains(key))
        .and_then(|l| l.split(key).nth(1));
    match after {
        None => (None, None),
        Some(s) => {
            let mut parts = s.split_whitespace();
            let first = parts.next().map(str::to_owned).filter(|v| !v.is_empty());
            let second = parts.next().map(str::to_owned).filter(|v| !v.is_empty());
            (first, second)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_dump_value_finds_version() {
        let dump = "    versionName=1.2.3\n    versionCode=42\n";
        assert_eq!(
            extract_dump_value(dump, "versionName="),
            Some("1.2.3".into())
        );
        assert_eq!(extract_dump_value(dump, "versionCode="), Some("42".into()));
    }

    #[test]
    fn extract_dump_value_returns_none_for_missing() {
        assert!(extract_dump_value("some text", "nothere=").is_none());
    }

    #[test]
    fn extract_dump_two_values_extracts_pss_and_rss() {
        let meminfo = "App Summary\n   Java Heap:        0                          13832\n   Native Heap:        4                            764\n";
        let (pss, rss) = extract_dump_two_values(meminfo, "Java Heap:");
        assert_eq!(pss.as_deref(), Some("0"));
        assert_eq!(rss.as_deref(), Some("13832"));
    }

    #[test]
    fn extract_dump_two_values_returns_none_none_for_missing_key() {
        let meminfo = "App Summary\n   Java Heap:  0   1234\n";
        let (pss, rss) = extract_dump_two_values(meminfo, "Graphics:");
        assert!(pss.is_none());
        assert!(rss.is_none());
    }

    #[test]
    fn extract_dump_two_values_handles_single_column() {
        let text = "   Graphics:        8\n";
        let (pss, rss) = extract_dump_two_values(text, "Graphics:");
        assert_eq!(pss.as_deref(), Some("8"));
        assert!(rss.is_none());
    }

    #[test]
    fn extract_dump_two_values_both_non_zero() {
        let meminfo = "   Native Heap:      512                          2048\n";
        let (pss, rss) = extract_dump_two_values(meminfo, "Native Heap:");
        assert_eq!(pss.as_deref(), Some("512"));
        assert_eq!(rss.as_deref(), Some("2048"));
    }
}
