use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Whether the device is physical hardware or an Android emulator.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DeviceKind {
    Physical,
    Emulator,
}

/// ADB connection state for a device.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DeviceConnectionState {
    Online,
    Offline,
    /// USB authorisation has not been granted.
    Unauthorized,
    Unknown,
}

/// An Android device or emulator visible to ADB.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Device {
    /// ADB transport serial (e.g. `emulator-5554`, `ZX1G22ABCD`).
    pub serial: String,
    /// Human-readable device name (from `ro.product.name` or parsed model).
    pub name: String,
    /// Device model string (from `ro.product.model`).
    pub model: Option<String>,
    pub device_kind: DeviceKind,
    pub connection_state: DeviceConnectionState,
    /// Android API level (from `ro.build.version.sdk`).
    pub api_level: Option<u32>,
    /// Android version string (from `ro.build.version.release`).
    pub android_version: Option<String>,
}

/// Metadata about an Android Virtual Device.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AvdInfo {
    /// AVD name used with `emulator @<name>`.
    pub name: String,
    /// Friendly display name from the AVD ini file.
    pub display_name: String,
    /// Android target (e.g. `android-35`).
    pub target: Option<String>,
    pub api_level: Option<u32>,
    /// ABI type (e.g. `arm64-v8a`, `x86_64`).
    pub abi: Option<String>,
    /// Path to the AVD directory.
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_serializes() {
        let d = Device {
            serial: "emulator-5554".into(),
            name: "Pixel 7".into(),
            model: Some("Pixel 7".into()),
            device_kind: DeviceKind::Emulator,
            connection_state: DeviceConnectionState::Online,
            api_level: Some(34),
            android_version: Some("14".into()),
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("emulator-5554"));
        assert!(json.contains("emulator"));
    }
}
