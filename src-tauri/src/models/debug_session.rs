//! Debug sessions: one install epoch of one build on one device, per package.
//! See `services/debug_sessions.rs` for storage and retention.

use crate::models::app_exit::AppExitRecord;
use crate::models::build::{BuildActor, BuildProvenance, LaunchTiming};
use crate::models::logcat::ProcessedEntry;
use crate::models::redaction::{RedactionCount, RedactionRules};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Version of `session.json` and `index.json`.
pub const DEBUG_SESSION_SCHEMA_VERSION: u32 = 1;

/// The device a session's app was installed on. An emulator is identified by
/// its AVD name, a physical device by its serial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionDevice {
    pub serial: String,
    pub avd_name: Option<String>,
    /// For display only.
    pub model: Option<String>,
}

/// The APK the session installed, as its build recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionApk {
    pub module: String,
    pub variant: String,
    pub sha256: String,
    pub version_code: Option<u32>,
}

/// An R8 mapping of the installed APK, by content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionMapping {
    pub sha256: String,
    pub pg_map_id: Option<String>,
}

/// The build that produced the installed APK: its history record's id and a
/// digest that outlives the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionBuild {
    pub id: u32,
    pub task: String,
    pub started_at: String,
    pub origin: Option<BuildActor>,
    pub apk: DebugSessionApk,
    pub mappings: Vec<DebugSessionMapping>,
    /// The build's source commit and build files, when it recorded them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provenance: Option<BuildProvenance>,
}

/// The install that opened the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionInstall {
    pub apk_sha256: String,
    pub version_code: Option<u32>,
    pub installed_at: String,
    pub by: BuildActor,
}

/// Why a session closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionCloseReason {
    /// A later install on the same device and package.
    Superseded,
    /// The user ended it.
    Ended,
    /// No event for 24 hours.
    Idle,
}

/// Which kind of Keynobi process opened the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionRecorder {
    /// The app, or an MCP session attached to it.
    App,
    /// A standalone `keynobi --mcp` process.
    Standalone,
    /// Nobody here: it was imported from a bundle. Read-only.
    Imported,
}

/// Where an imported session came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionImport {
    /// The bundle's file name, without its folder.
    pub file_name: String,
    /// When the bundle was exported, as its manifest says.
    pub exported_at: String,
    pub imported_at: String,
    /// The session's id where it was recorded.
    pub original_id: String,
    /// The Keynobi version that exported it.
    pub keynobi_version: String,
    /// What the export, and then the import, left out, and why.
    pub omitted: Vec<SessionExportOmission>,
    /// The export's redaction rules, whether each was on, and what it replaced.
    pub redactions: Vec<RedactionCount>,
}

/// How many events of each kind a session holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionCounts {
    pub launches: u32,
    pub crashes: u32,
    pub anrs: u32,
    pub exits: u32,
    pub bookmarks: u32,
    /// Crashes and ANRs whose log lines were kept.
    pub captures: u32,
    /// MCP tool calls that acted on the session's device.
    pub agent_actions: u32,
    /// Screenshots attached to the session.
    pub attachments: u32,
}

/// A session's manifest, `sessions/<id>/session.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSession {
    pub schema_version: u32,
    pub id: String,
    pub project_root: Option<String>,
    pub package: String,
    pub device: DebugSessionDevice,
    /// `None` when no recorded build wrote the installed APK.
    pub build: Option<DebugSessionBuild>,
    /// `None` for a session no Keynobi install opened.
    pub install: Option<DebugSessionInstall>,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub close_reason: Option<DebugSessionCloseReason>,
    pub recorded_by: DebugSessionRecorder,
    /// Exempt from age pruning, and pins the session's R8 mappings.
    pub kept: bool,
    pub counts: DebugSessionCounts,
    pub last_event_at: String,
    pub event_count: u32,
    /// Events not recorded because a cap was reached.
    pub dropped_events: u32,
    /// Size of the session's event log and attachments.
    #[ts(type = "number")]
    pub bytes: u64,
    /// Set on a session imported from a bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub imported: Option<DebugSessionImport>,
}

/// A session as `index.json` lists it: what the list view shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionSummary {
    pub id: String,
    pub project_root: Option<String>,
    pub package: String,
    pub device: DebugSessionDevice,
    pub build_id: Option<u32>,
    pub module: Option<String>,
    pub variant: Option<String>,
    pub version_code: Option<u32>,
    pub apk_sha256: Option<String>,
    /// SHA-256 of each R8 mapping of the installed APK.
    pub mapping_sha256s: Vec<String>,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub close_reason: Option<DebugSessionCloseReason>,
    pub recorded_by: DebugSessionRecorder,
    pub kept: bool,
    pub counts: DebugSessionCounts,
    pub last_event_at: String,
    pub event_count: u32,
    pub dropped_events: u32,
    #[ts(type = "number")]
    pub bytes: u64,
}

