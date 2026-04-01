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

/** Return the path of the project that was last active, for session restore. */
export async function getLastActiveProject(): Promise<string | null> {
  return invoke<string | null>("get_last_active_project");
}

/** Read versionName, versionCode and applicationId from the open project. */
export async function getProjectAppInfo(): Promise<ProjectAppInfo> {
  return invoke<ProjectAppInfo>("get_project_app_info");
}

/** Write versionName and versionCode back to the app-level build.gradle(.kts). */
export async function saveProjectAppInfo(
  versionName: string,
  versionCode: bigint
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

// ── Settings ──────────────────────────────────────────────────────────────────

export interface AppSettings {
  editor: {
    fontFamily: string;
    fontSize: number;
    tabSize: number;
    insertSpaces: boolean;
    wordWrap: boolean;
    lineNumbers: boolean;
    bracketMatching: boolean;
    highlightActiveLine: boolean;
    autoCloseBrackets: boolean;
  };
  appearance: { uiFontSize: number };
  search: { contextLines: number; maxResults: number; maxFiles: number };
  files: { excludedDirs: string[]; excludedExtensions: string[]; maxFileSizeMb: number };
  android: { sdkPath: string | null };
  lsp: { logLevel: string; requestTimeoutSec: number };
  java: { home: string | null };
  advanced: {
    treeSitterCacheSize: number;
    lspMaxMessageSizeMb: number;
    watcherDebounceMs: number;
    lspDidChangeDebounceMs: number;
    diagnosticsPullDelayMs: number;
    hoverDelayMs: number;
    navigationHistoryDepth: number;
    recentFilesLimit: number;
  };
  build: {
    gradleJvmArgs: string | null;
    gradleParallel: boolean;
    gradleOffline: boolean;
    autoInstallOnBuild: boolean;
    buildVariant: string | null;
    selectedDevice: string | null;
  };
  logcat: {
    autoStart: boolean;
  };
  mcp: {
    autoStart: boolean;
    buildTimeoutSec: number;
    logcatDefaultCount: number;
    buildLogDefaultLines: number;
  };
}

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

export async function getDefaultSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_default_settings");
}

export async function resetSettingsToDefaults(): Promise<AppSettings> {
  return invoke<AppSettings>("reset_settings");
}

export async function detectSdkPath(): Promise<string | null> {
  return invoke<string | null>("detect_sdk_path");
}

export async function detectJavaPath(): Promise<string | null> {
  return invoke<string | null>("detect_java_path");
}

// ── Health checks ─────────────────────────────────────────────────────────────

import type { SystemHealthReport } from "@/bindings";

/** Run system-level health probes (Java, SDK, Gradle, disk). */
export async function runHealthChecks(): Promise<SystemHealthReport> {
  return invoke<SystemHealthReport>("run_health_checks");
}

// ── Error helpers ─────────────────────────────────────────────────────────────

export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

// ── Build system ──────────────────────────────────────────────────────────────

import type {
  BuildLine,
  BuildError,
  BuildStatus,
  BuildRecord,
} from "@/bindings";
import { Channel } from "@tauri-apps/api/core";

export type { BuildLine, BuildError, BuildStatus, BuildRecord };

/** Start a Gradle task and stream output via a Channel.
 *  Returns a process ID that can be used to cancel. */
export async function runGradleTask(
  task: string,
  onLine: (line: BuildLine) => void
): Promise<number> {
  const channel = new Channel<BuildLine>();
  channel.onmessage = onLine;
  return invoke<number>("run_gradle_task", { task, onLine: channel });
}

