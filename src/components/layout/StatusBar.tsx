import { type JSX, Show, createMemo, createSignal } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { editorState } from "@/stores/editor.store";
import { projectState } from "@/stores/project.store";
import { lspState, getDiagnosticCounts } from "@/stores/lsp.store";
import { openSettings } from "@/components/settings/SettingsPanel";
import { openOutputPanel, setUIState, setActiveBottomTab } from "@/stores/ui.store";
import { openHealthPanel } from "@/components/health/HealthPanel";
import { overallHealth, healthSummary } from "@/stores/health.store";
import { buildState, isBuilding } from "@/stores/build.store";
import { selectedDevice, deviceCount } from "@/stores/device.store";
import { VariantSelectorPill } from "@/components/build/VariantSelector";
import { DevicePanel } from "@/components/device/DevicePanel";
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

// ── LSP status helpers ────────────────────────────────────────────────────────

function getLspInfo(): {
  label: string;
  sublabel: string | null;
  color: string;
  dotColor: string;
  indicator: "dot" | "spinner" | "pulse" | "none";
  tooltip: string;
} {
  const state = lspState.status.state;
  const msg = lspState.status.message;

  switch (state) {
    case "notInstalled":
      return {
        label: "Kotlin LSP",
        sublabel: "Not installed",
        color: "rgba(255,255,255,0.5)",
        dotColor: "rgba(255,255,255,0.3)",
        indicator: "dot",
        tooltip: "Kotlin Language Server is not installed. Click to open Settings → Tools.",
      };
    case "downloading":
      return {
        label: "Kotlin LSP",
        sublabel: msg ?? "Downloading...",
        color: "#fbbf24",
        dotColor: "#fbbf24",
        indicator: "spinner",
        tooltip: "Downloading Kotlin Language Server…",
      };
    case "starting":
      return {
        label: "Kotlin LSP",
        sublabel: "Starting…",
        color: "#fbbf24",
        dotColor: "#fbbf24",
        indicator: "pulse",
        tooltip: "Kotlin Language Server is starting — click to watch server logs.",
      };
    case "indexing":
      return {
        label: "Kotlin LSP",
        sublabel: msg ?? "Indexing…",
        color: "#60a5fa",
        dotColor: "#60a5fa",
        indicator: "spinner",
        tooltip: "Kotlin Language Server is indexing your project — click to watch server logs.",
      };
    case "ready":
      if (lspState.indexingJustCompleted === "success") {
        return {
          label: "Kotlin LSP",
          sublabel: "Indexed",
          color: "#4ade80",
          dotColor: "#4ade80",
          indicator: "dot",
          tooltip: "Project indexing complete — Cmd+click and completions are ready.",
        };
      }
      return {
        label: "Kotlin LSP",
        sublabel: "Running",
        color: "#ffffff",
        dotColor: "#4ade80",
        indicator: "dot",
        tooltip: "Kotlin Language Server is active — click to open server logs.",
      };
    case "error":
      if (lspState.indexingJustCompleted === "error") {
        return {
          label: "Kotlin LSP",
          sublabel: "Index failed",
          color: "#f87171",
          dotColor: "#f87171",
          indicator: "dot",
          tooltip: `Indexing error${msg ? `: ${msg}` : ""} — click to see server logs.`,
        };
      }
      return {
        label: "Kotlin LSP",
        sublabel: msg ?? "Error",
        color: "#f87171",
        dotColor: "#f87171",
        indicator: "dot",
        tooltip: `Kotlin Language Server error${msg ? `: ${msg}` : ""} — click to open server logs.`,
      };
    case "stopped":
    default:
      return {
        label: "Kotlin LSP",
        sublabel: "Stopped",
        color: "rgba(255,255,255,0.4)",
        dotColor: "rgba(255,255,255,0.25)",
        indicator: "dot",
        tooltip: "Kotlin Language Server is not running. Click to open Settings → Tools.",
      };
  }
}

