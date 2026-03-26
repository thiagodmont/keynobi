import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FileNode } from "@/bindings";
import type { FileEvent } from "@/bindings";

// ── File System ──────────────────────────────────────────────────────────────

export async function openFolderDialog(): Promise<string | null> {
  const result = await open({
    directory: true,
    multiple: false,
    title: "Open Android Project",
  });
  if (Array.isArray(result)) return result[0] ?? null;
  return result as string | null;
}

export async function openProject(path: string): Promise<FileNode> {
  return invoke<FileNode>("open_project", { path });
}

export async function getFileTree(): Promise<FileNode> {
  return invoke<FileNode>("get_file_tree");
}

export async function getDirectoryChildren(path: string): Promise<FileNode[]> {
  return invoke<FileNode[]>("get_directory_children", { path });
}

export async function readFile(path: string): Promise<string> {
  return invoke<string>("read_file", { path });
}

export async function writeFile(path: string, content: string): Promise<void> {
  return invoke<void>("write_file", { path, content });
}

export async function createFile(path: string): Promise<void> {
  return invoke<void>("create_file", { path });
}

export async function createDirectory(path: string): Promise<void> {
  return invoke<void>("create_directory", { path });
}

export async function deletePath(path: string): Promise<void> {
  return invoke<void>("delete_path", { path });
}

export async function renamePath(
  oldPath: string,
  newPath: string
): Promise<void> {
  return invoke<void>("rename_path", { oldPath, newPath });
}

export async function getProjectRoot(): Promise<string | null> {
  return invoke<string | null>("get_project_root");
}

export async function getGradleRoot(): Promise<string | null> {
  return invoke<string | null>("get_gradle_root");
}

// ── File Events ───────────────────────────────────────────────────────────────

export type { FileEvent };

export function onFileChanged(
  callback: (event: FileEvent) => void
): Promise<UnlistenFn> {
  return listen<FileEvent>("file:changed", (e) => callback(e.payload));
}

// ── Search ────────────────────────────────────────────────────────────────────

export interface SearchOptions {
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  includePattern: string | null;
  excludePattern: string | null;
}

export interface SearchMatch {
  line: number;
  col: number;
  endCol: number;
  lineContent: string;
  contextBefore: string[];
  contextAfter: string[];
}

export interface SearchResult {
  path: string;
  matches: SearchMatch[];
}

export async function searchProject(
  query: string,
  options: SearchOptions
): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_project", { query, options });
}

export async function searchInFile(
  path: string,
  query: string,
  options: SearchOptions
): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_in_file", { path, query, options });
}

// ── Tree-sitter ───────────────────────────────────────────────────────────────

export interface SymbolInfo {
  name: string;
  kind: string;
  range: { startLine: number; startCol: number; endLine: number; endCol: number };
  selectionRange: { startLine: number; startCol: number; endLine: number; endCol: number };
  children: SymbolInfo[] | null;
}

export async function getDocumentSymbols(path: string): Promise<SymbolInfo[]> {
  return invoke<SymbolInfo[]>("get_document_symbols", { path });
}

