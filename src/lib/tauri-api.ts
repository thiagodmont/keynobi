import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ── Project ────────────────────────────────────────────────────────────────────

export async function openFolderDialog(): Promise<string | null> {
  const result = await open({
    directory: true,
    multiple: false,
    title: "Open Android Project",
  });
  if (Array.isArray(result)) return result[0] ?? null;
  return result as string | null;
}

/** Open an Android project folder. Returns the project name on success. */
export async function openProject(path: string): Promise<string> {
  return invoke<string>("open_project", { path });
}

export async function getProjectRoot(): Promise<string | null> {
  return invoke<string | null>("get_project_root");
}

export async function getGradleRoot(): Promise<string | null> {
  return invoke<string | null>("get_gradle_root");
}

/** Reads applicationId from app/build.gradle(.kts) for `package:mine` resolution. */
export async function getApplicationId(): Promise<string | null> {
  return invoke<string | null>("get_application_id");
}

// ── Project registry ──────────────────────────────────────────────────────────

import type { ProjectEntry, ProjectAppInfo } from "@/bindings";
export type { ProjectEntry, ProjectAppInfo };

/** Return the sorted project registry list. */
export async function listProjects(): Promise<ProjectEntry[]> {
  return invoke<ProjectEntry[]>("list_projects");
}

/** Remove a project from the registry by its ID (does not delete from disk). */
export async function removeProject(id: string): Promise<void> {
  return invoke<void>("remove_project", { id });
}

/** Toggle the pinned flag for a project in the registry. */
export async function pinProject(id: string, pinned: boolean): Promise<void> {
  return invoke<void>("pin_project", { id, pinned });
}

/** Record whether Keynobi may run the project's Gradle build scripts. */
export async function setProjectTrust(id: string, trusted: boolean): Promise<void> {
  return invoke<void>("set_project_trust", { id, trusted });
}

/** Return the path of the project that was last active, for session restore. */
export async function getLastActiveProject(): Promise<string | null> {
  return invoke<string | null>("get_last_active_project");
}

/** Read versionName, versionCode and applicationId from the open project. */
export async function getProjectAppInfo(): Promise<ProjectAppInfo> {
  return invoke<ProjectAppInfo>("get_project_app_info");
}

/**
 * Write versionName and versionCode back to the app-level build.gradle(.kts).
 * A `null` field is left as it is.
 */
export async function saveProjectAppInfo(
  versionName: string | null,
  versionCode: number | null
): Promise<void> {
  return invoke<void>("save_project_app_info", { versionName, versionCode });
}

/** Persist per-project variant and device selections into the registry. */
export async function updateProjectMeta(
  id: string,
  lastBuildVariant: string | null,
  lastDevice: string | null
): Promise<void> {
  return invoke<void>("update_project_meta", { id, lastBuildVariant, lastDevice });
}

/** Rename a project's display name in the registry (does not rename folder). */
export async function renameProject(id: string, newName: string): Promise<void> {
  return invoke<void>("rename_project", { id, newName });
}

// ── Run configurations ────────────────────────────────────────────────────────

import type { ProjectRunConfigurations, RunConfiguration } from "@/bindings";
export type { ProjectRunConfigurations, RunConfiguration };

/**
 * The run configurations of a registered project (default: the open one).
 * The first read creates them from the project's application modules.
 */
export async function listRunConfigurations(
  projectRoot: string | null = null
): Promise<ProjectRunConfigurations> {
  return invoke<ProjectRunConfigurations>("list_run_configurations", { projectRoot });
}

/**
 * Save a run configuration of the open project, replacing the one of the
 * same name. Rejects with `invalidInput` naming the field that is not valid.
 */
export async function saveRunConfiguration(
  config: RunConfiguration
): Promise<ProjectRunConfigurations> {
  return invoke<ProjectRunConfigurations>("save_run_configuration", { config });
}

/** Delete a run configuration of the open project. */
export async function deleteRunConfiguration(name: string): Promise<ProjectRunConfigurations> {
  return invoke<ProjectRunConfigurations>("delete_run_configuration", { name });
}

/** Make a run configuration of the open project the active one. */
export async function setActiveRunConfiguration(name: string): Promise<ProjectRunConfigurations> {
  return invoke<ProjectRunConfigurations>("set_active_run_configuration", { name });
}

