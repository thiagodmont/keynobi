import { describe, it, expect, beforeEach, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import {
  appMemoryBytes,
  logFolderBytes,
  rotationTriggered,
  initMonitorListeners,
  resetMonitorListenersForTests,
} from "@/stores/monitor.store";

const mockListen = vi.mocked(listen);

describe("monitor.store", () => {
  beforeEach(() => {
    resetMonitorListenersForTests();
    vi.clearAllMocks();
    mockListen.mockResolvedValue(() => {});
  });

  it("registers the stats listener only once", async () => {
    await initMonitorListeners();
    await initMonitorListeners();

    expect(mockListen).toHaveBeenCalledTimes(1);
    expect(mockListen.mock.calls[0][0]).toBe("monitor://stats");
  });

  it("updates all three signals from a stats payload", async () => {
    let emit: ((e: { payload: unknown }) => void) | undefined;
    mockListen.mockImplementation((_name, cb) => {
      emit = cb as (e: { payload: unknown }) => void;
      return Promise.resolve(() => {});
    });

    await initMonitorListeners();
    emit?.({
      payload: { appMemoryBytes: 1024, logFolderBytes: 2048, rotationTriggered: true },
    });

    expect(appMemoryBytes()).toBe(1024);
    expect(logFolderBytes()).toBe(2048);
    expect(rotationTriggered()).toBe(true);
  });

  it("disposes the listener on reset", async () => {
    const unlisten = vi.fn();
    mockListen.mockResolvedValue(unlisten);

    await initMonitorListeners();
    resetMonitorListenersForTests();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("survives a failed registration and allows a retry", async () => {
    mockListen.mockRejectedValueOnce(new Error("ipc down"));
    await initMonitorListeners();

    mockListen.mockResolvedValue(() => {});
    await initMonitorListeners();

    expect(mockListen).toHaveBeenCalledTimes(2);
  });
});
