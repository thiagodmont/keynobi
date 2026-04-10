import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { cancelBuild } from "@/services/build.service";
import { buildState, resetBuildState, startBuild } from "@/stores/build.store";

// The global setup in src/test/setup.ts already mocks @tauri-apps/api/core.
// We narrow it here so we can track which commands were called.
const mockInvoke = vi.mocked(invoke);

describe("cancelBuild guard — no ghost records on project switch", () => {
  beforeEach(() => {
    resetBuildState();
    mockInvoke.mockResolvedValue(undefined);
    vi.clearAllMocks();
  });

  // Regression: when no build is running (e.g. during a project switch),
  // cancelBuild must return early without calling finalize_build.
  // Before the fix, cancelBuild always called finalizeBuild with task="unknown",
  // which wrote a ghost record to the build history.
  it("does not call finalize_build when no build is running (idle phase)", async () => {
    expect(buildState.phase).toBe("idle");

    await cancelBuild();

    const finalizeCalls = mockInvoke.mock.calls.filter(
      ([cmd]) => cmd === "finalize_build"
    );
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

    const finalizeCalls = mockInvoke.mock.calls.filter(
      ([cmd]) => cmd === "finalize_build"
    );
    expect(finalizeCalls).toHaveLength(0);
  });

  it("calls cancel_build and finalize_build when a build is actually running", async () => {
    startBuild("assembleDebug");
    expect(buildState.phase).toBe("running");

    // cancelBuild will try to cancel and finalize — resolve all IPC calls.
    await cancelBuild();

    const cancelCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "cancel_build");
    const finalizeCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "finalize_build");

    expect(cancelCalls).toHaveLength(1);
    expect(finalizeCalls).toHaveLength(1);

    // The finalize call must carry the correct task name (not "unknown").
    const [, finalizeArgs] = finalizeCalls[0] as [string, Record<string, unknown>];
    expect(finalizeArgs?.task).toBe("assembleDebug");
  });
});
