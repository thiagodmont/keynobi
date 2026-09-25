import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  addBuildLine,
  buildState,
  clearBuildHistory,
  flushPendingLines,
  isViewingHistory,
  resetBuildState,
  setBuildHistory,
  setBuildResult,
  startBuild,
  viewHistoryBuild,
  viewLiveBuild,
  viewedBuild,
} from "@/stores/build.store";
import { makeBuildError, makeBuildLine, makeBuildRecord } from "@/test/factories/build";
import type { BuildActor, BuildStatus } from "@/bindings";

const agent: BuildActor = { kind: "agent", sessionId: 3, clientName: "Codex", standalone: false };

function failLiveBuild(message: string): void {
  startBuild("assembleDebug");
  addBuildLine(makeBuildLine({ kind: "error", content: message, file: "Live.kt", line: 1 }));
  flushPendingLines();
  setBuildResult({ success: false, durationMs: 1200 });
}

const pastFailure = makeBuildRecord({
  id: 7,
  task: "assembleRelease",
  status: {
    state: "failed",
    success: false,
    durationMs: BigInt(65_000),
    errorCount: 1,
    warningCount: 1,
  } as BuildStatus,
  errors: [
    makeBuildError({ message: "Past error" }),
    makeBuildError({ message: "Past warning", severity: "warning" }),
  ],
  startedAt: "2026-09-24T10:15:00Z",
  origin: agent,
  cancelledBy: null,
});

describe("viewing a past build", () => {
  beforeEach(() => {
    resetBuildState();
  });

  it("describes the live build until a past one is picked", () => {
    failLiveBuild("Live error");
    const view = viewedBuild();
    expect(view.source).toBe("live");
    expect(view.phase).toBe("failed");
    expect(view.errors.map((e) => e.message)).toEqual(["Live error"]);
    expect(isViewingHistory()).toBe(false);
  });

  it("switches status, timing, problems, and who ran it together", () => {
    failLiveBuild("Live error");
    setBuildHistory([pastFailure]);

    viewHistoryBuild(7);

    const view = viewedBuild();
    expect(view).toMatchObject({
      source: "history",
      id: 7,
      missing: false,
      task: "assembleRelease",
      phase: "failed",
      durationMs: 65_000,
      startedAt: "2026-09-24T10:15:00Z",
      origin: agent,
      cancelledBy: null,
    });
    expect(view.errors.map((e) => e.message)).toEqual(["Past error"]);
    expect(view.warnings.map((e) => e.message)).toEqual(["Past warning"]);
    // The live build is untouched.
    expect(buildState.errors.map((e) => e.message)).toEqual(["Live error"]);
  });

  it("reports a viewed build that left the history as missing", () => {
    setBuildHistory([pastFailure]);
    viewHistoryBuild(7);

    setBuildHistory([makeBuildRecord({ id: 8 })]);

    expect(viewedBuild()).toMatchObject({ source: "history", id: 7, missing: true, errors: [] });
  });

  it("goes back to the live build", () => {
    setBuildHistory([pastFailure]);
    viewHistoryBuild(7);

    viewLiveBuild();

    expect(viewedBuild().source).toBe("live");
  });

  it("a build started in the app brings the panel back to it", () => {
    setBuildHistory([pastFailure]);
    viewHistoryBuild(7);

    startBuild("assembleDebug", { kind: "app" });

    expect(isViewingHistory()).toBe(false);
  });

  it("an agent's build does not replace the past build on screen", () => {
    setBuildHistory([pastFailure]);
    viewHistoryBuild(7);

    startBuild("assembleDebug", agent);

    expect(buildState.viewedHistoryId).toBe(7);
    expect(viewedBuild().phase).toBe("failed");
    expect(buildState.phase).toBe("running");
  });

  it("a project switch or a cleared history shows the live build", async () => {
    setBuildHistory([pastFailure]);
    viewHistoryBuild(7);
    resetBuildState();
    expect(isViewingHistory()).toBe(false);

    setBuildHistory([pastFailure]);
    viewHistoryBuild(7);
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await clearBuildHistory();
    expect(isViewingHistory()).toBe(false);
  });
});
