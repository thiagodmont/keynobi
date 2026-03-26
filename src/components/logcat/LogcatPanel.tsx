import {
  type JSX,
  createSignal,
  createEffect,
  onMount,
  onCleanup,
  For,
  Show,
  createMemo,
} from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  startLogcat,
  stopLogcat,
  clearLogcat,
  getLogcatEntries,
  getLogcatStatus,
  listenLogcatEntries,
  listenLogcatCleared,
  type LogcatEntry,
} from "@/lib/tauri-api";
import { selectedDevice } from "@/stores/device.store";
import { showToast } from "@/components/common/Toast";
import Icon from "@/components/common/Icon";

// ── Log level config ──────────────────────────────────────────────────────────

const LEVEL_CONFIG = {
  verbose: { label: "V", color: "#9ca3af", bg: "transparent" },
  debug:   { label: "D", color: "#60a5fa", bg: "transparent" },
  info:    { label: "I", color: "#4ade80", bg: "transparent" },
  warn:    { label: "W", color: "#fbbf24", bg: "rgba(251,191,36,0.08)" },
  error:   { label: "E", color: "#f87171", bg: "rgba(248,113,113,0.10)" },
  fatal:   { label: "F", color: "#e879f9", bg: "rgba(232,121,249,0.12)" },
  unknown: { label: "?", color: "#9ca3af", bg: "transparent" },
} as const;

const LEVELS_IN_ORDER = ["verbose", "debug", "info", "warn", "error", "fatal"] as const;
type Level = typeof LEVELS_IN_ORDER[number];

function levelPriority(l: string): number {
  const idx = LEVELS_IN_ORDER.indexOf(l as Level);
  return idx >= 0 ? idx : 0;
}

// ── Store ─────────────────────────────────────────────────────────────────────

interface LogcatStore {
  entries: LogcatEntry[];
  streaming: boolean;
}

const MAX_UI_ENTRIES = 10_000;

const [logcatStore, setLogcatStore] = createStore<LogcatStore>({
  entries: [],
  streaming: false,
});

// ── LogcatPanel ───────────────────────────────────────────────────────────────