export async function getSymbolAtPosition(
  path: string,
  line: number,
  col: number
): Promise<string | null> {
  return invoke<string | null>("get_symbol_at_position", { path, line, col });
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

// ── LSP Navigation ───────────────────────────────────────────────────────────

export interface LspLocation {
  uri: string;
  range: {
    start: { line: number; character: number };
    end: { line: number; character: number };
  };
}

export interface LspHighlight {
  range: {
    start: { line: number; character: number };
    end: { line: number; character: number };
  };
  /** 1=Text, 2=Read, 3=Write */
  kind?: number;
}

export interface LspSignatureHelp {
  signatures: Array<{
    label: string;
    documentation?: string;
    parameters?: Array<{ label: string | [number, number]; documentation?: string }>;
  }>;
  activeSignature?: number;
  activeParameter?: number;
}

export interface LspCodeAction {
  title: string;
  kind?: string;
  edit?: {
    changes?: Record<string, Array<{ range: LspLocation["range"]; newText: string }>>;
    documentChanges?: unknown[];
  };
}

export async function lspDidOpen(path: string, content: string, language: string): Promise<void> {
  return invoke<void>("lsp_did_open", { path, content, language });
}

export async function lspDidClose(path: string): Promise<void> {
  return invoke<void>("lsp_did_close", { path });
}

export async function lspDidSave(path: string, content: string): Promise<void> {
  return invoke<void>("lsp_did_save", { path, content });
}

export async function lspDefinition(path: string, line: number, col: number): Promise<LspLocation[]> {
  const result = await invoke<LspLocation | LspLocation[] | null>("lsp_definition", { path, line, col });
  if (!result) return [];
  return Array.isArray(result) ? result : [result];
}

export async function lspReferences(path: string, line: number, col: number): Promise<LspLocation[]> {
  const result = await invoke<LspLocation[] | null>("lsp_references", { path, line, col });
  return result ?? [];
}

export async function lspImplementation(path: string, line: number, col: number): Promise<LspLocation[]> {
  const result = await invoke<LspLocation | LspLocation[] | null>("lsp_implementation", { path, line, col });
  if (!result) return [];
  return Array.isArray(result) ? result : [result];
}

export async function lspDocumentHighlight(path: string, line: number, col: number): Promise<LspHighlight[]> {
  const result = await invoke<LspHighlight[] | null>("lsp_document_highlight", { path, line, col });
  return result ?? [];
}

export async function lspSignatureHelp(path: string, line: number, col: number): Promise<LspSignatureHelp | null> {
  return invoke<LspSignatureHelp | null>("lsp_signature_help", { path, line, col });
}

export async function lspCodeAction(
  path: string,
  startLine: number,
  startCol: number,
  endLine: number,
  endCol: number
): Promise<LspCodeAction[]> {
  const result = await invoke<LspCodeAction[] | null>("lsp_code_action", { path, startLine, startCol, endLine, endCol });
  return result ?? [];
}

/** Convert a file:// URI to a local path */
export function uriToPath(uri: string): string {
  return uri.replace(/^file:\/\//, "");
}

/**
 * Describe the kind of a URI returned by the LSP server.
 *
 * - `"file"` — a regular on-disk file (`file://` scheme).
 * - `"jar"` — an entry inside a JAR/ZIP archive.  The `archivePath` and
 *             `entryPath` fields are populated.
 * - `"jrt"` — a Java runtime module entry (`jrt:` scheme — JDK 9+).
 *             `archivePath` is the JDK home, `entryPath` is the module entry.
 * - `"unknown"` — anything else; treat as opaque.
 */
export type LspUriKind =
  | { kind: "file"; path: string }
  | { kind: "jar"; uri: string; archivePath: string; entryPath: string }
  | { kind: "jrt"; uri: string; archivePath: string; entryPath: string }
  | { kind: "unknown"; uri: string };

/**
 * Parse an LSP URI into a structured representation.
 *
 * The kotlin-lsp returns several URI forms for library/binary navigation:
 *   • `file:///path/to/file.kt`                 → regular file
 *   • `jar:file:///path/to/lib.jar!/entry.kt`   → JAR entry
 *   • `/path/to/src.zip!/java.base/UUID.java`   → archive path (no scheme)
 *   • `jrt:///modules/java.base/UUID.class`     → JRT (JDK modules)
 */
export function parseLspUri(uri: string): LspUriKind {
  // Standard file URI
  if (uri.startsWith("file://")) {
    return { kind: "file", path: uri.replace(/^file:\/\//, "") };
  }

  // jar:file:///path/to/archive.jar!/entry/path
  const jarMatch = uri.match(/^jar:file:\/\/(\/[^!]+)!\/(.+)$/);
  if (jarMatch) {
    return {
      kind: "jar",
      uri,
      archivePath: jarMatch[1],
      entryPath: jarMatch[2],
    };
  }

  // Bare filesystem path with !/ separator (e.g. /path/src.zip!/entry)
  const archiveSepIdx = uri.indexOf("!/");
  if (archiveSepIdx !== -1 && uri.startsWith("/")) {
    return {
      kind: "jar",
      uri,
      archivePath: uri.slice(0, archiveSepIdx),
      entryPath: uri.slice(archiveSepIdx + 2),
    };
  }

  // jrt: URI scheme (Java runtime modules, JDK 9+)
  if (uri.startsWith("jrt:")) {
    return { kind: "jrt", uri, archivePath: "", entryPath: uri };
  }

  return { kind: "unknown", uri };
}

/** Invoke the LSP's custom `decompile` command for a JAR/JRT URI. */
export async function lspDecompile(uri: string): Promise<{ code: string; language: string } | null> {
  try {
    return await invoke<{ code: string; language: string } | null>("lsp_decompile", { uri });
  } catch {
    return null;
  }
}

/** Read a text entry from a ZIP/JAR archive on disk. */
export async function lspReadArchiveEntry(
  archivePath: string,
  entryPath: string
): Promise<string | null> {
  try {
    return await invoke<string>("lsp_read_archive_entry", { archivePath, entryPath });
  } catch {
    return null;
  }
}

/** Request full semantic tokens for a document from the LSP server. */
export async function lspSemanticTokens(path: string): Promise<import("@tauri-apps/api/core").InvokeArgs | null> {
  try {
    return await invoke<unknown>("lsp_semantic_tokens", { path }) as import("@tauri-apps/api/core").InvokeArgs | null;
  } catch {
    return null;
  }
}

/** Request code actions with a specific `only` filter. */
export async function lspCodeActionFiltered(
  path: string,
  startLine: number,
  startCol: number,
  endLine: number,
  endCol: number,
  only: string[]
): Promise<unknown[]> {
  try {
    const result = await invoke<unknown[] | null>("lsp_code_action_filtered", {
      path, startLine, startCol, endLine, endCol, only,
    });
    return result ?? [];
  } catch {
    return [];
  }
}

/**
 * Generate `workspace.json` in the workspace root by scanning the Gradle
 * project structure from the filesystem.  This is the IDE-side fix for
 * "Package directive does not match" false positives on Android multi-module
 * projects using AGP convention plugins in a composite build.
 *
 * Returns the path to the generated file on success, or throws on failure.
 */
export async function lspExportWorkspace(): Promise<string> {
  return invoke<string>("lsp_generate_workspace_json");
}

// ── Log viewer ────────────────────────────────────────────────────────────────

import type { LogEntry, LspStatus, DownloadProgress } from "@/bindings";

/**
 * Subscribe to `lsp:status` events so the frontend store stays in sync with
 * the actual server lifecycle.  Returns the unlisten function.
 */
export function listenLspStatus(
  cb: (status: LspStatus) => void
): Promise<UnlistenFn> {
  return listen<LspStatus>("lsp:status", (event) => cb(event.payload));
}

/**
 * Subscribe to `lsp:capabilities` events emitted once after the LSP
 * `initialize` handshake.  Use this to know which methods are supported
 * before making requests.
 */
export function listenLspCapabilities(
  cb: (capabilities: Record<string, unknown>) => void
): Promise<UnlistenFn> {
  return listen<Record<string, unknown>>("lsp:capabilities", (event) =>
    cb(event.payload)
  );
}

/**
 * Subscribe to raw `lsp:progress` events.  The payload is the LSP
 * WorkDoneProgress params which may include `value.percentage` (0–100).
 * Returns the unlisten function.
 */
export function listenLspProgress(
  cb: (params: unknown) => void
): Promise<UnlistenFn> {
  return listen("lsp:progress", (event) => cb(event.payload));
}

/**
 * Subscribe to `lsp:download_progress` events during LSP sidecar download.
 * Returns the unlisten function.
 */
export function listenLspDownloadProgress(
  cb: (progress: DownloadProgress) => void
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("lsp:download_progress", (event) =>
    cb(event.payload)
  );
}

/**
 * Subscribe to real-time `lsp:log` events emitted by the Rust notification
 * task.  Returns the unlisten function — call it in `onCleanup`.
 */
export function listenLspLog(
  cb: (entry: LogEntry) => void
): Promise<UnlistenFn> {
  return listen<LogEntry>("lsp:log", (event) => cb(event.payload));
}

/**
 * Fetch the buffered log entries for the initial panel load (entries that
 * arrived before the panel was mounted).
 */
export async function lspGetLogs(): Promise<LogEntry[]> {
  return invoke<LogEntry[]>("lsp_get_logs");
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