// ── Settings ──────────────────────────────────────────────────────────────────

import type { AppSettings } from "@/bindings";
export type { AppSettings };

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

/** Acknowledge that the close-time settings flush has completed (see lib.rs shutdown handler). */
export async function notifySettingsFlushed(): Promise<void> {
  return invoke<void>("notify_settings_flushed");
}

export async function getDefaultSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_default_settings");
}

export async function resetSettingsToDefaults(): Promise<AppSettings> {
  return invoke<AppSettings>("reset_settings");
}

/** Single info-level event to the Rust Sentry project (telemetry feature + compile-time `SENTRY_DSN`, telemetry on at launch). */
export async function sendNativeSentryTestEvent(): Promise<void> {
  await invoke<void>("send_native_sentry_test_event");
}

export async function detectSdkPath(): Promise<string | null> {
  return invoke<string | null>("detect_sdk_path");
}

export async function detectJavaPath(): Promise<string | null> {
  return invoke<string | null>("detect_java_path");
}

// ── Health checks ─────────────────────────────────────────────────────────────

import type { AppError, SystemHealthReport } from "@/bindings";

/** Run system-level health probes (Java, SDK, Gradle, disk). */
export async function runHealthChecks(): Promise<SystemHealthReport> {
  return invoke<SystemHealthReport>("run_health_checks");
}

// ── Error helpers ─────────────────────────────────────────────────────────────

export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object") {
    const o = err as Record<string, unknown>;
    if (typeof o.error === "string" && o.error.length > 0) return o.error;
    if (typeof o.message === "string") {
      if (o.message.length > 0) {
        if (typeof o.kind === "string" && o.kind.length > 0) {
          return `${o.kind}: ${o.message}`;
        }
        return o.message;
      }
    }
    try {
      return JSON.stringify(o);
    } catch {
      return String(err);
    }
  }
  return String(err);
}

/** The command failed with this `AppError` kind (`{ kind, message }`). */
export function isAppErrorKind(err: unknown, kind: AppError["kind"]): boolean {
  return !!err && typeof err === "object" && (err as { kind?: unknown }).kind === kind;
}

// ── Build system ──────────────────────────────────────────────────────────────

import type {
  BuildLine,
  BuildError,
  BuildStatus,
  BuildRecord,
  BuildActor,
  BuildStartedEvent,
  BuildLinesEvent,
  BuildCompleteEvent,
  LaunchTimingEvent,
  RunApk,
} from "@/bindings";
import { Channel } from "@tauri-apps/api/core";

export type {
  BuildLine,
  BuildError,
  BuildStatus,
  BuildRecord,
  BuildActor,
  BuildStartedEvent,
  BuildLinesEvent,
  BuildCompleteEvent,
  LaunchTimingEvent,
};

/** Start a Gradle task. Resolves with its run ID once Gradle runs; its output
 *  arrives as `build:lines` and its result as `build:complete`. */
export async function runGradleTask(task: string): Promise<number> {
  return invoke<number>("run_gradle_task", { task });
}

export async function cancelBuild(): Promise<void> {
  return invoke<void>("cancel_build");
}

export async function getBuildStatus(): Promise<BuildStatus> {
  return invoke<BuildStatus>("get_build_status");
}

export async function getBuildErrors(): Promise<BuildError[]> {
  return invoke<BuildError[]>("get_build_errors");
}

export async function getBuildHistory(): Promise<BuildRecord[]> {
  return invoke<BuildRecord[]>("get_build_history");
}

export async function clearBuildHistory(): Promise<void> {
  return invoke<void>("clear_build_history");
}

/** The saved log of a past build. Rejects with `notFound` once log rotation removed it. */
export async function getBuildLogEntries(id: number): Promise<BuildLine[]> {
  return invoke<BuildLine[]>("get_build_log_entries", { id });
}

/**
 * The Gradle path of the application module to build (`:app`; `:` for the
 * root project): `module`, or the project's only one. Rejects listing the
 * application modules when there are several and none was named.
 */
export async function getApplicationModule(module: string | null = null): Promise<string> {
  return invoke<string>("get_application_module", { module });
}

/**
 * The APK to install after build `buildId` of `variant` in `module`: the one
 * that build recorded, else (Gradle found it up to date) the variant's APK in
 * the module's build outputs. Rejects with the reason (and the variants that
 * have outputs) when no APK matches; another variant's is never returned.
 */
