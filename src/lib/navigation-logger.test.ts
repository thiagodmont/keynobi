import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { lspLogStore } from "@/stores/log.store";

// Import navLog fresh in each test to avoid module-state bleed across suites.
// The module caches _clientId; resetting is not needed since we only check
// relative behaviour (each call increments the id).

describe("navLog", () => {
  const invokeStub = invoke as ReturnType<typeof vi.fn>;

  beforeEach(() => {
    lspLogStore.clearEntries();
    invokeStub.mockClear();
  });

  it("pushes an entry to lspLogStore with source lsp:navigate", async () => {
    const { navLog } = await import("./navigation-logger");
    navLog("info", "test navigation message");

    const entries = lspLogStore.entries;
    expect(entries).toHaveLength(1);
    expect(entries[0].source).toBe("lsp:navigate");
    expect(entries[0].message).toBe("test navigation message");
    expect(entries[0].level).toBe("info");
  });

  it("pushes entries with monotonically increasing ids", async () => {
    const { navLog } = await import("./navigation-logger");
    navLog("debug", "first");
    navLog("debug", "second");

    const entries = lspLogStore.entries;
    expect(entries).toHaveLength(2);
    expect(entries[1].id).toBeGreaterThan(entries[0].id);
  });

  it("calls lsp_append_client_log via invoke fire-and-forget", async () => {
    const { navLog } = await import("./navigation-logger");
    navLog("warn", "definition failed");

    // invoke is called with the correct command and args
    expect(invokeStub).toHaveBeenCalledWith("lsp_append_client_log", {
      message: "definition failed",
      level: "warn",
      source: "lsp:navigate",
    });
  });

  it("does not throw when invoke rejects (fire-and-forget)", async () => {
    invokeStub.mockRejectedValueOnce(new Error("LSP not running"));
    const { navLog } = await import("./navigation-logger");

    // Should not throw — errors are swallowed
    expect(() => navLog("error", "some error")).not.toThrow();
  });

  it("includes a timestamp string", async () => {
    const { navLog } = await import("./navigation-logger");
    navLog("info", "ts test");

    const entry = lspLogStore.entries[0];
    expect(typeof entry.timestamp).toBe("string");
    expect(entry.timestamp.length).toBeGreaterThan(0);
  });
});
