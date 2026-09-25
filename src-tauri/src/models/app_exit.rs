use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a process exited, as Android records it (`ApplicationExitInfo.REASON_*`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum AppExitReason {
    /// Code 0, or a code this version of Keynobi does not know.
    Unknown,
    /// The process called `System.exit` (status is the exit code).
    ExitSelf,
    /// The process was killed by a signal (status is the signal number).
    Signaled,
    /// The low memory killer stopped it.
    LowMemory,
    /// An uncaught Java/Kotlin exception.
    Crash,
    /// A native crash.
    CrashNative,
    /// Application Not Responding.
    Anr,
    InitializationFailure,
    PermissionChange,
    ExcessiveResourceUsage,
    UserRequested,
    UserStopped,
    DependencyDied,
    /// Killed by the system for another reason.
    Other,
    Freezer,
    PackageStateChange,
    PackageUpdated,
}

impl AppExitReason {
    /// The reason for an `ApplicationExitInfo` reason code.
    pub fn from_code(code: i32) -> Self {
        match code {
            1 => Self::ExitSelf,
            2 => Self::Signaled,
            3 => Self::LowMemory,
            4 => Self::Crash,
            5 => Self::CrashNative,
            6 => Self::Anr,
            7 => Self::InitializationFailure,
            8 => Self::PermissionChange,
            9 => Self::ExcessiveResourceUsage,
            10 => Self::UserRequested,
            11 => Self::UserStopped,
            12 => Self::DependencyDied,
            13 => Self::Other,
            14 => Self::Freezer,
            15 => Self::PackageStateChange,
            16 => Self::PackageUpdated,
            _ => Self::Unknown,
        }
    }

    /// The stable name used on the wire and in agent output.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::ExitSelf => "exitSelf",
            Self::Signaled => "signaled",
            Self::LowMemory => "lowMemory",
            Self::Crash => "crash",
            Self::CrashNative => "crashNative",
            Self::Anr => "anr",
            Self::InitializationFailure => "initializationFailure",
            Self::PermissionChange => "permissionChange",
            Self::ExcessiveResourceUsage => "excessiveResourceUsage",
            Self::UserRequested => "userRequested",
            Self::UserStopped => "userStopped",
            Self::DependencyDied => "dependencyDied",
            Self::Other => "other",
            Self::Freezer => "freezer",
            Self::PackageStateChange => "packageStateChange",
            Self::PackageUpdated => "packageUpdated",
        }
    }
}

/// One recorded process exit (`dumpsys activity exit-info`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AppExitRecord {
    /// When the process exited, as the device printed it (device local time).
    pub timestamp: Option<String>,
    /// The same time as ISO 8601 (`2024-05-02T10:15:03.482`), when the device
    /// printed a date and time Keynobi can read. It has no offset: the dump
    /// does not say which time zone the device uses.
    pub timestamp_local: Option<String>,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub reason: AppExitReason,
    /// The raw `REASON_*` code.
    pub reason_code: Option<i32>,
    /// The device's name for the reason, for example `APP CRASH(EXCEPTION)`.
    pub reason_label: Option<String>,
    pub sub_reason_code: Option<i32>,
    /// The device's name for the sub-reason, for example `TOO MANY CACHED`.
    pub sub_reason: Option<String>,
    /// The exit code for `exitSelf`, the signal for `signaled` and `crashNative`.
    pub status: Option<i32>,
    /// `RunningAppProcessInfo` importance when the process died (100 = foreground).
    pub importance: Option<i32>,
    /// A name for `importance`, for example `foreground` or `cached`.
    pub importance_name: Option<String>,
    /// Last proportional set size in KB (rounded by the device).
    #[ts(type = "number | null")]
    pub pss_kb: Option<u64>,
    /// Last resident set size in KB (rounded by the device).
    #[ts(type = "number | null")]
    pub rss_kb: Option<u64>,
    /// The system's description, capped at `MAX_EXIT_DESCRIPTION_CHARS`.
    pub description: Option<String>,
}

/// The exit history of one package on one device.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AppExitReasons {
    pub serial: String,
    pub package: String,
    /// The device's API level, when it reported one.
    pub api_level: Option<u32>,
    /// False when the device cannot report exit reasons (below Android 11).
    pub supported: bool,
    /// Why no records are listed, when none are.
    pub message: Option<String>,
    /// Newest first, at most `MAX_EXIT_RECORDS`.
    pub records: Vec<AppExitRecord>,
    /// How many records the device reported, before the cap.
    pub total_records: u32,
}
