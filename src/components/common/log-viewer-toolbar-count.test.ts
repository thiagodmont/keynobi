import { describe, it, expect } from "vitest";
import { formatBuildLogToolbarCount } from "./log-viewer-toolbar-count";

describe("formatBuildLogToolbarCount", () => {
  it("no filters: lines", () => {
    expect(
      formatBuildLogToolbarCount({ filterActive: false, visible: 10, total: 10 }).text
    ).toBe("10 lines");
  });

  it("filters with subset: slash", () => {
    const r = formatBuildLogToolbarCount({ filterActive: true, visible: 3, total: 100 });
    expect(r.text).toBe("3 / 100");
  });

  it("filters all match: single total", () => {
    const r = formatBuildLogToolbarCount({ filterActive: true, visible: 50, total: 50 });
    expect(r.text).toBe("50 lines");
  });
});
