import { describe, it, expect, beforeEach } from "vitest";
import {
  lspState,
  setLspStatus,
  updateDiagnostics,
  clearDiagnostics,
  setDownloadProgress,
  getDiagnosticsForFile,
  getDiagnosticCounts,
  resetLspState,
  type Diagnostic,
} from "./lsp.store";

function makeDiag(
  path: string,
  severity: Diagnostic["severity"] = "error"
): Diagnostic {
  return {
    path,
    range: { startLine: 1, startCol: 0, endLine: 1, endCol: 10 },
    severity,
    message: "test error",
    source: "kotlin",
    code: null,
  };
}

describe("lsp.store", () => {
  beforeEach(() => {
    resetLspState();
  });

  it("starts with stopped status", () => {
    expect(lspState.status.state).toBe("stopped");
    expect(lspState.status.message).toBeNull();
  });

  it("updates status", () => {
    setLspStatus("starting", "Initializing...");
    expect(lspState.status.state).toBe("starting");
    expect(lspState.status.message).toBe("Initializing...");
  });

  it("updates status without message", () => {
    setLspStatus("ready");
    expect(lspState.status.state).toBe("ready");
    expect(lspState.status.message).toBeNull();
  });

  it("adds diagnostics for a file", () => {
    const diag = makeDiag("/project/Main.kt");
    updateDiagnostics("/project/Main.kt", [diag]);
    expect(lspState.diagnostics["/project/Main.kt"]).toHaveLength(1);
    expect(lspState.diagnostics["/project/Main.kt"][0].message).toBe(
      "test error"
    );
  });

  it("replaces diagnostics for a file", () => {
    updateDiagnostics("/project/Main.kt", [makeDiag("/project/Main.kt")]);
    updateDiagnostics("/project/Main.kt", [
      makeDiag("/project/Main.kt"),
      makeDiag("/project/Main.kt", "warning"),
    ]);
    expect(lspState.diagnostics["/project/Main.kt"]).toHaveLength(2);
  });

  it("clears diagnostics for a specific file", () => {
    updateDiagnostics("/project/A.kt", [makeDiag("/project/A.kt")]);
    updateDiagnostics("/project/B.kt", [makeDiag("/project/B.kt")]);
    clearDiagnostics("/project/A.kt");
    expect(lspState.diagnostics["/project/A.kt"]).toBeUndefined();
    expect(lspState.diagnostics["/project/B.kt"]).toHaveLength(1);
  });

  it("clears all diagnostics", () => {
    updateDiagnostics("/project/A.kt", [makeDiag("/project/A.kt")]);
    updateDiagnostics("/project/B.kt", [makeDiag("/project/B.kt")]);
    clearDiagnostics();
    expect(Object.keys(lspState.diagnostics)).toHaveLength(0);
  });

  it("getDiagnosticsForFile returns empty array for unknown file", () => {
    expect(getDiagnosticsForFile("/unknown")).toEqual([]);
  });

  it("getDiagnosticsForFile returns diagnostics for known file", () => {
    const diag = makeDiag("/project/Main.kt");
    updateDiagnostics("/project/Main.kt", [diag]);
    expect(getDiagnosticsForFile("/project/Main.kt")).toHaveLength(1);
  });

  it("getDiagnosticCounts counts errors and warnings", () => {
    updateDiagnostics("/project/A.kt", [
      makeDiag("/project/A.kt", "error"),
      makeDiag("/project/A.kt", "warning"),
    ]);
    updateDiagnostics("/project/B.kt", [
      makeDiag("/project/B.kt", "error"),
      makeDiag("/project/B.kt", "information"),
    ]);
    const counts = getDiagnosticCounts();
    expect(counts.errors).toBe(2);
    expect(counts.warnings).toBe(1);
  });

  it("does not count information and hint in error/warning counts", () => {
    updateDiagnostics("/project/X.kt", [
      makeDiag("/project/X.kt", "information"),
      makeDiag("/project/X.kt", "hint"),
    ]);
    const counts = getDiagnosticCounts();
    expect(counts.errors).toBe(0);
    expect(counts.warnings).toBe(0);
  });

  it("handles empty diagnostics array update", () => {
    updateDiagnostics("/project/A.kt", [makeDiag("/project/A.kt")]);
    updateDiagnostics("/project/A.kt", []);
    expect(getDiagnosticsForFile("/project/A.kt")).toHaveLength(0);
  });

  it("tracks download progress", () => {
    expect(lspState.downloadProgress).toBeNull();
    setDownloadProgress({
      downloadedBytes: 1024,
      totalBytes: 2048,
      percent: 50,
    });
    expect(lspState.downloadProgress?.percent).toBe(50);
    setDownloadProgress(null);
    expect(lspState.downloadProgress).toBeNull();
  });

  it("resets to default state", () => {
    setLspStatus("ready");
    updateDiagnostics("/project/Main.kt", [makeDiag("/project/Main.kt")]);
    setDownloadProgress({ downloadedBytes: 100, totalBytes: 200, percent: 50 });
    resetLspState();
    expect(lspState.status.state).toBe("stopped");
    expect(Object.keys(lspState.diagnostics)).toHaveLength(0);
    expect(lspState.downloadProgress).toBeNull();
  });
});
