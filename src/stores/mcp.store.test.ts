import { describe, it, expect, beforeEach, vi } from "vitest";
import { waitFor } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { initMcpListeners, resetMcpListenersForTests } from "@/stores/mcp.store";

const mockListen = vi.mocked(listen);

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("mcp.store listener lifecycle", () => {
  beforeEach(() => {
    resetMcpListenersForTests();
    vi.clearAllMocks();
    mockListen.mockResolvedValue(() => {});
  });

  it("registers lifecycle listeners only once", () => {
    initMcpListeners();
    initMcpListeners();

    expect(mockListen).toHaveBeenCalledTimes(3);
  });

  it("disposes lifecycle listeners", async () => {
    const unlisteners: Array<() => void> = [
      vi.fn<() => void>(),
      vi.fn<() => void>(),
      vi.fn<() => void>(),
    ];
    const pending = [...unlisteners];
    mockListen.mockImplementation(async () => pending.shift() ?? vi.fn<() => void>());

    initMcpListeners();
    await waitFor(() => expect(pending).toHaveLength(0));

    resetMcpListenersForTests();

    expect(mockListen).toHaveBeenCalledTimes(3);
    expect(unlisteners[0]).toHaveBeenCalledTimes(1);
    expect(unlisteners[1]).toHaveBeenCalledTimes(1);
    expect(unlisteners[2]).toHaveBeenCalledTimes(1);
  });

  it("disposes listeners that resolve after reset", async () => {
    const unlisten = vi.fn<() => void>();
    const listener = deferred<() => void>();
    mockListen.mockReturnValue(listener.promise);

    initMcpListeners();
    expect(mockListen).toHaveBeenCalledTimes(3);

    resetMcpListenersForTests();
    expect(unlisten).not.toHaveBeenCalled();

    listener.resolve(unlisten);

    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(3));
  });
});
