/**
 * MCP sessions + activity log state.
 *
 * Tracks the AI clients attached to the app (pushed by `mcp:sessions_changed`),
 * standalone `keynobi --mcp` servers that run with their own state, and the
 * most recent activity entries read from the shared JSONL log file.
 * Used by StatusBar, McpPanel, and HealthPanel.
 */
import { createStore, produce } from "solid-js/store";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  type McpActivityEntry,
  type McpAttachedSession,
  type McpStandaloneServer,
  getMcpActivity,
  getMcpServerStatus,
  listenMcpSessionsChanged,
} from "@/lib/tauri-api";
import { showToast } from "@/components/ui";

export type { McpActivityEntry, McpAttachedSession, McpStandaloneServer };

export interface McpState {
  /** Whether the app accepts MCP sessions on its socket. */
  listening: boolean;
  /** The app's version, once the status has loaded. */
  appVersion: string | null;
  /** AI clients attached to the app; they share its builds, logcat, and devices. */
  attached: McpAttachedSession[];
  /** `keynobi --mcp` processes that could not attach and run with their own state. */
  standalone: McpStandaloneServer[];
  /** Most recent activity entries, newest last. */
  activityLog: McpActivityEntry[];
  /** True while `loadMcpActivity` is in flight. */
  activityLoading: boolean;
}

function initialMcpState(): McpState {
  return {
    listening: false,
    appVersion: null,
    attached: [],
    standalone: [],
    activityLog: [],
    activityLoading: false,
  };
}

const [mcpState, setMcpState] = createStore<McpState>(initialMcpState());

export { mcpState };

export function resetMcpStateForTests(): void {
  setMcpState(initialMcpState());
}

// ── Derived status ────────────────────────────────────────────────────────────

export type McpStatusTone = "attached" | "standalone" | "idle";

export interface McpStatusSummary {
  tone: McpStatusTone;
  /** Short text for the status bar, e.g. "2 agents". Empty when idle. */
  label: string;
  /** One sentence describing every live session. */
  description: string;
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** Summarize live MCP sessions for the status bar and panels. */
export function mcpStatusSummary(
  state: Pick<McpState, "attached" | "standalone"> = mcpState
): McpStatusSummary {
  const attached = state.attached.length;
  const standalone = state.standalone.length;
  const parts: string[] = [];
  if (attached > 0) parts.push(`${plural(attached, "agent", "agents")} connected`);
  if (standalone > 0) {
    parts.push(
      `${plural(standalone, "standalone server", "standalone servers")} (not shared with the app)`
    );
  }
  if (parts.length === 0) {
    return { tone: "idle", label: "", description: "No AI clients connected" };
  }
  return {
    tone: attached > 0 ? "attached" : "standalone",
    label: attached > 0 ? plural(attached, "agent", "agents") : `${standalone} standalone`,
    description: parts.join(", "),
  };
}

/** A live MCP server running another Keynobi version than the app. */
export interface McpVersionMismatch {
  /** Who runs it, e.g. "claude-code" or "Standalone server (PID 42)". */
  label: string;
  /** Its version, or `null` for a standalone server too old to report one. */
  version: string | null;
}

/** Live MCP servers whose version differs from the app's. Empty until the app version loads. */
export function mcpVersionMismatches(
  state: Pick<McpState, "appVersion" | "attached" | "standalone"> = mcpState
): McpVersionMismatch[] {
  const app = state.appVersion;
  if (!app) return [];
  const attached = state.attached
    .filter((s) => s.version !== app)
    .map((s) => ({ label: s.clientName ?? "AI client", version: s.version }));
  const standalone = state.standalone
    .filter((s) => s.version !== app)
    .map((s) => ({ label: `Standalone server (PID ${s.pid})`, version: s.version }));
  return [...attached, ...standalone];
}

/** One sentence warning about version mismatches, or `null` when every server matches. */
export function mcpVersionWarning(
  state: Pick<McpState, "appVersion" | "attached" | "standalone"> = mcpState
): string | null {
  const mismatches = mcpVersionMismatches(state);
  if (mismatches.length === 0) return null;
  const versions = [...new Set(mismatches.map((m) => m.version ?? "older"))];
  const servers =
    mismatches.length === 1 ? "1 MCP server runs" : `${mismatches.length} MCP servers run`;
  return (
    `${servers} a different Keynobi version (${versions.join(", ")}) than the app ` +
    `(${state.appVersion}). Restart your AI client to load the app's version.`
  );
}

// ── Event listeners ───────────────────────────────────────────────────────────

let mcpLifecycleUnlisteners: UnlistenFn[] | null = null;

function trackMcpListener(registration: Promise<UnlistenFn>): void {
  registration
    .then((unlisten) => {
      if (mcpLifecycleUnlisteners) {
        mcpLifecycleUnlisteners.push(unlisten);
      } else {
        unlisten();
      }
    })
    .catch((err) => {
      console.error("[mcp] Failed to register session listener:", err);
    });
}

export function initMcpListeners(): void {
  if (mcpLifecycleUnlisteners) return;

  mcpLifecycleUnlisteners = [];

  trackMcpListener(
    listenMcpSessionsChanged((sessions) => {
      setMcpState("attached", sessions);
    })
  );
}

export function resetMcpListenersForTests(): void {
  const unlisteners = mcpLifecycleUnlisteners;
  mcpLifecycleUnlisteners = null;
  if (!unlisteners) return;

  for (const unlisten of unlisteners) {
    unlisten();
  }
}

// ── Activity + status loader ──────────────────────────────────────────────────

/** Fetch the latest activity entries and live sessions from the backend. */
export async function loadMcpActivity(limit = 200): Promise<void> {
  setMcpState("activityLoading", true);
  try {
    const [entries, status] = await Promise.all([getMcpActivity(limit), getMcpServerStatus()]);
    setMcpState(
      produce((s) => {
        s.activityLog = entries;
        s.listening = status.listening;
        s.appVersion = status.appVersion;
        s.attached = status.attached;
        s.standalone = status.standalone;
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
