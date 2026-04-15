import { describe, it, expect } from "vitest";
import {
  isLogcatEntrySelectable,
  nextSelectableIndex,
  clampSelectionIndices,
} from "./logcat-selection-nav";

describe("isLogcatEntrySelectable", () => {
  it("treats normal rows as selectable", () => {
    expect(isLogcatEntrySelectable({ kind: "normal" })).toBe(true);
  });

  it("excludes process separator rows", () => {
    expect(isLogcatEntrySelectable({ kind: "processDied" })).toBe(false);
    expect(isLogcatEntrySelectable({ kind: "processStarted" })).toBe(false);
  });
});

describe("nextSelectableIndex", () => {
  const n = (i: number) => ({ kind: "normal" as const, i });
  const d = { kind: "processDied" as const };
  const s = { kind: "processStarted" as const };

  it("with null anchor, ArrowDown picks first selectable", () => {
    const entries = [d, n(1), s, n(2)];
    expect(nextSelectableIndex(entries, null, 1)).toBe(1);
  });

  it("with null anchor, ArrowUp picks last selectable", () => {
    const entries = [n(0), d, n(2)];
    expect(nextSelectableIndex(entries, null, -1)).toBe(2);
  });

  it("skips separators when moving down", () => {
    const entries = [n(0), d, n(2)];
    expect(nextSelectableIndex(entries, 0, 1)).toBe(2);
  });

  it("skips separators when moving up", () => {
    const entries = [n(0), s, n(2)];
    expect(nextSelectableIndex(entries, 2, -1)).toBe(0);
  });

  it("returns null at list end when moving down", () => {
    const entries = [n(0)];
    expect(nextSelectableIndex(entries, 0, 1)).toBeNull();
  });

  it("returns null at list start when moving up", () => {
    const entries = [n(0)];
    expect(nextSelectableIndex(entries, 0, -1)).toBeNull();
  });

  it("returns null when no selectable rows exist", () => {
    const entries = [d, s];
    expect(nextSelectableIndex(entries, null, 1)).toBeNull();
    expect(nextSelectableIndex(entries, null, -1)).toBeNull();
  });
});

describe("clampSelectionIndices", () => {
  it("returns nulls when entryCount is zero", () => {
    expect(clampSelectionIndices(5, 10, 0)).toEqual({ anchor: null, end: null });
  });

  it("returns nulls when anchor is null", () => {
    expect(clampSelectionIndices(null, 3, 10)).toEqual({ anchor: null, end: null });
  });

  it("clamps anchor and end into range", () => {
    expect(clampSelectionIndices(100, 100, 50)).toEqual({ anchor: 49, end: 49 });
    expect(clampSelectionIndices(0, 5, 10)).toEqual({ anchor: 0, end: 5 });
  });

  it("preserves null end", () => {
    expect(clampSelectionIndices(8, null, 10)).toEqual({ anchor: 8, end: null });
  });
});
