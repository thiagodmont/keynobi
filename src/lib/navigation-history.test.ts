import { describe, it, expect, beforeEach } from "vitest";
import {
  pushNavigation,
  navigateBack,
  navigateForward,
  canGoBack,
  canGoForward,
  getHistory,
  clearHistory,
} from "./navigation-history";

describe("navigation-history", () => {
  beforeEach(() => {
    clearHistory();
  });

  it("starts empty", () => {
    expect(getHistory()).toHaveLength(0);
    expect(canGoBack()).toBe(false);
    expect(canGoForward()).toBe(false);
  });

  it("pushes entries", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/b.kt", line: 5, col: 3 });
    expect(getHistory()).toHaveLength(2);
  });

  it("navigates back", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/b.kt", line: 5, col: 3 });

    expect(canGoBack()).toBe(true);
    const entry = navigateBack();
    expect(entry?.path).toBe("/a.kt");
    expect(entry?.line).toBe(1);
  });

  it("navigates forward", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/b.kt", line: 5, col: 3 });

    navigateBack();
    expect(canGoForward()).toBe(true);

    const entry = navigateForward();
    expect(entry?.path).toBe("/b.kt");
  });

  it("returns null when no back history", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    expect(navigateBack()).toBeNull();
  });

  it("returns null when no forward history", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    expect(navigateForward()).toBeNull();
  });

  it("truncates forward history on new push", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/b.kt", line: 2, col: 0 });
    pushNavigation({ path: "/c.kt", line: 3, col: 0 });

    navigateBack();
    navigateBack();
    pushNavigation({ path: "/d.kt", line: 4, col: 0 });

    expect(getHistory()).toHaveLength(2);
    expect(canGoForward()).toBe(false);
  });

  it("deduplicates consecutive identical entries", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    expect(getHistory()).toHaveLength(1);
  });

  it("limits history to max entries", () => {
    for (let i = 0; i < 60; i++) {
      pushNavigation({ path: `/file${i}.kt`, line: i, col: 0 });
    }
    expect(getHistory().length).toBeLessThanOrEqual(50);
  });

  it("navigates back through remaining history after truncation", () => {
    for (let i = 0; i < 60; i++) {
      pushNavigation({ path: `/file${i}.kt`, line: i, col: 0 });
    }
    const back = navigateBack();
    expect(back).not.toBeNull();
    expect(back!.path).toMatch(/^\/file\d+\.kt$/);
  });

  it("allows different lines in same file", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/a.kt", line: 50, col: 0 });
    expect(getHistory()).toHaveLength(2);
  });

  it("clears history", () => {
    pushNavigation({ path: "/a.kt", line: 1, col: 0 });
    pushNavigation({ path: "/b.kt", line: 2, col: 0 });
    clearHistory();
    expect(getHistory()).toHaveLength(0);
    expect(canGoBack()).toBe(false);
  });
});
