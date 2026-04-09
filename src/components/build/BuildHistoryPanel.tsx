import { type JSX, For, Show } from "solid-js";
import type { BuildRecord, BuildResult, BuildStatus } from "@/bindings";
import { buildState } from "@/stores/build.store";
import Icon from "@/components/common/Icon";

export interface BuildHistoryPanelProps {
  /** ID of the currently selected history entry. null = current build. */
  selectedId: number | null;
  /** Called when the user clicks a history entry. null = current build. */
  onSelect: (record: BuildRecord | null) => void;
  /** Called when the user clicks the clear-history button. */
  onClear?: () => void;
}

export function statusIcon(status: BuildStatus): string {
  if (status.state === "running") return "⟳";
  if (status.state === "success") return "✓";
  if (status.state === "failed") return "✗";
  if (status.state === "cancelled") return "◼";
  return "•";
}

export function statusColor(status: BuildStatus): string {
  if (status.state === "success") return "#4ade80";
  if (status.state === "failed") return "#f87171";
  if (status.state === "cancelled") return "rgba(255,255,255,0.3)";
  return "#60a5fa"; // running / idle
}

export function durationLabel(status: BuildStatus): string {
  if (status.state !== "success" && status.state !== "failed") return "";
  const ms = Number((status as BuildResult).durationMs ?? 0);
  if (!ms) return "";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  return `${mins}m ${secs}s`;
}

export function errorCount(record: BuildRecord): number {
  return record.errors.filter((e) => e.severity === "error").length;
}

export function relativeTime(isoString: string): string {
  const diffMs = Date.now() - new Date(isoString).getTime();
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return "just now";
  if (sec < 3600) return `${Math.floor(sec / 60)}m ago`;
  if (sec < 86400) return `${Math.floor(sec / 3600)}h ago`;
  return `${Math.floor(sec / 86400)}d ago`;
}

export function BuildHistoryPanel(props: BuildHistoryPanelProps): JSX.Element {
  const history = () => [...buildState.history].reverse();
  const currentTask = () => buildState.currentTask;
  const currentPhase = () => buildState.phase;
  const isCurrentSelected = () => props.selectedId === null;

  return (
    <div
      style={{
        width: "140px",
        "flex-shrink": "0",
        "border-right": "1px solid var(--border)",
        display: "flex",
        "flex-direction": "column",
        "overflow-y": "auto",
        "overflow-x": "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          "font-size": "9px",
          color: "var(--text-disabled)",
          padding: "5px 8px 3px",
          "text-transform": "uppercase",
          "letter-spacing": "0.06em",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
        }}
      >
        <span>Builds</span>
        <Show when={history().length > 0 && props.onClear}>
          <button
            title="Clear build history"
            onClick={() => props.onClear?.()}
            style={{
              background: "transparent",
              border: "none",
              cursor: "pointer",
              padding: "0 2px",
              color: "var(--text-disabled)",
              display: "flex",
              "align-items": "center",
              opacity: "0.6",
            }}
            onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.6"; }}
          >
            <Icon name="trash" size={10} color="currentColor" />
          </button>
        </Show>
      </div>

      {/* Current in-progress build */}
      <Show when={currentPhase() === "running" && currentTask()}>
        <button
          onClick={() => props.onSelect(null)}
          style={{
            display: "block",
            width: "100%",
            padding: "5px 8px",
            background: isCurrentSelected()
              ? "rgba(255,255,255,0.09)"
              : "transparent",
            "border-left": `2px solid ${isCurrentSelected() ? "#60a5fa" : "transparent"}`,
            "border-right": "none",
            "border-top": "none",
            "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.05))",
            cursor: "pointer",
            "text-align": "left",
          }}
        >
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "4px",
              "margin-bottom": "2px",
            }}
          >
            <span style={{ "font-size": "9px", color: "#60a5fa" }}>⟳</span>
            <span
              style={{
                "font-size": "9px",
                color: "rgba(255,255,255,0.8)",
                "font-weight": "600",
                overflow: "hidden",
                "text-overflow": "ellipsis",
                "white-space": "nowrap",
              }}
            >
              {currentTask()}
            </span>
          </div>
          <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>running…</div>
        </button>
      </Show>

      {/* History entries (newest first) */}
      <For each={history()}>
        {(record) => {
          const icon = statusIcon(record.status);
          const color = statusColor(record.status);
          const dur = durationLabel(record.status);
          const errs = errorCount(record);
          const rel = relativeTime(record.startedAt);
          const selected = () => props.selectedId === record.id;

          return (
            <button
              onClick={() => props.onSelect(record)}
              style={{
                display: "block",
                width: "100%",
                padding: "5px 8px",
                background: selected() ? "rgba(255,255,255,0.07)" : "transparent",
                "border-left": `2px solid ${selected() ? color : "transparent"}`,
                "border-right": "none",
                "border-top": "none",
                "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.05))",
                cursor: "pointer",
                "text-align": "left",
              }}
            >
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "4px",
                  "margin-bottom": "2px",
                }}
              >
                <span style={{ "font-size": "9px", color, "flex-shrink": "0" }}>{icon}</span>
                <span
                  style={{
                    "font-size": "9px",
                    color: "rgba(255,255,255,0.6)",
                    overflow: "hidden",
                    "text-overflow": "ellipsis",
                    "white-space": "nowrap",
                  }}
                  title={record.task}
                >
                  {record.task}
                </span>
              </div>
              <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>
                {dur ? `${dur} · ` : ""}{rel}
              </div>
              <Show when={errs > 0}>
                <div style={{ "font-size": "9px", color: "#f87171" }}>
                  {errs} error{errs !== 1 ? "s" : ""}
                </div>
              </Show>
            </button>
          );
        }}
      </For>

      {/* Empty state */}
      <Show when={history().length === 0 && currentPhase() !== "running"}>
        <div
          style={{
            padding: "12px 8px",
            "font-size": "10px",
            color: "var(--text-muted)",
            "text-align": "center",
            "line-height": "1.5",
          }}
        >
          No builds yet
        </div>
      </Show>
    </div>
  );
}
