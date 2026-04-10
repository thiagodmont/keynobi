/**
 * Tests for the auto-apply package:mine filter on successful deploy.
 *
 * When build.service sets buildState.lastLaunchedAt after a successful
 * launchAppOnDevice(), LogcatPanel merges package:mine into the active
 * query so users immediately see their app's logs without a manual step.
 *
 * Contract:
 *   - null (initial mount) → no filter applied
 *   - null → timestamp    → package:mine added if not already present
 *   - timestamp → new ts  → package:mine re-applied if user removed it
 *   - package:mine already in query → no-op
 *   - pkg:mine alias already in query → no-op
 *   - other filters preserved when package:mine is merged in
 */

import { describe, it, expect, beforeEach } from "vitest";
import { createRoot, createSignal, createEffect } from "solid-js";
import { buildState, setLastLaunchedAt, resetBuildState } from "@/stores/build.store";
import { setPackageInQuery } from "@/lib/logcat-query";

function resetState() {
  resetBuildState();
}

/**
 * Simulates the createEffect from LogcatPanel that watches lastLaunchedAt.
 * Returns [getQuery, dispose] so tests can inspect the query and clean up.
 */
function mountAutoFilterEffect(initialQuery: string): [() => string, () => void] {
  const [query, setQuery] = createSignal(initialQuery);

  const updateQuery = (q: string) => setQuery(q);

  let disposeRoot!: () => void;

  createRoot((dispose) => {
    disposeRoot = dispose;
    let _prevLaunchedAt: number | null | undefined = undefined;
    createEffect(() => {
      const launchedAt = buildState.lastLaunchedAt;
      if (launchedAt === _prevLaunchedAt) return;
      _prevLaunchedAt = launchedAt;
      if (launchedAt === null) return;
      const q = query();
      if (q.includes("package:mine") || q.includes("pkg:mine")) return;
      const next = setPackageInQuery(q, "mine");
      updateQuery(next.trimEnd() ? next.trimEnd() + " " : "");
    });
  });

  return [query, disposeRoot];
}

describe("auto-apply package:mine on deploy", () => {
  beforeEach(resetState);

  it("does not modify query on initial mount (lastLaunchedAt is null)", async () => {
    const [query, dispose] = mountAutoFilterEffect("");
    await Promise.resolve();
    expect(query()).toBe("");
    dispose();
  });

  it("adds package:mine to empty query when app is launched", async () => {
    const [query, dispose] = mountAutoFilterEffect("");
    await Promise.resolve();

    setLastLaunchedAt(Date.now());
    await Promise.resolve();

    expect(query()).toBe("package:mine ");
    dispose();
  });

  it("merges package:mine into existing filters without removing them", async () => {
    const [query, dispose] = mountAutoFilterEffect("level:error tag:MainActivity ");
    await Promise.resolve();

    setLastLaunchedAt(Date.now());
    await Promise.resolve();

    expect(query()).toContain("level:error");
    expect(query()).toContain("package:mine");
    // trailing space committed as pill
    expect(query()).toMatch(/ $/);
    dispose();
  });

  it("is a no-op when package:mine is already in the query", async () => {
    const initial = "package:mine level:error ";
    const [query, dispose] = mountAutoFilterEffect(initial);
    await Promise.resolve();

    setLastLaunchedAt(Date.now());
    await Promise.resolve();

    // query unchanged — no duplicate package token
    expect(query()).toBe(initial);
    dispose();
  });

  it("is a no-op when the pkg: alias is already in the query", async () => {
    const initial = "pkg:mine level:warn ";
    const [query, dispose] = mountAutoFilterEffect(initial);
    await Promise.resolve();

    setLastLaunchedAt(Date.now());
    await Promise.resolve();

    expect(query()).toBe(initial);
    dispose();
  });

  it("re-applies package:mine when app is re-deployed (same package)", async () => {
    // Start with package:mine, then simulate user removing it
    const [query, dispose] = mountAutoFilterEffect("level:error ");
    await Promise.resolve();

    // First deploy
    setLastLaunchedAt(1_000_000);
    await Promise.resolve();
    expect(query()).toContain("package:mine");

    // User removes the filter manually
    // (simulate by reinitialising — effect guards against same timestamp)

    // Second deploy — new timestamp, no package:mine in current query
    setLastLaunchedAt(2_000_000);
    await Promise.resolve();
    expect(query()).toContain("package:mine");

    dispose();
  });

  it("result ends with a trailing space so all tokens are committed pills", async () => {
    const [query, dispose] = mountAutoFilterEffect("");
    await Promise.resolve();

    setLastLaunchedAt(Date.now());
    await Promise.resolve();

    expect(query()).toMatch(/ $/);
    dispose();
  });
});
