/**
 * Auto-generated TypeScript bindings from Rust model types.
 *
 * These files are produced by `ts-rs` (https://github.com/Aleph-Alpha/ts-rs)
 * when running `cargo test` in the `src-tauri` directory. Do NOT edit the
 * individual `.ts` files manually — they will be overwritten.
 *
 * To regenerate after changing a Rust model:
 *   cd src-tauri && cargo test
 *
 * All frontend code should import IPC types from this module:
 *   import type { BuildError, Device } from "@/bindings";
 */

// Logs
export type { LogEntry } from "./LogEntry";
export type { LogLevel } from "./LogLevel";

// Health
export type { SystemHealthReport } from "./SystemHealthReport";
export type { JdkSource } from "./JdkSource";

// Build system
export type { BuildLine } from "./BuildLine";
export type { BuildLineKind } from "./BuildLineKind";
export type { BuildError } from "./BuildError";
export type { BuildErrorSeverity } from "./BuildErrorSeverity";
export type { BuildResult } from "./BuildResult";
export type { BuildStatus } from "./BuildStatus";
export type { BuildRecord } from "./BuildRecord";
export type { BuildSettings } from "./BuildSettings";
export type { BuildActor } from "./BuildActor";
export type { AgentActor } from "./AgentActor";
export type { BuildStartedEvent } from "./BuildStartedEvent";
export type { BuildLinesEvent } from "./BuildLinesEvent";
export type { BuildCompleteEvent } from "./BuildCompleteEvent";
export type { LaunchState } from "./LaunchState";
export type { LaunchTiming } from "./LaunchTiming";
export type { LaunchTimingEvent } from "./LaunchTimingEvent";
export type { LaunchResult } from "./LaunchResult";
export type { MappingSnapshot } from "./MappingSnapshot";
export type { BuiltApk } from "./BuiltApk";
export type { RunApk } from "./RunApk";
export type { InstalledBuild } from "./InstalledBuild";

// Debug sessions
export type { DebugSession } from "./DebugSession";
export type { DebugSessionApk } from "./DebugSessionApk";
export type { DebugSessionAttribution } from "./DebugSessionAttribution";
export type { DebugSessionAttributionMethod } from "./DebugSessionAttributionMethod";
export type { DebugSessionBookmark } from "./DebugSessionBookmark";
export type { DebugSessionBuild } from "./DebugSessionBuild";
export type { DebugSessionCapture } from "./DebugSessionCapture";
export type { DebugSessionCaptureRef } from "./DebugSessionCaptureRef";
export type { DebugSessionCloseReason } from "./DebugSessionCloseReason";
export type { DebugSessionCounts } from "./DebugSessionCounts";
export type { DebugSessionCrash } from "./DebugSessionCrash";
export type { DebugSessionDetail } from "./DebugSessionDetail";
export type { DebugSessionDevice } from "./DebugSessionDevice";
export type { DebugSessionDeviceChange } from "./DebugSessionDeviceChange";
export type { DebugSessionEvent } from "./DebugSessionEvent";
export type { DebugSessionEventData } from "./DebugSessionEventData";
export type { DebugSessionExit } from "./DebugSessionExit";
export type { DebugSessionExitMatch } from "./DebugSessionExitMatch";
export type { DebugSessionExitRefresh } from "./DebugSessionExitRefresh";
export type { DebugSessionInstall } from "./DebugSessionInstall";
export type { DebugSessionLaunch } from "./DebugSessionLaunch";
export type { DebugSessionLogcatChange } from "./DebugSessionLogcatChange";
export type { DebugSessionMapping } from "./DebugSessionMapping";
export type { DebugSessionRecorder } from "./DebugSessionRecorder";
export type { DebugSessionSummary } from "./DebugSessionSummary";

// Devices & variants
export type { Device } from "./Device";
export type { DeviceKind } from "./DeviceKind";
export type { DeviceConnectionState } from "./DeviceConnectionState";
export type { AvdInfo } from "./AvdInfo";
export type { SystemImageInfo } from "./SystemImageInfo";
export type { DeviceDefinition } from "./DeviceDefinition";
export type { AvailableSystemImage } from "./AvailableSystemImage";
export type { SdkDownloadProgress } from "./SdkDownloadProgress";
export type { DeviceListChangedEvent } from "./DeviceListChangedEvent";
export type { AppExitReason } from "./AppExitReason";
export type { AppExitRecord } from "./AppExitRecord";
export type { AppExitReasons } from "./AppExitReasons";
export type { BuildVariant } from "./BuildVariant";
export type { VariantList } from "./VariantList";

// Errors
export type { AppError } from "./AppError";

// Settings (keep only what the frontend actively uses)
export type { AppSettings } from "./AppSettings";
export type { McpSettings } from "./McpSettings";
export type { TelemetrySettings } from "./TelemetrySettings";

// Projects
export type { ProjectEntry } from "./ProjectEntry";
export type { ProjectAppInfo } from "./ProjectAppInfo";
export type { RunConfiguration } from "./RunConfiguration";
export type { RunLaunch } from "./RunLaunch";
export type { TargetPreference } from "./TargetPreference";
export type { LocalRunState } from "./LocalRunState";
export type { ProjectRunConfigurations } from "./ProjectRunConfigurations";
export type { ResolvedRun } from "./ResolvedRun";
export type { RunDevice } from "./RunDevice";

// MCP
export type { McpClientSetupStatus } from "./McpClientSetupStatus";
export type { McpSetupStatus } from "./McpSetupStatus";
export type { McpActivityEntry } from "./McpActivityEntry";
export type { McpServerStatus } from "./McpServerStatus";
export type { McpAttachedSession } from "./McpAttachedSession";
export type { McpStandaloneServer } from "./McpStandaloneServer";
export type { AgentSkillState } from "./AgentSkillState";
export type { AgentSkillStatus } from "./AgentSkillStatus";

// Logcat pipeline
export type { ProcessedEntry } from "./ProcessedEntry";
export type { LogcatLevel } from "./LogcatLevel";
export type { LogcatKind } from "./LogcatKind";
export type { EntryCategory } from "./EntryCategory";
export type { LogStats } from "./LogStats";
export type { LogcatFilterSpec } from "./LogcatFilterSpec";
export type { RetraceOutcome } from "./RetraceOutcome";
export type { RetraceStatus } from "./RetraceStatus";
export type { MappingMatch } from "./MappingMatch";

// App monitor
export type { MonitorStats } from "./MonitorStats";

// UI hierarchy (layout viewer)
export type { UiNode } from "./UiNode";
export type { UiLayoutContext } from "./UiLayoutContext";
export type { UiHierarchySnapshot } from "./UiHierarchySnapshot";
