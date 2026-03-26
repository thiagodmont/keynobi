import { createStore, produce } from "solid-js/store";

export type LspStatusState =
  | "notInstalled"
  | "downloading"
  | "starting"
  | "indexing"
  | "ready"
  | "error"
  | "stopped";

export interface LspStatus {
  state: LspStatusState;
  message: string | null;
}

export interface Diagnostic {
  path: string;
  range: {
    startLine: number;
    startCol: number;
    endLine: number;
    endCol: number;
  };
  severity: "error" | "warning" | "information" | "hint";
  message: string;
  source: string | null;
  code: string | null;
}

export interface DownloadProgress {
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number | null;
}

/**
 * Subset of LSP ServerCapabilities we check before making requests.
 * Only the fields our editor actually uses are listed — unknown fields
 * default to absent (treated as "not supported").
 */
export interface ServerCapabilities {
  completionProvider?: boolean | object;
  hoverProvider?: boolean | object;
  signatureHelpProvider?: boolean | object;
  definitionProvider?: boolean | object;
  referencesProvider?: boolean | object;
  implementationProvider?: boolean | object;
  documentHighlightProvider?: boolean | object;
  documentSymbolProvider?: boolean | object;
  codeActionProvider?: boolean | object;
  documentFormattingProvider?: boolean | object;
  renameProvider?: boolean | object;
  diagnosticProvider?: boolean | object;
}

interface LspStoreState {
  status: LspStatus;
  diagnostics: Record<string, Diagnostic[]>;
  downloadProgress: DownloadProgress | null;
  serverCapabilities: ServerCapabilities | null;
  /** 0–100 when the LSP reports a percentage; null for indeterminate. */
  indexingProgress: number | null;
  /**
   * Set briefly after indexing finishes to show a success/error flash in the
   * status bar.  Automatically cleared to null after 3 seconds on success.
   */
  indexingJustCompleted: "success" | "error" | null;
}

const [lspState, setLspState] = createStore<LspStoreState>({
  status: { state: "stopped", message: null },
  diagnostics: {},
  downloadProgress: null,
  serverCapabilities: null,
  indexingProgress: null,
  indexingJustCompleted: null,
});

export { lspState, setLspState };

export function setLspStatus(state: LspStatusState, message?: string) {
  setLspState("status", { state, message: message ?? null });
}

export function setServerCapabilities(caps: ServerCapabilities | null) {
  setLspState("serverCapabilities", caps);
}

export function setIndexingProgress(pct: number | null) {
  setLspState("indexingProgress", pct);
}

export function setIndexingJustCompleted(state: "success" | "error" | null) {
  setLspState("indexingJustCompleted", state);
}

/**
 * Check whether the connected LSP server supports a given capability.
 *
 * Returns `true` (optimistic) when capabilities have not been fetched yet —
 * callers should let the first request fail naturally and then the capability
 * will be known.  Returns `false` only when the server explicitly omits a
 * feature from its `initialize` response.
 */
export function hasCapability(key: keyof ServerCapabilities): boolean {
  const caps = lspState.serverCapabilities;
  if (!caps) return true; // not yet received → optimistic
  const val = caps[key as keyof typeof caps];
  if (val === undefined || val === null || val === false) return false;
  return true;
}

export function updateDiagnostics(path: string, diags: Diagnostic[]) {
  setLspState("diagnostics", path, diags);
}

export function clearDiagnostics(path?: string) {
  if (path) {
    setLspState(
      produce((s) => {
        delete s.diagnostics[path];
      })
    );
  } else {
    setLspState(
      produce((s) => {
        for (const key of Object.keys(s.diagnostics)) {
          delete s.diagnostics[key];
        }
      })
    );
  }
}

export function setDownloadProgress(progress: DownloadProgress | null) {
  setLspState("downloadProgress", progress);
}

export function getDiagnosticsForFile(path: string): Diagnostic[] {
  return lspState.diagnostics[path] ?? [];
}

export function getDiagnosticCounts(): { errors: number; warnings: number } {
  let errors = 0;
  let warnings = 0;
  for (const path of Object.keys(lspState.diagnostics)) {
    for (const diag of lspState.diagnostics[path]) {
      if (diag.severity === "error") errors++;
      else if (diag.severity === "warning") warnings++;
    }
  }
  return { errors, warnings };
}

export function resetLspState() {
  setLspState({
    status: { state: "stopped", message: null },
    diagnostics: {},
    downloadProgress: null,
    serverCapabilities: null,
    indexingProgress: null,
    indexingJustCompleted: null,
  });
}
