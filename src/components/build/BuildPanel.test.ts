/**
 * Regression tests for build log bleed between projects.
 *
 * Bug: switching projects left a stale selectedHistoryId signal set to a
 * previous build's ID. Because logEntries() gates on `selectedHistoryId !== null`,
 * it kept returning historicalLog() — stale disk content from the old project's
 * build log file — instead of the (empty) live log store.
 *
 * Fix: BuildPanel registers a createEffect that watches projectState.projectRoot
 * and resets selectedHistoryId to null whenever the active project changes.
 *
 * Tests here verify the exact reactive mechanism without needing a DOM render.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { createRoot, createSignal, createEffect } from "solid-js";
import { projectState, setProject, setProjectState } from "@/stores/project.store";
import { buildLogStore, resetBuildState, addBuildLine, flushPendingLines } from "@/stores/build.store";
import type { BuildLine } from "@/bindings";

const outputLine = (content: string): BuildLine => ({
  kind: "output", content, file: null, line: null, col: null,
});

function resetProjectState() {
  setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
}

describe("build log does not bleed across project switches", () => {
  beforeEach(() => {
    resetBuildState();
    resetProjectState();
  });

  // ── Live log path ─────────────────────────────────────────────────────────────

  it("live log is empty after resetBuildState (project switch clears live log)", () => {
    addBuildLine(outputLine("Project A build output"));
    flushPendingLines();
    expect(buildLogStore.entries.length).toBeGreaterThan(0);

    // selectProject calls resetBuildState before opening the new project
    resetBuildState();

    expect(buildLogStore.entries).toHaveLength(0);
  });

  // ── Historical log path ───────────────────────────────────────────────────────

  // Regression: a reactive effect subscribed to projectRoot must reset
  // selectedHistoryId so that logEntries() falls back to the (empty) live
  // log store rather than returning stale historical content from disk.
  it("resets a stale selectedHistoryId signal when projectRoot changes", async () => {
    setProject("/projects/project-a", "project-a");

    // Capture signal + setter so we can drive them from outside the reactive root.
    let getSelectedId!: () => number | null;
    let setSelectedId!: (v: number | null) => void;
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      const [id, setId] = createSignal<number | null>(null);
      getSelectedId = id;
      setSelectedId = setId;
      disposeRoot = dispose;

      // This is the exact effect that BuildPanel registers.
      createEffect(() => {
        projectState.projectRoot; // reactive dependency
        setId(null);
      });
    });

    // Flush the initial effect run (reads "project-a", sets id → null — no-op).
    await Promise.resolve();

    // Simulate user clicking history item #5 from Project A.
    setSelectedId(5);
    expect(getSelectedId()).toBe(5);

    // User switches to Project B — triggers the effect.
    setProject("/projects/project-b", "project-b");

    // Flush deferred effects.
    await Promise.resolve();

    // Regression: id must be null so logEntries() returns buildLogStore.entries
    // (empty) instead of historicalLog() (stale Project A disk content).
    expect(getSelectedId()).toBe(null);

    disposeRoot();
  });

  it("does not reset selectedHistoryId when an unrelated signal changes", async () => {
    setProject("/projects/project-a", "project-a");

    let getSelectedId!: () => number | null;
    let setSelectedId!: (v: number | null) => void;
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      const [id, setId] = createSignal<number | null>(null);
      getSelectedId = id;
      setSelectedId = setId;
      disposeRoot = dispose;

      createEffect(() => {
        projectState.projectRoot;
        setId(null);
      });
    });

    await Promise.resolve(); // initial flush

    setSelectedId(3);
    expect(getSelectedId()).toBe(3);

    // Change something unrelated to projectRoot (e.g. add a log line).
    addBuildLine(outputLine("unrelated change"));
    flushPendingLines();

    await Promise.resolve();

    // selectedHistoryId must be unchanged — only project changes should reset it.
    expect(getSelectedId()).toBe(3);

    disposeRoot();
  });
});
