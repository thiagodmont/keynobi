import { describe, it, expect, beforeEach, afterEach, vi, expectTypeOf } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  cancelBuild,
  initBuildService,
  resetBuildServiceForTests,
  runAndDeploy,
  runBuild,
  runBuildOnly,
} from "@/services/build.service";
import {
  buildLogStore,
  buildState,
  flushPendingLines,
  resetBuildState,
  startBuild,
} from "@/stores/build.store";
import { resetDeviceState } from "@/stores/device.store";
import { setProjects } from "@/stores/projects.store";
import { resetVariantState } from "@/stores/variant.store";
import { updateSetting } from "@/stores/settings.store";
import { beginProjectOpen, clearProject, setProject } from "@/stores/project.store";
import {
  makeDeployResult,
  makeLaunchTiming,
  makeProjectRunConfigurations,
  makeResolvedRun,
  makeRunConfiguration,
} from "@/test/factories/build";
import type {
  BuildActor,
  DeployPhase,
  DeployPhaseEvent,
  DeployResult,
  ResolvedRun,
  TargetPreference,
} from "@/bindings";
import type * as UiModule from "@/components/ui";

const devicePickerMock = vi.hoisted(() => ({
  showDevicePicker: vi.fn<() => Promise<string | null>>(),
}));

vi.mock("@/components/device/DevicePickerDialog", () => devicePickerMock);

const dialogMock = vi.hoisted(() => ({
  showDialog: vi.fn<(dialog: { title: string; message: string }) => Promise<string>>(),
}));

vi.mock("@/components/ui", async (importOriginal) => ({
  ...(await importOriginal<typeof UiModule>()),
  showDialog: dialogMock.showDialog,
}));

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

    // The Default configuration targets the last used device, and none is online.
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "resolve_run_configuration") {
        return Promise.reject({ kind: "notFound", message: "no device is selected" });
      }
      if (cmd === "list_run_configurations") {
        return Promise.resolve({ configurations: [], active: "Default", local: {} });
      }
      return Promise.resolve(undefined);
    });
    const deploy = runAndDeploy();
    await vi.waitFor(() => expect(devicePickerMock.showDevicePicker).toHaveBeenCalled());

    expect(buildState.phase).toBe("idle");
    await expect(runBuild()).rejects.toThrow("A build or deploy is already running.");
    await expect(runBuildOnly()).rejects.toThrow("A build or deploy is already running.");

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

