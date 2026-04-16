import { describe, expect, it } from "vitest";
import { effectiveLogcatFollowTail } from "./logcat-follow-tail";

describe("effectiveLogcatFollowTail", () => {
  it("is true only when follow is on and nothing blocks", () => {
    expect(
      effectiveLogcatFollowTail({
        autoScroll: true,
        selectionAnchor: null,
        selectedJsonEntry: null,
      })
    ).toBe(true);
  });

  it("is false when autoScroll is off", () => {
    expect(
      effectiveLogcatFollowTail({
        autoScroll: false,
        selectionAnchor: null,
        selectedJsonEntry: null,
      })
    ).toBe(false);
  });

  it("is false when a row is selected", () => {
    expect(
      effectiveLogcatFollowTail({
        autoScroll: true,
        selectionAnchor: 3,
        selectedJsonEntry: null,
      })
    ).toBe(false);
  });

  it("is false when JSON detail is open", () => {
    expect(
      effectiveLogcatFollowTail({
        autoScroll: true,
        selectionAnchor: null,
        selectedJsonEntry: { id: "x" },
      })
    ).toBe(false);
  });
});
