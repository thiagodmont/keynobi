/**
 * MCP server lifecycle + activity log state.
 *
 * Tracks whether the MCP stdio server is running, which client connected,
 * and the most recent activity entries read from the shared JSONL log file.
 * Used by StatusBar, McpPanel, and HealthPanel.
 */
import { createStore, produce } from "solid-js/store";
import { listen } from "@tauri-apps/api/event";
import {
  type McpActivityEntry,
  getMcpActivity,
  getMcpServerStatus,
} from "@/lib/tauri-api";
import { showToast } from "@/components/ui";

export type { McpActivityEntry };

export interface McpState {
  running: boolean;
  clientName: string | null;
  connectedAt: string | null;
  transport: "stdio" | "http" | null;
  /** Whether a headless MCP server process is alive (from PID file check). */
  serverAlive: boolean;
  /** PID of the alive headless process, if known. */
  serverPid: number | null;
  /** Most recent activity entries, newest last. */
  activityLog: McpActivityEntry[];
  /** True while `loadMcpActivity` is in flight. */
  activityLoading: boolean;
}

const [mcpState, setMcpState] = createStore<McpState>({
  running: false,
  clientName: null,
  connectedAt: null,
  transport: null,
  serverAlive: false,
  serverPid: null,
  activityLog: [],
  activityLoading: false,
});

export { mcpState };

// ── Event listeners ───────────────────────────────────────────────────────────

export function initMcpListeners() {
  listen<{ transport: string }>("mcp:started", (event) => {
    setMcpState({
      running: true,
      transport: (event.payload.transport as "stdio" | "http") ?? "stdio",
      clientName: null,
      connectedAt: null,
    });
  });

  listen<{ clientName: string; connectedAt: string }>(
    "mcp:client_connected",
    (event) => {
      setMcpState({
        clientName: event.payload.clientName,
        connectedAt: event.payload.connectedAt,
      });
      // Refresh activity log when a client connects.
      loadMcpActivity();
    }
  );

  listen("mcp:stopped", () => {
    setMcpState({
      running: false,
      clientName: null,
      connectedAt: null,
      transport: null,
    });
    loadMcpActivity();
  });
}

// ── Activity + status loader ──────────────────────────────────────────────────

/** Fetch the latest activity entries and server status from the backend. */
export async function loadMcpActivity(limit = 200): Promise<void> {
  setMcpState("activityLoading", true);
  try {
    const [entries, status] = await Promise.all([
      getMcpActivity(limit),
      getMcpServerStatus(),
    ]);
    setMcpState(
      produce((s) => {
        s.activityLog = entries;
        s.serverAlive = status.alive;
        s.serverPid = status.pid;
        s.activityLoading = false;
      })
    );
  } catch (err) {
    console.error("[mcp] Failed to load MCP activity:", err);
    showToast(`MCP activity failed to load: ${err}`, "error");
    setMcpState("activityLoading", false);
  }
}

// ── Polling ───────────────────────────────────────────────────────────────────

let _pollInterval: ReturnType<typeof setInterval> | null = null;

/** Start polling the activity log every `intervalMs` ms. */
export function startMcpActivityPolling(intervalMs = 3000): void {
  if (_pollInterval !== null) return;
  _pollInterval = setInterval(() => loadMcpActivity(), intervalMs);
}

/** Stop activity polling (call when the MCP panel closes). */
export function stopMcpActivityPolling(): void {
  if (_pollInterval !== null) {
    clearInterval(_pollInterval);
    _pollInterval = null;
  }
}