export async function findApkPath(
  variant: string,
  opts: { module?: string | null; buildId?: number | null } = {}
): Promise<RunApk> {
  return invoke<RunApk>("find_apk_path", {
    variant,
    module: opts.module ?? null,
    buildId: opts.buildId ?? null,
  });
}

/**
 * Extract the package name directly from an APK binary using `aapt2`.
 * Returns the exact installed package name including any `applicationIdSuffix`
 * (e.g. `com.example.app.debug` for a debug build).
 */
export async function getPackageNameFromApk(apkPath: string): Promise<string> {
  return invoke<string>("get_package_name_from_apk", { apkPath });
}

/** A build started, whoever started it (the app or an agent). */
export function listenBuildStarted(cb: (e: BuildStartedEvent) => void): Promise<UnlistenFn> {
  return listen<BuildStartedEvent>("build:started", (event) => cb(event.payload));
}

/** Batched output of a running build. */
export function listenBuildLines(cb: (e: BuildLinesEvent) => void): Promise<UnlistenFn> {
  return listen<BuildLinesEvent>("build:lines", (event) => cb(event.payload));
}

export function listenBuildComplete(cb: (e: BuildCompleteEvent) => void): Promise<UnlistenFn> {
  return listen<BuildCompleteEvent>("build:complete", (event) => cb(event.payload));
}

/** Display times of a launch arrived after the launch returned; the build's record has them. */
export function listenBuildLaunchTiming(cb: (e: LaunchTimingEvent) => void): Promise<UnlistenFn> {
  return listen<LaunchTimingEvent>("build:launch_timing", (event) => cb(event.payload));
}

// ── Variants ──────────────────────────────────────────────────────────────────

import type { BuildVariant, VariantList } from "@/bindings";
export type { BuildVariant, VariantList };

/**
 * Fast variant preview from static build.gradle parse.
 * Returns only explicitly declared variants — resolves instantly.
 * May return an empty list; use getVariantsFromGradle for the full picture.
 */
export async function getVariantsPreview(module: string | null = null): Promise<VariantList> {
  return invoke<VariantList>("get_variants_preview", { module });
}

/**
 * Authoritative variant list from `./gradlew tasks --group Build`.
 * Discovers every variant the project actually exposes, regardless of
 * how they are defined. Takes a few seconds on first run (daemon startup).
 */
export async function getVariantsFromGradle(module: string | null = null): Promise<VariantList> {
  return invoke<VariantList>("get_variants_from_gradle", { module });
}

export async function setActiveVariant(variant: string): Promise<void> {
  return invoke<void>("set_active_variant", { variant });
}

// ── Devices ───────────────────────────────────────────────────────────────────

import type {
  Device,
  AvdInfo,
  SystemImageInfo,
  DeviceDefinition,
  AvailableSystemImage,
  SdkDownloadProgress,
  UiHierarchySnapshot,
  DeviceListChangedEvent,
  LaunchResult,
  AppExitReasons,
  InstalledBuild,
} from "@/bindings";
export type {
  Device,
  AvdInfo,
  SystemImageInfo,
  DeviceDefinition,
  AvailableSystemImage,
  SdkDownloadProgress,
  UiHierarchySnapshot,
  LaunchResult,
  AppExitReasons,
  InstalledBuild,
};

export async function listAdbDevices(): Promise<Device[]> {
  return invoke<Device[]>("list_adb_devices");
}

export async function refreshDevices(): Promise<Device[]> {
  return invoke<Device[]>("refresh_devices");
}

export async function selectDevice(serial: string): Promise<void> {
  return invoke<void>("select_device", { serial });
}

export async function getSelectedDevice(): Promise<string | null> {
  return invoke<string | null>("get_selected_device");
}

/** UI Automator / accessibility hierarchy for the focused window (`deviceSerial` null = selected device). */
export async function dumpUiHierarchy(deviceSerial?: string | null): Promise<UiHierarchySnapshot> {
  return invoke<UiHierarchySnapshot>("dump_ui_hierarchy", {
    deviceSerial: deviceSerial ?? null,
  });
}

