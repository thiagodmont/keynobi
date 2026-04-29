import { Show, type JSX } from "solid-js";
import { Icon } from "@/components/ui";
import { btnStyle } from "./logcat-styles";

export function LogcatToolbar(props: {
  streaming: boolean;
  paused: boolean;
  restarting: boolean;
  crashes: number;
  selectedCount: number;
  autoScroll: boolean;
  toolbarCount: { text: string; title: string };
  onStart: () => void;
  onStop: () => void;
  onTogglePaused: () => void;
  onRestart: () => void;
  onClear: () => void;
  onJumpToLastCrash: () => void;
  onJumpToPreviousCrash: () => void;
  onJumpToNextCrash: () => void;
  onCopySelectedRows: () => void;
  onScrollToEnd: () => void;
  onExport: () => void;
  renderSavedFilterMenu: () => JSX.Element;
}): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "align-items": "flex-start",
        gap: "6px",
        padding: "5px 10px",
        background: "var(--bg-secondary)",
        "border-bottom": "1px solid var(--border)",
        "flex-shrink": "0",
        "flex-wrap": "wrap",
      }}
    >
      <Show
        when={props.streaming}
        fallback={
          <button
            onClick={() => props.onStart()}
            title="Start Logcat"
            style={btnStyle("var(--success)")}
          >
            <Icon name="play" size={13} /> Start
          </button>
        }
      >
        <button onClick={() => props.onStop()} title="Stop Logcat" style={btnStyle("var(--error)")}>
          <Icon name="stop" size={13} /> Stop
        </button>
      </Show>

      <button
        onClick={() => props.onTogglePaused()}
        title={props.paused ? "Resume" : "Pause new entries"}
        style={btnStyle(props.paused ? "var(--warning)" : "var(--text-muted)")}
      >
        {props.paused ? "▶" : "⏸"}
      </button>

      <button
        onClick={() => props.onRestart()}
        title="Stop, clear and restart logcat"
        disabled={props.restarting}
        style={btnStyle("var(--text-muted)")}
      >
        ↺ Restart
      </button>

      <button
        onClick={() => props.onClear()}
        title="Clear logcat buffer"
        style={btnStyle("var(--text-muted)")}
      >
        <Icon name="trash" size={12} />
      </button>

      <div
        style={{ width: "1px", height: "18px", background: "var(--border)", "flex-shrink": "0" }}
      />

      <Show when={props.crashes > 0}>
        <div style={{ display: "flex", "align-items": "center", gap: "2px", "flex-shrink": "0" }}>
          <button
            onClick={() => props.onJumpToLastCrash()}
            title={`${props.crashes} crash${props.crashes !== 1 ? "es" : ""} — click to jump`}
            style={{
              ...btnStyle("var(--error)"),
              gap: "3px",
              animation: "lsp-dot-pulse 3s ease-in-out infinite",
            }}
          >
            ⚡ {props.crashes}
          </button>
          <button
            onClick={() => props.onJumpToPreviousCrash()}
            title="Previous crash"
            style={btnStyle("var(--text-muted)")}
          >
            ↑
          </button>
          <button
            onClick={() => props.onJumpToNextCrash()}
            title="Next crash"
            style={btnStyle("var(--text-muted)")}
          >
            ↓
          </button>
        </div>
      </Show>

      <Show when={props.selectedCount > 0}>
        <button
          onClick={() => props.onCopySelectedRows()}
          title={
            props.selectedCount === 1
              ? "Copy selected row"
              : `Copy ${props.selectedCount} selected rows`
          }
          style={btnStyle("var(--accent)")}
        >
          ⎘ {props.selectedCount === 1 ? "1 row" : `${props.selectedCount} rows`}
        </button>
      </Show>

      {props.renderSavedFilterMenu()}

      <button
        onClick={() => props.onScrollToEnd()}
        title="Scroll to end"
        style={btnStyle(props.autoScroll ? "var(--text-muted)" : "var(--accent)")}
      >
        ↓
      </button>

      <button
        onClick={() => props.onExport()}
        title="Export filtered log to file"
        style={btnStyle("var(--text-muted)")}
      >
        ↓ Export
      </button>

      <div style={{ flex: "1" }} />

      <span
        title={props.toolbarCount.title}
        style={{
          "font-size": "11px",
          color: "var(--text-muted)",
          "flex-shrink": "0",
          cursor: "default",
        }}
      >
        {props.toolbarCount.text}
      </span>

      <Show when={props.streaming}>
        <span
          style={{
            width: "6px",
            height: "6px",
            "border-radius": "50%",
            background: "var(--success)",
            "flex-shrink": "0",
            animation: "lsp-dot-pulse 2s ease-in-out infinite",
          }}
        />
      </Show>
    </div>
  );
}
