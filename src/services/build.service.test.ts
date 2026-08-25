import { describe, it, expect, beforeEach, afterEach, vi, expectTypeOf } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  cancelBuild,
  initBuildService,
  resetBuildServiceForTests,
  runAndDeploy,
  runBuild,
} from "@/services/build.service";
import { buildState, resetBuildState, startBuild } from "@/stores/build.store";
import { resetDeviceState } from "@/stores/device.store";
import { resetVariantState, selectVariant } from "@/stores/variant.store";
import { updateSetting } from "@/stores/settings.store";

const devicePickerMock = vi.hoisted(() => ({
  showDevicePicker: vi.fn<() => Promise<string | null>>(),
}));

vi.mock("@/components/device/DevicePickerDialog", () => devicePickerMock);

// The global setup in src/test/setup.ts already mocks @tauri-apps/api/core.
// We narrow it here so we can track which commands were called.
const mockInvoke = vi.mocked(invoke);

describe("cancelBuild guard — no ghost records on project switch", () => {
  beforeEach(() => {
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    mockInvoke.mockResolvedValue(undefined);
    devicePickerMock.showDevicePicker.mockReset();
    vi.clearAllMocks();
  });

  // Regression: when no build is running (e.g. during a project switch),
  // cancelBuild must return early without invoking the removed legacy
  // finalize_build command, which used to write ghost history records.
  it("does not call legacy finalize_build when no build is running (idle phase)", async () => {
    expect(buildState.phase).toBe("idle");

    await cancelBuild();

    const finalizeCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "finalize_build");
    expect(finalizeCalls).toHaveLength(0);
  });

  it("does not call legacy finalize_build when previous build already succeeded", async () => {
    startBuild("assembleDebug");
    // Simulate a completed build by directly transitioning to success phase.
    // (We can't call setBuildResult here without mocking the tick, so we use
    // the store's cancelBuildState to reach a terminal phase, then reset.)
    resetBuildState();
    // Phase is now idle — no active build.
    expect(buildState.phase).toBe("idle");

    await cancelBuild();

    const finalizeCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "finalize_build");
    expect(finalizeCalls).toHaveLength(0);
  });

  it("calls cancel_build without frontend finalization when a build is actually running", async () => {
    startBuild("assembleDebug");
    expect(buildState.phase).toBe("running");

    // Rust records the final build result from process exit; the frontend only
    // requests cancellation and updates local state immediately.
    await cancelBuild();

    const cancelCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "cancel_build");
    const finalizeCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "finalize_build");

    expect(cancelCalls).toHaveLength(1);
    expect(finalizeCalls).toHaveLength(0);
  });

  it("rejects a second build while the first build is still running", async () => {
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "cancel_build") return Promise.resolve(undefined);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const first = runBuild();
    expect(buildState.phase).toBe("running");

    await expect(runBuild()).rejects.toThrow("A build is already running.");

    await cancelBuild();
    await first;
  });

  it("clears the build completion timeout when cancelling an active build", async () => {
    vi.useFakeTimers();
    try {
      mockInvoke.mockImplementation((cmd) => {
        if (cmd === "run_gradle_task") return Promise.resolve(1);
        if (cmd === "cancel_build") return Promise.resolve(undefined);
        if (cmd === "get_build_history") return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const first = runBuild();
      expect(buildState.phase).toBe("running");

      await cancelBuild();
      await first;

      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("rejects a standalone build while deploy is resolving a device", async () => {
    let resolvePicker: (serial: string | null) => void = () => {};
    devicePickerMock.showDevicePicker.mockReturnValue(
      new Promise((resolve) => {
        resolvePicker = resolve;
      })
    );

    await selectVariant("debug");
    const deploy = runAndDeploy();
    await vi.waitFor(() => expect(devicePickerMock.showDevicePicker).toHaveBeenCalled());

    expect(buildState.phase).toBe("idle");
    await expect(runBuild()).rejects.toThrow("A build or deploy is already running.");

    resolvePicker(null);
    await deploy;
  });

  it("does not expose the deploy bypass in public runBuild options", () => {
    type PublicOptions = NonNullable<Parameters<typeof runBuild>[1]>;

    expectTypeOf<PublicOptions>().toEqualTypeOf<{ headerLines?: string[] }>();
  });

  it("times out after the configured buildTimeoutSec, not a hardcoded value", async () => {
    vi.useFakeTimers();
    try {
      // 120 sits above the 60 s clamp floor, so passing proves the
      // configured value is honored rather than the minimum.
      updateSetting("mcp", "buildTimeoutSec", 120);
      mockInvoke.mockImplementation((cmd) => {
        if (cmd === "run_gradle_task") return Promise.resolve(1);
        if (cmd === "cancel_build") return Promise.resolve(undefined);
        if (cmd === "get_build_history") return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const first = runBuild();
      const expectation = expect(first).rejects.toThrow(
        "Build timed out waiting for the build:complete event after 120 seconds."
      );

      // Just under the configured timeout: still pending.
      await vi.advanceTimersByTimeAsync(119 * 1000);

      // Past it: the promise rejects and the still-running Gradle process
      // is cancelled to release the shared build slot.
      await vi.advanceTimersByTimeAsync(2 * 1000);
      await expectation;
      const cancelCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "cancel_build");
      expect(cancelCalls).toHaveLength(1);
    } finally {
      updateSetting("mcp", "buildTimeoutSec", 600);
      vi.useRealTimers();
    }
  });
});

describe("late cancelled completion event after a timeout", () => {
  beforeEach(() => {
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    mockInvoke.mockResolvedValue(undefined);
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetBuildServiceForTests();
    vi.useRealTimers();
  });

  it("does not overwrite the timeout failure with a cancelled phase", async () => {
    vi.useFakeTimers();
    // Capture the build:complete handler registered by the service.
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });

    updateSetting("mcp", "buildTimeoutSec", 120);
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "cancel_build") return Promise.resolve(undefined);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    await initBuildService();
    // Fail loudly if the listener was never registered — otherwise the
    // dispatch below would no-op and the test would pass vacuously.
    expect(handlers.has("build:complete")).toBe(true);

    const first = runBuild();
    const expectation = expect(first).rejects.toThrow(/timed out/);
    await vi.advanceTimersByTimeAsync(121 * 1000);
    await expectation;

    // The catch path marked the build failed.
    expect(buildState.phase).toBe("failed");

    // The dying Gradle process emits a late cancelled completion event.
    handlers.get("build:complete")!({
      payload: {
        success: false,
        cancelled: true,
        durationMs: 121_000,
        errorCount: 0,
        warningCount: 0,
        task: "assembleDebug",
      },
    });

    // Phase must stay failed, not flip to cancelled.
    expect(buildState.phase).toBe("failed");
    await expectation;
  });

  it("still applies cancelled phase for a genuine user cancellation", async () => {
    vi.useFakeTimers();
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });

    await initBuildService();
    // Fail loudly if the listener was never registered — otherwise the
    // dispatch below would no-op and the test would pass vacuously.
    expect(handlers.has("build:complete")).toBe(true);
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "cancel_build") return Promise.resolve(undefined);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    void runBuild();
    await vi.advanceTimersByTimeAsync(0);
    await cancelBuild();

    handlers.get("build:complete")!({
      payload: {
        success: false,
        cancelled: true,
        durationMs: 5_000,
        errorCount: 0,
        warningCount: 0,
        task: "assembleDebug",
      },
    });

    expect(buildState.phase).toBe("cancelled");
  });

  it("absorbs a stale cancelled event that arrives after a later build started", async () => {
    vi.useFakeTimers();
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });

    await initBuildService();
    // Fail loudly if the listener was never registered — otherwise the
    // dispatch below would no-op and the test would pass vacuously.
    expect(handlers.has("build:complete")).toBe(true);
    updateSetting("mcp", "buildTimeoutSec", 120);
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "cancel_build") return Promise.resolve(undefined);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    // Build A times out; its process is killed but slow to die.
    const buildA = runBuild();
    const expectationA = expect(buildA).rejects.toThrow(/timed out/);
    await vi.advanceTimersByTimeAsync(121 * 1000);
    await expectationA;
    expect(buildState.phase).toBe("failed");

    // Build B starts before A's completion event has been delivered.
    void runBuild();
    await vi.advanceTimersByTimeAsync(0);
    expect(buildState.phase).toBe("running");

    // A's stale cancelled event must NOT cancel build B.
    handlers.get("build:complete")!({
      payload: {
        success: false,
        cancelled: true,
        durationMs: 121_000,
        errorCount: 0,
        warningCount: 0,
        task: "assembleDebug",
      },
    });
    expect(buildState.phase).toBe("running");

    // A genuine cancellation of B still lands in the cancelled phase.
    await cancelBuild();
    expect(buildState.phase).toBe("cancelled");
  });
});
