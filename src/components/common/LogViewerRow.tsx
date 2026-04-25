import { type JSX, Show } from "solid-js";
import { copyToClipboard } from "@/utils/clipboard";
import type { LogEntry, LogLevel } from "@/bindings";
import { formatLogViewerTime } from "./log-viewer-format";

interface LevelMeta {
  label: string;
  color: string;
  badgeBg: string;
}

const LEVEL_META: Record<LogLevel, LevelMeta> = {
  error: { label: "ERR", color: "var(--error)", badgeBg: "rgba(241,76,76,0.15)" },
  warn: { label: "WARN", color: "var(--warning)", badgeBg: "rgba(204,167,0,0.15)" },
  info: { label: "INFO", color: "var(--info)", badgeBg: "rgba(117,190,255,0.12)" },
  debug: { label: "DBG", color: "var(--text-secondary)", badgeBg: "rgba(255,255,255,0.06)" },
  trace: { label: "TRC", color: "var(--text-disabled)", badgeBg: "rgba(255,255,255,0.03)" },
};

function LevelBadge(props: { level: LogLevel }): JSX.Element {
  const meta = () => LEVEL_META[props.level];
  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        "font-size": "10px",
        "font-weight": "600",
        "letter-spacing": "0.03em",
        padding: "1px 5px",
        "border-radius": "3px",
        background: meta().badgeBg,
        color: meta().color,
        "flex-shrink": "0",
        "min-width": "30px",
        "justify-content": "center",
      }}
    >
      {meta().label}
    </span>
  );
}

export function LogViewerRow(props: {
  entry: LogEntry;
  index: number;
  showTimestamp: boolean;
  showSource: boolean;
}): JSX.Element {
  const meta = () => LEVEL_META[props.entry.level];
  const isEven = () => props.index % 2 === 0;

  async function handleClick(): Promise<void> {
    await copyToClipboard(props.entry.message);
  }

  return (
    <div
      title="Click to copy message"
      onClick={handleClick}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "6px",
        padding: "0 8px",
        height: "20px",
        overflow: "hidden",
        background: isEven() ? "transparent" : "rgba(255,255,255,0.018)",
        cursor: "pointer",
        "border-left": `2px solid ${
          props.entry.level === "error"
            ? "var(--error)"
            : props.entry.level === "warn"
              ? "var(--warning)"
              : "transparent"
        }`,
        transition: "background 0.05s",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "var(--bg-hover)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = isEven() ? "transparent" : "rgba(255,255,255,0.018)";
      }}
    >
      <Show when={props.showTimestamp}>
        <span
          style={{
            color: "var(--text-disabled)",
            "flex-shrink": "0",
            "user-select": "none",
            "font-size": "11px",
          }}
        >
          {formatLogViewerTime(props.entry.timestamp)}
        </span>
      </Show>

      <LevelBadge level={props.entry.level} />

      <Show when={props.showSource && props.entry.source}>
        <span
          style={{
            color: "var(--text-muted)",
            "flex-shrink": "0",
            "font-size": "10px",
            "max-width": "120px",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.entry.source}
        </span>
      </Show>

      <span
        style={{
          color: meta().color,
          overflow: "hidden",
          "text-overflow": "ellipsis",
          "white-space": "nowrap",
          flex: "1",
          "min-width": "0",
          "line-height": "20px",
        }}
        title={props.entry.message}
      >
        {props.entry.message}
      </span>
    </div>
  );
}
