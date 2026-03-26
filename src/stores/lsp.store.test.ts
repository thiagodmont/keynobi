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
  setServerCapabilities,
  hasCapability,
  setIndexingProgress,
  setIndexingJustCompleted,
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
    setIndexingProgress(42);
    setIndexingJustCompleted("success");
    resetLspState();
    expect(lspState.status.state).toBe("stopped");
    expect(Object.keys(lspState.diagnostics)).toHaveLength(0);
    expect(lspState.downloadProgress).toBeNull();
    expect(lspState.indexingProgress).toBeNull();
    expect(lspState.indexingJustCompleted).toBeNull();
  });
});

// ── hasCapability ─────────────────────────────────────────────────────────────

describe("hasCapability", () => {
  beforeEach(() => {
    resetLspState();
  });

  it("returns true (optimistic) when capabilities are null — not yet received", () => {
    expect(lspState.serverCapabilities).toBeNull();
    expect(hasCapability("documentHighlightProvider")).toBe(true);
    expect(hasCapability("completionProvider")).toBe(true);
  });

  it("returns true when the capability is a truthy object (e.g. {} or { resolveProvider: true })", () => {
    setServerCapabilities({
      completionProvider: {},
      definitionProvider: true,
      hoverProvider: { workDoneProgress: false },
    });
    expect(hasCapability("completionProvider")).toBe(true);
    expect(hasCapability("definitionProvider")).toBe(true);
    expect(hasCapability("hoverProvider")).toBe(true);
  });

  it("returns false when the capability is explicitly false", () => {
    setServerCapabilities({ documentHighlightProvider: false });
    expect(hasCapability("documentHighlightProvider")).toBe(false);
  });

  it("returns false when the capability is absent from the server response", () => {
    // Server that only advertises completion — nothing else
    setServerCapabilities({ completionProvider: {} });
    expect(hasCapability("documentHighlightProvider")).toBe(false);
    expect(hasCapability("referencesProvider")).toBe(false);
  });

  it("returns false when capabilities object is empty (server supports nothing explicitly)", () => {
    setServerCapabilities({});
    expect(hasCapability("documentHighlightProvider")).toBe(false);
    expect(hasCapability("hoverProvider")).toBe(false);
  });

  it("correctly identifies that Kotlin LSP does NOT support documentHighlight", () => {
    // Simulate the real Kotlin LSP 262 capabilities (documentHighlight absent)
    setServerCapabilities({
      completionProvider: { resolveProvider: true },
      hoverProvider: true,
      signatureHelpProvider: { triggerCharacters: ["(", ","] },
      definitionProvider: true,
      referencesProvider: true,
      implementationProvider: true,
      documentSymbolProvider: true,
      codeActionProvider: true,
      // documentHighlightProvider intentionally absent
    });
    expect(hasCapability("definitionProvider")).toBe(true);
    expect(hasCapability("referencesProvider")).toBe(true);
    // This is the exact case that was causing ERROR spam in the Output panel
    expect(hasCapability("documentHighlightProvider")).toBe(false);
  });

  it("resets serverCapabilities on resetLspState", () => {
    setServerCapabilities({ completionProvider: {} });
    resetLspState();
    expect(lspState.serverCapabilities).toBeNull();
  });
});

// ── Indexing progress state ───────────────────────────────────────────────────

describe("indexingProgress and indexingJustCompleted", () => {
  beforeEach(() => {
    resetLspState();
  });

  it("starts as null", () => {
    expect(lspState.indexingProgress).toBeNull();
    expect(lspState.indexingJustCompleted).toBeNull();
  });

  it("setIndexingProgress stores a percentage", () => {
    setIndexingProgress(55);
    expect(lspState.indexingProgress).toBe(55);
  });

  it("setIndexingProgress accepts null for indeterminate", () => {
    setIndexingProgress(80);
    setIndexingProgress(null);
    expect(lspState.indexingProgress).toBeNull();
  });

  it("setIndexingJustCompleted stores success", () => {
    setIndexingJustCompleted("success");
    expect(lspState.indexingJustCompleted).toBe("success");
  });

  it("setIndexingJustCompleted stores error", () => {
    setIndexingJustCompleted("error");
    expect(lspState.indexingJustCompleted).toBe("error");
  });

  it("setIndexingJustCompleted can be cleared to null", () => {
    setIndexingJustCompleted("success");
    setIndexingJustCompleted(null);
    expect(lspState.indexingJustCompleted).toBeNull();
  });

  it("resetLspState zeros out indexing fields", () => {
    setIndexingProgress(75);
    setIndexingJustCompleted("success");
    resetLspState();
    expect(lspState.indexingProgress).toBeNull();
    expect(lspState.indexingJustCompleted).toBeNull();
  });
});
