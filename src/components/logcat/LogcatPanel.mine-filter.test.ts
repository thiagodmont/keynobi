/**
 * Regression tests for the package:mine filter startup race.
 *
 * Bug: LogcatPanel.onMount restores a saved `package:mine` query and calls
 * syncBackendFilter before doOpenProject() has resolved getApplicationId().
 * Because _minePackage was null at that point, no package filter was sent to
 * the backend. The createEffect guard (_prevDebouncedQuery) then prevented
 * re-running for the same query string even after setMinePackage() was called
 * later, leaving the filter permanently broken until the user edited the query.
 *
 * Fix: LogcatPanel registers a createEffect that subscribes to the reactive
 * projectState.applicationId. When it changes (null → real value on startup,
 * or old → new on project switch), setMinePackage is updated and, if the
 * current query contains package:mine, syncBackendFilter is re-triggered.
 *
 * These tests verify the reactive mechanism in isolation using the same
 * createRoot / createEffect pattern as BuildPanel.test.ts.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { createRoot, createSignal, createEffect } from "solid-js";
import { projectState, setApplicationId, setProjectState } from "@/stores/project.store";
import { setMinePackage, getMinePackage } from "@/lib/logcat-query";

function resetState() {
  setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, applicationId: null, loading: false });
  setMinePackage(null);
}

describe("package:mine re-sync on applicationId change", () => {
  beforeEach(resetState);

  it("updates minePackage when applicationId resolves from null", async () => {
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
      });
    });

    await Promise.resolve(); // flush initial run (null → null, no-op)
    expect(getMinePackage()).toBeNull();

    // Simulate doOpenProject() finishing getApplicationId()
    setApplicationId("com.example.app");
    await Promise.resolve();

    expect(getMinePackage()).toBe("com.example.app");

    disposeRoot();
  });

  it("triggers re-sync when applicationId resolves and query contains package:mine", async () => {
    let syncCount = 0;
    const [debouncedQuery] = createSignal("package:mine");
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
        const q = debouncedQuery();
        if (q.includes("package:mine") || q.includes("pkg:mine")) {
          syncCount++;
        }
      });
    });

    await Promise.resolve(); // initial flush — appId null, query has package:mine → sync #1
    const afterMount = syncCount;

    // ApplicationId resolves (startup race resolved)
    setApplicationId("com.example.app");
    await Promise.resolve();

    // Effect must have fired again and triggered another sync
    expect(syncCount).toBeGreaterThan(afterMount);
    expect(getMinePackage()).toBe("com.example.app");

    disposeRoot();
  });

  it("does not trigger re-sync when applicationId resolves but query has no package:mine", async () => {
    let syncCount = 0;
    const [debouncedQuery] = createSignal("level:error tag:App");
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
        const q = debouncedQuery();
        if (q.includes("package:mine") || q.includes("pkg:mine")) {
          syncCount++;
        }
      });
    });

    await Promise.resolve();
    const afterMount = syncCount;

    setApplicationId("com.example.app");
    await Promise.resolve();

    // Effect fires (appId changed) but query has no package:mine → no sync
    expect(syncCount).toBe(afterMount);

    disposeRoot();
  });

  it("recognises the pkg: alias in addition to package:", async () => {
    let syncCount = 0;
    const [debouncedQuery] = createSignal("pkg:mine level:error");
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
        const q = debouncedQuery();
        if (q.includes("package:mine") || q.includes("pkg:mine")) {
          syncCount++;
        }
      });
    });

    await Promise.resolve();
    const afterMount = syncCount;

    setApplicationId("com.example.app");
    await Promise.resolve();

    expect(syncCount).toBeGreaterThan(afterMount);

    disposeRoot();
  });

  it("does not re-sync when the same applicationId is set again", async () => {
    setApplicationId("com.example.app");

    let syncCount = 0;
    const [debouncedQuery] = createSignal("package:mine");
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
        const q = debouncedQuery();
        if (q.includes("package:mine") || q.includes("pkg:mine")) {
          syncCount++;
        }
      });
    });

    await Promise.resolve();
    const afterMount = syncCount;

    // Set the same id again — should not re-trigger
    setApplicationId("com.example.app");
    await Promise.resolve();

    expect(syncCount).toBe(afterMount);

    disposeRoot();
  });

  it("updates minePackage correctly on project switch", async () => {
    setApplicationId("com.project-a.app");
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
      });
    });

    await Promise.resolve();
    expect(getMinePackage()).toBe("com.project-a.app");

    // User switches to project B
    setApplicationId("com.project-b.app");
    await Promise.resolve();

    expect(getMinePackage()).toBe("com.project-b.app");

    disposeRoot();
  });

  it("triggers re-sync on project switch when query contains package:mine", async () => {
    setApplicationId("com.project-a.app");
    let syncCount = 0;
    const [debouncedQuery] = createSignal("package:mine");
    let disposeRoot!: () => void;

    createRoot((dispose) => {
      disposeRoot = dispose;
      let _prevAppId: string | null | undefined = undefined;
      createEffect(() => {
        const appId = projectState.applicationId;
        if (appId === _prevAppId) return;
        _prevAppId = appId;
        setMinePackage(appId);
        const q = debouncedQuery();
        if (q.includes("package:mine") || q.includes("pkg:mine")) {
          syncCount++;
        }
      });
    });

    await Promise.resolve();
    const afterMount = syncCount;

    setApplicationId("com.project-b.app");
    await Promise.resolve();

    expect(syncCount).toBeGreaterThan(afterMount);
    expect(getMinePackage()).toBe("com.project-b.app");

    // Verify the filter now resolves to the new project's package
    const { parseQuery, matchesQuery } = await import("@/lib/logcat-query");
    const tokens = parseQuery("package:mine");
    const NOW = Date.now();
    expect(matchesQuery(
      { id: 1n, timestamp: "01-23 12:34:56.789", pid: 1, tid: 1, level: "debug", tag: "T", message: "", isCrash: false, package: "com.project-b.app", kind: "normal", flags: 0, category: "general", crashGroupId: null, jsonBody: null },
      tokens, NOW
    )).toBe(true);
    expect(matchesQuery(
      { id: 2n, timestamp: "01-23 12:34:56.789", pid: 1, tid: 1, level: "debug", tag: "T", message: "", isCrash: false, package: "com.project-a.app", kind: "normal", flags: 0, category: "general", crashGroupId: null, jsonBody: null },
      tokens, NOW
    )).toBe(false);

    disposeRoot();
  });
});
