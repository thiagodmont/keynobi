import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as tauriApi from "@/lib/tauri-api";
import { updateSetting, flushPendingSettingsSave } from "@/stores/settings.store";

// Suite lives in its own file because the store's debounce timer and
// in-flight save are module-level state: sharing a file with suites that use
// real timers lets their stray saves leak into these fake-timer tests.
describe("flushPendingSettingsSave", () => {
  beforeEach(async () => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
    // Drain any debounced save left pending so each test starts from an
    // idle timer regardless of execution order.
    await flushPendingSettingsSave();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("is a no-op when no save is pending", async () => {
    const saveSpy = vi.spyOn(tauriApi, "saveSettings").mockResolvedValue(undefined);

    await flushPendingSettingsSave();
    expect(saveSpy).not.toHaveBeenCalled();
  });

  it("persists settings when the debounce window elapses", async () => {
    const saveSpy = vi.spyOn(tauriApi, "saveSettings").mockResolvedValue(undefined);

    updateSetting("appearance", "uiFontSize", 18);
    expect(saveSpy).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(500);
    expect(saveSpy).toHaveBeenCalledTimes(1);
    expect(saveSpy.mock.calls[0]?.[0].appearance.uiFontSize).toBe(18);

    // Restore for subsequent tests.
    updateSetting("appearance", "uiFontSize", 12);
    await flushPendingSettingsSave();
  });

  it("saves immediately instead of waiting for the debounce window", async () => {
    const saveSpy = vi.spyOn(tauriApi, "saveSettings").mockResolvedValue(undefined);

    updateSetting("appearance", "uiFontSize", 16);
    await flushPendingSettingsSave();

    expect(saveSpy).toHaveBeenCalledTimes(1);

    // Restore state and drain the (now-cancelled) debounce timer.
    updateSetting("appearance", "uiFontSize", 12);
    await flushPendingSettingsSave();
    expect(saveSpy).toHaveBeenCalledTimes(2);
  });

  it("awaits an in-flight debounce save instead of returning early", async () => {
    let resolveSave!: () => void;
    // Only the FIRST call hangs on a manually-resolved promise; later calls
    // (e.g. the trailing restore flush) resolve immediately.
    const saveSpy = vi
      .spyOn(tauriApi, "saveSettings")
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveSave = resolve;
          })
      )
      .mockResolvedValue(undefined);

    // The debounce timer fires and puts a save in flight.
    updateSetting("appearance", "uiFontSize", 17);
    await vi.advanceTimersByTimeAsync(500);
    expect(saveSpy).toHaveBeenCalledTimes(1);

    // A close-time flush must NOT complete while that save is unresolved —
    // otherwise shutdown could acknowledge Rust before the write lands.
    let flushed = false;
    const flushPromise = flushPendingSettingsSave().then(() => {
      flushed = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(flushed).toBe(false);

    resolveSave();
    await flushPromise;
    expect(flushed).toBe(true);

    // Restore for subsequent tests.
    updateSetting("appearance", "uiFontSize", 12);
    await flushPendingSettingsSave();
  });

  it("persists changes made while an earlier save is still in flight", async () => {
    let resolveFirstSave!: () => void;
    const saveSpy = vi
      .spyOn(tauriApi, "saveSettings")
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveFirstSave = resolve;
          })
      )
      .mockResolvedValue(undefined);

    // First change: timer fires, save #1 in flight.
    updateSetting("appearance", "uiFontSize", 17);
    await vi.advanceTimersByTimeAsync(500);

    // Second change arrives before save #1 finished; close-time flush must
    // persist it too instead of dropping it with the cancelled timer.
    updateSetting("appearance", "uiFontSize", 19);
    const flushPromise = flushPendingSettingsSave();

    resolveFirstSave();
    await flushPromise;

    expect(saveSpy).toHaveBeenCalledTimes(2);
    expect(saveSpy.mock.calls[1]?.[0].appearance.uiFontSize).toBe(19);

    // Restore for subsequent tests.
    updateSetting("appearance", "uiFontSize", 12);
    await flushPendingSettingsSave();
  });

  it("serializes overlapping saves so an older snapshot cannot land last", async () => {
    let resolveFirst!: () => void;
    const saveSpy = vi
      .spyOn(tauriApi, "saveSettings")
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveFirst = resolve;
          })
      )
      .mockResolvedValue(undefined);

    // Save #1 in flight (debounce fired).
    updateSetting("appearance", "uiFontSize", 17);
    await vi.advanceTimersByTimeAsync(500);

    // A second debounce fires while #1 is still in flight: it must queue
    // behind #1, not start concurrently.
    updateSetting("appearance", "uiFontSize", 19);
    await vi.advanceTimersByTimeAsync(500);
    expect(saveSpy).toHaveBeenCalledTimes(1);

    // Completing #1 lets the queued save run with the newest state.
    resolveFirst();
    await flushPendingSettingsSave();
    expect(saveSpy).toHaveBeenCalledTimes(2);
    expect(saveSpy.mock.calls[1]?.[0].appearance.uiFontSize).toBe(19);

    // Restore for subsequent tests.
    updateSetting("appearance", "uiFontSize", 12);
    await flushPendingSettingsSave();
  });

  it("propagates save failures through the toast path without throwing", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.spyOn(tauriApi, "saveSettings").mockRejectedValue(new Error("disk full"));

    updateSetting("appearance", "uiFontSize", 16);
    await expect(flushPendingSettingsSave()).resolves.toBeUndefined();

    updateSetting("appearance", "uiFontSize", 12);
    await flushPendingSettingsSave();
    expect(errorSpy).toHaveBeenCalled();
  });
});
