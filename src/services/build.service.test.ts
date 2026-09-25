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
import {
  buildLogStore,
  buildState,
  flushPendingLines,
  resetBuildState,
  startBuild,
} from "@/stores/build.store";
import { resetDeviceState } from "@/stores/device.store";
import { resetVariantState, selectVariant } from "@/stores/variant.store";
import { updateSetting } from "@/stores/settings.store";
import { beginProjectOpen, setApplicationId } from "@/stores/project.store";
import { makeLaunchTiming } from "@/test/factories/build";

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
        runId: 1,
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
        runId: 1,
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

  it("ignores a late completion from a timed-out run after a later build started", async () => {
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
    let nextRunId = 1;
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(nextRunId++);
      if (cmd === "cancel_build") return Promise.resolve(undefined);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    // Build A (run 1) times out; its process is killed but slow to die.
    const buildA = runBuild();
    const expectationA = expect(buildA).rejects.toThrow(/timed out/);
    await vi.advanceTimersByTimeAsync(121 * 1000);
    await expectationA;
    expect(buildState.phase).toBe("failed");

    // Build B (run 2) starts before A's completion event has been delivered.
    const buildB = runBuild();
    await vi.advanceTimersByTimeAsync(0);
    expect(buildState.phase).toBe("running");

    // A's late events, cancelled or not, must not finish build B.
    for (const cancelled of [true, false]) {
      handlers.get("build:complete")!({
        payload: {
          runId: 1,
          success: !cancelled,
          cancelled,
          durationMs: 121_000,
          errorCount: 0,
          warningCount: 0,
          task: "assembleDebug",
        },
      });
    }
    expect(buildState.phase).toBe("running");

    handlers.get("build:complete")!({
      payload: {
        runId: 2,
        success: true,
        cancelled: false,
        durationMs: 3_000,
        errorCount: 0,
        warningCount: 0,
        task: "assembleDebug",
      },
    });
    await buildB;
    expect(buildState.phase).toBe("success");
  });

  it("applies a completion that arrives before run_gradle_task returns its run ID", async () => {
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });
    await initBuildService();
    expect(handlers.has("build:complete")).toBe(true);

    let returnRunId: (id: number) => void = () => {};
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") {
        return new Promise<number>((resolve) => {
          returnRunId = resolve;
        });
      }
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const build = runBuild();
    expect(buildState.phase).toBe("running");

    // A build that fails fast can finish before the spawn call returns.
    handlers.get("build:complete")!({
      payload: {
        runId: 7,
        success: false,
        cancelled: false,
        durationMs: 200,
        errorCount: 1,
        warningCount: 0,
        task: "assembleDebug",
      },
    });
    expect(buildState.phase).toBe("running");

    returnRunId(7);
    await build;
    expect(buildState.phase).toBe("failed");
  });

  it("unblocks the running build when the cancel request fails", async () => {
    vi.useFakeTimers();
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "cancel_build") return Promise.reject("backend unavailable");
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    let settled = false;
    const build = runBuild().finally(() => {
      settled = true;
    });
    await vi.advanceTimersByTimeAsync(0);

    await expect(cancelBuild()).rejects.toBe("backend unavailable");
    await vi.advanceTimersByTimeAsync(0);

    // Resolved by the cancel, not left waiting for a timer that was cleared.
    expect(settled).toBe(true);
    await build;
    expect(buildState.phase).toBe("cancelled");
  });
});

