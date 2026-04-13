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

// Build system
export type { BuildLine } from "./BuildLine";
export type { BuildLineKind } from "./BuildLineKind";
export type { BuildError } from "./BuildError";
export type { BuildErrorSeverity } from "./BuildErrorSeverity";
export type { BuildResult } from "./BuildResult";
export type { BuildStatus } from "./BuildStatus";
export type { BuildRecord } from "./BuildRecord";
export type { BuildSettings } from "./BuildSettings";

// Devices & variants
export type { Device } from "./Device";
export type { DeviceKind } from "./DeviceKind";
export type { DeviceConnectionState } from "./DeviceConnectionState";
export type { AvdInfo } from "./AvdInfo";
export type { SystemImageInfo } from "./SystemImageInfo";
export type { DeviceDefinition } from "./DeviceDefinition";
export type { AvailableSystemImage } from "./AvailableSystemImage";
export type { SdkDownloadProgress } from "./SdkDownloadProgress";
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

// MCP
export type { McpSetupStatus } from "./McpSetupStatus";
export type { McpActivityEntry } from "./McpActivityEntry";
export type { McpServerStatus } from "./McpServerStatus";

// Logcat pipeline
export type { ProcessedEntry } from "./ProcessedEntry";
export type { LogcatLevel } from "./LogcatLevel";
export type { LogcatKind } from "./LogcatKind";
export type { EntryCategory } from "./EntryCategory";
export type { LogStats } from "./LogStats";
export type { LogcatFilterSpec } from "./LogcatFilterSpec";

// UI hierarchy (layout viewer)
export type { UiNode } from "./UiNode";
export type { UiLayoutContext } from "./UiLayoutContext";
export type { UiHierarchySnapshot } from "./UiHierarchySnapshot";