impl From<&DebugSession> for DebugSessionSummary {
    fn from(session: &DebugSession) -> Self {
        let apk = session.build.as_ref().map(|b| &b.apk);
        DebugSessionSummary {
            id: session.id.clone(),
            project_root: session.project_root.clone(),
            package: session.package.clone(),
            device: session.device.clone(),
            build_id: session.build.as_ref().map(|b| b.id),
            module: apk.map(|a| a.module.clone()),
            variant: apk.map(|a| a.variant.clone()),
            version_code: apk
                .and_then(|a| a.version_code)
                .or(session.install.as_ref().and_then(|i| i.version_code)),
            apk_sha256: session.install.as_ref().map(|i| i.apk_sha256.clone()),
            mapping_sha256s: session
                .build
                .iter()
                .flat_map(|b| b.mappings.iter().map(|m| m.sha256.clone()))
                .collect(),
            opened_at: session.opened_at.clone(),
            closed_at: session.closed_at.clone(),
            close_reason: session.close_reason,
            recorded_by: session.recorded_by,
            kept: session.kept,
            counts: session.counts.clone(),
            last_event_at: session.last_event_at.clone(),
            event_count: session.event_count,
            dropped_events: session.dropped_events,
            bytes: session.bytes,
        }
    }
}

/// An app launch (Run App, MCP `launch_app`, or `restart_app`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionLaunch {
    pub serial: String,
    /// `None` when the launch method reported no timing.
    pub timing: Option<LaunchTiming>,
    /// A force-stop and relaunch rather than a plain launch.
    pub restart: bool,
}

/// A change of the logcat stream reading the session's device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionLogcatChange {
    pub serial: String,
    /// Why the stream stopped, for `logcatStopped`.
    pub reason: Option<String>,
}

/// The session's device went offline or came back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionDeviceChange {
    pub serial: String,
}

/// A note the user added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionBookmark {
    pub note: String,
    /// The logcat entry the note is about.
    #[ts(type = "number | null")]
    pub log_entry_id: Option<u64>,
}

/// How a crash was matched to its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionAttributionMethod {
    /// The session of Keynobi's install of the package on the device.
    InstallRecord,
    /// No Keynobi install could be trusted: the crash went to a session
    /// without a build.
    Unattributed,
}

/// Why a crash belongs to its session's build, and whether the device
/// confirmed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionAttribution {
    pub method: DebugSessionAttributionMethod,
    /// The device still ran Keynobi's install when the crash was recorded.
    pub verified: bool,
    /// What the device said, or why it was not asked or not believed.
    pub reason: Option<String>,
}

/// The log lines kept with a crash: `captures/crash-<seq>.jsonl`, named by
/// the crash event's `seq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionCaptureRef {
    pub entries: u32,
    #[ts(type = "number")]
    pub bytes: u64,
    /// Lines around the crash were left out by the capture caps.
    pub truncated: bool,
}

/// A crash or ANR that logcat showed for the session's app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionCrash {
    pub serial: String,
    pub pid: Option<u32>,
    /// The exception (`java.lang.RuntimeException: boom`) or the ANR line.
    pub summary: String,
    /// Hash of the crash's first log line, which with the device, package,
    /// pid, and time identifies one crash seen by several processes.
    pub signature: String,
    /// When the first line reached Keynobi (host clock).
    pub received_at: String,
    /// The first line's time as logcat printed it (device local time, no year).
    pub device_time: String,
    pub attribution: DebugSessionAttribution,
    /// `None` past `MAX_CAPTURES_PER_SESSION`, or when no line was left to keep.
    pub capture: Option<DebugSessionCaptureRef>,
    /// Logcat lines dropped by the stream so far (a flood), when it was captured.
    #[ts(type = "number")]
    pub dropped_lines: u64,
}

/// How an exit record was matched to the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionExitMatch {
    /// Its pid is one the session's crashes named.
    Pid,
    /// Its process is the package's main process.
    ProcessName,
    /// Only its time falls within the session.
    TimeWindow,
}

/// A process exit Android recorded for the session's app (Android 11+).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionExit {
    pub serial: String,
    /// The exit time on the host clock, from the device's local time and UTC offset.
    pub exited_at: String,
    pub matched_by: DebugSessionExitMatch,
    pub record: AppExitRecord,
}

/// How an MCP tool changes things, as its annotations declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionToolKind {
    /// Changes state, not destructively.
    Write,
    /// Destructive or hard to reverse.
    Destructive,
    /// Runs arbitrary project code.
    OpenWorld,
}

/// An MCP tool call that acted on the session's device. Its arguments are
/// never recorded; the agent is the event's actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionAgentAction {
    pub tool: String,
    pub kind: DebugSessionToolKind,
    /// The tool succeeded.
    pub ok: bool,
    pub duration_ms: u32,
    pub serial: String,
}

/// What an attachment holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionAttachmentKind {
    /// A PNG screenshot of the session's device.
    Screenshot,
    /// The UI hierarchy of the session's device, as JSON.
    Hierarchy,
}

