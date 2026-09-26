use crate::models::build::MappingSnapshot;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What happened when a crash's stack trace was to be deobfuscated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum RetraceStatus {
    /// Deobfuscated with the mapping the outcome names.
    Retraced,
    /// `retrace` or a JDK 17+ is missing; the reason says what to install.
    Unavailable,
    /// The mapping of the build on the device could not be identified with
    /// certainty, so nothing was deobfuscated; the reason says why.
    Refused,
    /// `retrace` ran and failed (error, timeout, or too much output).
    Failed,
}

/// How the mapping was matched to the crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum MappingMatch {
    /// Keynobi's record of the build it installed on the device, checked
    /// against the version code and last update time the device reports.
    InstallRecord,
}

/// A crash stack trace, deobfuscated or not, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RetraceOutcome {
    pub status: RetraceStatus,
    /// The deobfuscated trace when `status` is `retraced`; otherwise the
    /// trace as logcat printed it.
    pub trace: String,
    /// The build whose mapping was used, or was to be used.
    pub build_id: Option<u32>,
    /// The mapping used, or the one a check refused.
    pub mapping: Option<MappingSnapshot>,
    pub matched_by: Option<MappingMatch>,
    /// The device: its AVD name, else its serial.
    pub device: Option<String>,
    pub package: Option<String>,
    /// Why the trace was not deobfuscated.
    pub reason: Option<String>,
    /// One line naming the mapping and how it was matched, or the reason.
    pub summary: String,
}