describe("runAndDeploy runs the configuration in the backend", () => {
  /** The event listeners the build service registered. */
  let handlers = new Map<string, (e: { payload: unknown }) => void>();

  beforeEach(async () => {
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    clearProject();
    setProjects([]);
    mockInvoke.mockResolvedValue(undefined);
    devicePickerMock.showDevicePicker.mockReset();
    vi.clearAllMocks();
    handlers = new Map();
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      handlers.set(String(event), cb as unknown as (e: { payload: unknown }) => void);
      return () => {};
    });
    await initBuildService();
    // Fail loudly if a listener was never registered — otherwise the
    // dispatches below would no-op and the tests would pass vacuously.
    expect(handlers.has("build:complete")).toBe(true);
    expect(handlers.has("deploy:phase")).toBe(true);
  });

  afterEach(() => {
    resetBuildServiceForTests();
    updateSetting("build", "autoInstallOnBuild", true);
    vi.useRealTimers();
  });

  function emit(event: string, payload: unknown): void {
    handlers.get(event)!({ payload });
  }

  function callsTo(command: string) {
    return mockInvoke.mock.calls.filter(([cmd]) => cmd === command);
  }

  function buildLog(): string[] {
    flushPendingLines();
    return buildLogStore.entries.map((entry) => entry.message);
  }

  function phaseEvent(
    phase: DeployPhase,
    steps: string[] = [],
    run: ResolvedRun = makeResolvedRun(),
    origin: BuildActor = { kind: "app" }
  ): DeployPhaseEvent {
    return {
      phase,
      origin,
      name: run.name,
      plan: run.plan,
      device: run.device ?? { serial: "emulator-5554", label: "Pixel_7" },
      buildId: phase === "building" ? null : 7,
      steps,
      error: null,
    };
  }

  /** What the backend's build of a run sends: started, its output, and complete. */
  function backendBuild(
    task = ":app:assembleDebug",
    completion: Record<string, unknown> = {}
  ): void {
    const origin = { kind: "app" };
    emit("build:started", {
      runId: 1,
      task,
      origin,
      startedAt: new Date().toISOString(),
      projectRoot: "/p",
    });
    emit("build:lines", {
      runId: 1,
      lines: [
        { kind: "output", content: "> Task :app:assembleDebug", file: null, line: null, col: null },
      ],
    });
    emit("build:complete", {
      runId: 1,
      recordId: 7,
      success: true,
      cancelled: false,
      durationMs: 1_000,
      errorCount: 0,
      warningCount: 0,
      task,
      origin,
      cancelledBy: null,
      ...completion,
    });
  }

  /**
   * IPC replies for a run: `resolve_run_configuration` answers `run`, and
   * `run_run_configuration` plays the backend (`backend`, by default a run
   * whose build succeeds, then installs and launches) and answers `result`.
   */
  function replies(
    opts: {
      run?: ResolvedRun;
      result?: DeployResult;
      backend?: () => Promise<DeployResult>;
      overrides?: Record<string, (args: unknown) => Promise<unknown>>;
    } = {}
  ): void {
    const run = opts.run ?? makeResolvedRun();
    const result = opts.result ?? makeDeployResult({}, run);
    const backend =
      opts.backend ??
      (async () => {
        emit("deploy:phase", phaseEvent("building", [], run));
        backendBuild(run.task);
        emit("deploy:phase", phaseEvent("installing", ["APK (build #7): /tmp/app-debug.apk"], run));
        emit("deploy:phase", phaseEvent("launching", ["Install: Success (1.2s)"], run));
        emit("deploy:phase", phaseEvent("done", [], run));
        return result;
      });
    mockInvoke.mockImplementation((cmd, args) => {
      const override = opts.overrides?.[cmd];
      if (override) return override(args);
      if (cmd === "resolve_run_configuration") return Promise.resolve(run);
      if (cmd === "run_run_configuration") return backend();
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
  }

  it("skips device resolution, install, and launch when autoInstallOnBuild is off", async () => {
    updateSetting("build", "autoInstallOnBuild", false);
    replies({
      run: makeResolvedRun({ device: null, plan: "Build 'Default': build :app:assembleDebug" }),
    });

    const deploy = runAndDeploy();
    await vi.waitFor(() => expect(buildState.phase).toBe("running"));
    backendBuild();
    await deploy;

    expect(buildState.phase).toBe("success");
    // The plan is asked for without a device.
    expect(callsTo("resolve_run_configuration")[0]?.[1]).toMatchObject({ buildOnly: true });
    // Build-only run: the device picker must not even open.
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
    expect(callsTo("run_gradle_task")[0]?.[1]).toEqual({ task: ":app:assembleDebug" });
    expect(callsTo("run_run_configuration")).toHaveLength(0);
    expect(buildLog()).toContain(
      "▶ Auto Install on Build is disabled — skipping install and launch."
    );
  });

  it("runs the resolved configuration on its device, for the open project", async () => {
    setProject("/projects/app", "app");
    setProjects([
      {
        id: "app",
        path: "/projects/app",
        name: "app",
        gradleRoot: "/projects/app",
        lastOpened: "2026-01-01T00:00:00Z",
        pinned: false,
        lastBuildVariant: null,
        lastDevice: null,
        trusted: true,
      },
    ]);
    replies({
      run: makeResolvedRun({
        name: "Staging",
        device: { serial: "28151FDH2000Q4", label: "Pixel 7" },
      }),
    });

    await runAndDeploy("Staging");

    expect(callsTo("resolve_run_configuration")[0]?.[1]).toMatchObject({
      name: "Staging",
      buildOnly: false,
    });
    expect(callsTo("run_run_configuration")).toEqual([
      [
        "run_run_configuration",
        { name: "Staging", selectedSerial: "28151FDH2000Q4", projectRoot: "/projects/app" },
      ],
    ]);
    // The backend builds, installs, and launches: no step runs from here.
    for (const command of ["run_gradle_task", "install_apk_on_device", "launch_app_on_device"]) {
      expect(callsTo(command)).toHaveLength(0);
    }
    // The picker is for Ask and Last used targets that found no device.
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
  });

  it("heads the log with the plan and shows the backend's build", async () => {
    replies();

    await runAndDeploy();

    const log = buildLog();
    expect(log[0]).toBe(
      "Run 'Default': build :app:assembleDebug → install this build's APK → launch the app on Pixel_7 → filter package:mine"
    );
    expect(log).toContain("▶ Build started: :app:assembleDebug");
    expect(log).toContain("> Task :app:assembleDebug");
    expect(buildState.phase).toBe("success");
    expect(buildState.deployPhase).toBeNull();
  });

  it("logs each phase's steps and shows the phase while it runs", async () => {
    let seen: unknown[] = [];
    replies({
      backend: async () => {
        emit("deploy:phase", phaseEvent("building"));
        backendBuild();
        emit("deploy:phase", phaseEvent("installing", ["APK (build #7): /tmp/app-debug.apk"]));
        seen = [buildState.deployPhase];
        emit("deploy:phase", phaseEvent("launching", ["Package (from APK): com.example.app"]));
        seen.push(buildState.deployPhase);
        emit("deploy:phase", phaseEvent("done"));
        return makeDeployResult();
      },
    });

    await runAndDeploy();

    expect(seen).toEqual(["installing", "launching"]);
    expect(buildLog()).toEqual(
      expect.arrayContaining([
        "▶ APK (build #7): /tmp/app-debug.apk",
        "▶ Package (from APK): com.example.app",
      ])
    );
    expect(buildState.deployPhase).toBeNull();
  });

  it("keeps an agent's phases out of this window's run", async () => {
    const agent: BuildActor = {
      kind: "agent",
      sessionId: 3,
      clientName: "Claude Code",
      standalone: false,
    };
    let seen: unknown = undefined;
    replies({
      backend: async () => {
        emit("deploy:phase", phaseEvent("building"));
        backendBuild();
        emit("deploy:phase", phaseEvent("installing", ["APK (build #7): /tmp/app-debug.apk"]));
        // An agent's run, even one naming the same build, is not this run.
        emit(
          "deploy:phase",
          phaseEvent("launching", ["adb shell am start -W (agent)"], makeResolvedRun(), agent)
        );
        emit("deploy:phase", phaseEvent("failed", [], makeResolvedRun(), agent));
        seen = buildState.deployPhase;
        emit("deploy:phase", phaseEvent("done"));
        return makeDeployResult();
      },
    });

    await runAndDeploy();

    expect(seen).toBe("installing");
    const log = buildLog();
    expect(log).toContain("▶ APK (build #7): /tmp/app-debug.apk");
    expect(log.join("\n")).not.toContain("(agent)");
    expect(log.join("\n")).not.toContain("by an agent");
  });

  it("logs steps that arrive after the run answered", async () => {
    replies({
      backend: async () => {
        backendBuild();
        // The last phase's event is still on its way when the answer arrives.
        setTimeout(() =>
          emit(
            "deploy:phase",
            phaseEvent("done", ["Run configuration 'Default' installs only — not launching."])
          )
        );
        return makeDeployResult({ launch: null });
      },
    });

    await runAndDeploy();

    await vi.waitFor(() =>
      expect(buildLog()).toContain("▶ Run configuration 'Default' installs only — not launching.")
    );
    expect(buildState.deployPhase).toBeNull();
  });

  it("logs the launch and its time, and records nothing itself", async () => {
    replies({
      result: makeDeployResult({
        launch: { output: "Status: ok", timing: makeLaunchTiming({ displayedMs: 790 }) },
      }),
    });

    await runAndDeploy();

    expect(buildLog()).toEqual(
      expect.arrayContaining([
        "▶ Launch: Status: ok",
        "▶ Launch time: 812 ms (cold) · displayed 790 ms",
      ])
    );
    // The launch time is on the run's build record: the history is reloaded.
    expect(callsTo("get_build_history").length).toBeGreaterThan(0);
  });

  it("says when the launch method reported no launch time", async () => {
    replies({
      result: makeDeployResult({
        launch: { output: "monkey OK: Events injected: 1", timing: null },
      }),
    });

    await runAndDeploy();

    expect(buildLog()).toContain("▶ Launch time: not reported by this launch method");
  });

  it("says a deep link reports no launch time", async () => {
    const run = makeResolvedRun({ launch: { kind: "deepLink", uri: "myapp://home" } });
    replies({
      run,
      result: makeDeployResult({ launch: { output: "Starting: Intent { … }", timing: null } }, run),
    });

    await runAndDeploy();

    expect(buildLog()).toContain("▶ Launch time: not reported for a deep link");
    expect(buildState.lastLaunchedPackage).toBe("com.example.app");
  });

  it("applies the configuration's logcat filter after the launch", async () => {
    const run = makeResolvedRun({ logcatFilter: "package:mine level:warn" });
    replies({ run });

    await runAndDeploy();

    expect(buildState.lastLaunchedAt).not.toBeNull();
    expect(buildState.lastLaunchedPackage).toBe("com.example.app");
    expect(buildState.lastLaunchedFilter).toBe("package:mine level:warn");
  });

  it("merges package:mine after the launch when the configuration has no filter", async () => {
    replies();

    await runAndDeploy();

    expect(buildState.lastLaunchedAt).not.toBeNull();
    expect(buildState.lastLaunchedFilter).toBeNull();
  });

  it("applies no filter when the run did not launch", async () => {
    const run = makeResolvedRun({ launch: { kind: "none" } });
    replies({ run, result: makeDeployResult({ launch: null }, run) });

    await runAndDeploy();

    expect(buildState.lastLaunchedAt).toBeNull();
  });

  it("stops after a failed build, without launching or failing the call", async () => {
    replies({
      backend: async () => {
        backendBuild(":app:assembleDebug", { success: false, errorCount: 1 });
        return makeDeployResult({ outcome: "buildFailed", apk: null, package: null, launch: null });
      },
    });

    await runAndDeploy();

    expect(buildState.phase).toBe("failed");
    expect(buildState.lastLaunchedAt).toBeNull();
    expect(buildLog().join("\n")).toContain('Build phase is "failed" — skipping install.');
  });

  it("cancels the backend's build from the Cancel button", async () => {
    let finish: (result: DeployResult) => void = () => {};
    replies({
      backend: () => {
        emit("build:started", {
          runId: 1,
          task: ":app:assembleDebug",
          origin: { kind: "app" },
          startedAt: new Date().toISOString(),
          projectRoot: "/p",
        });
        return new Promise((resolve) => {
          finish = resolve;
        });
      },
      overrides: {
        cancel_build: async () => {
          emit("build:complete", {
            runId: 1,
            recordId: 7,
            success: false,
            cancelled: true,
            durationMs: 500,
            errorCount: 0,
            warningCount: 0,
            task: ":app:assembleDebug",
            origin: { kind: "app" },
            cancelledBy: { kind: "app" },
          });
          finish(makeDeployResult({ outcome: "cancelled", launch: null, package: null }));
        },
      },
    });

    const deploy = runAndDeploy();
    await vi.waitFor(() => expect(callsTo("run_run_configuration")).toHaveLength(1));
    await cancelBuild();
    await deploy;

    expect(callsTo("cancel_build")).toHaveLength(1);
    expect(buildState.phase).toBe("cancelled");
    expect(buildState.lastLaunchedAt).toBeNull();
  });

  it("fails with the backend's reason when the install fails", async () => {
    const reason = { kind: "processFailed", message: "adb: failed to install: INSTALL_FAILED" };
    replies({
      backend: async () => {
        backendBuild();
        emit("deploy:phase", phaseEvent("installing", ["adb install /tmp/app-debug.apk"]));
        throw reason;
      },
    });

    await expect(runAndDeploy()).rejects.toBe(reason);

    expect(buildState.phase).toBe("success");
    expect(buildState.deployPhase).toBeNull();
    expect(buildLog()).toContain("▶ adb install /tmp/app-debug.apk");
    expect(buildLog().join("\n")).toContain("adb: failed to install: INSTALL_FAILED");
  });

  it("ends the build in the panel when the run stops before its build starts", async () => {
    const reason = {
      kind: "invalidInput",
      message: "The project changed before the run started. Nothing was built.",
    };
    replies({ backend: () => Promise.reject(reason) });

    await expect(runAndDeploy()).rejects.toBe(reason);

    expect(buildState.phase).toBe("failed");
    expect(buildState.deployPhase).toBeNull();
  });

  it("does not run once the project changed while the configuration resolved", async () => {
    replies({
      overrides: {
        resolve_run_configuration: async () => {
          // The user opens another project meanwhile.
          beginProjectOpen();
          return makeResolvedRun();
        },
      },
    });

    await expect(runAndDeploy()).rejects.toThrow("The project changed during deploy");

    expect(callsTo("run_run_configuration")).toHaveLength(0);
  });

  it("stops before building when several modules have no active configuration", async () => {
    const reason = {
      kind: "invalidInput",
      message:
        "No run configuration is active. Choose the one to run: mobile (:mobile debug), wear (:wear debug).",
    };
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "resolve_run_configuration") return Promise.reject(reason);
      return Promise.resolve(undefined);
    });

    await expect(runAndDeploy()).rejects.toBe(reason);

    expect(callsTo("resolve_run_configuration")[0]?.[1]).toEqual({
      name: null,
      selectedSerial: null,
      buildOnly: false,
    });
    expect(callsTo("run_run_configuration")).toHaveLength(0);
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
    expect(buildLog().join("\n")).toContain("mobile (:mobile debug), wear (:wear debug)");
  });

  /** Resolution fails for want of a device, for a configuration with `target`. */
  function noDeviceFor(target: TargetPreference) {
    const noDevice = { kind: "notFound", message: "no device is online for this run" };
    let resolved = 0;
    mockInvoke.mockImplementation((cmd, args) => {
      if (cmd === "resolve_run_configuration") {
        resolved++;
        const picked = (args as { selectedSerial?: string | null }).selectedSerial;
        return resolved === 1 || !picked
          ? Promise.reject(noDevice)
          : Promise.resolve(makeResolvedRun({ device: { serial: picked, label: picked } }));
      }
      if (cmd === "list_run_configurations") {
        return Promise.resolve({
          configurations: [],
          active: "Default",
          local: {
            Default: { target, lastDevice: null, approvedProjectFileSha256: null },
          },
        });
      }
      if (cmd === "run_run_configuration") {
        backendBuild();
        return Promise.resolve(makeDeployResult({ launch: null }));
      }
      return Promise.resolve(undefined);
    });
    return noDevice;
  }

  for (const kind of ["ask", "lastUsed"] as const) {
    it(`asks for a device when a target of ${kind} finds none, and runs on it`, async () => {
      noDeviceFor({ kind });
      devicePickerMock.showDevicePicker.mockResolvedValue("28151FDH2000Q4");

      await runAndDeploy();

      expect(devicePickerMock.showDevicePicker).toHaveBeenCalledTimes(1);
      expect(callsTo("resolve_run_configuration")[1]?.[1]).toMatchObject({
        selectedSerial: "28151FDH2000Q4",
      });
      expect(callsTo("run_run_configuration")[0]?.[1]).toMatchObject({
        selectedSerial: "28151FDH2000Q4",
      });
    });
  }

  it("builds nothing when the device picker is cancelled", async () => {
    noDeviceFor({ kind: "ask" });
    devicePickerMock.showDevicePicker.mockResolvedValue(null);

    await runAndDeploy();

    expect(callsTo("run_run_configuration")).toHaveLength(0);
    expect(buildLog()).toContain("▶ No device selected — run cancelled.");
  });

  it("offers to launch the preferred AVD when it is not running, and stops on Cancel", async () => {
    const noDevice = noDeviceFor({ kind: "avd", name: "Pixel_7" });
    dialogMock.showDialog.mockResolvedValue("cancel");

    await expect(runAndDeploy()).rejects.toBe(noDevice);

    expect(dialogMock.showDialog).toHaveBeenCalledWith(
      expect.objectContaining({ title: "AVD not running", message: noDevice.message })
    );
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
    expect(callsTo("launch_avd")).toHaveLength(0);
    expect(callsTo("run_run_configuration")).toHaveLength(0);
  });

  it("launches the preferred AVD when asked, and runs nothing until it is online", async () => {
    noDeviceFor({ kind: "avd", name: "Pixel_7" });
    dialogMock.showDialog.mockResolvedValue("launch");

    await runAndDeploy();

    await vi.waitFor(() => expect(callsTo("launch_avd")).toHaveLength(1));
    expect(callsTo("launch_avd")[0][1]).toEqual({ avdName: "Pixel_7" });
    expect(callsTo("run_run_configuration")).toHaveLength(0);
    expect(buildLog()).toContain("▶ Launching Pixel_7 — run again once it is online.");
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

  it("reloads the history when a launch's display times arrive after it returned", async () => {
    const launch = makeLaunchTiming({ displayedMs: 790, fullyDrawnMs: 1400 });
    const record = { id: 12, task: "assembleDebug", launch };
    mockInvoke.mockImplementation((cmd) =>
      cmd === "get_build_history" ? Promise.resolve([record]) : Promise.resolve(undefined)
    );

    emit("build:launch_timing", { recordId: 12, launch });

    await vi.waitFor(() => expect(buildState.history[0]?.launch?.fullyDrawnMs).toBe(1400));
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

  /** An agent's run of the configuration Default entered `phase`. */
  function agentPhase(
    phase: DeployPhase,
    buildId: number | null,
    steps: string[] = [],
    error: string | null = null
  ): DeployPhaseEvent {
    const run = makeResolvedRun();
    return {
      phase,
      origin: claude,
      name: run.name,
      plan: run.plan,
      device: { serial: "emulator-5554", label: "Pixel_7" },
      buildId,
      steps,
      error,
    };
  }

  it("logs an agent's install and launch under its build, without taking the run over", () => {
    emit("deploy:phase", agentPhase("building", null));
    started(4, ":app:assembleDebug");
    emit("build:lines", { runId: 4, lines: [line("> Task :app:assembleDebug")] });
    complete(4, { recordId: 21 });
    emit(
      "deploy:phase",
      agentPhase("installing", 21, [
        "APK (build #21): /p/app-debug.apk",
        "adb install /p/app-debug.apk",
      ])
    );
    emit(
      "deploy:phase",
      agentPhase("launching", 21, [
        "Install: Success (1.2s)",
        "adb shell am start -W (package: com.example.app)",
      ])
    );
    emit("deploy:phase", agentPhase("done", 21));

    expect(logContents()).toEqual([
      "> Task :app:assembleDebug",
      "▶ APK (build #21): /p/app-debug.apk",
      "▶ adb install /p/app-debug.apk",
      "▶ Install: Success (1.2s)",
      "▶ adb shell am start -W (package: com.example.app)",
      "▶ Run 'Default' by an agent (Claude Code): done on Pixel_7",
    ]);
    expect(buildState.origin).toEqual(claude);
    expect(buildState.phase).toBe("success");
    // The app's own run state is untouched: no phase, no logcat filter, no picker.
    expect(buildState.deployPhase).toBeNull();
    expect(buildState.lastLaunchedAt).toBeNull();
    expect(buildState.lastLaunchedFilter).toBeNull();
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd !== "get_build_history")).toHaveLength(0);
  });

  it("logs why an agent's run failed", () => {
    started(4);
    complete(4, { recordId: 21 });

    emit(
      "deploy:phase",
      agentPhase("failed", 21, ["adb install /p/app-debug.apk"], "adb: INSTALL_FAILED")
    );

    expect(logContents()).toEqual([
      "▶ adb install /p/app-debug.apk",
      "Run 'Default' by an agent (Claude Code) failed: adb: INSTALL_FAILED",
    ]);
  });

  it("does not log an agent's run whose build the panel does not show", async () => {
    // Another build's record.
    started(4);
    complete(4, { recordId: 21 });
    emit("deploy:phase", agentPhase("installing", 22, ["adb install /p/other.apk"]));
    expect(logContents()).toEqual([]);

    // This window's build replaced the agent's in the panel.
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "run_gradle_task") return Promise.resolve(5);
      if (cmd === "get_build_history") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const build = runBuild("assembleDebug");
    emit("deploy:phase", agentPhase("launching", 21, ["adb shell am start -W (late)"]));
    complete(5, { origin: { kind: "app" }, recordId: 22 });
    await build;
    emit("deploy:phase", agentPhase("done", 21));

    expect(logContents().join("\n")).not.toContain("late");
    expect(logContents().join("\n")).not.toContain("by an agent");
  });

  it("does not log an agent's run after a project switch", () => {
    started(4);
    complete(4, { recordId: 21 });
    beginProjectOpen();
    resetBuildState();

    emit("deploy:phase", agentPhase("installing", 21, ["adb install /p/app-debug.apk"]));

    expect(logContents()).toEqual([]);
  });

  it("does not bring back an agent's build that finished after a project switch", () => {
    started(4);
    beginProjectOpen();
    resetBuildState();

    complete(4);

    expect(buildState.phase).toBe("idle");
  });
});

