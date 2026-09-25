import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Device, LogStats, ProcessedEntry } from "@/bindings";
import { pickDevice, resetDeviceState, setDevices } from "@/stores/device.store";
import {
  replaceLogcatEntries,
  setLogcatRingBufferTotal,
  setLogcatStreaming,
} from "@/stores/logcat.store";
import { LogcatPanel } from "./LogcatPanel";

const OLD_ENTRY = {
  id: 1,
  timestamp: "04-29 13:00:00.000",
  pid: 1234,
  tid: 5678,
  level: "info",
  tag: "MainActivity",
  message: "Entry from before the clear",
  package: "com.example.app",
  kind: "normal",
  isCrash: false,
  flags: 0,
  category: "general",
  crashGroupId: null,
  jsonBody: null,
} satisfies ProcessedEntry;

function device(serial: string): Device {
  return {
    serial,
    name: serial,
    model: null,
    deviceKind: "emulator",
    connectionState: "online",
    apiLevel: null,
    androidVersion: null,
  };
}

interface Deferred {
  resolve: (entries: ProcessedEntry[]) => void;
}

function installMocks(options: { deferMountBackfill?: boolean } = {}) {
  let stored: ProcessedEntry[] = [OLD_ENTRY];
  const pending: Deferred[] = [];
  let deferNext = options.deferMountBackfill ?? false;
  const listeners = new Map<string, (event: { payload: unknown }) => void>();

  const emit = (event: string, payload: unknown = undefined) => listeners.get(event)?.({ payload });

  vi.mocked(invoke).mockImplementation(async (command: string) => {
    switch (command) {
      case "get_logcat_entries":
        if (deferNext) {
          deferNext = false;
          return new Promise<ProcessedEntry[]>((resolve) => pending.push({ resolve }));
        }
        return stored;
      case "get_logcat_status":
        return false;
      case "get_logcat_stats":
        return {
          totalIngested: stored.length,
          countsByLevel: [0, 0, 0, 0, 0, 0, 0],
          crashCount: 0,
          jsonCount: 0,
          packagesSeen: 1,
          bufferUsagePct: 0,
          bufferEntryCount: stored.length,
          droppedLines: 0,
          backlogLines: 0,
        } satisfies LogStats;
      case "clear_logcat":
        stored = [];
        emit("logcat:cleared");
        return undefined;
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
    emit,
    listening: (event: string) => listeners.has(event),
    deferNextBackfill: () => {
      deferNext = true;
    },
    pending,
  };
}

/** Let resolved IPC promises and their continuations run. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function invokeCalls(command: string): unknown[][] {
  return vi.mocked(invoke).mock.calls.filter(([name]) => name === command);
}

describe("LogcatPanel clear and auto-start races", () => {
  beforeEach(() => {
    localStorage.clear();
    replaceLogcatEntries([]);
    setLogcatStreaming(false);
    setLogcatRingBufferTotal(null);
    resetDeviceState();
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
    resetDeviceState();
  });

  async function startFilterBackfill(mocks: ReturnType<typeof installMocks>): Promise<Deferred> {
    mocks.deferNextBackfill();
    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "tag:MainActivity" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(mocks.pending).toHaveLength(1));
    return mocks.pending[0];
  }

  it("does not restore entries from a filter backfill that was in flight when Clear was clicked", async () => {
    const mocks = installMocks();
    render(() => <LogcatPanel />);
    await screen.findByText(OLD_ENTRY.message);

    const backfill = await startFilterBackfill(mocks);
    fireEvent.click(screen.getByTitle("Clear logcat buffer"));
    await waitFor(() => expect(screen.queryByText(OLD_ENTRY.message)).toBeNull());

    backfill.resolve([OLD_ENTRY]);
    await settle();

    expect(screen.queryByText(OLD_ENTRY.message)).toBeNull();
  });

  it("does not restore entries from a filter backfill when a clear arrives from elsewhere", async () => {
    const mocks = installMocks();
    render(() => <LogcatPanel />);
    await screen.findByText(OLD_ENTRY.message);

    const backfill = await startFilterBackfill(mocks);
    // An MCP client cleared the buffer; only the event reaches the panel.
    mocks.emit("logcat:cleared");
    await waitFor(() => expect(screen.queryByText(OLD_ENTRY.message)).toBeNull());

    backfill.resolve([OLD_ENTRY]);
    await settle();

    expect(screen.queryByText(OLD_ENTRY.message)).toBeNull();
  });

  it("does not restore entries from the mount backfill when a clear lands during it", async () => {
    const mocks = installMocks({ deferMountBackfill: true });
    render(() => <LogcatPanel />);
    await waitFor(() => expect(mocks.pending).toHaveLength(1));

    mocks.emit("logcat:cleared");
    mocks.pending[0].resolve([OLD_ENTRY]);
    await waitFor(() => expect(invokeCalls("get_logcat_status")).toHaveLength(1));
    await settle();

    expect(screen.queryByText(OLD_ENTRY.message)).toBeNull();
  });

  it("auto-starts logcat on the selected device, not the first online one", async () => {
    const mocks = installMocks();
    setDevices([device("emulator-5554"), device("emulator-5556")]);
    await pickDevice("emulator-5556");
    render(() => <LogcatPanel />);
    await waitFor(() => expect(mocks.listening("device:list_changed")).toBe(true));

    mocks.emit("device:list_changed", {
      devices: [device("emulator-5554"), device("emulator-5556")],
    });

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("start_logcat", { deviceSerial: "emulator-5556" })
    );
    expect(invokeCalls("start_logcat")).toHaveLength(1);
  });
});