export function LogcatPanel(): JSX.Element {
  const [tagFilter, setTagFilter] = createSignal("");
  const [textFilter, setTextFilter] = createSignal("");
  const [minLevel, setMinLevel] = createSignal<Level>("verbose");
  const [paused, setPaused] = createSignal(false);
  const [autoScroll, setAutoScroll] = createSignal(true);

  let scrollRef!: HTMLDivElement;
  let unlistenEntries: (() => void) | undefined;
  let unlistenCleared: (() => void) | undefined;

  const filteredEntries = createMemo(() => {
    const tag = tagFilter().toLowerCase().trim();
    const text = textFilter().toLowerCase().trim();
    const minPrio = levelPriority(minLevel());

    return logcatStore.entries.filter((e) => {
      if (levelPriority(e.level) < minPrio) return false;
      if (tag && !e.tag.toLowerCase().includes(tag)) return false;
      if (text && !e.message.toLowerCase().includes(text) && !e.tag.toLowerCase().includes(text)) return false;
      return true;
    });
  });

  onMount(async () => {
    // Load buffered entries from the backend.
    try {
      const entries = await getLogcatEntries({ count: 2000 });
      setLogcatStore("entries", entries);
    } catch {
      // may fail if logcat hasn't started
    }

    // Check if already streaming.
    try {
      const isStreaming = await getLogcatStatus();
      setLogcatStore("streaming", isStreaming);
    } catch {
      // ignore
    }

    // Subscribe to new entries.
    unlistenEntries = await listenLogcatEntries((newEntries) => {
      if (paused()) return;
      setLogcatStore(
        produce((s) => {
          for (const e of newEntries) s.entries.push(e);
          if (s.entries.length > MAX_UI_ENTRIES) {
            s.entries.splice(0, s.entries.length - MAX_UI_ENTRIES);
          }
        })
      );
      if (autoScroll()) {
        setTimeout(() => {
          if (scrollRef) scrollRef.scrollTop = scrollRef.scrollHeight;
        }, 0);
      }
    });

    unlistenCleared = await listenLogcatCleared(() => {
      setLogcatStore("entries", []);
    });
  });

  onCleanup(() => {
    unlistenEntries?.();
    unlistenCleared?.();
  });

  // Auto-scroll when entries change and autoScroll is on.
  createEffect(() => {
    filteredEntries(); // read to subscribe
    if (autoScroll() && scrollRef) {
      scrollRef.scrollTop = scrollRef.scrollHeight;
    }
  });

  function handleScroll() {
    if (!scrollRef) return;
    const atBottom = scrollRef.scrollHeight - scrollRef.scrollTop - scrollRef.clientHeight < 50;
    setAutoScroll(atBottom);
  }

  async function handleStart() {
    try {
      const device = selectedDevice();
      await startLogcat(device?.serial ?? undefined);
      setLogcatStore("streaming", true);
      showToast("Logcat started", "success");
    } catch (e) {
      showToast(`Failed to start logcat: ${e}`, "error");
    }
  }

  async function handleStop() {
    try {
      await stopLogcat();
      setLogcatStore("streaming", false);
    } catch (e) {
      showToast(`Failed to stop logcat: ${e}`, "error");
    }
  }

  async function handleClear() {
    try {
      await clearLogcat();
    } catch (e) {
      showToast(`Failed to clear logcat: ${e}`, "error");
    }
  }

  const count = () => filteredEntries().length;

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        flex: "1",
        overflow: "hidden",
        background: "var(--bg-primary)",
      }}
    >
      {/* Toolbar */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "6px",
          padding: "6px 10px",
          background: "var(--bg-secondary)",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
          "flex-wrap": "wrap",
        }}
      >
        {/* Start/Stop button */}
        <Show
          when={logcatStore.streaming}
          fallback={
            <button onClick={handleStart} class="toolbar-btn" title="Start Logcat" style={btnStyle("#4ade80")}>
              <Icon name="play" size={13} /> Start
            </button>
          }
        >
          <button onClick={handleStop} class="toolbar-btn" title="Stop Logcat" style={btnStyle("#f87171")}>
            <Icon name="stop" size={13} /> Stop
          </button>
        </Show>

        {/* Pause/Resume */}
        <button
          onClick={() => setPaused((v) => !v)}
          title={paused() ? "Resume" : "Pause"}
          style={btnStyle(paused() ? "#fbbf24" : "var(--text-muted)")}
        >
          {paused() ? "▶ Resume" : "⏸ Pause"}
        </button>

        {/* Clear */}
        <button onClick={handleClear} title="Clear logcat" style={btnStyle("var(--text-muted)")}>
          <Icon name="trash" size={12} /> Clear
        </button>

        <div style={{ width: "1px", height: "18px", background: "var(--border)", "flex-shrink": "0" }} />

        {/* Level filter */}
        <select
          value={minLevel()}
          onChange={(e) => setMinLevel(e.currentTarget.value as Level)}
          style={{
            background: "var(--bg-primary)",
            border: "1px solid var(--border)",
            color: LEVEL_CONFIG[minLevel()].color,
            "border-radius": "4px",
            padding: "2px 6px",
            "font-size": "11px",
            cursor: "pointer",
            "font-weight": "600",
          }}
        >
          {LEVELS_IN_ORDER.map((l) => (
            <option value={l} style={{ color: LEVEL_CONFIG[l].color }}>
              {l.charAt(0).toUpperCase() + l.slice(1)}
            </option>
          ))}
        </select>

        {/* Tag filter */}
        <input
          type="text"
          placeholder="Filter by tag…"
          value={tagFilter()}
          onInput={(e) => setTagFilter(e.currentTarget.value)}
          style={filterInputStyle()}
        />

        {/* Text filter */}
        <input
          type="text"
          placeholder="Filter by text…"
          value={textFilter()}
          onInput={(e) => setTextFilter(e.currentTarget.value)}
          style={filterInputStyle()}
        />

        <div style={{ flex: "1" }} />

        {/* Entry count */}
        <span style={{ "font-size": "11px", color: "var(--text-muted)", "flex-shrink": "0" }}>
          {count().toLocaleString()} entries
        </span>

        {/* Streaming indicator */}
        <Show when={logcatStore.streaming}>
          <span
            style={{
              width: "6px",
              height: "6px",
              "border-radius": "50%",
              background: "#4ade80",
              "flex-shrink": "0",
              animation: "lsp-dot-pulse 2s ease-in-out infinite",
            }}
          />
        </Show>
      </div>

      {/* Empty state */}
      <Show
        when={logcatStore.entries.length === 0}
        fallback={null}
      >
        <div
          style={{
            flex: "1",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            "flex-direction": "column",
            gap: "8px",
            color: "var(--text-muted)",
            "font-size": "13px",
          }}
        >
          <Show
            when={logcatStore.streaming}
            fallback={
              <>
                <span style={{ "font-size": "24px", opacity: "0.3" }}>📋</span>
                <span>No logcat data yet</span>
                <span style={{ "font-size": "11px", opacity: "0.6" }}>
                  Connect a device and click Start
                </span>
              </>
            }
          >
            <span style={{ "font-size": "24px", opacity: "0.3" }}>⏳</span>
            <span>Waiting for log entries…</span>
          </Show>
        </div>
      </Show>

      {/* Log entries */}
      <Show when={logcatStore.entries.length > 0}>
        <div
          ref={scrollRef}
          onScroll={handleScroll}
          style={{
            flex: "1",
            overflow: "auto",
            "font-family": "var(--font-mono)",
            "font-size": "11px",
            "line-height": "1.5",
            padding: "4px 0",
          }}
        >
          <For each={filteredEntries()}>
            {(entry) => <LogcatRow entry={entry} />}
          </For>
        </div>
      </Show>
    </div>
  );
}

