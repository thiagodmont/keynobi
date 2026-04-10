import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  buildState,
  buildLogStore,
  startBuild,
  addBuildLine,
  flushPendingLines,
  setBuildResult,
  cancelBuildState,
  clearBuild,
  setBuildHistory,
  resetBuildState,
  setLastLaunchedAt,
} from "@/stores/build.store";
import type { BuildLine } from "@/bindings";

describe("build.store", () => {
  beforeEach(() => {
    resetBuildState();
  });

  it("starts in idle phase", () => {
    expect(buildState.phase).toBe("idle");
    expect(buildState.currentTask).toBeNull();
    expect(buildState.errors).toHaveLength(0);
  });

  it("startBuild sets running phase and clears logs", () => {
    startBuild("assembleDebug");
    expect(buildState.phase).toBe("running");
    expect(buildState.currentTask).toBe("assembleDebug");
    expect(buildState.errors).toHaveLength(0);
    expect(buildState.startedAt).not.toBeNull();
  });

  it("setBuildResult success sets success phase", () => {
    startBuild("assembleDebug");
    setBuildResult({ success: true, durationMs: 5000, errorCount: 0, warningCount: 0 });
    expect(buildState.phase).toBe("success");
    expect(buildState.durationMs).toBe(5000);
  });

  it("setBuildResult failure sets failed phase", () => {
    startBuild("assembleDebug");
    setBuildResult({ success: false, durationMs: 3000, errorCount: 2, warningCount: 1 });
    expect(buildState.phase).toBe("failed");
  });

  it("cancelBuildState sets cancelled phase", () => {
    startBuild("assembleDebug");
    cancelBuildState();
    expect(buildState.phase).toBe("cancelled");
  });

  it("addBuildLine appends to log store", () => {
    const line: BuildLine = {
      kind: "output",
      content: "> Task :app:compileDebugKotlin",
      file: null,
      line: null,
      col: null,
    };
    addBuildLine(line);
    flushPendingLines();
    expect(buildLogStore.entries).toHaveLength(1);
    expect(buildLogStore.entries[0].message).toContain("compileDebugKotlin");
  });

  it("addBuildLine accumulates errors", () => {
    const errLine: BuildLine = {
      kind: "error",
      content: "Unresolved reference: foo",
      file: "/src/Main.kt",
      line: 10,
      col: 5,
    };
    addBuildLine(errLine);
    flushPendingLines();
    expect(buildState.errors).toHaveLength(1);
    expect(buildState.errors[0].message).toBe("Unresolved reference: foo");
    expect(buildState.errors[0].line).toBe(10);
  });

  it("clearBuild resets to idle and empties log", () => {
    startBuild("assembleDebug");
    const line: BuildLine = { kind: "output", content: "hello", file: null, line: null, col: null };
    addBuildLine(line);
    clearBuild();
    expect(buildState.phase).toBe("idle");
    expect(buildLogStore.entries).toHaveLength(0);
  });

  it("setBuildHistory stores records", () => {
    setBuildHistory([
      {
        id: 1,
        task: "assembleDebug",
        status: { state: "Success", success: true, durationMs: 1000, errorCount: 0, warningCount: 0 } as any,
        errors: [],
        startedAt: new Date().toISOString(),
        projectRoot: "/home/user/my-app",
      },
    ]);
    expect(buildState.history).toHaveLength(1);
    expect(buildState.history[0].task).toBe("assembleDebug");
    expect(buildState.history[0].projectRoot).toBe("/home/user/my-app");
  });

  // Regression: resetBuildState must clear history (not just current build state).
  // Bug: selectProject was calling clearBuild() which preserved history, so
  // switching projects left the previous project's builds visible.
  it("resetBuildState clears history along with build state", () => {
    setBuildHistory([
      {
        id: 1, task: "assembleDebug",
        status: { state: "success" } as any, errors: [],
        startedAt: new Date().toISOString(), projectRoot: "/home/user/my-app",
      },
    ]);
    startBuild("assembleDebug");
    resetBuildState();
    expect(buildState.phase).toBe("idle");
    expect(buildState.history).toHaveLength(0);
  });

  // Contrast: clearBuild intentionally preserves history (used during live builds).
  it("clearBuild does NOT clear history — only resetBuildState does", () => {
    setBuildHistory([
      {
        id: 1, task: "assembleDebug",
        status: { state: "success" } as any, errors: [],
        startedAt: new Date().toISOString(), projectRoot: "/home/user/my-app",
      },
    ]);
    startBuild("assembleDebug");
    clearBuild();
    expect(buildState.phase).toBe("idle");
    expect(buildState.history).toHaveLength(1); // history preserved intentionally
  });
});