/** Install an APK. The backend records which build produced it (see `listInstalledBuilds`). */
export async function installApkOnDevice(serial: string, apkPath: string): Promise<string> {
  return invoke<string>("install_apk_on_device", { serial, apkPath });
}

/** What Keynobi last installed on each device, per package, oldest first. */
export async function listInstalledBuilds(): Promise<InstalledBuild[]> {
  return invoke<InstalledBuild[]>("list_installed_builds");
}

/**
 * Launch an app. With `buildId`, the backend records the launch time on that
 * build's history entry (the build whose APK was installed).
 */
export async function launchAppOnDevice(
  serial: string,
  pkg: string,
  opts: { activity?: string; buildId?: number | null } = {}
): Promise<LaunchResult> {
  return invoke<LaunchResult>("launch_app_on_device", {
    serial,
    package: pkg,
    activity: opts.activity ?? null,
    buildId: opts.buildId ?? null,
  });
}

export async function stopAppOnDevice(serial: string, pkg: string): Promise<void> {
  return invoke<void>("stop_app_on_device", { serial, package: pkg });
}

/** Why the app's processes exited on `serial` (Android 11+). `pkg` defaults to the project's app. */
export async function getExitReasons(serial: string, pkg?: string | null): Promise<AppExitReasons> {
  return invoke<AppExitReasons>("get_exit_reasons", { serial, package: pkg ?? null });
}

export async function listAvdDevices(): Promise<AvdInfo[]> {
  return invoke<AvdInfo[]>("list_avd_devices");
}

export async function launchAvd(avdName: string): Promise<string> {
  return invoke<string>("launch_avd", { avdName });
}

export async function stopAvd(serial: string): Promise<void> {
  return invoke<void>("stop_avd", { serial });
}

export async function startDevicePolling(): Promise<void> {
  return invoke<void>("start_device_polling");
}

export async function stopDevicePolling(): Promise<void> {
  return invoke<void>("stop_device_polling");
}

export function listenDeviceListChanged(cb: (devices: Device[]) => void): Promise<UnlistenFn> {
  return listen<DeviceListChangedEvent>("device:list_changed", (event) =>
    cb(event.payload.devices)
  );
}

export async function listSystemImages(): Promise<SystemImageInfo[]> {
  return invoke<SystemImageInfo[]>("list_system_images_cmd");
}

export async function listDeviceDefinitions(): Promise<DeviceDefinition[]> {
  return invoke<DeviceDefinition[]>("list_device_definitions_cmd");
}

export async function createAvdDevice(
  name: string,
  systemImage: string,
  device?: string
): Promise<AvdInfo[]> {
  return invoke<AvdInfo[]>("create_avd_device", {
    name,
    systemImage,
    device: device ?? null,
  });
}

export async function deleteAvdDevice(name: string): Promise<AvdInfo[]> {
  return invoke<AvdInfo[]>("delete_avd_device", { name });
}

export async function wipeAvdData(name: string): Promise<void> {
  return invoke<void>("wipe_avd_data_cmd", { name });
}

export async function listAvailableSystemImages(): Promise<AvailableSystemImage[]> {
  return invoke<AvailableSystemImage[]>("list_available_system_images_cmd");
}

export function downloadSystemImage(
  sdkId: string,
  onProgress: (progress: SdkDownloadProgress) => void
): Promise<void> {
  const channel = new Channel<SdkDownloadProgress>();
  channel.onmessage = onProgress;
  return invoke<void>("download_system_image_cmd", { sdkId, onProgress: channel });
}

// ── Logcat ────────────────────────────────────────────────────────────────────

// Use the generated ProcessedEntry as the canonical logcat entry type.
// The type alias keeps existing code working unchanged.
import type { ProcessedEntry, LogStats, LogcatFilterSpec, RetraceOutcome } from "@/bindings";
export type { ProcessedEntry, LogStats, LogcatFilterSpec, RetraceOutcome };
export type LogcatEntry = ProcessedEntry;

export async function startLogcat(deviceSerial?: string): Promise<void> {
  return invoke<void>("start_logcat", { deviceSerial: deviceSerial ?? null });
}

export async function stopLogcat(): Promise<void> {
  return invoke<void>("stop_logcat");
}

export async function clearLogcat(): Promise<void> {
  return invoke<void>("clear_logcat");
}

