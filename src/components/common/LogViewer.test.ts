/**
 * Tests for the LogViewer source filter logic.
 * We test the pure filtering function and the unique-sources derivation
 * without rendering the component (which requires a real DOM).
 */

import { describe, it, expect } from "vitest";
import type { LogEntry, LogLevel } from "@/bindings";
import { matchesLogViewerFilter, uniqueLogViewerSources } from "./log-viewer-filter";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEntry(
  id: number,
  source: string,
  level: LogLevel = "info",
  message = "test"
): LogEntry {
  return { id, timestamp: new Date().toISOString(), level, source, message };
}

// ── uniqueSources ─────────────────────────────────────────────────────────────

describe("uniqueSources", () => {
  it("returns empty array when no entries", () => {
    expect(uniqueLogViewerSources([])).toEqual([]);
  });

  it("returns sorted unique sources", () => {
    const entries = [
      makeEntry(1, "lsp:server"),
      makeEntry(2, "lsp:navigate"),
      makeEntry(3, "lsp:server"),
      makeEntry(4, "lsp:progress"),
    ];
    expect(uniqueLogViewerSources(entries)).toEqual(["lsp:navigate", "lsp:progress", "lsp:server"]);
  });

  it("ignores entries with empty source", () => {
    const entries = [makeEntry(1, "lsp:server"), { ...makeEntry(2, ""), source: "" }];
    expect(uniqueLogViewerSources(entries)).toEqual(["lsp:server"]);
  });

  it("all expected LSP sources are surfaced", () => {
    const entries = [
      makeEntry(1, "lsp:startup"),
      makeEntry(2, "lsp:server"),
      makeEntry(3, "lsp:stderr"),
      makeEntry(4, "lsp:client"),
      makeEntry(5, "lsp:progress"),
      makeEntry(6, "lsp:navigate"),
    ];
    const sources = uniqueLogViewerSources(entries);
    expect(sources).toContain("lsp:startup");
    expect(sources).toContain("lsp:server");
    expect(sources).toContain("lsp:stderr");
    expect(sources).toContain("lsp:client");
    expect(sources).toContain("lsp:progress");
    expect(sources).toContain("lsp:navigate");
    expect(sources).toHaveLength(6);
  });
});

// ── matchesFilter ─────────────────────────────────────────────────────────────

describe("matchesFilter (with source)", () => {
  const entry = makeEntry(1, "lsp:navigate", "info", "definition not found");

  it("passes with all filters set to all", () => {
    expect(matchesLogViewerFilter(entry, "all", "all", "")).toBe(true);
  });

  it("passes when source matches", () => {
    expect(matchesLogViewerFilter(entry, "all", "lsp:navigate", "")).toBe(true);
  });

  it("fails when source does not match", () => {
    expect(matchesLogViewerFilter(entry, "all", "lsp:server", "")).toBe(false);
  });

  it("passes when level is at or above the selected minimum", () => {
    expect(matchesLogViewerFilter(entry, "info", "all", "")).toBe(true);
    expect(matchesLogViewerFilter(makeEntry(2, "build", "error"), "warn", "all", "")).toBe(true);
    expect(matchesLogViewerFilter(makeEntry(3, "build", "warn"), "info", "all", "")).toBe(true);
  });

  it("fails when level is below the selected minimum", () => {
    expect(matchesLogViewerFilter(entry, "error", "all", "")).toBe(false);
    expect(matchesLogViewerFilter(makeEntry(2, "build", "trace"), "debug", "all", "")).toBe(false);
  });

  it("passes when search matches message", () => {
    expect(matchesLogViewerFilter(entry, "all", "all", "definition")).toBe(true);
  });

  it("fails when search does not match message", () => {
    expect(matchesLogViewerFilter(entry, "all", "all", "compile error")).toBe(false);
  });

  it("search is case-insensitive", () => {
    expect(matchesLogViewerFilter(entry, "all", "all", "DEFINITION")).toBe(true);
  });

  it("applies all three filters simultaneously", () => {
    expect(matchesLogViewerFilter(entry, "info", "lsp:navigate", "not found")).toBe(true);
    expect(matchesLogViewerFilter(entry, "info", "lsp:server", "not found")).toBe(false);
    expect(matchesLogViewerFilter(entry, "error", "lsp:navigate", "not found")).toBe(false);
    expect(matchesLogViewerFilter(entry, "info", "lsp:navigate", "compile")).toBe(false);
  });
});
