import { type JSX, Show, createMemo } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { projectState } from "@/stores/project.store";
import { openSettings } from "@/components/settings/SettingsPanel";
import { openHealthPanel } from "@/components/health/HealthPanel";
import { overallHealth, healthSummary } from "@/stores/health.store";
import { buildState, isBuilding, isDeploying, buildDurationMs } from "@/stores/build.store";
import { VariantSelectorPill } from "@/components/build/VariantSelector";
import { setActiveTab } from "@/stores/ui.store";
import { mcpState } from "@/stores/mcp.store";
import { openMcpPanel } from "@/components/mcp/McpPanel";
import Icon from "@/components/common/Icon";
import { appMemoryBytes, logFolderBytes, rotationTriggered } from "@/stores/monitor.store";
import { settingsState } from "@/stores/settings.store";

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
      case "ok":
        return "#4ade80";
      case "warning":
        return "#fbbf24";
      case "error":
        return "#f87171";
      default:
        return "rgba(255,255,255,0.4)";
    }
  };

  const tooltip = () => {
    const s = overall();
    const { ok, total } = summary();
    if (s === "loading") return "Running health checks…";
    if (s === "ok") return `App health: all ${total} checks passing — click for details`;
    return `App health: ${ok}/${total} checks passing — click to see issues`;
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
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.15)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.08)";
      }}
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
      <span
        style={{
          color: "#ffffff",
          "font-size": "11px",
          "line-height": "1",
          "white-space": "nowrap",
        }}
      >
        Health
      </span>
      <Show when={overall() !== "ok" && overall() !== "loading"}>
        <span
          style={{
            color: dotColor(),
            "font-size": "10px",
            "line-height": "1",
            "white-space": "nowrap",
          }}
        >
          {summary().ok}/{summary().total}
        </span>
      </Show>
    </button>
  );
}

// ── Build status indicator ─────────────────────────────────────────────────────

function formatElapsed(ms: number): string {
  if (ms < 1000) return "";
  const totalSec = Math.floor(ms / 1000);
  if (totalSec < 60) return ` (${totalSec}s)`;
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return s > 0 ? ` (${m}m ${s}s)` : ` (${m}m)`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 MB";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

// ── Memory indicator ─────────────────────────────────────────────────────────

function MemoryIndicator(): JSX.Element {
  const bytes = () => appMemoryBytes();

  const color = () => {
    const mb = bytes() / (1024 * 1024);
    if (mb >= 500) return "#f87171";
    if (mb >= 300) return "#fbbf24";
    return "rgba(255,255,255,0.6)";
  };

  return (
    <span
      title={`App memory: ${formatBytes(bytes())}`}
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        color: color(),
        "font-size": "11px",
        "white-space": "nowrap",
        "flex-shrink": "0",
      }}
    >
      {formatBytes(bytes())}
    </span>
  );
}

// ── Log size indicator ────────────────────────────────────────────────────────

function LogSizeIndicator(): JSX.Element {
  const bytes = () => logFolderBytes();
  const rotating = () => rotationTriggered();

  const color = () => {
    const maxBytes = (settingsState.advanced.logMaxSizeMb ?? 500) * 1024 * 1024;
    const pct = maxBytes > 0 ? bytes() / maxBytes : 0;
    if (rotating() || pct >= 0.9) return "#f87171";
    if (pct >= 0.7) return "#fbbf24";
    return "rgba(255,255,255,0.6)";
  };

  const label = () => `${formatBytes(bytes())}${rotating() ? " ↻" : ""}`;

  const tooltip = () => {
    const maxMb = settingsState.advanced.logMaxSizeMb ?? 500;
    const usedMb = Math.round(bytes() / (1024 * 1024));
    return `Log folder: ${usedMb} MB / ${maxMb} MB${rotating() ? " (rotation triggered)" : ""}`;
  };

  return (
    <span
      title={tooltip()}
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        color: color(),
        "font-size": "11px",
        "white-space": "nowrap",
        "flex-shrink": "0",
      }}
    >
      {label()}
    </span>
  );
}

