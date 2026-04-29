import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LogcatFilterSpec, LogStats, ProcessedEntry } from "@/bindings";
import {
  replaceLogcatEntries,
  setLogcatRingBufferTotal,
  setLogcatStreaming,
} from "@/stores/logcat.store";
import { LogcatPanel } from "./LogcatPanel";

const ROW_TITLE = "Click to copy · Shift+click to select range";

const BASE_ENTRY = {
  id: 1n,
  timestamp: "04-29 13:00:00.000",
  pid: 1234,
  tid: 5678,
  level: "info",
  tag: "MainActivity",
  message: "Activity started",
  package: "com.example.app",
  kind: "normal",
  isCrash: false,
  flags: 0,
  category: "lifecycle",
  crashGroupId: null,
  jsonBody: null,
} satisfies ProcessedEntry;

function emptyFilter(): LogcatFilterSpec {
  return { minLevel: null, tag: null, text: null, package: null, onlyCrashes: false };
}

function priority(level: string): number {
  switch (level.toLowerCase()) {
    case "verbose":
      return 0;
    case "debug":
      return 1;
    case "info":
      return 2;
    case "warn":
      return 3;
    case "error":
      return 4;
    case "fatal":
      return 5;
    default:
      return 6;
  }
}

function filterEntries(entries: ProcessedEntry[], spec: LogcatFilterSpec): ProcessedEntry[] {
  return entries.filter((entry) => {
    if (spec.onlyCrashes && !entry.isCrash) return false;
    if (spec.minLevel && priority(entry.level) < priority(spec.minLevel)) return false;
    if (spec.tag && !entry.tag.toLowerCase().includes(spec.tag.toLowerCase())) return false;
    if (
      spec.text &&
      !entry.message.toLowerCase().includes(spec.text.toLowerCase()) &&
      !entry.tag.toLowerCase().includes(spec.text.toLowerCase())
    ) {
      return false;
    }
    if (
      spec.package &&
      !(entry.package ?? entry.tag).toLowerCase().includes(spec.package.toLowerCase())
    ) {
      return false;
    }
    return true;
  });
}

function installLogcatPanelMocks(entries: ProcessedEntry[]): void {
  let activeFilter = emptyFilter();

  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    switch (command) {
      case "get_logcat_entries":
        return filterEntries(entries, activeFilter);
      case "get_logcat_status":
        return false;
      case "get_logcat_stats":
        return {
          totalIngested: BigInt(entries.length),
          countsByLevel: [0n, 0n, 0n, 0n, 0n, 0n, 0n],
          crashCount: 0n,
          jsonCount: 0n,
          packagesSeen: 1,
          bufferUsagePct: 0,
          bufferEntryCount: BigInt(entries.length),
        } satisfies LogStats;
      case "set_logcat_filter": {
        const payload = args as { filterSpec?: LogcatFilterSpec };
        activeFilter = payload.filterSpec ?? emptyFilter();
        return undefined;
      }
      default:
        return undefined;
    }
  });

  vi.mocked(listen).mockResolvedValue(() => {});
}

describe("LogcatPanel Entry Detail click-to-filter integration", () => {
  beforeEach(() => {
    localStorage.clear();
    replaceLogcatEntries([]);
    setLogcatStreaming(false);
    setLogcatRingBufferTotal(null);
    vi.clearAllMocks();

    if (!window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as typeof ResizeObserver;
    }
  });

  afterEach(() => {
    replaceLogcatEntries([]);
    setLogcatStreaming(false);
    setLogcatRingBufferTotal(null);
  });

  it("adds a clicked Entry Detail metadata value to the visible query bar", async () => {
    installLogcatPanelMocks([BASE_ENTRY]);
    render(() => <LogcatPanel />);

    fireEvent.click(await screen.findByText("Activity started"));
    fireEvent.click(screen.getByTitle("Filter by Tag"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as AND" }));

    expect(await screen.findByText("tag:MainActivity")).not.toBeNull();
    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(1));
  });

  it("keeps quoted message detail filters intact after a QueryBar rebuild", async () => {
    const quotedEntry = {
      ...BASE_ENTRY,
      id: 2n,
      tag: "QuotedTag",
      message: 'hello "quoted" value',
    } satisfies ProcessedEntry;
    installLogcatPanelMocks([quotedEntry]);
    render(() => <LogcatPanel />);

    fireEvent.click(await screen.findByText('hello "quoted" value'));
    fireEvent.click(screen.getByTitle("Filter by message"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as AND" }));

    expect(await screen.findByText('message:hello "quoted" value')).not.toBeNull();

    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "tag:QuotedTag" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(await screen.findByText("tag:QuotedTag")).not.toBeNull();
    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(1));
    expect(screen.getAllByText('hello "quoted" value').length).toBeGreaterThan(0);
  });
});
