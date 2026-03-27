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

/// An installed Android system image available for AVD creation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SystemImageInfo {
    /// SDK ID used with `avdmanager create avd -k "..."` (e.g. `"system-images;android-35;google_apis;arm64-v8a"`).
    pub sdk_id: String,
    /// Android API level.
    pub api_level: u32,
    /// Image variant (e.g. `"google_apis"`, `"google_apis_playstore"`, `"default"`).
    pub variant: String,
    /// CPU ABI (e.g. `"arm64-v8a"`, `"x86_64"`).
    pub abi: String,
    /// Human-friendly label (e.g. `"Android 15 (Google APIs) · arm64-v8a"`).
    pub display_name: String,
}

/// A hardware device definition from `avdmanager list device`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DeviceDefinition {
    /// Identifier used with `avdmanager create avd -d "..."` (e.g. `"pixel_7"`).
    pub id: String,
    /// Human-readable name (e.g. `"Pixel 7"`).
    pub name: String,
    /// Manufacturer name (e.g. `"Google"`).
    pub manufacturer: String,
}

/// An Android system image available for download via `sdkmanager`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AvailableSystemImage {
    /// Full SDK package ID (e.g. `"system-images;android-35;google_apis;arm64-v8a"`).
    pub sdk_id: String,
    /// Android API level.
    pub api_level: u32,
    /// Image variant (e.g. `"google_apis"`, `"google_apis_playstore"`, `"default"`).
    pub variant: String,
    /// CPU ABI (e.g. `"arm64-v8a"`, `"x86_64"`).
    pub abi: String,
    /// Human-friendly label.
    pub display_name: String,
    /// Whether the image is already installed locally.
    pub installed: bool,
}

/// A single line of progress from an `sdkmanager` download.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SdkDownloadProgress {
    /// Progress percentage 0–100, or `None` if indeterminate.
    pub percent: Option<u32>,
    /// Raw status line from sdkmanager (e.g. "Downloading...").
    pub message: String,
    /// Whether the download has finished (success or failure).
    pub done: bool,
    /// Whether the download ended in an error.
    pub error: bool,
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
