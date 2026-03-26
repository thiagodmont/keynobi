import {
  type JSX,
  For,
  Show,
  createMemo,
  createSignal,
  createEffect,
  on,
} from "solid-js";
import type { LogEntry, LogLevel } from "@/bindings";

// ── Types ─────────────────────────────────────────────────────────────────────

type LevelFilter = "all" | LogLevel;

export interface LogViewerProps {
  /** Reactive array of log entries (from a log store). */
  entries: LogEntry[];
  /** Called when the user clicks "Clear". */
  onClear?: () => void;
  /** Show the source chip beside each log row. Default: true. */
  showSource?: boolean;
  /** Placeholder text shown when there are no entries. */
  emptyMessage?: string;
}

// ── Level metadata ─────────────────────────────────────────────────────────────

interface LevelMeta {
  label: string;
  color: string;
  badgeBg: string;
}

const LEVEL_META: Record<LogLevel, LevelMeta> = {
  error: { label: "ERR",   color: "var(--error)",          badgeBg: "rgba(241,76,76,0.15)"  },
  warn:  { label: "WARN",  color: "var(--warning)",        badgeBg: "rgba(204,167,0,0.15)"  },
  info:  { label: "INFO",  color: "var(--info)",           badgeBg: "rgba(117,190,255,0.12)"},
  debug: { label: "DBG",   color: "var(--text-secondary)", badgeBg: "rgba(255,255,255,0.06)"},
  trace: { label: "TRC",   color: "var(--text-disabled)",  badgeBg: "rgba(255,255,255,0.03)"},
};

const LEVEL_FILTERS: { id: LevelFilter; label: string; color?: string }[] = [
  { id: "all",   label: "ALL" },
  { id: "error", label: "ERR",  color: "var(--error)"          },
  { id: "warn",  label: "WARN", color: "var(--warning)"        },
  { id: "info",  label: "INFO", color: "var(--info)"           },
  { id: "debug", label: "DBG",  color: "var(--text-secondary)" },
];

// ── Helpers ────────────────────────────────────────────────────────────────────

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    const ss = String(d.getSeconds()).padStart(2, "0");
    const ms = String(d.getMilliseconds()).padStart(3, "0");
    return `${hh}:${mm}:${ss}.${ms}`;
  } catch {
    return iso;
  }
}

function matchesFilter(entry: LogEntry, level: LevelFilter, source: string, search: string): boolean {
  if (level !== "all" && entry.level !== level) return false;
  if (source !== "all" && entry.source !== source) return false;
  if (search && !entry.message.toLowerCase().includes(search)) return false;
  return true;
}

async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Fallback: create a temporary textarea
    const el = document.createElement("textarea");
    el.value = text;
    document.body.appendChild(el);
    el.select();
    document.execCommand("copy");
    document.body.removeChild(el);
  }
}

// ── Sub-components ─────────────────────────────────────────────────────────────

