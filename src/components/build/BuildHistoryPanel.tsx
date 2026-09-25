import { type JSX, For, Show, createMemo } from "solid-js";
import type { BuildRecord, BuildResult, BuildStatus } from "@/bindings";
import { buildState } from "@/stores/build.store";
import { Icon, Listbox } from "@/components/ui";
import { buildActorLabels, isAgent, startedByLabel } from "@/lib/build-actor";
import { LaunchTimingSummary } from "@/components/build/LaunchTimingSummary";

export interface BuildHistoryPanelProps {
  /** ID of the currently selected history entry. null = current build. */
  selectedId: number | null;
  /** Called when the user picks a history entry. null = current build. */
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
  if (status.state === "success") return "var(--success)";
  if (status.state === "failed") return "var(--error)";
  if (status.state === "cancelled") return "rgba(255,255,255,0.3)";
  return "var(--info)"; // running / idle
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

type HistoryItem = { kind: "live" } | { kind: "record"; record: BuildRecord };

const LIVE: HistoryItem = { kind: "live" };

function itemKey(item: HistoryItem): string {
  return item.kind === "live" ? "live" : `build-${item.record.id}`;
}

function recordOf(item: HistoryItem): BuildRecord | null {
  return item.kind === "record" ? item.record : null;
}

export function BuildHistoryPanel(props: BuildHistoryPanelProps): JSX.Element {
  const history = () => [...buildState.history].reverse();
  const currentTask = () => buildState.currentTask;
  const currentPhase = () => buildState.phase;

  const items = createMemo<HistoryItem[]>(() => [
    ...(currentPhase() === "running" && currentTask() ? [LIVE] : []),
    ...history().map((record): HistoryItem => ({ kind: "record", record })),
  ]);

  const isSelected = (item: HistoryItem) =>
    item.kind === "live" ? props.selectedId === null : props.selectedId === item.record.id;

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
            aria-label="Clear build history"
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
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.opacity = "1";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.opacity = "0.6";
            }}
          >
            <Icon name="trash" size={10} color="currentColor" />
          </button>
        </Show>
      </div>

      {/* The running build, then past builds newest first */}
      <Listbox
        label="Builds"
        items={items()}
        getKey={itemKey}
        isSelected={isSelected}
        onSelect={(item) => props.onSelect(item.kind === "live" ? null : item.record)}
      >
        {(item) => (
          <Show
            when={recordOf(item())}
            fallback={<LiveBuildRow selected={props.selectedId === null} />}
          >
            {(record) => <HistoryRow record={record()} selected={isSelected(item())} />}
          </Show>
        )}
      </Listbox>

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

function rowStyle(selected: boolean, accent: string): JSX.CSSProperties {
  return {
    display: "block",
    width: "100%",
    padding: "5px 8px",
    background: selected ? "rgba(255,255,255,0.07)" : "transparent",
    "border-left": `2px solid ${selected ? accent : "transparent"}`,
    "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.05))",
    "text-align": "left",
  };
}

function LiveBuildRow(props: { selected: boolean }): JSX.Element {
  return (
    <div style={rowStyle(props.selected, "var(--info)")}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          "margin-bottom": "2px",
        }}
      >
        <span style={{ "font-size": "9px", color: "var(--info)" }}>⟳</span>
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
          {buildState.currentTask}
        </span>
      </div>
      <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>running…</div>
      <Show when={isAgent(buildState.origin)}>
        <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>
          {startedByLabel(buildState.origin)}
        </div>
      </Show>
    </div>
  );
}

function HistoryRow(props: { record: BuildRecord; selected: boolean }): JSX.Element {
  const color = () => statusColor(props.record.status);
  const dur = () => durationLabel(props.record.status);
  const errs = () => errorCount(props.record);

  return (
    <div style={rowStyle(props.selected, color())}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          "margin-bottom": "2px",
        }}
      >
        <span style={{ "font-size": "9px", color: color(), "flex-shrink": "0" }}>
          {statusIcon(props.record.status)}
        </span>
        <span
          style={{
            "font-size": "9px",
            color: "rgba(255,255,255,0.6)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
          title={props.record.task}
        >
          {props.record.task}
        </span>
      </div>
      <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>
        {dur() ? `${dur()} · ` : ""}
        {relativeTime(props.record.startedAt)}
      </div>
      <Show when={errs() > 0}>
        <div style={{ "font-size": "9px", color: "var(--error)" }}>
          {errs()} error{errs() !== 1 ? "s" : ""}
        </div>
      </Show>
      <Show when={props.record.launch}>
        <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>
          <LaunchTimingSummary record={props.record} history={buildState.history} />
        </div>
      </Show>
      <For each={buildActorLabels(props.record.origin, props.record.cancelledBy)}>
        {(label) => <div style={{ "font-size": "9px", color: "var(--text-muted)" }}>{label}</div>}
      </For>
    </div>
  );
}
