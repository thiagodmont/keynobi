import { describe, it, expect } from "vitest";
import { formatLogcatToolbarCount } from "./logcat-toolbar-count";

describe("formatLogcatToolbarCount", () => {
  it("without ring stats shows lines only", () => {
    const r = formatLogcatToolbarCount({
      queryActive: true,
      visible: 50,
      ringTotal: null,
    });
    expect(r.text).toBe("50 lines");
    expect(r.title).toContain("unavailable");
  });

  it("no query and visible equals ring shows single count", () => {
    const r = formatLogcatToolbarCount({
      queryActive: false,
      visible: 1000,
      ringTotal: 1000,
    });
    expect(r.text).toBe("1,000 lines");
  });

  it("query active with ring shows slash visible first", () => {
    const r = formatLogcatToolbarCount({
      queryActive: true,
      visible: 50,
      ringTotal: 1000,
    });
    expect(r.text).toBe("50 / 1,000");
    expect(r.title).toContain("First");
  });

  it("no query but FE list smaller than ring shows slash", () => {
    const r = formatLogcatToolbarCount({
      queryActive: false,
      visible: 2000,
      ringTotal: 8000,
    });
    expect(r.text).toBe("2,000 / 8,000");
  });

  it("query active equal counts shows slash", () => {
    const r = formatLogcatToolbarCount({
      queryActive: true,
      visible: 100,
      ringTotal: 100,
    });
    expect(r.text).toBe("100 / 100");
  });
});