export async function getLogcatEntries(opts?: {
  count?: number;
  minLevel?: string;
  tag?: string;
  text?: string;
  package?: string;
  onlyCrashes?: boolean;
}): Promise<ProcessedEntry[]> {
  return invoke<ProcessedEntry[]>("get_logcat_entries", {
    count: opts?.count ?? null,
    minLevel: opts?.minLevel ?? null,
    tag: opts?.tag ?? null,
    text: opts?.text ?? null,
    package: opts?.package ?? null,
    onlyCrashes: opts?.onlyCrashes ?? false,
  });
}

export async function getLogcatContextEntries(opts: {
  anchorId: number;
  direction: "before" | "after";
  count?: number;
}): Promise<ProcessedEntry[]> {
  return invoke<ProcessedEntry[]>("get_logcat_context_entries", {
    anchorId: opts.anchorId,
    direction: opts.direction,
    count: opts.count ?? null,
  });
}

export async function getLogcatStatus(): Promise<boolean> {
  return invoke<boolean>("get_logcat_status");
}

export async function listLogcatPackages(): Promise<string[]> {
  return invoke<string[]>("list_logcat_packages");
}

/** Show a save dialog and write `contents` there. Resolves to the saved path, or null if cancelled. */
export async function exportLogcat(contents: string): Promise<string | null> {
  return invoke<string | null>("export_logcat", { contents });
}

/**
 * Update the backend stream filter.
 * After this call, only entries matching the spec will be emitted via
 * `logcat:entries`. Pass an empty spec to disable backend filtering.
 */
export async function setLogcatFilter(spec: LogcatFilterSpec): Promise<void> {
  return invoke<void>("set_logcat_filter", { filterSpec: spec });
}

/** Return running statistics for the current logcat session. */
export async function getLogcatStats(): Promise<LogStats> {
  return invoke<LogStats>("get_logcat_stats");
}

/**
 * Deobfuscate a crash from the logcat buffer with the saved R8 mapping of the
 * build that produced it. Refusals and a missing tool are outcomes;
 * a crash that left the buffer rejects with `NotFound`.
 */
export async function retraceCrash(crashGroupId: number): Promise<RetraceOutcome> {
  return invoke<RetraceOutcome>("retrace_crash", { crashGroupId });
}

export function listenLogcatEntries(cb: (entries: ProcessedEntry[]) => void): Promise<UnlistenFn> {
  return listen<ProcessedEntry[]>("logcat:entries", (e) => cb(e.payload));
}

export function listenLogcatCleared(cb: () => void): Promise<UnlistenFn> {
  return listen("logcat:cleared", () => cb());
}

/**
 * Fired when the logcat stream disconnects unexpectedly (e.g. because Android
 * Studio restarted the ADB server) and the backend is about to reconnect.
 * The stream will resume automatically; the frontend can use this to show a
 * brief "reconnecting…" indicator without treating it as a stop.
 */
export function listenLogcatReconnecting(cb: () => void): Promise<UnlistenFn> {
  return listen("logcat:reconnecting", () => cb());
}

/**
 * Emitted when the backend gives up on a logcat stream after repeated failed
 * reconnects. Unlike `logcat:reconnecting`, this is terminal — the stream will
 * NOT resume on its own.
 */
export function listenLogcatStopped(cb: (reason: string) => void): Promise<UnlistenFn> {
  return listen<string>("logcat:stopped", (event) => cb(event.payload));
}

// ── Debug sessions ────────────────────────────────────────────────────────────

import type {
  DebugSessionCapture,
  DebugSessionDetail,
  DebugSessionEvent,
  DebugSessionExitRefresh,
  DebugSessionSummary,
} from "@/bindings";
export type {
  DebugSessionCapture,
  DebugSessionDetail,
  DebugSessionEvent,
  DebugSessionExitRefresh,
  DebugSessionSummary,
};

/** Every debug session (one per install on a device), newest first. */
export async function listDebugSessions(): Promise<DebugSessionSummary[]> {
  return invoke<DebugSessionSummary[]>("list_debug_sessions");
}

/** A debug session and its most recent timeline events. Rejects with `NotFound` once pruned. */
export async function getDebugSession(id: string): Promise<DebugSessionDetail> {
  return invoke<DebugSessionDetail>("get_debug_session", { id });
}

/** End an open debug session. */
export async function endDebugSession(id: string): Promise<void> {
  return invoke<void>("end_debug_session", { id });
}

