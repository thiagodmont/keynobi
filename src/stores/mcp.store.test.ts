import { describe, it, expect, beforeEach, vi } from "vitest";
import { waitFor } from "@solidjs/testing-library";
import { listen, type Event } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  initMcpListeners,
  loadMcpActivity,
  mcpState,
  mcpStatusSummary,
  resetMcpListenersForTests,
  resetMcpStateForTests,
  type McpAttachedSession,
  type McpStandaloneServer,
} from "@/stores/mcp.store";

const mockListen = vi.mocked(listen);
const mockInvoke = vi.mocked(invoke);

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function attached(id: number, overrides: Partial<McpAttachedSession> = {}): McpAttachedSession {
  return {
    id,
    pid: 1000 + id,
    project: null,
    connectedAt: "2026-01-01T00:00:00Z",
    clientName: null,
    ...overrides,
  };
}

function standalone(pid: number): McpStandaloneServer {
  return {
    pid,
    startedAt: "2026-01-01T00:00:00Z",
    project: "/p/app",
    reason: "the Keynobi app is not running",
  };
}

describe("mcp.store listener lifecycle", () => {
  beforeEach(() => {
    resetMcpListenersForTests();
    resetMcpStateForTests();
    vi.clearAllMocks();
    mockListen.mockResolvedValue(() => {});
  });

  it("registers the session listener only once", () => {
    initMcpListeners();
    initMcpListeners();

    expect(mockListen).toHaveBeenCalledTimes(1);
    expect(mockListen).toHaveBeenCalledWith("mcp:sessions_changed", expect.any(Function));
  });

  it("disposes the session listener", async () => {
    const unlisten = vi.fn<() => void>();
    mockListen.mockResolvedValue(unlisten);

    initMcpListeners();
    await Promise.resolve();
    await Promise.resolve();

    resetMcpListenersForTests();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("disposes a listener that resolves after reset", async () => {
    const unlisten = vi.fn<() => void>();
    const listener = deferred<() => void>();
    mockListen.mockReturnValue(listener.promise);

    initMcpListeners();
    resetMcpListenersForTests();
    expect(unlisten).not.toHaveBeenCalled();

    listener.resolve(unlisten);

    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("replaces the attached sessions when they change", async () => {
    initMcpListeners();
    const handler = mockListen.mock.calls[0][1] as (e: Event<McpAttachedSession[]>) => void;

    handler({ event: "mcp:sessions_changed", id: 1, payload: [attached(1), attached(2)] });
    expect(mcpState.attached.map((s) => s.id)).toEqual([1, 2]);

    handler({ event: "mcp:sessions_changed", id: 2, payload: [attached(2)] });
    expect(mcpState.attached.map((s) => s.id)).toEqual([2]);
  });
});

describe("loadMcpActivity", () => {
  beforeEach(() => {
    resetMcpStateForTests();
    vi.clearAllMocks();
  });

  it("stores attached sessions and standalone servers from the status", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_activity") return [];
      if (cmd === "get_mcp_server_status") {
        return { listening: true, attached: [attached(1)], standalone: [standalone(42)] };
      }
      throw new Error(`unexpected ${cmd}`);
    });

    await loadMcpActivity();

    expect(mcpState.listening).toBe(true);
    expect(mcpState.attached).toHaveLength(1);
    expect(mcpState.standalone[0].pid).toBe(42);
  });
});

describe("mcpStatusSummary", () => {
  it("is idle with no sessions", () => {
    expect(mcpStatusSummary({ attached: [], standalone: [] })).toEqual({
      tone: "idle",
      label: "",
      description: "No AI clients connected",
    });
  });

  it("counts attached agents", () => {
    const summary = mcpStatusSummary({ attached: [attached(1), attached(2)], standalone: [] });
    expect(summary.tone).toBe("attached");
    expect(summary.label).toBe("2 agents");
    expect(summary.description).toBe("2 agents connected");
  });

  it("calls out standalone servers as not shared with the app", () => {
    const summary = mcpStatusSummary({ attached: [], standalone: [standalone(1)] });
    expect(summary.tone).toBe("standalone");
    expect(summary.label).toBe("1 standalone");
    expect(summary.description).toBe("1 standalone server (not shared with the app)");
  });

  it("lists both kinds together", () => {
    const summary = mcpStatusSummary({
      attached: [attached(1)],
      standalone: [standalone(1), standalone(2)],
    });
    expect(summary.tone).toBe("attached");
    expect(summary.description).toBe(
      "1 agent connected, 2 standalone servers (not shared with the app)"
    );
  });
});
