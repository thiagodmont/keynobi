import { type JSX, Show, createMemo } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { projectState } from "@/stores/project.store";
import { openSettings } from "@/components/settings/SettingsPanel";
import { openHealthPanel } from "@/components/health/HealthPanel";
import { overallHealth, healthSummary } from "@/stores/health.store";
import { buildState, isBuilding, isDeploying } from "@/stores/build.store";
import { VariantSelectorPill } from "@/components/build/VariantSelector";
import { setActiveTab } from "@/stores/ui.store";
import { startMcpServer, getMcpSetupStatus } from "@/lib/tauri-api";
import { mcpState } from "@/stores/mcp.store";
import Icon from "@/components/common/Icon";

async function startDrag(e: MouseEvent) {
  if (e.button !== 0) return;
  e.preventDefault();
  try {
    await getCurrentWindow().startDragging();
  } catch {
    // ignore — mouse already released
  }
}

// ── Health indicator ──────────────────────────────────────────────────────────

function HealthIndicator(): JSX.Element {
  const overall = () => overallHealth();
  const summary = () => healthSummary();

  const dotColor = () => {
    switch (overall()) {
      case "ok":      return "#4ade80";
      case "warning": return "#fbbf24";
      case "error":   return "#f87171";
      default:        return "rgba(255,255,255,0.4)";
    }
  };

  const tooltip = () => {
    const s = overall();
    const { ok, total } = summary();
    if (s === "loading") return "Running health checks…";
    if (s === "ok")      return `IDE Health: all ${total} checks passing — click for details`;
    return `IDE Health: ${ok}/${total} checks passing — click to see issues`;
  };

  return (
    <button
      onClick={openHealthPanel}
      onMouseDown={(e) => e.stopPropagation()}
      title={tooltip()}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "4px",
        padding: "0 6px",
        height: "18px",
        background: "rgba(255,255,255,0.08)",
        border: "1px solid rgba(255,255,255,0.12)",
        "border-radius": "3px",
        cursor: "pointer",
        "flex-shrink": "0",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.15)"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.08)"; }}
    >
      <span
        style={{
          width: "6px",
          height: "6px",
          "border-radius": "50%",
          background: dotColor(),
          "flex-shrink": "0",
          display: "inline-block",
        }}
      />
      <span style={{ color: "#ffffff", "font-size": "11px", "line-height": "1", "white-space": "nowrap" }}>
        Health
      </span>
      <Show when={overall() !== "ok" && overall() !== "loading"}>
        <span style={{ color: dotColor(), "font-size": "10px", "line-height": "1", "white-space": "nowrap" }}>
          {summary().ok}/{summary().total}
        </span>
      </Show>
    </button>
  );
}

// ── Build status indicator ─────────────────────────────────────────────────────

function BuildStatusIndicator(): JSX.Element {
  const phase = () => buildState.phase;
  const task = () => buildState.currentTask;
  const deployPhase = () => buildState.deployPhase;

  const label = createMemo(() => {
    // Deploy phases take precedence over build phase labels.
    if (deployPhase() === "installing") return "Installing…";
    if (deployPhase() === "launching") return "Launching…";
    switch (phase()) {
      case "running":   return task() ? `Building: ${task()}` : "Building…";
      case "success":   return deployPhase() ? null : "Build: OK";
      case "failed":    return "Build: Failed";
      case "cancelled": return "Build: Cancelled";
      default:          return null;
    }
  });

  const color = createMemo(() => {
    if (deployPhase() === "installing" || deployPhase() === "launching") return "#60a5fa";
    switch (phase()) {
      case "running":  return "#fbbf24";
      case "success":  return "#4ade80";
      case "failed":   return "#f87171";
      default:         return "rgba(255,255,255,0.4)";
    }
  });

  const spinning = () => isBuilding() || isDeploying();

  return (
    <Show when={label()}>
      <button
        onClick={() => setActiveTab("build")}
        onMouseDown={(e) => e.stopPropagation()}
        title="Open build panel"
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: "0 6px",
          height: "18px",
          background: "rgba(255,255,255,0.08)",
          border: "1px solid rgba(255,255,255,0.12)",
          "border-radius": "3px",
          cursor: "pointer",
          "flex-shrink": "0",
          transition: "background 0.1s",
        }}
        onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.15)"; }}
        onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.08)"; }}
      >
        <Show when={spinning()}>
          <span class="lsp-spinner" style={{ "line-height": "0", "flex-shrink": "0" }}>
            <Icon name="spinner" size={10} color={color()} />
          </span>
        </Show>
        <Show when={!spinning()}>
          <span
            style={{
              width: "6px", height: "6px",
              "border-radius": "50%",
              background: color(),
              "flex-shrink": "0",
              display: "inline-block",
            }}
          />
        </Show>
        <span style={{ color: color(), "font-size": "11px", "line-height": "1", "white-space": "nowrap" }}>
          {label()}
        </span>
      </button>
    </Show>
  );
}