/** Keep a debug session (exempt from age pruning; keeps its R8 mappings), or stop keeping it. */
export async function setDebugSessionKept(id: string, kept: boolean): Promise<void> {
  return invoke<void>("set_debug_session_kept", { id, kept });
}

/**
 * Add a note to a debug session: `sessionId`, else the newest open session on
 * the selected device. `logEntryId` anchors it to a logcat entry.
 */
export async function addSessionBookmark(
  note: string,
  opts: { sessionId?: string | null; logEntryId?: number | null } = {}
): Promise<DebugSessionEvent> {
  return invoke<DebugSessionEvent>("add_session_bookmark", {
    sessionId: opts.sessionId ?? null,
    note,
    logEntryId: opts.logEntryId ?? null,
  });
}

/**
 * The log lines kept with crash event `seq` of a debug session: the newest
 * `limit` (at most 1,000), ending with the crash. Rejects with `NotFound`
 * when none were kept.
 */
export async function getSessionCapture(
  id: string,
  seq: number,
  limit: number | null = null
): Promise<DebugSessionCapture> {
  return invoke<DebugSessionCapture>("get_session_capture", { id, seq, limit });
}

/** Read the app's exit reasons from the session's device and add those that belong to it. */
export async function refreshSessionExitReasons(id: string): Promise<DebugSessionExitRefresh> {
  return invoke<DebugSessionExitRefresh>("refresh_session_exit_reasons", { id });
}

// ── MCP Server ─────────────────────────────────────────────────────────────────

import type {
  AgentSkillStatus,
  McpSetupStatus,
  McpClientSetupStatus,
  McpActivityEntry,
  McpServerStatus,
  McpAttachedSession,
  McpStandaloneServer,
} from "@/bindings";
export type {
  McpSetupStatus,
  McpClientSetupStatus,
  McpActivityEntry,
  McpServerStatus,
  McpAttachedSession,
  McpStandaloneServer,
};

/**
 * Query the real binary path and read-only MCP registration status for supported clients.
 * Used to show setup commands and setup state in the Health panel.
 */
export async function getMcpSetupStatus(): Promise<McpSetupStatus> {
  return invoke<McpSetupStatus>("get_mcp_setup_status");
}

/** Return the last `limit` activity log entries (default 200). */
export async function getMcpActivity(limit?: number): Promise<McpActivityEntry[]> {
  return invoke<McpActivityEntry[]>("get_mcp_activity", { limit: limit ?? null });
}

/** List MCP clients attached to the app and standalone MCP servers that are running. */
export async function getMcpServerStatus(): Promise<McpServerStatus> {
  return invoke<McpServerStatus>("get_mcp_server_status");
}

/** Truncate the MCP activity log. */
export async function clearMcpActivity(): Promise<void> {
  return invoke<void>("clear_mcp_activity");
}

/** The Keynobi agent skill and whether it is installed for Claude Code. Reads only. */
export async function getAgentSkillStatus(): Promise<AgentSkillStatus> {
  return invoke<AgentSkillStatus>("get_agent_skill_status");
}

/**
 * Install the Keynobi agent skill for Claude Code (`~/.claude/skills/keynobi/SKILL.md`).
 * An existing different file is replaced only with `replace: true`.
 */
export async function installAgentSkill(replace: boolean): Promise<AgentSkillStatus> {
  return invoke<AgentSkillStatus>("install_agent_skill", { replace });
}

/** Listen for MCP clients attaching to or leaving the app. */
export function listenMcpSessionsChanged(
  cb: (sessions: McpAttachedSession[]) => void
): Promise<UnlistenFn> {
  return listen<McpAttachedSession[]>("mcp:sessions_changed", (event) => cb(event.payload));
}

// ── Android Studio integration ────────────────────────────────────────────────

/**
 * Open a source file in Android Studio at the given line.
 *
 * @param classPath  – fully-qualified package, e.g. `com.example.app`
 * @param filename   – source filename from the stack frame, e.g. `MainActivity.kt`
 * @param line       – 1-based line number
 * @returns the absolute path of the opened file
 */
export async function openInStudio(
  classPath: string,
  filename: string,
  line: number
): Promise<string> {
  return invoke<string>("open_in_studio", { classPath, filename, line });
}
