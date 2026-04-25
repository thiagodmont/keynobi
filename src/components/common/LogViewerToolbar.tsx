import { type JSX, For, Show } from "solid-js";
import type { LogViewerLevelFilter } from "./log-viewer-filter";

const LEVEL_FILTERS: { id: LogViewerLevelFilter; label: string; color?: string }[] = [
  { id: "all", label: "ALL" },
  { id: "error", label: "ERR", color: "var(--error)" },
  { id: "warn", label: "WARN", color: "var(--warning)" },
  { id: "info", label: "INFO", color: "var(--info)" },
  { id: "debug", label: "DBG", color: "var(--text-secondary)" },
];

interface LogViewerToolbarProps {
  levelFilter: LogViewerLevelFilter;
  onLevelFilterChange: (level: LogViewerLevelFilter) => void;
  sourceFilter: string;
  onSourceFilterChange: (source: string) => void;
  sources: string[];
  rawSearch: string;
  onRawSearchChange: (search: string) => void;
  countText: string;
  countTitle: string;
  showTimestamps: boolean;
  onToggleTimestamps: () => void;
  autoScroll: boolean;
  onToggleAutoScroll: () => void;
  copiedAll: boolean;
  onCopyAll: () => void | Promise<void>;
  canClear: boolean;
  onClear: () => void;
}

function ToolbarButton(props: {
  title: string;
  active?: boolean;
  activeColor?: string;
  onClick: () => void | Promise<void>;
  children: JSX.Element;
}): JSX.Element {
  return (
    <button
      title={props.title}
      onClick={() => void props.onClick()}
      style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        height: "22px",
        padding: "0 6px",
        "border-radius": "3px",
        border: "none",
        background: activeToolbarButtonBackground(props.active, props.activeColor),
        color: props.active ? (props.activeColor ?? "var(--text-primary)") : "var(--text-muted)",
        cursor: "pointer",
        "font-size": "11px",
        "line-height": "1",
        transition: "background 0.1s, color 0.1s",
        "flex-shrink": "0",
      }}
    >
      {props.children}
    </button>
  );
}

export function activeToolbarButtonBackground(active?: boolean, activeColor?: string): string {
  if (!active) return "transparent";
  if (!activeColor) return "var(--bg-active)";
  return `color-mix(in srgb, ${activeColor} 14%, transparent)`;
}

function ToolbarDivider(): JSX.Element {
  return (
    <div
      style={{
        width: "1px",
        height: "16px",
        background: "var(--border)",
        "flex-shrink": "0",
        margin: "0 2px",
      }}
    />
  );
}

export function LogViewerToolbar(props: LogViewerToolbarProps): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        gap: "4px",
        padding: "4px 8px",
        background: "var(--bg-tertiary)",
        "border-bottom": "1px solid var(--border)",
        "flex-shrink": "0",
        "flex-wrap": "nowrap",
        overflow: "hidden",
      }}
    >
      <div style={{ display: "flex", gap: "2px", "flex-shrink": "0" }}>
        <For each={LEVEL_FILTERS}>
          {(filter) => (
            <ToolbarButton
              title={`Show ${filter.label === "ALL" ? "all levels" : filter.label + " and above"}`}
              active={props.levelFilter === filter.id}
              activeColor={filter.color}
              onClick={() => props.onLevelFilterChange(filter.id)}
            >
              {filter.label}
            </ToolbarButton>
          )}
        </For>
      </div>

      <ToolbarDivider />

      <Show when={props.sources.length > 1 || props.sourceFilter !== "all"}>
        <select
          value={props.sourceFilter}
          onChange={(e) => props.onSourceFilterChange(e.currentTarget.value)}
          title="Filter by log source"
          style={{
            height: "22px",
            padding: "0 4px",
            background: "var(--bg-quaternary)",
            border: `1px solid ${props.sourceFilter !== "all" ? "var(--accent)" : "var(--border)"}`,
            "border-radius": "3px",
            color: props.sourceFilter !== "all" ? "var(--accent)" : "var(--text-muted)",
            "font-family": "var(--font-ui)",
            "font-size": "11px",
            cursor: "pointer",
            "flex-shrink": "0",
            outline: "none",
          }}
        >
          <option
            value="all"
            style={{ background: "var(--bg-secondary)", color: "var(--text-primary)" }}
          >
            All sources
          </option>
          <For each={props.sources}>
            {(source) => (
              <option
                value={source}
                style={{ background: "var(--bg-secondary)", color: "var(--text-primary)" }}
              >
                {source}
              </option>
            )}
          </For>
        </select>
        <ToolbarDivider />
      </Show>

      <input
        type="text"
        placeholder="Filter logs…"
        value={props.rawSearch}
        onInput={(e) => props.onRawSearchChange(e.currentTarget.value)}
        style={{
          flex: "1",
          "min-width": "80px",
          "max-width": "200px",
          height: "22px",
          padding: "0 8px",
          background: "var(--bg-quaternary)",
          border: "1px solid var(--border)",
          "border-radius": "3px",
          color: "var(--text-primary)",
          "font-family": "var(--font-ui)",
          "font-size": "11px",
          outline: "none",
        }}
      />

      <span
        title={props.countTitle}
        style={{
          "font-family": "var(--font-ui)",
          "font-size": "11px",
          color: "var(--text-muted)",
          "white-space": "nowrap",
          "flex-shrink": "0",
          cursor: "default",
        }}
      >
        {props.countText}
      </span>

      <div style={{ flex: "1" }} />

      <ToolbarButton
        title={props.showTimestamps ? "Hide timestamps" : "Show timestamps"}
        active={props.showTimestamps}
        onClick={props.onToggleTimestamps}
      >
        TS
      </ToolbarButton>

      <ToolbarButton
        title={
          props.autoScroll
            ? "Auto-scroll on (click to pause)"
            : "Auto-scroll paused (click to resume)"
        }
        active={props.autoScroll}
        activeColor="var(--accent)"
        onClick={props.onToggleAutoScroll}
      >
        ↓
      </ToolbarButton>

      <ToolbarButton title="Copy visible logs to clipboard" onClick={props.onCopyAll}>
        {props.copiedAll ? "✓" : "⎘"}
      </ToolbarButton>

      <Show when={props.canClear}>
        <ToolbarButton title="Clear all logs" onClick={props.onClear}>
          ⊘
        </ToolbarButton>
      </Show>
    </div>
  );
}
