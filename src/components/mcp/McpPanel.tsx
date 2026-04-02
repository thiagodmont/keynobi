import { type JSX, Show, For, createSignal, onMount, onCleanup, createMemo } from "solid-js";
import {
  mcpState,
  loadMcpActivity,
  startMcpActivityPolling,
  stopMcpActivityPolling,
  type McpActivityEntry,
} from "@/stores/mcp.store";
import { getMcpSetupStatus, clearMcpActivity } from "@/lib/tauri-api";

// ── Panel visibility signal ───────────────────────────────────────────────────

const [mcpPanelOpen, setMcpPanelOpen] = createSignal(false);

export function openMcpPanel() {
  setMcpPanelOpen(true);
}

export function closeMcpPanel() {
  setMcpPanelOpen(false);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  } catch {
    return iso;
  }
}

function kindLabel(kind: string): string {
  switch (kind) {
    case "tool_call":    return "Tool";
    case "resource_read": return "Resource";
    case "prompt":       return "Prompt";
    case "lifecycle":    return "System";
    default:             return kind;
  }
}

function kindColor(kind: string): string {
  switch (kind) {
    case "tool_call":    return "#60a5fa";
    case "resource_read": return "#a78bfa";
    case "prompt":       return "#34d399";
    case "lifecycle":    return "rgba(255,255,255,0.4)";
    default:             return "rgba(255,255,255,0.4)";
  }
}

// ── Activity row ──────────────────────────────────────────────────────────────

