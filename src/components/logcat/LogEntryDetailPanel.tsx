import { onCleanup, onMount } from "solid-js";
import { showToast } from "@/components/common/Toast";
import type { LogcatEntry } from "@/lib/tauri-api";

// ── Level color config (mirrors LEVEL_CONFIG in LogcatPanel) ──────────────────

const LEVEL_CONFIG: Record<string, { label: string; color: string; bg: string }> = {
  verbose: { label: "V", color: "#9ca3af", bg: "transparent" },
  debug:   { label: "D", color: "#60a5fa", bg: "transparent" },
  info:    { label: "I", color: "#4ade80", bg: "transparent" },
  warn:    { label: "W", color: "#fbbf24", bg: "rgba(251,191,36,0.08)" },
  error:   { label: "E", color: "#f87171", bg: "rgba(248,113,113,0.10)" },
  fatal:   { label: "F", color: "#e879f9", bg: "rgba(232,121,249,0.12)" },
  unknown: { label: "?", color: "#9ca3af", bg: "transparent" },
};

function getLevelConfig(level: string) {
  return LEVEL_CONFIG[level.toLowerCase()] ?? LEVEL_CONFIG.unknown;
}

// ── Format helper (matches LogcatPanel.formatEntry) ───────────────────────────

function formatEntry(e: LogcatEntry): string {
  const pkg = e.package ? `[${e.package}] ` : "";
  return `${e.timestamp}  ${e.level.toUpperCase()}  ${pkg}${e.tag}: ${e.message}`;
}

// ── Props ─────────────────────────────────────────────────────────────────────

interface LogEntryDetailPanelProps {
  entry: LogcatEntry;
  onClose: () => void;
}

// ── Styles ────────────────────────────────────────────────────────────────────

const PANEL_STYLE = {
  background: "#111827",
  "border-top": "1px solid #374151",
  "max-height": "30vh",
  display: "flex",
  "flex-direction": "column",
  "flex-shrink": "0",
  "font-size": "12px",
  color: "#e5e7eb",
} as const;

const HEADER_STYLE = {
  display: "flex",
  "align-items": "center",
  "justify-content": "space-between",
  padding: "6px 12px",
  background: "#1f2937",
  "border-bottom": "1px solid #374151",
  "flex-shrink": "0",
} as const;

const HEADER_TITLE_STYLE = {
  "font-size": "11px",
  "font-weight": "600",
  color: "#9ca3af",
  "letter-spacing": "0.05em",
  "text-transform": "uppercase",
} as const;

const HEADER_ACTIONS_STYLE = {
  display: "flex",
  gap: "6px",
  "align-items": "center",
} as const;

const ICON_BTN_STYLE = {
  background: "none",
  border: "none",
  cursor: "pointer",
  color: "#9ca3af",
  "font-size": "12px",
  padding: "2px 6px",
  "border-radius": "3px",
  display: "flex",
  "align-items": "center",
  gap: "4px",
  transition: "color 0.15s, background 0.15s",
} as const;

const GRID_STYLE = {
  display: "grid",
  "grid-template-columns": "1fr 1fr 1fr",
  "flex-shrink": "0",
} as const;

const CELL_STYLE_BASE = {
  padding: "6px 12px",
  "border-bottom": "1px solid #374151",
  "min-width": "0",
} as const;

const LABEL_STYLE = {
  "font-size": "10px",
  color: "#6b7280",
  "letter-spacing": "0.06em",
  "text-transform": "uppercase",
  "margin-bottom": "2px",
} as const;

const VALUE_STYLE = {
  "font-family": '"JetBrains Mono", "Fira Code", ui-monospace, monospace',
  "font-size": "11px",
  color: "#e5e7eb",
  overflow: "hidden",
  "text-overflow": "ellipsis",
  "white-space": "nowrap",
} as const;

const MESSAGE_AREA_STYLE = {
  padding: "8px 12px",
  overflow: "auto",
  flex: "1",
} as const;

const MESSAGE_VALUE_STYLE = {
  "font-family": '"JetBrains Mono", "Fira Code", ui-monospace, monospace',
  "font-size": "11px",
  color: "#e5e7eb",
  "white-space": "pre-wrap",
  "word-break": "break-all",
  margin: "0",
  "line-height": "1.5",
} as const;

// ── Cell component ─────────────────────────────────────────────────────────────

function MetaCell(props: {
  label: string;
  value: string;
  valueStyle?: Record<string, string>;
  borderRight?: boolean;
  borderLeft?: boolean;
}) {
  const cellStyle = () => ({
    ...CELL_STYLE_BASE,
    ...(props.borderRight ? { "border-right": "1px solid #374151" } : {}),
    ...(props.borderLeft ? { "border-left": "1px solid #374151" } : {}),
  });

  return (
    // eslint-disable-next-line solid/reactivity
    <div style={cellStyle}>
      <div style={LABEL_STYLE}>{props.label}</div>
      <div style={{ ...VALUE_STYLE, ...(props.valueStyle ?? {}) }}>{props.value}</div>
    </div>
  );
}

// ── Component ─────────────────────────────────────────────────────────────────

export function LogEntryDetailPanel(props: LogEntryDetailPanelProps) {
  const levelCfg = () => getLevelConfig(props.entry.level);

  // Escape key closes the panel
  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      props.onClose();
    }
  }
  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });
  onCleanup(() => document.removeEventListener("keydown", handleKeyDown));

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(formatEntry(props.entry));
      showToast("Copied to clipboard", "success");
    } catch {
      showToast("Failed to copy to clipboard", "error");
    }
  }

  return (
    <div style={PANEL_STYLE} aria-label="Log entry detail">
      {/* Header */}
      <div style={HEADER_STYLE}>
        <span style={HEADER_TITLE_STYLE}>Log Entry Detail</span>
        <div style={HEADER_ACTIONS_STYLE}>
          <button
            style={ICON_BTN_STYLE}
            title="Copy entry to clipboard"
            onClick={handleCopy}
          >
            <span style={{ "font-size": "13px" }}>⎘</span>
            <span>copy</span>
          </button>
          <button
            style={{ ...ICON_BTN_STYLE, "font-size": "14px", padding: "2px 4px" }}
            title="Close detail panel (Esc)"
            onClick={() => props.onClose()}
            aria-label="Close"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Metadata grid — row 1: timestamp / level / tag */}
      <div style={GRID_STYLE}>
        <MetaCell label="Timestamp" value={props.entry.timestamp} borderRight />
        <MetaCell
          label="Level"
          value={props.entry.level.toUpperCase()}
          valueStyle={{ color: levelCfg().color, "font-weight": "600" }}
          borderRight
        />
        <MetaCell label="Tag" value={props.entry.tag} />
      </div>

      {/* Metadata grid — row 2: package / pid / tid */}
      <div style={GRID_STYLE}>
        <MetaCell
          label="Package"
          value={props.entry.package ?? "—"}
          borderRight
        />
        <MetaCell
          label="PID"
          value={String(props.entry.pid)}
          borderRight
        />
        <MetaCell label="TID" value={String(props.entry.tid)} />
      </div>

      {/* Full message */}
      <div style={MESSAGE_AREA_STYLE}>
        <pre style={MESSAGE_VALUE_STYLE}>{props.entry.message}</pre>
      </div>
    </div>
  );
}