function BuildStatusIndicator(): JSX.Element {
  const phase = () => buildState.phase;
  const task = () => buildState.currentTask;
  const deployPhase = () => buildState.deployPhase;
  const dur = () => formatElapsed(buildDurationMs());

  const label = createMemo(() => {
    // Deploy phases take precedence over build phase labels.
    if (deployPhase() === "installing") return "Installing…";
    if (deployPhase() === "launching") return "Launching…";
    switch (phase()) {
      case "running":
        return task() ? `Building: ${task()}${dur()}` : `Building…${dur()}`;
      case "success":
        return deployPhase() ? null : `Build: OK${dur()}`;
      case "failed":
        return `Build: Failed${dur()}`;
      case "cancelled":
        return "Build: Cancelled";
      default:
        return null;
    }
  });

  const color = createMemo(() => {
    if (deployPhase() === "installing" || deployPhase() === "launching") return "#60a5fa";
    switch (phase()) {
      case "running":
        return "#fbbf24";
      case "success":
        return "#4ade80";
      case "failed":
        return "#f87171";
      default:
        return "rgba(255,255,255,0.4)";
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
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.15)";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.08)";
        }}
      >
        <Show when={spinning()}>
          <span class="lsp-spinner" style={{ "line-height": "0", "flex-shrink": "0" }}>
            <Icon name="spinner" size={10} color={color()} />
          </span>
        </Show>
        <Show when={!spinning()}>
          <span
            style={{
              width: "6px",
              height: "6px",
              "border-radius": "50%",
              background: color(),
              "flex-shrink": "0",
              display: "inline-block",
            }}
          />
        </Show>
        <span
          style={{
            color: color(),
            "font-size": "11px",
            "line-height": "1",
            "white-space": "nowrap",
          }}
        >
          {label()}
        </span>
      </button>
    </Show>
  );
}

// ── MCP status indicator ───────────────────────────────────────────────────────

function McpStatusIndicator(): JSX.Element {
  const connected = () => mcpState.running || mcpState.serverAlive;
  const clientName = () => mcpState.clientName;

  const dotColor = () => {
    if (mcpState.clientName) return "#4ade80"; // client actively connected
    if (mcpState.serverAlive) return "#60a5fa"; // server alive, no client
    if (mcpState.running) return "#fbbf24"; // GUI-mode server running
    return "rgba(255,255,255,0.3)"; // idle
  };

  const tooltip = () => {
    if (clientName()) return `MCP: ${clientName()} connected — click for activity log`;
    if (mcpState.serverAlive)
      return `MCP server running (PID ${mcpState.serverPid ?? "?"}) — click for activity log`;
    if (mcpState.running) return "MCP server running — click for activity log";
    return "MCP — click to set up or view activity log";
  };

  return (
    <button
      onClick={openMcpPanel}
      onMouseDown={(e) => e.stopPropagation()}
      title={tooltip()}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "4px",
        padding: "0 6px",
        height: "18px",
        background: connected() ? "rgba(96,165,250,0.15)" : "rgba(255,255,255,0.08)",
        border: connected() ? "1px solid rgba(96,165,250,0.4)" : "1px solid rgba(255,255,255,0.12)",
        "border-radius": "3px",
        cursor: "pointer",
        "flex-shrink": "0",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = connected()
          ? "rgba(96,165,250,0.25)"
          : "rgba(255,255,255,0.15)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = connected()
          ? "rgba(96,165,250,0.15)"
          : "rgba(255,255,255,0.08)";
      }}
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
      <span
        style={{
          color: connected() ? "#93c5fd" : "rgba(255,255,255,0.7)",
          "font-size": "11px",
          "line-height": "1",
          "white-space": "nowrap",
        }}
      >
        {clientName() ? `MCP: ${clientName()}` : "MCP"}
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
      <div
        style={{
          display: "flex",
          gap: "6px",
          "align-items": "center",
          flex: "1",
          "min-width": "0",
        }}
      >
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
          onMouseEnter={(e) => {
            e.currentTarget.style.color = "#fff";
            e.currentTarget.style.background = "rgba(255,255,255,0.1)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.color = "rgba(255,255,255,0.7)";
            e.currentTarget.style.background = "none";
          }}
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
          "flex-shrink": "0",
          opacity: "0.85",
        }}
      >
        <MemoryIndicator />
        <LogSizeIndicator />
      </div>
    </div>
  );
}

export default StatusBar;
