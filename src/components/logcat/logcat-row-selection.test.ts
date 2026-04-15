import { describe, it, expect } from "vitest";
import { rowInSelectionRange, rowFocusMarked } from "./logcat-row-selection";
import type { LogcatEntry } from "@/lib/tauri-api";

function mockEntry(id: bigint): LogcatEntry {
  return {
    id,
    timestamp: "12:00:00.000",
    pid: 1,
    tid: 1,
    level: "info",
    tag: "t",
    message: "m",
    package: null,
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "general",
    crashGroupId: null,
    jsonBody: null,
  };
}

describe("rowInSelectionRange", () => {
  it("returns false when range is null", () => {
    expect(rowInSelectionRange(3, null)).toBe(false);
  });

  it("returns true when index is inside inclusive range", () => {
    expect(rowInSelectionRange(2, [1, 5])).toBe(true);
    expect(rowInSelectionRange(1, [1, 5])).toBe(true);
    expect(rowInSelectionRange(5, [1, 5])).toBe(true);
  });

  it("returns false when index is outside range", () => {
    expect(rowInSelectionRange(0, [1, 5])).toBe(false);
    expect(rowInSelectionRange(6, [1, 5])).toBe(false);
  });
});

describe("rowFocusMarked", () => {
  const id1 = 1n;
  const id2 = 2n;
  const e1 = mockEntry(id1);

  it("marks row matching detail entry", () => {
    expect(rowFocusMarked(0, 0, null, e1, id1)).toBe(true);
    expect(rowFocusMarked(1, 0, null, e1, id2)).toBe(false);
  });

  it("with no detail, marks anchor when end is null", () => {
    expect(rowFocusMarked(3, 3, null, null, id1)).toBe(true);
    expect(rowFocusMarked(2, 3, null, null, id1)).toBe(false);
  });

  it("with shift range and no detail, marks anchor only", () => {
    expect(rowFocusMarked(2, 2, 5, null, id1)).toBe(true);
    expect(rowFocusMarked(3, 2, 5, null, id1)).toBe(false);
    expect(rowFocusMarked(5, 2, 5, null, id1)).toBe(false);
  });

  it("returns false when anchor is null", () => {
    expect(rowFocusMarked(0, null, null, null, id1)).toBe(false);
  });
});
