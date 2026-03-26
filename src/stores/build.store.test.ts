import { describe, it, expect, beforeEach } from "vitest";
import {
  buildState,
  buildLogStore,
  startBuild,
  addBuildLine,
  setBuildResult,
  cancelBuildState,
  clearBuild,
  setBuildHistory,
  resetBuildState,
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
      },
    ]);
    expect(buildState.history).toHaveLength(1);
    expect(buildState.history[0].task).toBe("assembleDebug");
  });
});
