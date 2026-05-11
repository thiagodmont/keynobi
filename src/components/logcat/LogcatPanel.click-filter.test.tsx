import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LogcatFilterSpec, LogStats, ProcessedEntry } from "@/bindings";
import { addSavedFilter } from "@/lib/logcat-filter-storage";
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

function contextEntries(entries: ProcessedEntry[], args: unknown): ProcessedEntry[] {
  const opts = (args ?? {}) as {
    anchorId?: bigint | number | string | null;
    direction?: string | null;
    count?: number | null;
  };
  if (opts.anchorId === null || opts.anchorId === undefined) return [];
  const anchorId = BigInt(opts.anchorId);
  const anchorIndex = entries.findIndex((entry) => entry.id === anchorId);
  if (anchorIndex < 0) return [];
  const count = Math.max(0, Math.floor(opts.count ?? 10));
  if (opts.direction === "before") {
    return entries.slice(Math.max(0, anchorIndex - count), anchorIndex);
  }
  if (opts.direction === "after") {
    return entries.slice(anchorIndex + 1, anchorIndex + 1 + count);
  }
  return [];
}

function installLogcatPanelMocks(entries: ProcessedEntry[]): {
  emitLogcatEntries: (entries: ProcessedEntry[]) => void;
  setFilterCalls: () => LogcatFilterSpec[];
} {
  let activeFilter = emptyFilter();
  const setFilterCalls: LogcatFilterSpec[] = [];
  const listeners = new Map<string, (event: { payload: unknown }) => void>();

  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    switch (command) {
      case "get_logcat_entries":
        return filterEntries(entries, activeFilter);
      case "get_logcat_context_entries":
        return contextEntries(entries, args);
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
        setFilterCalls.push(activeFilter);
        return undefined;
      }
      default:
        return undefined;
    }
  });

  vi.mocked(listen).mockImplementation(async (event, callback) => {
    listeners.set(String(event), callback as (event: { payload: unknown }) => void);
    return () => {
      listeners.delete(String(event));
    };
  });

  return {
    emitLogcatEntries(nextEntries: ProcessedEntry[]) {
      listeners.get("logcat:entries")?.({ payload: filterEntries(nextEntries, activeFilter) });
    },
    setFilterCalls() {
      return [...setFilterCalls];
    },
  };
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
    vi.useRealTimers();
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

  it("opens detail for the clicked row after filtering changes visible indices", async () => {
    const alphaEntry = {
      ...BASE_ENTRY,
      id: 10n,
      tag: "AlphaTag",
      message: "Alpha unfiltered message",
    } satisfies ProcessedEntry;
    const betaEntry = {
      ...BASE_ENTRY,
      id: 11n,
      tag: "BetaTag",
      message: "Beta target message",
    } satisfies ProcessedEntry;
    const gammaEntry = {
      ...BASE_ENTRY,
      id: 12n,
      tag: "GammaTag",
      message: "Gamma target message",
    } satisfies ProcessedEntry;
    installLogcatPanelMocks([alphaEntry, betaEntry, gammaEntry]);
    render(() => <LogcatPanel />);

    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(3));

    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "message:target" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(2));
    fireEvent.click(screen.getByText("Beta target message"));

    expect(screen.getByTitle("Filter by message").textContent).toBe("Beta target message");
  });

  it("freezes the visible log list after selecting a row until jumping to the end", async () => {
    const selectedEntry = {
      ...BASE_ENTRY,
      id: 20n,
      tag: "SelectedTag",
      message: "Selected row should stay visible",
    } satisfies ProcessedEntry;
    const incomingEntry = {
      ...BASE_ENTRY,
      id: 21n,
      tag: "IncomingTag",
      message: "Incoming row should wait",
    } satisfies ProcessedEntry;
    const { emitLogcatEntries } = installLogcatPanelMocks([selectedEntry]);
    render(() => <LogcatPanel />);

    await waitFor(() =>
      expect(vi.mocked(listen)).toHaveBeenCalledWith("logcat:entries", expect.any(Function))
    );
    fireEvent.click(await screen.findByText("Selected row should stay visible"));

    emitLogcatEntries([incomingEntry]);

    const virtualList = screen.getByTestId("logcat-virtual-list");
    expect(virtualList.textContent).toContain("Selected row should stay visible");
    expect(virtualList.textContent).not.toContain("Incoming row should wait");
    expect(screen.queryByText("Incoming row should wait")).toBeNull();
    expect(await screen.findByText("1 new")).not.toBeNull();

    fireEvent.click(screen.getByTitle("1 new log available - Jump to end"));

    expect(await screen.findByText("Incoming row should wait")).not.toBeNull();
  });

  it("hides and restores buffered lifecycle and process entries from the quick filter", async () => {
    const appEntry = {
      ...BASE_ENTRY,
      id: 30n,
      tag: "App",
      message: "visible app row",
      category: "general",
      kind: "normal",
    } satisfies ProcessedEntry;
    const lifecycleEntry = {
      ...BASE_ENTRY,
      id: 31n,
      tag: "ActivityManager",
      message: "lifecycle row",
      category: "lifecycle",
      kind: "normal",
    } satisfies ProcessedEntry;
    const processEntry = {
      ...BASE_ENTRY,
      id: 32n,
      tag: "---",
      message: "com.example.app process died",
      category: "lifecycle",
      kind: "processDied",
    } satisfies ProcessedEntry;

    installLogcatPanelMocks([appEntry, lifecycleEntry, processEntry]);
    render(() => <LogcatPanel />);

    expect(await screen.findByText("visible app row")).not.toBeNull();
    expect(screen.getByText("lifecycle row")).not.toBeNull();
    expect(screen.getByText(/PROCESS DIED/)).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Lifecycle" }));

    expect(screen.getByText("visible app row")).not.toBeNull();
    expect(screen.queryByText("lifecycle row")).toBeNull();
    expect(screen.queryByText(/PROCESS DIED/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Lifecycle" }));

    expect(screen.getByText("lifecycle row")).not.toBeNull();
    expect(screen.getByText(/PROCESS DIED/)).not.toBeNull();
  });

  it("excludes hidden lifecycle crash rows from the read-mode crash count", async () => {
    const appCrashEntry = {
      ...BASE_ENTRY,
      id: 40n,
      tag: "AppCrash",
      message: "visible app crash",
      category: "general",
      kind: "normal",
      isCrash: true,
    } satisfies ProcessedEntry;
    const lifecycleCrashEntry = {
      ...BASE_ENTRY,
      id: 41n,
      tag: "ActivityManager",
      message: "hidden lifecycle crash",
      category: "lifecycle",
      kind: "normal",
      isCrash: true,
    } satisfies ProcessedEntry;

    installLogcatPanelMocks([appCrashEntry, lifecycleCrashEntry]);
    render(() => <LogcatPanel />);

    fireEvent.click(await screen.findByText("visible app crash"));
    expect(screen.getByTitle("2 crashes — click to jump")).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Lifecycle" }));

    expect(screen.getAllByText("visible app crash").length).toBeGreaterThan(0);
    expect(screen.queryByText("hidden lifecycle crash")).toBeNull();
    expect(screen.getByTitle("1 crash — click to jump")).not.toBeNull();
  });

  it("clears disabled pill state when applying a saved filter", async () => {
    addSavedFilter("Beta only", "tag:Beta");
    installLogcatPanelMocks([
      {
        ...BASE_ENTRY,
        id: 50n,
        tag: "AlphaOnly",
        message: "Alpha only saved-filter row",
      },
      {
        ...BASE_ENTRY,
        id: 51n,
        tag: "AlphaBeta",
        message: "Alpha beta saved-filter row",
      },
      {
        ...BASE_ENTRY,
        id: 52n,
        tag: "BetaOnly",
        message: "Beta only saved-filter row",
      },
    ]);
    render(() => <LogcatPanel />);

    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(3));

    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "tag:Alpha tag:Beta " } });

    await waitFor(() => expect(screen.getByText("Alpha beta saved-filter row")).not.toBeNull());
    await waitFor(() => expect(screen.queryByText("Alpha only saved-filter row")).toBeNull());

    fireEvent.click(screen.getByRole("button", { name: "Disable filter tag:Beta" }));

    await waitFor(() => expect(screen.getByText("Alpha only saved-filter row")).not.toBeNull());

    fireEvent.click(screen.getByTitle("Filter presets"));
    fireEvent.click(screen.getByText("Beta only"));

    await waitFor(() => expect(screen.getByText("Beta only saved-filter row")).not.toBeNull());
    expect(screen.getByText("Alpha beta saved-filter row")).not.toBeNull();
    expect(screen.queryByText("Alpha only saved-filter row")).toBeNull();
  });

  it("expands unfiltered context above a filtered row from the row context menu", async () => {
    const beforeEntry = {
      ...BASE_ENTRY,
      id: 60n,
      tag: "Before",
      message: "Raw row before target",
    } satisfies ProcessedEntry;
    const targetEntry = {
      ...BASE_ENTRY,
      id: 61n,
      tag: "Target",
      message: "Filtered target row",
    } satisfies ProcessedEntry;
    const afterEntry = {
      ...BASE_ENTRY,
      id: 62n,
      tag: "After",
      message: "Raw row after target",
    } satisfies ProcessedEntry;
    installLogcatPanelMocks([beforeEntry, targetEntry, afterEntry]);
    render(() => <LogcatPanel />);

    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "message:Filtered" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(1));
    expect(screen.getByText("Filtered target row")).not.toBeNull();
    expect(screen.queryByText("Raw row before target")).toBeNull();

    fireEvent.contextMenu(screen.getByText("Filtered target row"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Expand 10 up" }));

    expect(await screen.findByText("Raw row before target")).not.toBeNull();
    expect(screen.queryByText("Raw row after target")).toBeNull();
    const row = screen.getByText("Raw row before target").closest(`div[title="${ROW_TITLE}"]`);
    expect(row?.getAttribute("data-expanded-context")).toBe("true");
  });

  it("debounces backend filter sync while editing variable values", async () => {
    vi.useFakeTimers();
    const mocks = installLogcatPanelMocks([BASE_ENTRY]);
    render(() => <LogcatPanel />);

    await vi.runOnlyPendingTimersAsync();
    await waitFor(() => expect(screen.getAllByTitle(ROW_TITLE)).toHaveLength(1));

    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "message:action_${action_name}_done " } });
    await vi.advanceTimersByTimeAsync(150);
    await Promise.resolve();

    const afterQueryCommit = mocks.setFilterCalls().length;
    const variableInput = screen.getByTitle("Variable action_name");

    fireEvent.input(variableInput, { target: { value: "c" } });
    fireEvent.input(variableInput, { target: { value: "ch" } });
    fireEvent.input(variableInput, { target: { value: "checkout" } });
    await Promise.resolve();

    expect(mocks.setFilterCalls()).toHaveLength(afterQueryCommit);

    await vi.advanceTimersByTimeAsync(149);
    await Promise.resolve();

    expect(mocks.setFilterCalls()).toHaveLength(afterQueryCommit);

    await vi.advanceTimersByTimeAsync(1);
    await Promise.resolve();

    expect(mocks.setFilterCalls()).toHaveLength(afterQueryCommit + 1);
  });
});
