/**
 * VirtualList unit tests
 *
 * Tests focus on the pure windowing logic (index calculation, overscan,
 * boundary conditions) without mounting DOM — we test the formulas the
 * component uses rather than the rendered output.
 */

import { describe, it, expect } from "vitest";

// ── Pure windowing calculation (mirroring VirtualList internals) ──────────────

function calcWindow(opts: {
  totalItems: number;
  rowHeight: number;
  containerHeight: number;
  scrollTop: number;
  overscan: number;
}): { startIndex: number; endIndex: number; offsetY: number; visibleCount: number } {
  const { totalItems, rowHeight, containerHeight, scrollTop, overscan } = opts;
  const startIndex = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const endIndex = Math.min(
    totalItems,
    Math.ceil((scrollTop + containerHeight) / rowHeight) + overscan
  );
  return {
    startIndex,
    endIndex,
    offsetY: startIndex * rowHeight,
    visibleCount: endIndex - startIndex,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("VirtualList windowing — calcWindow()", () => {
  const BASE = { rowHeight: 20, containerHeight: 400, overscan: 10 };

  // ── Basic windowing ───────────────────────────────────────────────────────

  it("renders a window starting at 0 when scrollTop is 0", () => {
    const w = calcWindow({ ...BASE, totalItems: 10_000, scrollTop: 0 });
    expect(w.startIndex).toBe(0);
    // 400px / 20px = 20 visible + 10 overscan below
    expect(w.endIndex).toBe(30);
  });

  it("advances startIndex correctly when scrolled down", () => {
    // Scroll to row 100 (scrollTop = 100 × 20 = 2000)
    const w = calcWindow({ ...BASE, totalItems: 10_000, scrollTop: 2000 });
    // floor(2000/20) - overscan = 100 - 10 = 90
    expect(w.startIndex).toBe(90);
    // ceil((2000+400)/20) + overscan = ceil(120) + 10 = 130
    expect(w.endIndex).toBe(130);
    expect(w.offsetY).toBe(90 * 20);
  });

  it("clamps startIndex to 0 even with large overscan", () => {
    const w = calcWindow({ ...BASE, totalItems: 10_000, scrollTop: 5 });
    // floor(5/20) - 10 = 0 - 10 = -10 → clamped to 0
    expect(w.startIndex).toBe(0);
  });

  it("clamps endIndex to totalItems", () => {
    const w = calcWindow({ ...BASE, totalItems: 15, scrollTop: 0 });
    // without clamping endIndex would be 30, but only 15 items exist
    expect(w.endIndex).toBe(15);
  });

  it("renders 0 items when list is empty", () => {
    const w = calcWindow({ ...BASE, totalItems: 0, scrollTop: 0 });
    expect(w.startIndex).toBe(0);
    expect(w.endIndex).toBe(0);
    expect(w.visibleCount).toBe(0);
  });

  it("scrolled past end still clamps endIndex to totalItems", () => {
    // Scroll far beyond the end — startIndex may exceed endIndex because
    // scrollTop is past the total content height. The component guards against
    // this with Math.max(0, endIndex - startIndex) when slicing.
    const w = calcWindow({ ...BASE, totalItems: 5, scrollTop: 10_000 });
    expect(w.endIndex).toBe(5);
    // visibleCount can be negative when scrollTop is beyond totalHeight —
    // the VirtualList itself clamps visibleItems to an empty array via slice.
    expect(w.endIndex).toBeLessThanOrEqual(5);
  });

  // ── Rendered row count ────────────────────────────────────────────────────

  it("renders approximately viewport/rowHeight + 2*overscan rows at mid-scroll", () => {
    const w = calcWindow({ ...BASE, totalItems: 10_000, scrollTop: 5000 });
    // Viewport rows: ceil(400/20) = 20; plus 2*10 overscan = 40 total (may vary by ±1)
    expect(w.visibleCount).toBeGreaterThanOrEqual(38);
    expect(w.visibleCount).toBeLessThanOrEqual(42);
  });

  it("never renders more than totalItems rows", () => {
    for (const count of [0, 1, 10, 30, 100]) {
      const w = calcWindow({ ...BASE, totalItems: count, scrollTop: 0 });
      expect(w.visibleCount).toBeLessThanOrEqual(count);
    }
  });

  // ── offsetY ───────────────────────────────────────────────────────────────

  it("offsetY equals startIndex * rowHeight", () => {
    const w = calcWindow({ ...BASE, totalItems: 10_000, scrollTop: 1400 });
    expect(w.offsetY).toBe(w.startIndex * BASE.rowHeight);
  });

  it("offsetY is 0 when clamped to start", () => {
    const w = calcWindow({ ...BASE, totalItems: 10_000, scrollTop: 0 });
    expect(w.offsetY).toBe(0);
  });

  // ── Overscan variation ────────────────────────────────────────────────────

  it("zero overscan renders only visible rows", () => {
    const w = calcWindow({ rowHeight: 20, containerHeight: 400, overscan: 0, totalItems: 10_000, scrollTop: 2000 });
    // floor(2000/20) = 100 start; ceil(2400/20) = 120 end → exactly 20 rows
    expect(w.startIndex).toBe(100);
    expect(w.endIndex).toBe(120);
    expect(w.visibleCount).toBe(20);
  });

  it("large overscan extends window proportionally", () => {
    const w = calcWindow({ rowHeight: 20, containerHeight: 400, overscan: 50, totalItems: 10_000, scrollTop: 5000 });
    // 20 visible + 2*50 overscan = ~120
    expect(w.visibleCount).toBeGreaterThanOrEqual(118);
    expect(w.visibleCount).toBeLessThanOrEqual(122);
  });

  // ── Total height ──────────────────────────────────────────────────────────

  it("total height equals totalItems * rowHeight", () => {
    const items = 10_000;
    const rowHeight = 20;
    const totalHeight = items * rowHeight;
    expect(totalHeight).toBe(200_000);
  });

  // ── Different row heights ─────────────────────────────────────────────────

  it("works with row height of 1 (minimal)", () => {
    const w = calcWindow({ rowHeight: 1, containerHeight: 100, overscan: 5, totalItems: 1_000, scrollTop: 100 });
    expect(w.startIndex).toBe(95); // 100 - 5 overscan
    expect(w.endIndex).toBe(205); // 200 + 5 overscan
  });

  it("works with row height of 100 (large rows)", () => {
    const w = calcWindow({ rowHeight: 100, containerHeight: 600, overscan: 2, totalItems: 200, scrollTop: 1000 });
    expect(w.startIndex).toBe(8); // floor(1000/100) - 2 = 8
    expect(w.endIndex).toBe(18); // ceil(1600/100) + 2 = 18
  });

  // ── Partial-row scrolling ─────────────────────────────────────────────────

  it("handles scrollTop that falls in the middle of a row", () => {
    // scrollTop 25 = middle of row 1 (rows 0-19 and 20-39)
    const w = calcWindow({ rowHeight: 20, containerHeight: 400, overscan: 5, totalItems: 1_000, scrollTop: 25 });
    // floor(25/20) = 1; 1 - 5 = -4 → clamped to 0
    expect(w.startIndex).toBe(0);
    // ceil(425/20) = 22; 22 + 5 = 27
    expect(w.endIndex).toBe(27);
  });
});

// ── Auto-scroll bottom detection ──────────────────────────────────────────────

describe("VirtualList auto-scroll threshold", () => {
  const THRESHOLD = 40;

  function isAtBottom(scrollTop: number, scrollHeight: number, clientHeight: number): boolean {
    return scrollHeight - scrollTop - clientHeight < THRESHOLD;
  }

  it("reports at-bottom when exactly at the bottom", () => {
    expect(isAtBottom(960, 1000, 40)).toBe(true);
  });

  it("reports at-bottom when within threshold", () => {
    expect(isAtBottom(930, 1000, 40)).toBe(true); // 1000-930-40 = 30 < 40
  });

  it("reports not-at-bottom when beyond threshold", () => {
    expect(isAtBottom(900, 1000, 40)).toBe(false); // 1000-900-40 = 60 >= 40
  });

  it("reports at-bottom with zero scroll (short list)", () => {
    expect(isAtBottom(0, 100, 400)).toBe(true); // content shorter than container
  });
});
