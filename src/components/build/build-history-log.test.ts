import { afterEach, describe, expect, it, vi } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { createHistoricalLog } from "./build-history-log";
import { makeBuildLine } from "@/test/factories/build";

type Deferred = { resolve: (v: unknown) => void; reject: (e: unknown) => void };

/** Each get_build_log_entries call waits until the test settles it. */
function deferLogCalls(): Map<number, Deferred> {
  const pending = new Map<number, Deferred>();
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd !== "get_build_log_entries") return Promise.resolve(undefined);
    return new Promise((resolve, reject) => {
      pending.set((args as { id: number }).id, { resolve, reject });
    });
  });
  return pending;
}

async function settle(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

function mount(initial: number | null) {
  return createRoot((dispose) => {
    const [id, setId] = createSignal<number | null>(initial);
    const log = createHistoricalLog(id);
    return { ...log, setId, dispose };
  });
}

describe("createHistoricalLog", () => {
  afterEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  it("has nothing to show for the live build", () => {
    const log = mount(null);
    expect(log.state()).toEqual({ status: "none" });
    log.dispose();
  });

  it("is loading until the log arrives, then shows it", async () => {
    const pending = deferLogCalls();
    const log = mount(4);
    expect(log.state().status).toBe("loading");

    pending.get(4)!.resolve([makeBuildLine({ content: "> Task :app:lint" })]);
    await settle();

    const state = log.state();
    expect(state.status).toBe("loaded");
    expect(state.status === "loaded" && state.entries.map((e) => e.message)).toEqual([
      "> Task :app:lint",
    ]);
    log.dispose();
  });

  it("an empty log is loaded, not missing or expired", async () => {
    const pending = deferLogCalls();
    const log = mount(4);
    pending.get(4)!.resolve([]);
    await settle();
    expect(log.state()).toEqual({ status: "loaded", entries: [] });
    log.dispose();
  });

  it("a log removed by rotation is expired, not a failure", async () => {
    const pending = deferLogCalls();
    const log = mount(4);
    pending
      .get(4)!
      .reject({ kind: "notFound", message: "The log of build #4 is no longer on disk" });
    await settle();
    expect(log.state()).toEqual({ status: "expired" });
    log.dispose();
  });

  it("any other error is a failure with its message, and Retry loads again", async () => {
    const pending = deferLogCalls();
    const log = mount(4);
    pending.get(4)!.reject({ kind: "io", message: "permission denied" });
    await settle();
    expect(log.state()).toEqual({ status: "failed", message: "io: permission denied" });

    log.retry();
    expect(log.state().status).toBe("loading");
    pending.get(4)!.resolve([makeBuildLine()]);
    await settle();
    expect(log.state().status).toBe("loaded");
    log.dispose();
  });

  it("drops a slow response for a build that is no longer selected", async () => {
    const pending = deferLogCalls();
    const log = mount(1);
    log.setId(2);
    pending.get(2)!.resolve([makeBuildLine({ content: "second" })]);
    await settle();
    pending.get(1)!.resolve([makeBuildLine({ content: "first" })]);
    await settle();

    const state = log.state();
    expect(state.status === "loaded" && state.entries.map((e) => e.message)).toEqual(["second"]);
    log.dispose();
  });
});