describe("build store error state transitions", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    resetBuildState();
  });

  it("reflects cancelled phase when cancelBuildState is called mid-run", () => {
    startBuild("assembleDebug");
    expect(buildState.phase).toBe("running");
    cancelBuildState();
    expect(buildState.phase).toBe("cancelled");
    expect(buildState.startedAt).toBeNull();
    expect(buildState.deployPhase).toBeNull();
  });

  it("reflects failed phase with correct error count when build fails", () => {
    startBuild("assembleDebug");
    const errLine: BuildLine = {
      kind: "error",
      content: "Unresolved reference: bar",
      file: "/src/Foo.kt",
      line: 5,
      col: 1,
    };
    addBuildLine(errLine);
    addBuildLine({ ...errLine, content: "Unresolved reference: baz" });
    addBuildLine({ ...errLine, content: "Unresolved reference: qux" });
    flushPendingLines();
    setBuildResult({ success: false, durationMs: 3000, errorCount: 3, warningCount: 0 });
    expect(buildState.phase).toBe("failed");
    expect(buildState.durationMs).toBe(3000);
    // Errors were accumulated via addBuildLine before setBuildResult was called.
    expect(buildState.errors).toHaveLength(3);
  });

  it("reflects failed phase immediately after setBuildResult with success: false", () => {
    startBuild("assembleRelease");
    setBuildResult({ success: false, durationMs: 1500, errorCount: 1, warningCount: 0 });
    expect(buildState.phase).toBe("failed");
    expect(buildState.currentTask).toBe("assembleRelease");
    expect(buildState.startedAt).toBeNull();
  });

  it("reflects failed phase with zero duration when build fails to start", () => {
    startBuild("assembleDebug");
    setBuildResult({ success: false, durationMs: 0, errorCount: 1, warningCount: 0 });
    expect(buildState.phase).toBe("failed");
    expect(buildState.durationMs).toBe(0);
    expect(buildState.startedAt).toBeNull();
    expect(buildState.currentTask).toBe("assembleDebug");
  });

  it("transitions from failed to running when a new build starts", () => {
    startBuild("assembleDebug");
    setBuildResult({ success: false, durationMs: 2000, errorCount: 1, warningCount: 0 });
    expect(buildState.phase).toBe("failed");
    // Start a second build — store should reset error list and go back to running.
    startBuild("assembleDebug");
    expect(buildState.phase).toBe("running");
    expect(buildState.errors).toHaveLength(0);
  });

  it("transitions from cancelled to running when a new build starts", () => {
    startBuild("assembleDebug");
    cancelBuildState();
    expect(buildState.phase).toBe("cancelled");
    startBuild("assembleDebug");
    expect(buildState.phase).toBe("running");
  });

  it("clearBuild after failure resets to idle and clears error list", () => {
    startBuild("assembleDebug");
    addBuildLine({
      kind: "error",
      content: "Type mismatch",
      file: "/src/Main.kt",
      line: 20,
      col: 3,
    });
    setBuildResult({ success: false, durationMs: 500, errorCount: 1, warningCount: 0 });
    expect(buildState.phase).toBe("failed");
    clearBuild();
    expect(buildState.phase).toBe("idle");
    expect(buildState.errors).toHaveLength(0);
    expect(buildLogStore.entries).toHaveLength(0);
  });
});

describe("build.store batching", () => {
  beforeEach(() => {
    resetBuildState();
  });

  it("addBuildLine does not immediately push to log store before flush", () => {
    const line: BuildLine = {
      kind: "output",
      content: "buffered line",
      file: null,
      line: null,
      col: null,
    };
    addBuildLine(line);
    // Before flush, nothing should be in the log store
    expect(buildLogStore.entries).toHaveLength(0);
  });

  it("flushPendingLines pushes all buffered lines at once", () => {
    const lines: BuildLine[] = [
      { kind: "output", content: "line 1", file: null, line: null, col: null },
      { kind: "output", content: "line 2", file: null, line: null, col: null },
      { kind: "output", content: "line 3", file: null, line: null, col: null },
    ];
    for (const l of lines) addBuildLine(l);
    expect(buildLogStore.entries).toHaveLength(0); // not yet flushed
    flushPendingLines();
    expect(buildLogStore.entries).toHaveLength(3);
  });

  it("flushPendingLines accumulates errors from batch", () => {
    addBuildLine({ kind: "error", content: "err A", file: null, line: null, col: null });
    addBuildLine({ kind: "error", content: "err B", file: "/src/Foo.kt", line: 5, col: 1 });
    addBuildLine({ kind: "warning", content: "warn C", file: null, line: null, col: null });
    flushPendingLines();
    expect(buildState.errors).toHaveLength(2);
    expect(buildState.warnings).toHaveLength(1);
    expect(buildState.errors[1].file).toBe("/src/Foo.kt");
  });

  it("flushPendingLines is idempotent — double flush does not double-push", () => {
    addBuildLine({ kind: "output", content: "once", file: null, line: null, col: null });
    flushPendingLines();
    flushPendingLines(); // second flush — buffer is empty
    expect(buildLogStore.entries).toHaveLength(1);
  });

  it("clearBuild drains pending buffer without pushing to log", () => {
    addBuildLine({ kind: "output", content: "unflushed", file: null, line: null, col: null });
    clearBuild(); // should discard pending lines
    flushPendingLines(); // flush after clear — buffer should be empty
    expect(buildLogStore.entries).toHaveLength(0);
  });
});

// ── lastLaunchedAt ────────────────────────────────────────────────────────────

describe("lastLaunchedAt", () => {
  beforeEach(() => { resetBuildState(); });

  it("is null on initial state", () => {
    expect(buildState.lastLaunchedAt).toBeNull();
  });

  it("setLastLaunchedAt stores the timestamp", () => {
    setLastLaunchedAt(1_000_000);
    expect(buildState.lastLaunchedAt).toBe(1_000_000);
  });

  it("clearBuild resets lastLaunchedAt to null", () => {
    setLastLaunchedAt(1_000_000);
    clearBuild();
    expect(buildState.lastLaunchedAt).toBeNull();
  });

  it("resetBuildState resets lastLaunchedAt to null", () => {
    setLastLaunchedAt(1_000_000);
    resetBuildState();
    expect(buildState.lastLaunchedAt).toBeNull();
  });

  it("successive deploys each update the timestamp", () => {
    setLastLaunchedAt(1_000_000);
    setLastLaunchedAt(2_000_000);
    expect(buildState.lastLaunchedAt).toBe(2_000_000);
  });
});
