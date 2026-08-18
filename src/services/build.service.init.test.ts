import { describe, it, expect, beforeEach, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { initBuildService, resetBuildServiceForTests } from "@/services/build.service";

const mockListen = vi.mocked(listen);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("initBuildService listener lifecycle", () => {
  beforeEach(() => {
    resetBuildServiceForTests();
    vi.clearAllMocks();
    mockListen.mockResolvedValue(() => {});
  });

  // Regression: the `if (buildCompleteUnlisten) return` guard was checked
  // BEFORE the await, so two calls that interleave before the first listen()
  // resolved both registered — producing double setBuildResult and a double
  // history fetch on every build. doOpenProject calls initBuildService
  // fire-and-forget on every project open, so this was reachable.
  it("registers exactly one listener when called twice concurrently", async () => {
    const gate = deferred<() => void>();
    mockListen.mockReturnValue(gate.promise);

    const first = initBuildService();
    const second = initBuildService();

    gate.resolve(() => {});
    await Promise.all([first, second]);

    expect(mockListen).toHaveBeenCalledTimes(1);
  });

  it("registers exactly one listener when called twice sequentially", async () => {
    await initBuildService();
    await initBuildService();

    expect(mockListen).toHaveBeenCalledTimes(1);
  });

  it("allows a retry after a failed registration", async () => {
    mockListen.mockRejectedValueOnce(new Error("ipc down"));
    await expect(initBuildService()).rejects.toThrow("ipc down");

    mockListen.mockResolvedValue(() => {});
    await initBuildService();

    expect(mockListen).toHaveBeenCalledTimes(2);
  });

  it("disposes the listener on reset", async () => {
    const unlisten = vi.fn();
    mockListen.mockResolvedValue(unlisten);

    await initBuildService();
    resetBuildServiceForTests();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