/// A file attached to the session: `attachments/<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionAttachment {
    pub kind: DebugSessionAttachmentKind,
    /// `screenshot-<seq>.png` or `hierarchy-<seq>.json`, named by the
    /// event's `seq`.
    pub name: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// A screenshot's size in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub height: Option<u32>,
    /// A hierarchy's nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_count: Option<u32>,
    /// The device it shows.
    pub serial: String,
}

/// An attached UI hierarchy (`attachments/hierarchy-<seq>.json`): its nodes
/// in pre-order, a parent before its children, each with its depth, so the
/// file reads without recursion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionHierarchy {
    /// When it was captured, on the host clock.
    pub captured_at: String,
    /// The resumed activity, when the device said.
    pub foreground_activity: Option<String>,
    /// Whether nodes or text were cut to the caps.
    pub truncated: bool,
    pub nodes: Vec<DebugSessionHierarchyNode>,
}

/// One node of an attached UI hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionHierarchyNode {
    /// 0 for a top-level node; a child is one deeper than its parent.
    pub depth: u32,
    pub class: String,
    pub resource_id: String,
    pub text: String,
    pub content_desc: String,
    /// As UI Automator prints it: `[left,top][right,bottom]`.
    pub bounds: String,
}

/// What happened, by kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DebugSessionEventData {
    Build(DebugSessionBuild),
    Install(DebugSessionInstall),
    Launch(DebugSessionLaunch),
    /// Display times that arrived after the launch was recorded: the whole
    /// timing again, matched to its `launch` by `measuredAt`.
    LaunchTiming(LaunchTiming),
    LogcatReconnect(DebugSessionLogcatChange),
    LogcatStopped(DebugSessionLogcatChange),
    LogcatCleared(DebugSessionLogcatChange),
    DeviceOffline(DebugSessionDeviceChange),
    DeviceOnline(DebugSessionDeviceChange),
    Bookmark(DebugSessionBookmark),
    Crash(DebugSessionCrash),
    Anr(DebugSessionCrash),
    Exit(DebugSessionExit),
    AgentAction(DebugSessionAgentAction),
    Attachment(DebugSessionAttachment),
}

/// One line of `sessions/<id>/events.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionEvent {
    /// 1 for the session's first event.
    pub seq: u32,
    /// Host time (RFC 3339).
    pub at: String,
    /// Who caused it, when known.
    pub actor: Option<BuildActor>,
    #[serde(flatten)]
    pub event: DebugSessionEventData,
}

/// A session and its most recent events, for the session view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionDetail {
    pub session: DebugSession,
    /// Oldest first; at most `MAX_EVENTS_RETURNED`, the newest.
    pub events: Vec<DebugSessionEvent>,
    /// Whether older events were left out.
    pub events_truncated: bool,
    /// The session's crash and ANR events, even those older than `events`,
    /// oldest first; at most `MAX_CRASHES_RETURNED`, the newest.
    pub crashes: Vec<DebugSessionEvent>,
    /// The session's attachment events, even those older than `events`,
    /// oldest first.
    pub attachments: Vec<DebugSessionEvent>,
}

/// An attachment's content (`get_session_attachment`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionAttachmentData {
    /// The attachment event's `seq`.
    pub seq: u32,
    /// `image/png`.
    pub media_type: String,
    pub base64: String,
}

/// The log lines kept with a crash (`get_session_capture`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionCapture {
    /// The crash event's `seq`.
    pub seq: u32,
    /// Oldest first, ending with the crash's own lines.
    pub entries: Vec<ProcessedEntry>,
    /// Older lines of the capture were left out by the requested limit.
    pub truncated: bool,
}

/// What reading a session's exit reasons found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DebugSessionExitRefresh {
    /// Exit events added to the session.
    pub added: u32,
    /// Why nothing could be read (Android 10 or older), when so.
    pub message: Option<String>,
}

/// What to put in an exported debug session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SessionExportOptions {
    /// The redaction rules to apply; all on by default.
    pub redaction: RedactionRules,
    /// The log lines kept with each crash and ANR.
    pub include_crash_logs: bool,
    /// Attached screenshots, which redaction cannot reach.
    pub include_attachments: bool,
}

impl Default for SessionExportOptions {
    fn default() -> Self {
        SessionExportOptions {
            redaction: RedactionRules::default(),
            include_crash_logs: true,
            include_attachments: true,
        }
    }
}

/// Something an exported bundle leaves out, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SessionExportOmission {
    pub item: String,
    pub reason: String,
}

/// A debug session saved as a bundle (`export_debug_session`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SessionExportResult {
    /// Where the bundle was saved, as chosen in the save dialog.
    pub path: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// The bundle's files, in order.
    pub entries: Vec<String>,
    /// Matches each redaction rule replaced.
    pub redactions: Vec<RedactionCount>,
    pub omitted: Vec<SessionExportOmission>,
}
