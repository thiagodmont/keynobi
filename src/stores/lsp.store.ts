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

interface LspStoreState {
  status: LspStatus;
  diagnostics: Record<string, Diagnostic[]>;
  downloadProgress: DownloadProgress | null;
}

const [lspState, setLspState] = createStore<LspStoreState>({
  status: { state: "stopped", message: null },
  diagnostics: {},
  downloadProgress: null,
});

export { lspState, setLspState };

export function setLspStatus(state: LspStatusState, message?: string) {
  setLspState("status", { state, message: message ?? null });
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
  });
}