// ── LogcatRow ─────────────────────────────────────────────────────────────────

function LogcatRow(props: { entry: LogcatEntry }): JSX.Element {
  const cfg = () => LEVEL_CONFIG[props.entry.level] ?? LEVEL_CONFIG.unknown;

  return (
    <div
      style={{
        display: "flex",
        "align-items": "flex-start",
        gap: "6px",
        padding: "1px 10px",
        "border-bottom": "1px solid transparent",
        background: cfg().bg,
        "min-height": "20px",
      }}
    >
      {/* Timestamp */}
      <span
        style={{
          color: "var(--text-disabled, #4b5563)",
          "white-space": "nowrap",
          "flex-shrink": "0",
          "font-size": "10px",
          "padding-top": "1px",
        }}
      >
        {props.entry.timestamp}
      </span>

      {/* Level badge */}
      <span
        style={{
          color: cfg().color,
          "font-weight": "700",
          "min-width": "12px",
          "text-align": "center",
          "flex-shrink": "0",
        }}
      >
        {cfg().label}
      </span>

      {/* Tag */}
      <span
        style={{
          color: "var(--text-secondary)",
          "min-width": "80px",
          "max-width": "120px",
          overflow: "hidden",
          "text-overflow": "ellipsis",
          "white-space": "nowrap",
          "flex-shrink": "0",
          "font-size": "10px",
          "padding-top": "1px",
        }}
        title={props.entry.tag}
      >
        {props.entry.tag}
      </span>

      {/* Message */}
      <span
        style={{
          flex: "1",
          color: props.entry.isCrash ? "#f87171" : cfg().color === "#4ade80" ? "var(--text-primary)" : cfg().color,
          "white-space": "pre-wrap",
          "word-break": "break-all",
        }}
      >
        {props.entry.message}
      </span>
    </div>
  );
}

// ── Style helpers ─────────────────────────────────────────────────────────────

function btnStyle(color: string): Record<string, string> {
  return {
    display: "flex",
    "align-items": "center",
    gap: "4px",
    padding: "3px 8px",
    background: "none",
    border: "1px solid var(--border)",
    color,
    "border-radius": "4px",
    cursor: "pointer",
    "font-size": "11px",
    "white-space": "nowrap",
  };
}

function filterInputStyle(): Record<string, string> {
  return {
    background: "var(--bg-primary)",
    border: "1px solid var(--border)",
    color: "var(--text-primary)",
    "border-radius": "4px",
    padding: "3px 7px",
    "font-size": "11px",
    outline: "none",
    width: "130px",
  };
}