describe("runAndDeploy honors the autoInstallOnBuild setting", () => {
  /** The event listeners the current deploy registered. */
  let deployHandlers = new Map<string, (e: { payload: unknown }) => void>();

  beforeEach(() => {
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    mockInvoke.mockResolvedValue(undefined);
    devicePickerMock.showDevicePicker.mockReset();
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetBuildServiceForTests();
    updateSetting("build", "autoInstallOnBuild", true);
    vi.useRealTimers();
  });

  /**
   * Start a deploy and complete its build phase with a success event.
   * `overrides` replace individual IPC replies. Resolves to the error the
   * deploy failed with, or null.
   */
  async function deployThroughSuccessfulBuild(
    overrides: Record<string, () => Promise<unknown>> = {},
    completion: Record<string, unknown> = {}
  ): Promise<unknown> {
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    deployHandlers = handlers;
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });

    await initBuildService();
    // Fail loudly if the listener was never registered — otherwise the
    // dispatch below would no-op and the test would pass vacuously.
    expect(handlers.has("build:complete")).toBe(true);

    mockInvoke.mockImplementation((cmd) => {
      if (overrides[cmd]) return overrides[cmd]();
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "get_build_history") return Promise.resolve([]);
      if (cmd === "find_apk_path") return Promise.resolve("/tmp/app-debug.apk");
      if (cmd === "get_package_name_from_apk") return Promise.resolve("com.example.app");
      if (cmd === "install_apk_on_device") return Promise.resolve("Success");
      if (cmd === "launch_app_on_device") {
        return Promise.resolve({ output: "Status: ok", timing: makeLaunchTiming() });
      }
      return Promise.resolve(undefined);
    });

    devicePickerMock.showDevicePicker.mockResolvedValue("emulator-5554");
    await selectVariant("debug");

    const deploy = runAndDeploy();
    await vi.waitFor(() => expect(buildState.phase).toBe("running"));

    handlers.get("build:complete")!({
      payload: {
        runId: 1,
        recordId: 7,
        success: true,
        cancelled: false,
        durationMs: 1_000,
        errorCount: 0,
        warningCount: 0,
        task: "assembleDebug",
        ...completion,
      },
    });
    return deploy.then(
      () => null,
      (e: unknown) => e
    );
  }

  it("skips device resolution, install, and launch when autoInstallOnBuild is off", async () => {
    updateSetting("build", "autoInstallOnBuild", false);
    await deployThroughSuccessfulBuild();

    expect(buildState.phase).toBe("success");
    // Build-only run: the device picker must not even open.
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "find_apk_path")).toHaveLength(0);
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "install_apk_on_device")).toHaveLength(
      0
    );
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "launch_app_on_device")).toHaveLength(0);
  });

  it("still installs and launches when autoInstallOnBuild is on (default)", async () => {
    updateSetting("build", "autoInstallOnBuild", true);
    await deployThroughSuccessfulBuild();

    expect(buildState.phase).toBe("success");
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "install_apk_on_device")).toEqual([
      ["install_apk_on_device", { serial: "emulator-5554", apkPath: "/tmp/app-debug.apk" }],
    ]);
    expect(
      mockInvoke.mock.calls.filter(([cmd]) => cmd === "launch_app_on_device")[0]?.[1]
    ).toMatchObject({ serial: "emulator-5554", package: "com.example.app" });
  });

  function launchCalls() {
    return mockInvoke.mock.calls.filter(([cmd]) => cmd === "launch_app_on_device");
  }

  function buildLog(): string[] {
    flushPendingLines();
    return buildLogStore.entries.map((entry) => entry.message);
  }

  it("records the launch time on the build this deploy ran", async () => {
    const error = await deployThroughSuccessfulBuild();

    expect(error).toBeNull();
    expect(launchCalls()[0]?.[1]).toMatchObject({ buildId: 7 });
    expect(buildLog()).toContain("▶ Launch time: 812 ms (cold)");
  });

  it("names its own build even when another build finished before the launch", async () => {
    const error = await deployThroughSuccessfulBuild({
      install_apk_on_device: () => {
        // An agent's build finishes (and is recorded) while the APK installs.
        deployHandlers.get("build:complete")!({
          payload: {
            runId: 2,
            recordId: 8,
            success: true,
            cancelled: false,
            durationMs: 900,
            errorCount: 0,
            warningCount: 0,
            task: "assembleRelease",
            origin: { kind: "agent", sessionId: 1, clientName: "Codex", standalone: false },
            cancelledBy: null,
          },
        });
        return Promise.resolve("Success");
      },
    });

    expect(error).toBeNull();
    expect(launchCalls()).toHaveLength(1);
    expect(launchCalls()[0]?.[1]).toMatchObject({ buildId: 7 });
  });

  it("does not launch, so records no launch time, after a failed build", async () => {
    await deployThroughSuccessfulBuild({}, { success: false, errorCount: 1 });

    expect(buildState.phase).toBe("failed");
    expect(launchCalls()).toHaveLength(0);
  });

  it("does not launch, so records no launch time, after a cancelled build", async () => {
    await deployThroughSuccessfulBuild({}, { success: false, cancelled: true });

    expect(buildState.phase).toBe("cancelled");
    expect(launchCalls()).toHaveLength(0);
  });

  it("says when the launch method reported no launch time", async () => {
    const error = await deployThroughSuccessfulBuild({
      launch_app_on_device: () =>
        Promise.resolve({ output: "monkey OK: Events injected: 1", timing: null }),
    });

    expect(error).toBeNull();
    expect(buildLog()).toContain("▶ Launch time: not reported by this launch method");
  });

  it("stops before installing when the variant has no APK, with the backend's reason", async () => {
    const reason =
      "No APK for variant 'debug'. Found outputs for: freerelease. Build that variant first.";
    const error = await deployThroughSuccessfulBuild({
      find_apk_path: () => Promise.reject(reason),
    });

    expect(String(error)).toContain("Found outputs for: freerelease");
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "install_apk_on_device")).toHaveLength(
      0
    );
  });

  it("installs but does not launch a guessed package when the APK's package is unknown", async () => {
    // The base applicationId ignores applicationIdSuffix, so launching it
    // could start a different app than the one just installed.
    setApplicationId("com.example.app");
    const error = await deployThroughSuccessfulBuild({
      get_package_name_from_apk: () => Promise.reject("aapt2 not found"),
    });

    expect(error).toBeNull();
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "install_apk_on_device")).toHaveLength(
      1
    );
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "launch_app_on_device")).toHaveLength(0);
    setApplicationId(null);
  });
  it("stops before installing when the project changes while the APK is looked up", async () => {
    const error = await deployThroughSuccessfulBuild({
      find_apk_path: () => {
        // The user opens another project; the backend now answers for it.
        beginProjectOpen();
        return Promise.resolve("/other-project/app/build/outputs/apk/debug/app-debug.apk");
      },
    });

    expect(String(error)).toContain("The project changed during deploy");
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "install_apk_on_device")).toHaveLength(
      0
    );
  });
});

