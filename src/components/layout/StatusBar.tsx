import { type JSX, Show, createMemo } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { editorState } from "@/stores/editor.store";
import { projectState } from "@/stores/project.store";
import { lspState, getDiagnosticCounts } from "@/stores/lsp.store";
import { openSettings } from "@/components/settings/SettingsPanel";
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
        tooltip: "Kotlin Language Server is starting up.",
      };
    case "indexing":
      return {
        label: "Kotlin LSP",
        sublabel: msg ?? "Indexing…",
        color: "#60a5fa",
        dotColor: "#60a5fa",
        indicator: "spinner",
        tooltip: "Kotlin Language Server is indexing your project.",
      };
    case "ready":
      return {
        label: "Kotlin LSP",
        sublabel: "Running",
        color: "#ffffff",
        dotColor: "#4ade80",
        indicator: "dot",
        tooltip: "Kotlin Language Server is active. Click to open Settings → Tools.",
      };
    case "error":
      return {
        label: "Kotlin LSP",
        sublabel: msg ?? "Error",
        color: "#f87171",
        dotColor: "#f87171",
        indicator: "dot",
        tooltip: `Kotlin Language Server encountered an error${msg ? `: ${msg}` : ""}. Click to open Settings → Tools.`,
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

  return (
    <button
      onClick={() => openSettings()}
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
    </div>
  );
}

export default StatusBar;
