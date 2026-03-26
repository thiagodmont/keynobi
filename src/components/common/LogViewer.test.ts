/**
 * Tests for the LogViewer source filter logic.
 * We test the pure filtering function and the unique-sources derivation
 * without rendering the component (which requires a real DOM).
 */

import { describe, it, expect } from "vitest";
import type { LogEntry, LogLevel } from "@/bindings";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEntry(
  id: number,
  source: string,
  level: LogLevel = "info",
  message = "test"
): LogEntry {
  return { id, timestamp: new Date().toISOString(), level, source, message };
}

/** Re-implements the matchesFilter logic from LogViewer so we can test it. */
function matchesFilter(
  entry: LogEntry,
  level: string,
  source: string,
  search: string
): boolean {
  if (level !== "all" && entry.level !== level) return false;
  if (source !== "all" && entry.source !== source) return false;
  if (search && !entry.message.toLowerCase().includes(search.toLowerCase())) return false;
  return true;
}

/** Re-implements uniqueSources derivation from LogViewer. */
function uniqueSources(entries: LogEntry[]): string[] {
  const seen = new Set<string>();
  for (const e of entries) {
    if (e.source) seen.add(e.source);
  }
  return Array.from(seen).sort();
}

// ── uniqueSources ─────────────────────────────────────────────────────────────

describe("uniqueSources", () => {
  it("returns empty array when no entries", () => {
    expect(uniqueSources([])).toEqual([]);
  });

  it("returns sorted unique sources", () => {
    const entries = [
      makeEntry(1, "lsp:server"),
      makeEntry(2, "lsp:navigate"),
      makeEntry(3, "lsp:server"),
      makeEntry(4, "lsp:progress"),
    ];
    expect(uniqueSources(entries)).toEqual(["lsp:navigate", "lsp:progress", "lsp:server"]);
  });

  it("ignores entries with empty source", () => {
    const entries = [
      makeEntry(1, "lsp:server"),
      { ...makeEntry(2, ""), source: "" },
    ];
    expect(uniqueSources(entries)).toEqual(["lsp:server"]);
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
    const sources = uniqueSources(entries);
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
    expect(matchesFilter(entry, "all", "all", "")).toBe(true);
  });

  it("passes when source matches", () => {
    expect(matchesFilter(entry, "all", "lsp:navigate", "")).toBe(true);
  });

  it("fails when source does not match", () => {
    expect(matchesFilter(entry, "all", "lsp:server", "")).toBe(false);
  });

  it("passes when level matches", () => {
    expect(matchesFilter(entry, "info", "all", "")).toBe(true);
  });

  it("fails when level does not match", () => {
    expect(matchesFilter(entry, "error", "all", "")).toBe(false);
  });

  it("passes when search matches message", () => {
    expect(matchesFilter(entry, "all", "all", "definition")).toBe(true);
  });

  it("fails when search does not match message", () => {
    expect(matchesFilter(entry, "all", "all", "compile error")).toBe(false);
  });

  it("search is case-insensitive", () => {
    expect(matchesFilter(entry, "all", "all", "DEFINITION")).toBe(true);
  });

  it("applies all three filters simultaneously", () => {
    expect(matchesFilter(entry, "info", "lsp:navigate", "not found")).toBe(true);
    expect(matchesFilter(entry, "info", "lsp:server", "not found")).toBe(false);
    expect(matchesFilter(entry, "error", "lsp:navigate", "not found")).toBe(false);
    expect(matchesFilter(entry, "info", "lsp:navigate", "compile")).toBe(false);
  });
});