describe("Build Only builds the active run configuration", () => {
  beforeEach(() => {
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetBuildServiceForTests();
  });

  function callsTo(command: string) {
    return mockInvoke.mock.calls.filter(([cmd]) => cmd === command);
  }

  it("builds the configuration's task with its plan, without a device", async () => {
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "resolve_run_configuration") {
        return Promise.resolve(
          makeResolvedRun({
            task: ":wear:assembleFreeRelease",
            device: null,
            plan: "Build 'wear': build :wear:assembleFreeRelease",
          })
        );
      }
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      return Promise.resolve(undefined);
    });

    const build = runBuildOnly();
    await vi.waitFor(() => expect(buildState.phase).toBe("running"));
    await cancelBuild();
    await build;

    expect(callsTo("resolve_run_configuration")[0]?.[1]).toMatchObject({ buildOnly: true });
    expect(callsTo("run_gradle_task")[0]?.[1]).toEqual({ task: ":wear:assembleFreeRelease" });
    flushPendingLines();
    expect(buildLogStore.entries[0]?.message).toBe("Build 'wear': build :wear:assembleFreeRelease");
    expect(devicePickerMock.showDevicePicker).not.toHaveBeenCalled();
  });

  it("builds a configuration by name", async () => {
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "resolve_run_configuration") {
        return Promise.resolve(makeResolvedRun({ name: "Wear", task: ":wear:assembleDebug" }));
      }
      if (cmd === "run_gradle_task") return Promise.resolve(1);
      return Promise.resolve(undefined);
    });

    const build = runBuildOnly("Wear");
    await vi.waitFor(() => expect(buildState.phase).toBe("running"));
    await cancelBuild();
    await build;

    expect(callsTo("resolve_run_configuration")[0]?.[1]).toMatchObject({ name: "Wear" });
    expect(callsTo("run_gradle_task")[0]?.[1]).toEqual({ task: ":wear:assembleDebug" });
  });

  describe("a shared configuration that needs approval", () => {
    const needsApproval = {
      kind: "approvalRequired",
      message:
        "Run configuration 'Bundle' is shared with the project (.keynobi/run-configurations.json) and builds :app:bundleDebug, which is not an assemble task. You have not approved it yet. Review it, then approve it to run it.",
    };

    beforeEach(() => {
      setProject("/projects/app", "app");
      setProjects([
        {
          id: "app",
          path: "/projects/app",
          name: "app",
          gradleRoot: "/projects/app",
          lastOpened: "2026-01-01T00:00:00Z",
          pinned: false,
          lastBuildVariant: null,
          lastDevice: null,
          trusted: true,
        },
      ]);
      let approved = false;
      mockInvoke.mockImplementation((cmd) => {
        if (cmd === "resolve_run_configuration") {
          return approved
            ? Promise.resolve(makeResolvedRun({ name: "Bundle", task: ":app:bundleDebug" }))
            : Promise.reject(needsApproval);
        }
        if (cmd === "list_run_configurations") {
          return Promise.resolve(
            makeProjectRunConfigurations(
              [makeRunConfiguration({ name: "Bundle", task: ":app:bundleDebug" })],
              {},
              ["Bundle"]
            )
          );
        }
        if (cmd === "approve_shared_run_configuration") {
          approved = true;
          return Promise.resolve(makeProjectRunConfigurations());
        }
        if (cmd === "run_gradle_task") return Promise.resolve(1);
        return Promise.resolve(undefined);
      });
    });

    afterEach(() => {
      clearProject();
      setProjects([]);
    });

    it("asks once, records the approval for the file it showed, and builds", async () => {
      dialogMock.showDialog.mockResolvedValue("approve");

      const build = runBuildOnly("Bundle");
      await vi.waitFor(() => expect(buildState.phase).toBe("running"));
      await cancelBuild();
      await build;

      expect(dialogMock.showDialog).toHaveBeenCalledTimes(1);
      expect(dialogMock.showDialog).toHaveBeenCalledWith(
        expect.objectContaining({
          title: "Approve shared run configuration?",
          message: expect.stringContaining(
            "builds :app:bundleDebug, which is not an assemble task"
          ),
        })
      );
      expect(callsTo("approve_shared_run_configuration")[0]?.[1]).toEqual({
        name: "Bundle",
        sha256: "a".repeat(64),
      });
      expect(callsTo("resolve_run_configuration")).toHaveLength(2);
      expect(callsTo("run_gradle_task")[0]?.[1]).toEqual({ task: ":app:bundleDebug" });
    });

    it("builds nothing when the approval is declined", async () => {
      dialogMock.showDialog.mockResolvedValue("cancel");

      await runBuildOnly("Bundle");

      expect(callsTo("approve_shared_run_configuration")).toHaveLength(0);
      expect(callsTo("run_gradle_task")).toHaveLength(0);
      flushPendingLines();
      expect(buildLogStore.entries.map((e) => e.message).join("\n")).toContain(
        "The shared run configuration was not approved — build cancelled."
      );
    });

    it("runs nothing when the approval is declined for Run App", async () => {
      dialogMock.showDialog.mockResolvedValue("cancel");

      await runAndDeploy();

      expect(callsTo("run_gradle_task")).toHaveLength(0);
      expect(callsTo("run_run_configuration")).toHaveLength(0);
      flushPendingLines();
      expect(buildLogStore.entries.map((e) => e.message).join("\n")).toContain(
        "The shared run configuration was not approved — run cancelled."
      );
    });
  });

  it("builds nothing and says why when no configuration is active", async () => {
    const reason = {
      kind: "invalidInput",
      message: "No run configuration is active. Choose the one to run: mobile (:mobile debug).",
    };
    mockInvoke.mockImplementation((cmd) =>
      cmd === "resolve_run_configuration" ? Promise.reject(reason) : Promise.resolve(undefined)
    );

    await expect(runBuildOnly()).rejects.toBe(reason);

    expect(callsTo("run_gradle_task")).toHaveLength(0);
    flushPendingLines();
    expect(buildLogStore.entries.map((e) => e.message).join("\n")).toContain(
      "No run configuration is active"
    );
  });
});