/** Persist the final build result into history after the Channel closes. */
export async function finalizeBuild(opts: {
  success: boolean;
  durationMs: number;
  errors: BuildError[];
  task: string;
  startedAt: string;
}): Promise<void> {
  return invoke<void>("finalize_build", {
    success: opts.success,
    durationMs: opts.durationMs,
    errors: opts.errors,
    task: opts.task,
    startedAt: opts.startedAt,
  });
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

export async function findApkPath(variant: string): Promise<string | null> {
  return invoke<string | null>("find_apk_path", { variant });
}

export function listenBuildComplete(
  cb: (e: {
    success: boolean;
    durationMs: number;
    errorCount: number;
    warningCount: number;
    task: string;
  }) => void
): Promise<UnlistenFn> {
  return listen<{
    success: boolean;
    durationMs: number;
    errorCount: number;
    warningCount: number;
    task: string;
  }>("build:complete", (event) => cb(event.payload));
}

// ── Variants ──────────────────────────────────────────────────────────────────

import type { BuildVariant, VariantList } from "@/bindings";
export type { BuildVariant, VariantList };

/**
 * Fast variant preview from static build.gradle parse.
 * Returns only explicitly declared variants — resolves instantly.
 * May return an empty list; use getVariantsFromGradle for the full picture.
 */
export async function getVariantsPreview(): Promise<VariantList> {
  return invoke<VariantList>("get_variants_preview");
}

/**
 * Authoritative variant list from `./gradlew tasks --group Build`.
 * Discovers every variant the project actually exposes, regardless of
 * how they are defined. Takes a few seconds on first run (daemon startup).
 */
export async function getVariantsFromGradle(): Promise<VariantList> {
  return invoke<VariantList>("get_variants_from_gradle");
}

export async function setActiveVariant(variant: string): Promise<void> {
  return invoke<void>("set_active_variant", { variant });
}

// ── Devices ───────────────────────────────────────────────────────────────────

import type { Device, AvdInfo, SystemImageInfo, DeviceDefinition, AvailableSystemImage, SdkDownloadProgress } from "@/bindings";
export type { Device, AvdInfo, SystemImageInfo, DeviceDefinition, AvailableSystemImage, SdkDownloadProgress };

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

export async function installApkOnDevice(
  serial: string,
  apkPath: string
): Promise<string> {
  return invoke<string>("install_apk_on_device", { serial, apkPath });
}

export async function launchAppOnDevice(
  serial: string,
  pkg: string,
  activity?: string
): Promise<string> {
  return invoke<string>("launch_app_on_device", {
    serial,
    package: pkg,
    activity: activity ?? null,
  });
}

export async function stopAppOnDevice(serial: string, pkg: string): Promise<void> {
  return invoke<void>("stop_app_on_device", { serial, package: pkg });
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

export function listenDeviceListChanged(
  cb: (devices: Device[]) => void
): Promise<UnlistenFn> {
  return listen<{ devices: Device[] }>("device:list_changed", (event) =>
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
import type { ProcessedEntry, LogStats, LogcatFilterSpec } from "@/bindings";
export type { ProcessedEntry, LogStats, LogcatFilterSpec };
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

export async function getLogcatStatus(): Promise<boolean> {
  return invoke<boolean>("get_logcat_status");
}

export async function listLogcatPackages(): Promise<string[]> {
  return invoke<string[]>("list_logcat_packages");
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

export function listenLogcatEntries(
  cb: (entries: ProcessedEntry[]) => void
): Promise<UnlistenFn> {
  return listen<ProcessedEntry[]>("logcat:entries", (e) => cb(e.payload));
}

export function listenLogcatCleared(cb: () => void): Promise<UnlistenFn> {
  return listen("logcat:cleared", () => cb());
}

// ── MCP Server ─────────────────────────────────────────────────────────────────

export interface McpSetupStatus {
  exePath: string;
  setupCommand: string;
  claudeFound: boolean;
  isConfigured: boolean;
  configuredCommand: string | null;
}

/** Start the MCP server on stdio. For use in MCP mode only. */
export async function startMcpServer(): Promise<void> {
  return invoke<void>("start_mcp_server");
}

/**
 * Query the real binary path, Claude CLI presence, and MCP registration status.
 * Used to show the correct setup command and button state in the Health panel.
 */
export async function getMcpSetupStatus(): Promise<McpSetupStatus> {
  return invoke<McpSetupStatus>("get_mcp_setup_status");
}

/**
 * Run `claude mcp add android-companion --command "<real_path> --mcp"`.
 * Returns a success message or throws with an error description.
 */
export async function configureMcpInClaude(): Promise<string> {
  return invoke<string>("configure_mcp_in_claude");
}

export interface McpActivityEntry {
  timestamp: string;
  kind: string;
  name: string;
  durationMs: number | null;
  status: string;
  summary: string | null;
}

export interface McpServerStatus {
  alive: boolean;
  pid: number | null;
}

/** Return the last `limit` activity log entries (default 200). */
export async function getMcpActivity(limit?: number): Promise<McpActivityEntry[]> {
  return invoke<McpActivityEntry[]>("get_mcp_activity", { limit: limit ?? null });
}

/** Check whether a headless MCP server process is currently running. */
export async function getMcpServerStatus(): Promise<McpServerStatus> {
  return invoke<McpServerStatus>("get_mcp_server_status");
}

/** Truncate the MCP activity log. */
export async function clearMcpActivity(): Promise<void> {
  return invoke<void>("clear_mcp_activity");
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
  line: number,
): Promise<string> {
  return invoke<string>("open_in_studio", { classPath, filename, line });
}
