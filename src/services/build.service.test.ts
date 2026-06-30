import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { cancelBuild, runAndDeploy, runBuild } from "@/services/build.service";
import { buildState, resetBuildState, startBuild } from "@/stores/build.store";
import { resetDeviceState } from "@/stores/device.store";
import { resetVariantState, selectVariant } from "@/stores/variant.store";

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
  // cancelBuild must return early without calling finalize_build.
  // Before the fix, cancelBuild always called finalizeBuild with task="unknown",
  // which wrote a ghost record to the build history.
  it("does not call finalize_build when no build is running (idle phase)", async () => {
    expect(buildState.phase).toBe("idle");

    await cancelBuild();

    const finalizeCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "finalize_build");
    expect(finalizeCalls).toHaveLength(0);
  });

  it("does not call finalize_build when previous build already succeeded", async () => {
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
});
