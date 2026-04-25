import { afterEach, describe, expect, it, vi } from "vitest";
import { debounce } from "./debounce";

describe("debounce", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("cancels a pending invocation", () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const debounced = debounce(fn, 150);

    debounced("stale");
    debounced.cancel();
    vi.advanceTimersByTime(150);

    expect(fn).not.toHaveBeenCalled();
  });
});