function ToolbarButton(props: {
  title: string;
  active?: boolean;
  activeColor?: string;
  onClick: () => void;
  children: JSX.Element;
}): JSX.Element {
  return (
    <button
      title={props.title}
      onClick={props.onClick}
      style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        height: "22px",
        padding: "0 6px",
        "border-radius": "3px",
        border: "none",
        background: props.active
          ? (props.activeColor ? `${props.activeColor}22` : "var(--bg-active)")
          : "transparent",
        color: props.active
          ? (props.activeColor ?? "var(--text-primary)")
          : "var(--text-muted)",
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

// ── Main component ─────────────────────────────────────────────────────────────

export function LogViewer(props: LogViewerProps): JSX.Element {
  const showSource = () => props.showSource ?? true;

  // UI state
  const [levelFilter, setLevelFilter] = createSignal<LevelFilter>("all");
  const [sourceFilter, setSourceFilter] = createSignal<string>("all");
  const [search, setSearch] = createSignal("");
  const [showTimestamps, setShowTimestamps] = createSignal(true);
  const [autoScroll, setAutoScroll] = createSignal(true);
  const [copiedAll, setCopiedAll] = createSignal(false);

  let scrollRef!: HTMLDivElement;
  let userScrolledUp = false;

  // Derive unique sorted sources from the current entries.
  const uniqueSources = createMemo(() => {
    const seen = new Set<string>();
    for (const e of props.entries) {
      if (e.source) seen.add(e.source);
    }
    return Array.from(seen).sort();
  });

  // Filtered view
  const filtered = createMemo(() => {
    const q = search().toLowerCase();
    const lf = levelFilter();
    const sf = sourceFilter();
    return props.entries.filter((e) => matchesFilter(e, lf, sf, q));
  });

  // Auto-scroll to bottom when new entries arrive (if enabled)
  createEffect(
    on(
      () => filtered().length,
      () => {
        if (autoScroll() && scrollRef) {
          scrollRef.scrollTop = scrollRef.scrollHeight;
        }
      },
      { defer: true }
    )
  );

  function handleScroll(): void {
    if (!scrollRef) return;
    const atBottom =
      scrollRef.scrollHeight - scrollRef.scrollTop - scrollRef.clientHeight < 20;
    if (atBottom) {
      if (userScrolledUp) {
        userScrolledUp = false;
        setAutoScroll(true);
      }
    } else {
      userScrolledUp = true;
      setAutoScroll(false);
    }
  }

  async function handleCopyAll(): Promise<void> {
    const text = filtered()
      .map((e) => {
        const parts: string[] = [];
        if (showTimestamps()) parts.push(formatTime(e.timestamp));
        parts.push(`[${e.level.toUpperCase()}]`);
        if (showSource()) parts.push(`[${e.source}]`);
        parts.push(e.message);
        return parts.join(" ");
      })
      .join("\n");
    await copyToClipboard(text);
    setCopiedAll(true);
    setTimeout(() => setCopiedAll(false), 1500);
  }

  const totalCount = () => props.entries.length;
  const filteredCount = () => filtered().length;
  const isFiltered = () => levelFilter() !== "all" || sourceFilter() !== "all" || search() !== "";

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        overflow: "hidden",
        "font-family": "var(--font-mono)",
        "font-size": "12px",
        background: "var(--bg-secondary)",
      }}
    >
      {/* ── Toolbar ── */}
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
        {/* Level filter pills */}
        <div style={{ display: "flex", gap: "2px", "flex-shrink": "0" }}>
          <For each={LEVEL_FILTERS}>
            {(f) => (
              <ToolbarButton
                title={`Show ${f.label === "ALL" ? "all levels" : f.label + " and above"}`}
                active={levelFilter() === f.id}
                activeColor={f.color}
                onClick={() => setLevelFilter(f.id)}
              >
                {f.label}
              </ToolbarButton>
            )}
          </For>
        </div>

        {/* Divider */}
        <div
          style={{
            width: "1px",
            height: "16px",
            background: "var(--border)",
            "flex-shrink": "0",
            margin: "0 2px",
          }}
        />

        {/* Source filter — dropdown appears once there are 2+ distinct sources */}
        <Show when={uniqueSources().length > 1}>
          <select
            value={sourceFilter()}
            onChange={(e) => setSourceFilter(e.currentTarget.value)}
            title="Filter by log source"
            style={{
              height: "22px",
              padding: "0 4px",
              background: "var(--bg-quaternary)",
              border: `1px solid ${sourceFilter() !== "all" ? "var(--accent)" : "var(--border)"}`,
              "border-radius": "3px",
              color: sourceFilter() !== "all" ? "var(--accent)" : "var(--text-muted)",
              "font-family": "var(--font-ui)",
              "font-size": "11px",
              cursor: "pointer",
              "flex-shrink": "0",
              outline: "none",
            }}
          >
            <option value="all" style={{ background: "var(--bg-secondary)", color: "var(--text-primary)" }}>
              All sources
            </option>
            <For each={uniqueSources()}>
              {(src) => (
                <option value={src} style={{ background: "var(--bg-secondary)", color: "var(--text-primary)" }}>
                  {src}
                </option>
              )}
            </For>
          </select>
          {/* Divider after source filter */}
          <div
            style={{
              width: "1px",
              height: "16px",
              background: "var(--border)",
              "flex-shrink": "0",
              margin: "0 2px",
            }}
          />
        </Show>

        {/* Search box */}
        <input
          type="text"
          placeholder="Filter logs…"
          value={search()}
          onInput={(e) => setSearch(e.currentTarget.value)}
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

        {/* Entry count */}
        <span
          style={{
            "font-family": "var(--font-ui)",
            "font-size": "11px",
            color: "var(--text-muted)",
            "white-space": "nowrap",
            "flex-shrink": "0",
          }}
        >
          <Show
            when={isFiltered()}
            fallback={<>{totalCount()} entries</>}
          >
            {filteredCount()}/{totalCount()}
          </Show>
        </span>

        {/* Spacer */}
        <div style={{ flex: "1" }} />

        {/* Timestamp toggle */}
        <ToolbarButton
          title={showTimestamps() ? "Hide timestamps" : "Show timestamps"}
          active={showTimestamps()}
          onClick={() => setShowTimestamps((v) => !v)}
        >
          TS
        </ToolbarButton>

        {/* Auto-scroll toggle */}
        <ToolbarButton
          title={autoScroll() ? "Auto-scroll on (click to pause)" : "Auto-scroll paused (click to resume)"}
          active={autoScroll()}
          activeColor="var(--accent)"
          onClick={() => {
            const next = !autoScroll();
            setAutoScroll(next);
            if (next && scrollRef) {
              scrollRef.scrollTop = scrollRef.scrollHeight;
            }
          }}
        >
          ↓
        </ToolbarButton>

        {/* Copy all */}
        <ToolbarButton
          title="Copy visible logs to clipboard"
          onClick={handleCopyAll}
        >
          {copiedAll() ? "✓" : "⎘"}
        </ToolbarButton>

        {/* Clear */}
        <Show when={props.onClear}>
          <ToolbarButton
            title="Clear all logs"
            onClick={() => {
              props.onClear?.();
              setSourceFilter("all");
            }}
          >
            ⊘
          </ToolbarButton>
        </Show>
      </div>

      {/* ── Log list ── */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        style={{
          flex: "1",
          "overflow-y": "auto",
          "overflow-x": "hidden",
        }}
      >
        <Show
          when={filtered().length > 0}
          fallback={
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                height: "100%",
                "min-height": "60px",
                color: "var(--text-muted)",
                "font-family": "var(--font-ui)",
                "font-size": "12px",
              }}
            >
              {isFiltered()
                ? "No entries match the current filter"
                : (props.emptyMessage ?? "No log output yet")}
            </div>
          }
        >
          <For each={filtered()}>
            {(entry, index) => (
              <LogRow
                entry={entry}
                index={index()}
                showTimestamp={showTimestamps()}
                showSource={showSource()}
              />
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}

// ── Log row ────────────────────────────────────────────────────────────────────

interface LogRowProps {
  entry: LogEntry;
  index: number;
  showTimestamp: boolean;
  showSource: boolean;
}

function LogRow(props: LogRowProps): JSX.Element {
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
        "align-items": "flex-start",
        gap: "6px",
        padding: "2px 8px",
        background: isEven() ? "transparent" : "rgba(255,255,255,0.018)",
        cursor: "pointer",
        "border-left": `2px solid ${
          props.entry.level === "error"
            ? "var(--error)"
            : props.entry.level === "warn"
            ? "var(--warning)"
            : "transparent"
        }`,
        "min-height": "20px",
        transition: "background 0.05s",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLDivElement).style.background = "var(--bg-hover)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLDivElement).style.background = isEven()
          ? "transparent"
          : "rgba(255,255,255,0.018)";
      }}
    >
      {/* Timestamp */}
      <Show when={props.showTimestamp}>
        <span
          style={{
            color: "var(--text-disabled)",
            "flex-shrink": "0",
            "user-select": "none",
            "font-size": "11px",
            "padding-top": "1px",
          }}
        >
          {formatTime(props.entry.timestamp)}
        </span>
      </Show>

      {/* Level badge */}
      <LevelBadge level={props.entry.level} />

      {/* Source chip */}
      <Show when={props.showSource && props.entry.source}>
        <span
          style={{
            color: "var(--text-muted)",
            "flex-shrink": "0",
            "font-size": "10px",
            "padding-top": "2px",
            "max-width": "120px",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.entry.source}
        </span>
      </Show>

      {/* Message */}
      <span
        style={{
          color: meta().color,
          "word-break": "break-word",
          "white-space": "pre-wrap",
          flex: "1",
          "min-width": "0",
          "line-height": "1.5",
        }}
      >
        {props.entry.message}
      </span>
    </div>
  );
}