describe("builds this window did not start", () => {
  const claude = {
    kind: "agent" as const,
    sessionId: 3,
    clientName: "Claude Code",
    standalone: false,
  };
  let handlers: Map<string, (e: { payload: unknown }) => void>;

  function emit(event: string, payload: unknown): void {
    const handler = handlers.get(event);
    // Fail loudly rather than pass vacuously when the listener is missing.
    expect(handler, `${event} listener`).toBeDefined();
    handler!({ payload });
  }

  function started(runId: number, task = "assembleDebug", origin: unknown = claude): void {
    emit("build:started", {
      runId,
      task,
      origin,
      startedAt: new Date().toISOString(),
      projectRoot: "/p",
    });
  }

  function complete(runId: number, extra: Record<string, unknown> = {}): void {
    emit("build:complete", {
      runId,
      success: true,
      cancelled: false,
      durationMs: 2_000,
      errorCount: 0,
      warningCount: 0,
      task: "assembleDebug",
      origin: claude,
      cancelledBy: null,
      ...extra,
    });
  }

  function line(content: string) {
    return { kind: "output", content, file: null, line: null, col: null };
  }

  function logContents(): string[] {
    flushPendingLines();
    return buildLogStore.entries.map((entry) => entry.message);
  }

  beforeEach(async () => {
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    vi.clearAllMocks();
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    handlers = new Map();
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });
    await initBuildService();
  });

  afterEach(() => {
    resetBuildServiceForTests();
  });

  it("shows an agent's build with who started it, its output and outcome, and never deploys it", () => {
    started(4, "assembleRelease");

    expect(buildState.phase).toBe("running");
    expect(buildState.currentTask).toBe("assembleRelease");
    expect(buildState.origin).toEqual(claude);

    emit("build:lines", { runId: 4, lines: [line("> Task :app:compileReleaseKotlin")] });
    emit("build:lines", { runId: 99, lines: [line("another run's output")] });
    expect(logContents()).toEqual(["> Task :app:compileReleaseKotlin"]);

    complete(4);

    expect(buildState.phase).toBe("success");
    expect(buildState.origin).toEqual(claude);
    const deployCalls = mockInvoke.mock.calls.filter(([cmd]) =>
      ["find_apk_path", "install_apk_on_device", "launch_app_on_device"].includes(String(cmd))
    );
    expect(deployCalls).toHaveLength(0);
  });

  it("refuses to start a build while an agent's build runs, naming the agent", async () => {
    started(4);

    await expect(runBuild()).rejects.toThrow(
      "A build started by an agent (Claude Code) is running."
    );
    await expect(runAndDeploy()).rejects.toThrow(
      "A build started by an agent (Claude Code) is running."
    );
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "run_gradle_task")).toHaveLength(0);
  });

  it("cancels an agent's build and shows who cancelled it", async () => {
    started(4);

    await cancelBuild();

    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "cancel_build")).toHaveLength(1);
    expect(buildState.phase).toBe("cancelled");
    expect(buildState.cancelledBy).toEqual({ kind: "app" });

    complete(4, { success: false, cancelled: true, cancelledBy: { kind: "app" } });
    expect(buildState.phase).toBe("cancelled");
    expect(buildState.cancelledBy).toEqual({ kind: "app" });
  });

  it("shows that an agent cancelled its own build", () => {
    started(4);

    complete(4, { success: false, cancelled: true, cancelledBy: claude });

    expect(buildState.phase).toBe("cancelled");
    expect(buildState.cancelledBy).toEqual(claude);
  });

  it("streams this window's own build from build:lines once build:started names it", async () => {
    let returnRunId: (id: number) => void = () => {};
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") {
        return new Promise<number>((resolve) => {
          returnRunId = resolve;
        });
      }
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const build = runBuild("assembleDebug");
    started(5, "assembleDebug", { kind: "app" });
    emit("build:lines", { runId: 5, lines: [line("> Task :app:preBuild")] });

    expect(logContents()).toContain("> Task :app:preBuild");
    expect(buildState.origin).toEqual({ kind: "app" });

    complete(5, { origin: { kind: "app" } });
    returnRunId(5);
    await build;
    expect(buildState.phase).toBe("success");
  });

  it("hands the panel to an agent's build that took the slot first", async () => {
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") {
        // The agent's build started while this request was in flight.
        started(8, "testDebugUnitTest");
        emit("build:lines", { runId: 8, lines: [line("> Task :app:testDebugUnitTest")] });
        return Promise.reject("Invalid input: A build is already running");
      }
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    await expect(runBuild("assembleDebug")).rejects.toBe(
      "Invalid input: A build is already running"
    );

    expect(buildState.phase).toBe("running");
    expect(buildState.currentTask).toBe("testDebugUnitTest");
    expect(buildState.origin).toEqual(claude);
    expect(logContents()).toEqual(["> Task :app:testDebugUnitTest"]);

    complete(8);
    expect(buildState.phase).toBe("success");
  });

  it("does not bring back an agent's build that finished after a project switch", () => {
    started(4);
    beginProjectOpen();
    resetBuildState();

    complete(4);

    expect(buildState.phase).toBe("idle");
  });
});
