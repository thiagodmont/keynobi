import { describe, it, expect } from "vitest";
import {
  clampLogcatMaxUiLines,
  clampLogcatRingMaxEntries,
  LOGCAT_DEFAULT_UI_LINES,
  LOGCAT_MIN_UI_LINES,
  LOGCAT_RING_ABS_MAX,
  LOGCAT_RING_DEFAULT,
  LOGCAT_RING_MIN,
} from "./logcat-ui-lines";

describe("clampLogcatRingMaxEntries", () => {
  it("clamps below minimum to LOGCAT_RING_MIN", () => {
    expect(clampLogcatRingMaxEntries(0)).toBe(LOGCAT_RING_MIN);
    expect(clampLogcatRingMaxEntries(500)).toBe(LOGCAT_RING_MIN);
    expect(clampLogcatRingMaxEntries(LOGCAT_RING_MIN - 1)).toBe(LOGCAT_RING_MIN);
  });

  it("clamps above maximum to LOGCAT_RING_ABS_MAX", () => {
    expect(clampLogcatRingMaxEntries(LOGCAT_RING_ABS_MAX + 1)).toBe(LOGCAT_RING_ABS_MAX);
    expect(clampLogcatRingMaxEntries(9e15)).toBe(LOGCAT_RING_ABS_MAX);
  });

  it("floors fractional values and accepts in-range integers", () => {
    expect(clampLogcatRingMaxEntries(12_345.7)).toBe(12_345);
    expect(clampLogcatRingMaxEntries(LOGCAT_RING_DEFAULT)).toBe(LOGCAT_RING_DEFAULT);
  });

  it("uses default ring for non-finite input", () => {
    expect(clampLogcatRingMaxEntries(Number.NaN)).toBe(LOGCAT_RING_DEFAULT);
    expect(clampLogcatRingMaxEntries(Number.POSITIVE_INFINITY)).toBe(LOGCAT_RING_DEFAULT);
  });
});

describe("clampLogcatMaxUiLines", () => {
  it("never exceeds the (clamped) ring cap", () => {
    expect(clampLogcatMaxUiLines(50_000, 10_000)).toBe(10_000);
    expect(clampLogcatMaxUiLines(99_000, 5_000)).toBe(5_000);
  });

  it("respects LOGCAT_MIN_UI_LINES and default for bad UI input", () => {
    expect(clampLogcatMaxUiLines(100, 50_000)).toBe(LOGCAT_MIN_UI_LINES);
    expect(clampLogcatMaxUiLines(Number.NaN, 50_000)).toBe(LOGCAT_DEFAULT_UI_LINES);
  });

  it("uses clamped ring when ringCap is out of range", () => {
    expect(clampLogcatMaxUiLines(50_000, 500)).toBe(LOGCAT_RING_MIN);
  });
});
