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

// ── Error helpers ─────────────────────────────────────────────────────────────

export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}
