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

export async function getBuildVariants(): Promise<VariantList> {
  return invoke<VariantList>("get_build_variants");
}

export async function setActiveVariant(variant: string): Promise<void> {
  return invoke<void>("set_active_variant", { variant });
}

// ── Devices ───────────────────────────────────────────────────────────────────

import type { Device, AvdInfo } from "@/bindings";
export type { Device, AvdInfo };

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
): Promise<void> {
  return invoke<void>("launch_app_on_device", {
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

// ── Logcat ────────────────────────────────────────────────────────────────────

export interface LogcatEntry {
  id: number;
  timestamp: string;
  pid: number;
  tid: number;
  level: "verbose" | "debug" | "info" | "warn" | "error" | "fatal" | "unknown";
  tag: string;
  message: string;
  isCrash: boolean;
  /** Package/process name resolved from the ActivityManager pid→package map. */
  package: string | null;
  /** Entry kind — normal log line vs process lifecycle separator. */
  kind?: "normal" | "processDied" | "processStarted";
}

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
}): Promise<LogcatEntry[]> {
  return invoke<LogcatEntry[]>("get_logcat_entries", {
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

export function listenLogcatEntries(
  cb: (entries: LogcatEntry[]) => void
): Promise<UnlistenFn> {
  return listen<LogcatEntry[]>("logcat:entries", (e) => cb(e.payload));
}

export function listenLogcatCleared(cb: () => void): Promise<UnlistenFn> {
  return listen("logcat:cleared", () => cb());
}

// ── MCP Server ─────────────────────────────────────────────────────────────────

/** Start the MCP server on stdio. For use in MCP mode only. */
export async function startMcpServer(): Promise<void> {
  return invoke<void>("start_mcp_server");
}