function LspStatusIndicator(): JSX.Element {
  const info = createMemo(() => getLspInfo());
  const counts = createMemo(() => getDiagnosticCounts());

  /** Navigate to logs when LSP is active; open Settings when it's not set up. */
  function handleClick() {
    const state = lspState.status.state;
    if (state === "notInstalled" || state === "stopped") {
      openSettings();
    } else {
      openOutputPanel();
    }
  }

  return (
    <button
      onClick={handleClick}
      onMouseDown={(e) => e.stopPropagation()}
      title={info().tooltip}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "5px",
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
      {/* Status dot / spinner */}
      <Show when={info().indicator === "dot"}>
        <span
          style={{
            width: "6px",
            height: "6px",
            "border-radius": "50%",
            background: info().dotColor,
            "flex-shrink": "0",
            display: "inline-block",
          }}
        />
      </Show>
      <Show when={info().indicator === "pulse"}>
        <span
          class="lsp-dot-pulse"
          style={{
            width: "6px",
            height: "6px",
            background: info().dotColor,
            "flex-shrink": "0",
          }}
        />
      </Show>
      <Show when={info().indicator === "spinner"}>
        <span class="lsp-spinner" style={{ "line-height": "0", "flex-shrink": "0" }}>
          <Icon name="spinner" size={10} color={info().dotColor} />
        </span>
      </Show>

      {/* Labels */}
      <span style={{ color: info().color, "font-size": "11px", "line-height": "1", "white-space": "nowrap" }}>
        {info().label}
      </span>
      <Show when={info().sublabel}>
        <span
          style={{
            color: info().dotColor,
            "font-size": "10px",
            "line-height": "1",
            "white-space": "nowrap",
            opacity: "0.9",
          }}
        >
          {info().sublabel}
        </span>
      </Show>

      {/* Diagnostic counts (only when ready) */}
      <Show when={lspState.status.state === "ready" && (counts().errors > 0 || counts().warnings > 0)}>
        <span style={{ display: "flex", gap: "4px", "align-items": "center", "margin-left": "2px" }}>
          <Show when={counts().errors > 0}>
            <span style={{ display: "flex", "align-items": "center", gap: "2px", color: "#f87171", "font-size": "10px" }}>
              <Icon name="error-circle" size={10} color="#f87171" />
              {counts().errors}
            </span>
          </Show>
          <Show when={counts().warnings > 0}>
            <span style={{ display: "flex", "align-items": "center", gap: "2px", color: "#fbbf24", "font-size": "10px" }}>
              <Icon name="warning" size={10} color="#fbbf24" />
              {counts().warnings}
            </span>
          </Show>
        </span>
      </Show>
    </button>
  );
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

  const label = createMemo(() => {
    switch (phase()) {
      case "running":   return task() ? `Building: ${task()}` : "Building…";
      case "success":   return "Build: OK";
      case "failed":    return "Build: Failed";
      case "cancelled": return "Build: Cancelled";
      default:          return null;
    }
  });

  const color = createMemo(() => {
    switch (phase()) {
      case "running":  return "#fbbf24";
      case "success":  return "#4ade80";
      case "failed":   return "#f87171";
      default:         return "rgba(255,255,255,0.4)";
    }
  });

  function handleClick() {
    setUIState("bottomPanelVisible", true);
    setActiveBottomTab("build");
  }

  return (
    <Show when={label()}>
      <button
        onClick={handleClick}
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
        <Show when={isBuilding()}>
          <span class="lsp-spinner" style={{ "line-height": "0", "flex-shrink": "0" }}>
            <Icon name="spinner" size={10} color={color()} />
          </span>
        </Show>
        <Show when={!isBuilding()}>
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

// ── Device selector ────────────────────────────────────────────────────────────

function DeviceSelectorPill(): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const device = () => selectedDevice();
  const count = () => deviceCount();

  const label = createMemo(() => {
    const d = device();
    if (d) return d.model ?? d.name;
    return count() > 0 ? `${count()} device${count() !== 1 ? "s" : ""}` : "No Device";
  });

  const dotColor = createMemo(() => {
    const d = device();
    if (!d) return count() > 0 ? "#4ade80" : "rgba(255,255,255,0.3)";
    return d.connectionState === "online" ? "#4ade80" : "#6b7280";
  });

  return (
    <div style={{ position: "relative" }}>
      <button
        id="device-selector-btn"
        onClick={() => setOpen((v) => !v)}
        onMouseDown={(e) => e.stopPropagation()}
        title={`Device: ${label()} — click to select`}
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
            width: "6px", height: "6px",
            "border-radius": "50%",
            background: dotColor(),
            "flex-shrink": "0",
            display: "inline-block",
          }}
        />
        <span style={{ color: "#ffffff", "font-size": "11px", "line-height": "1", "white-space": "nowrap", "max-width": "120px", overflow: "hidden", "text-overflow": "ellipsis" }}>
          {label()}
        </span>
      </button>

      <Show when={open()}>
        <div
          style={{
            position: "fixed",
            bottom: "32px",
            left: "auto",
            right: "auto",
            "z-index": "1000",
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <DevicePanel onClose={() => setOpen(false)} />
        </div>
        {/* Overlay to close on outside click */}
        <div
          style={{
            position: "fixed",
            inset: "0",
            "z-index": "999",
          }}
          onClick={() => setOpen(false)}
        />
      </Show>
    </div>
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
          {projectState.projectName ?? "Ready"}
        </span>

        {/* Separator */}
        <Show when={projectState.projectName}>
          <span style={{ opacity: "0.3", "flex-shrink": "0" }}>·</span>
        </Show>

        {/* LSP Status pill — always visible */}
        <LspStatusIndicator />

        {/* Health indicator */}
        <HealthIndicator />

        {/* Phase 3: Build status indicator */}
        <BuildStatusIndicator />

        {/* Phase 3: Variant selector */}
        <Show when={projectState.projectName}>
          <VariantSelectorPill />
        </Show>

        {/* Phase 3: Device selector */}
        <Show when={projectState.projectName}>
          <DeviceSelectorPill />
        </Show>

        {/* Standalone diagnostic counts when NOT ready (e.g. error state) */}
        <Show when={lspState.status.state !== "ready"}>
          <Show when={getDiagnosticCounts().errors > 0 || getDiagnosticCounts().warnings > 0}>
            <div style={{ display: "flex", gap: "6px", "align-items": "center", "pointer-events": "none" }}>
              <Show when={getDiagnosticCounts().errors > 0}>
                <span style={{ display: "flex", "align-items": "center", gap: "2px" }}>
                  <Icon name="error-circle" size={12} color="#f87171" />
                  <span style={{ color: "#f87171" }}>{getDiagnosticCounts().errors}</span>
                </span>
              </Show>
              <Show when={getDiagnosticCounts().warnings > 0}>
                <span style={{ display: "flex", "align-items": "center", gap: "2px" }}>
                  <Icon name="warning" size={12} color="#fbbf24" />
                  <span style={{ color: "#fbbf24" }}>{getDiagnosticCounts().warnings}</span>
                </span>
              </Show>
            </div>
          </Show>
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
        <Show when={editorState.cursorLine !== null && editorState.cursorCol !== null}>
          <span>Ln {editorState.cursorLine}, Col {editorState.cursorCol}</span>
        </Show>
        <Show when={editorState.activeLanguage}>
          <span>
            {editorState.activeLanguage
              ? editorState.activeLanguage.charAt(0).toUpperCase() + editorState.activeLanguage.slice(1)
              : ""}
          </span>
        </Show>
        <span>UTF-8</span>
      </div>

      {/* ── Indexing progress bar ── */}
      {/* Thin 2px line at the bottom edge of the status bar that appears
          during LSP project indexing. Shows a determinate fill when the
          server reports a percentage, or an indeterminate sweep otherwise. */}
      <Show when={lspState.status.state === "indexing"}>
        <div
          style={{
            position: "absolute",
            bottom: "0",
            left: "0",
            right: "0",
            height: "2px",
            background: "rgba(96,165,250,0.25)",
            overflow: "hidden",
          }}
        >
          <Show
            when={lspState.indexingProgress !== null}
            fallback={<div class="lsp-progress-indeterminate" />}
          >
            <div
              style={{
                height: "100%",
                width: `${lspState.indexingProgress}%`,
                background: "#60a5fa",
                transition: "width 0.3s ease",
              }}
            />
          </Show>
        </div>
      </Show>

      {/* Brief green flash bar when indexing completes successfully */}
      <Show when={lspState.indexingJustCompleted === "success"}>
        <div
          style={{
            position: "absolute",
            bottom: "0",
            left: "0",
            right: "0",
            height: "2px",
            background: "#4ade80",
            animation: "lsp-progress-done 3s ease forwards",
          }}
        />
      </Show>
    </div>
  );
}

export default StatusBar;
