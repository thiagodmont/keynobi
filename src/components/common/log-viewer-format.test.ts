import { describe, expect, it } from "vitest";
import { formatLogViewerEntry, formatLogViewerTime } from "./log-viewer-format";
import type { LogEntry } from "@/bindings";

function makeEntry(overrides: Partial<LogEntry> = {}): LogEntry {
  return {
    id: 1,
    timestamp: "2026-04-25T12:34:56.789Z",
    level: "warn",
    source: "build",
    message: "compile warning",
    ...overrides,
  };
}

describe("log viewer formatting", () => {
  it("formats timestamps as local time with milliseconds", () => {
    const timestamp = new Date(2026, 3, 25, 12, 34, 56, 789).toISOString();

    expect(formatLogViewerTime(timestamp)).toBe("12:34:56.789");
  });

  it("formats copied rows with optional timestamp and source", () => {
    expect(formatLogViewerEntry(makeEntry(), { showTimestamp: false, showSource: true })).toBe(
      "[WARN] [build] compile warning"
    );
    expect(formatLogViewerEntry(makeEntry(), { showTimestamp: false, showSource: false })).toBe(
      "[WARN] compile warning"
    );
  });
});
