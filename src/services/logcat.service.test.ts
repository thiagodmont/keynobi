import { describe, expect, it } from "vitest";
import { createLatestOnlyGuard } from "./logcat.service";

describe("createLatestOnlyGuard", () => {
  it("marks only the newest request token as current", () => {
    const guard = createLatestOnlyGuard();

    const first = guard.begin();
    const second = guard.begin();

    expect(guard.isLatest(first)).toBe(false);
    expect(guard.isLatest(second)).toBe(true);
  });

  it("can invalidate in-flight work without starting a replacement", () => {
    const guard = createLatestOnlyGuard();

    const token = guard.begin();
    guard.invalidate();

    expect(guard.isLatest(token)).toBe(false);
  });
});