function ActivityRow(props: { entry: McpActivityEntry }): JSX.Element {
  const [expanded, setExpanded] = createSignal(false);
  const e = props.entry;
  const isError = () => e.status === "error";

  return (
    <div
      style={{
        "border-bottom": "1px solid rgba(255,255,255,0.06)",
        cursor: e.summary ? "pointer" : "default",
      }}
      onClick={() => e.summary && setExpanded((v) => !v)}
    >
      <div
        style={{
          display: "grid",
          "grid-template-columns": "72px 60px 1fr auto",
          gap: "8px",
          padding: "5px 12px",
          "align-items": "center",
          background: expanded() ? "rgba(255,255,255,0.04)" : "transparent",
        }}
      >
        {/* Time */}
        <span style={{ color: "rgba(255,255,255,0.35)", "font-size": "10px", "font-family": "var(--font-mono)", "white-space": "nowrap" }}>
          {formatTime(e.timestamp)}
        </span>

        {/* Kind badge */}
        <span
          style={{
            color: kindColor(e.kind),
            "font-size": "10px",
            "font-weight": "600",
            "text-transform": "uppercase",
            "letter-spacing": "0.04em",
          }}
        >
          {kindLabel(e.kind)}
        </span>

        {/* Name */}
        <span
          style={{
            color: isError() ? "#f87171" : "var(--text-primary)",
            "font-size": "12px",
            "font-family": "var(--font-mono)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
          title={e.name}
        >
          {e.name}
        </span>

        {/* Duration + status */}
        <div style={{ display: "flex", "align-items": "center", gap: "6px", "flex-shrink": "0" }}>
          <Show when={e.durationMs != null}>
            <span style={{ color: "rgba(255,255,255,0.3)", "font-size": "10px", "font-family": "var(--font-mono)" }}>
              {String(e.durationMs)}ms
            </span>
          </Show>
          <span
            style={{
              width: "6px",
              height: "6px",
              "border-radius": "50%",
              background: isError() ? "#f87171" : "#4ade80",
              "flex-shrink": "0",
            }}
          />
        </div>
      </div>

      {/* Expanded summary */}
      <Show when={expanded() && e.summary}>
        <div
          style={{
            padding: "4px 12px 8px 144px",
            color: isError() ? "#fca5a5" : "rgba(255,255,255,0.55)",
            "font-size": "11px",
            "font-family": "var(--font-mono)",
            "white-space": "pre-wrap",
            "word-break": "break-word",
            "line-height": "1.4",
          }}
        >
          {e.summary}
        </div>
      </Show>
    </div>
  );
}

// ── Panel component ───────────────────────────────────────────────────────────

export function McpPanel(): JSX.Element {
  const [setupCmd, setSetupCmd] = createSignal<string | null>(null);
  const [copied, setCopied] = createSignal(false);

  const reversedLog = createMemo(() =>
    [...mcpState.activityLog].reverse()
  );

  onMount(async () => {
    await loadMcpActivity();
    startMcpActivityPolling(3000);

    try {
      const s = await getMcpSetupStatus();
      setSetupCmd(`claude mcp add --transport stdio android-companion -- "${s.exePath}" --mcp`);
    } catch {
      // non-fatal
    }
  });

  onCleanup(() => {
    stopMcpActivityPolling();
  });

  const handleCopy = async () => {
    const cmd = setupCmd();
    if (!cmd) return;
    try {
      await navigator.clipboard.writeText(cmd);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // non-fatal
    }
  };

  const handleClearLog = async () => {
    await clearMcpActivity();
    await loadMcpActivity();
  };

  return (
    <Show when={mcpPanelOpen()}>
      {/* Backdrop */}
      <div
        onClick={closeMcpPanel}
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "2000",
          background: "rgba(0,0,0,0.45)",
        }}
      />

      {/* Panel */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="MCP Server Activity"
        style={{
          position: "fixed",
          top: "6%",
          left: "50%",
          transform: "translateX(-50%)",
          width: "min(760px, 94vw)",
          "max-height": "86vh",
          display: "flex",
          "flex-direction": "column",
          "z-index": "2001",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "8px",
          "box-shadow": "0 20px 60px rgba(0,0,0,0.5)",
          overflow: "hidden",
        }}
      >
        {/* ── Header ─────────────────────────────────────────────────────── */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "14px 16px 12px",
            "border-bottom": "1px solid var(--border)",
            "flex-shrink": "0",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <span style={{ "font-size": "14px", "font-weight": "600", color: "var(--text-primary)" }}>
              MCP Server
            </span>

            {/* Server alive badge */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "5px",
                padding: "2px 8px",
                background: mcpState.serverAlive
                  ? "rgba(74,222,128,0.12)"
                  : "rgba(255,255,255,0.06)",
                border: `1px solid ${mcpState.serverAlive ? "rgba(74,222,128,0.3)" : "rgba(255,255,255,0.1)"}`,
                "border-radius": "4px",
              }}
            >
              <span
                style={{
                  width: "6px",
                  height: "6px",
                  "border-radius": "50%",
                  background: mcpState.serverAlive ? "#4ade80" : "rgba(255,255,255,0.25)",
                }}
              />
              <span
                style={{
                  "font-size": "11px",
                  color: mcpState.serverAlive ? "#4ade80" : "rgba(255,255,255,0.5)",
                }}
              >
                {mcpState.serverAlive
                  ? mcpState.serverPid
                    ? `Running (PID ${mcpState.serverPid})`
                    : "Running"
                  : mcpState.clientName
                  ? `Connected: ${mcpState.clientName}`
                  : "No server detected"}
              </span>
            </div>
          </div>

          <button
            onClick={closeMcpPanel}
            style={{
              background: "none",
              border: "none",
              color: "rgba(255,255,255,0.5)",
              cursor: "pointer",
              "font-size": "18px",
              padding: "0 2px",
              "line-height": "1",
            }}
            title="Close"
          >
            ×
          </button>
        </div>

        {/* ── Setup command ───────────────────────────────────────────────── */}
        <div
          style={{
            padding: "10px 16px",
            "border-bottom": "1px solid var(--border)",
            "flex-shrink": "0",
            background: "rgba(0,0,0,0.15)",
          }}
        >
          <div style={{ "font-size": "11px", color: "rgba(255,255,255,0.4)", "margin-bottom": "5px" }}>
            Register with Claude Code
          </div>
          <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
            <code
              style={{
                flex: "1",
                "font-family": "var(--font-mono)",
                "font-size": "11px",
                background: "rgba(0,0,0,0.3)",
                border: "1px solid rgba(255,255,255,0.08)",
                "border-radius": "4px",
                padding: "5px 8px",
                color: "var(--text-primary)",
                overflow: "hidden",
                "text-overflow": "ellipsis",
                "white-space": "nowrap",
              }}
            >
              {setupCmd() ?? "Loading…"}
            </code>
            <button
              onClick={handleCopy}
              title="Copy to clipboard"
              style={{
                background: copied() ? "rgba(74,222,128,0.15)" : "rgba(255,255,255,0.08)",
                border: `1px solid ${copied() ? "rgba(74,222,128,0.3)" : "rgba(255,255,255,0.12)"}`,
                color: copied() ? "#4ade80" : "rgba(255,255,255,0.7)",
                "border-radius": "4px",
                padding: "4px 10px",
                cursor: "pointer",
                "font-size": "11px",
                "white-space": "nowrap",
                "flex-shrink": "0",
              }}
            >
              {copied() ? "Copied!" : "Copy"}
            </button>
          </div>
          <div style={{ "font-size": "10px", color: "rgba(255,255,255,0.3)", "margin-top": "4px" }}>
            The MCP uses the project currently open in the companion app. Append{" "}
            <code style={{ "font-family": "var(--font-mono)" }}>--project /path</code>
            {" "}to override.
          </div>
        </div>

        {/* ── Activity log ────────────────────────────────────────────────── */}
        <div
          style={{
            flex: "1",
            overflow: "hidden",
            display: "flex",
            "flex-direction": "column",
          }}
        >
          {/* Log toolbar */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              padding: "6px 12px",
              "border-bottom": "1px solid rgba(255,255,255,0.06)",
              "flex-shrink": "0",
            }}
          >
            <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
              <span style={{ "font-size": "11px", color: "rgba(255,255,255,0.5)" }}>
                Activity Log
              </span>
              <Show when={mcpState.activityLog.length > 0}>
                <span
                  style={{
                    "font-size": "10px",
                    color: "rgba(255,255,255,0.25)",
                    background: "rgba(255,255,255,0.06)",
                    padding: "1px 6px",
                    "border-radius": "3px",
                  }}
                >
                  {mcpState.activityLog.length}
                </span>
              </Show>
              <Show when={mcpState.activityLoading}>
                <span style={{ "font-size": "10px", color: "rgba(255,255,255,0.3)" }}>↻</span>
              </Show>
            </div>
            <div style={{ display: "flex", gap: "6px" }}>
              <button
                onClick={() => loadMcpActivity()}
                title="Refresh"
                style={{
                  background: "rgba(255,255,255,0.06)",
                  border: "1px solid rgba(255,255,255,0.1)",
                  color: "rgba(255,255,255,0.6)",
                  "border-radius": "3px",
                  padding: "2px 8px",
                  cursor: "pointer",
                  "font-size": "11px",
                }}
              >
                Refresh
              </button>
              <button
                onClick={handleClearLog}
                title="Clear log"
                style={{
                  background: "rgba(255,255,255,0.06)",
                  border: "1px solid rgba(255,255,255,0.1)",
                  color: "rgba(255,255,255,0.6)",
                  "border-radius": "3px",
                  padding: "2px 8px",
                  cursor: "pointer",
                  "font-size": "11px",
                }}
              >
                Clear
              </button>
            </div>
          </div>

          {/* Column headers */}
          <div
            style={{
              display: "grid",
              "grid-template-columns": "72px 60px 1fr auto",
              gap: "8px",
              padding: "4px 12px",
              background: "rgba(0,0,0,0.2)",
              "border-bottom": "1px solid rgba(255,255,255,0.06)",
              "flex-shrink": "0",
            }}
          >
            {["Time", "Kind", "Name", "Status"].map((h) => (
              <span style={{ "font-size": "10px", color: "rgba(255,255,255,0.3)", "text-transform": "uppercase", "letter-spacing": "0.05em" }}>
                {h}
              </span>
            ))}
          </div>

          {/* Log rows */}
          <div
            style={{
              flex: "1",
              overflow: "auto",
              "font-size": "12px",
            }}
          >
            <Show
              when={reversedLog().length > 0}
              fallback={
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    "justify-content": "center",
                    height: "100px",
                    color: "rgba(255,255,255,0.25)",
                    "font-size": "12px",
                  }}
                >
                  No activity yet — connect Claude Code to start logging.
                </div>
              }
            >
              <For each={reversedLog()}>
                {(entry) => <ActivityRow entry={entry} />}
              </For>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}