// ── MCP status indicator ───────────────────────────────────────────────────────

function McpStatusIndicator(): JSX.Element {
  const running = () => mcpState.running;
  const clientName = () => mcpState.clientName;

  const handleClick = async () => {
    if (running()) {
      try {
        const s = await getMcpSetupStatus();
        await navigator.clipboard.writeText(
          `claude mcp add android-companion --command "${s.setupCommand}"`
        );
      } catch {
        // Non-fatal: copy silently fails
      }
      return;
    }
    try {
      await startMcpServer();
    } catch {
      // Error toast shown by App.tsx action handler
    }
  };

  const tooltip = () => {
    if (!running()) return "Start MCP server for Claude Code integration";
    if (clientName()) return `MCP: ${clientName()} connected — click to copy setup command`;
    return "MCP server running (stdio) — click to copy setup command";
  };

  return (
    <button
      onClick={handleClick}
      onMouseDown={(e) => e.stopPropagation()}
      title={tooltip()}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "4px",
        padding: "0 6px",
        height: "18px",
        background: running()
          ? "rgba(96,165,250,0.15)"
          : "rgba(255,255,255,0.08)",
        border: running()
          ? "1px solid rgba(96,165,250,0.4)"
          : "1px solid rgba(255,255,255,0.12)",
        "border-radius": "3px",
        cursor: "pointer",
        "flex-shrink": "0",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = running()
          ? "rgba(96,165,250,0.25)"
          : "rgba(255,255,255,0.15)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = running()
          ? "rgba(96,165,250,0.15)"
          : "rgba(255,255,255,0.08)";
      }}
    >
      <span
        style={{
          width: "6px",
          height: "6px",
          "border-radius": "50%",
          background: running() ? "#60a5fa" : "rgba(255,255,255,0.3)",
          "flex-shrink": "0",
          display: "inline-block",
        }}
      />
      <span
        style={{
          color: running() ? "#93c5fd" : "rgba(255,255,255,0.7)",
          "font-size": "11px",
          "line-height": "1",
          "white-space": "nowrap",
        }}
      >
        {running() && clientName() ? `MCP: ${clientName()}` : "MCP"}
      </span>
    </button>
  );
}

// ── StatusBar ─────────────────────────────────────────────────────────────────

export function StatusBar(): JSX.Element {
  return (
    <div
      onMouseDown={startDrag}
      style={{
        height: "var(--statusbar-height)",
        background: "var(--accent)",
        display: "flex",
        "align-items": "center",
        "justify-content": "space-between",
        padding: "0 6px",
        "flex-shrink": "0",
        "font-size": "11px",
        color: "#ffffff",
        "user-select": "none",
        cursor: "default",
        gap: "6px",
        position: "relative",
      }}
    >
      {/* Left side */}
      <div style={{ display: "flex", gap: "6px", "align-items": "center", flex: "1", "min-width": "0" }}>

        {/* Settings gear */}
        <button
          onClick={() => openSettings()}
          onMouseDown={(e) => e.stopPropagation()}
          title="Settings (Cmd+,)"
          style={{
            background: "none",
            border: "none",
            color: "rgba(255,255,255,0.7)",
            cursor: "pointer",
            padding: "2px",
            display: "flex",
            "align-items": "center",
            "border-radius": "3px",
            "flex-shrink": "0",
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = "#fff"; e.currentTarget.style.background = "rgba(255,255,255,0.1)"; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = "rgba(255,255,255,0.7)"; e.currentTarget.style.background = "none"; }}
        >
          <Icon name="gear" size={14} />
        </button>

        {/* Project name */}
        <span
          style={{
            "pointer-events": "none",
            opacity: "0.85",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
            "max-width": "160px",
          }}
        >
          {projectState.projectName ?? "Android Dev Companion"}
        </span>

        <Show when={projectState.projectName}>
          <span style={{ opacity: "0.3", "flex-shrink": "0" }}>·</span>
        </Show>

        {/* Health indicator */}
        <HealthIndicator />

        {/* Build status indicator */}
        <BuildStatusIndicator />

        {/* MCP server indicator */}
        <McpStatusIndicator />

        {/* Variant selector */}
        <Show when={projectState.projectName}>
          <VariantSelectorPill />
        </Show>
      </div>

      {/* Right side */}
      <div
        style={{
          display: "flex",
          gap: "10px",
          "align-items": "center",
          "pointer-events": "none",
          "flex-shrink": "0",
          opacity: "0.8",
        }}
      >
        <span>Android Dev Companion</span>
      </div>
    </div>
  );
}

export default StatusBar;
