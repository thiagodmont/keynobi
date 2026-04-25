import { type JSX, Show, createMemo, createSignal, onCleanup } from "solid-js";
import { VirtualList } from "@/components/ui";
import { copyToClipboard } from "@/utils/clipboard";
import { debounce } from "@/utils/debounce";
import { formatBuildLogToolbarCount } from "./log-viewer-toolbar-count";
import { formatLogViewerEntry } from "./log-viewer-format";
import { LogViewerRow } from "./LogViewerRow";
import { LogViewerToolbar } from "./LogViewerToolbar";
import {
  matchesLogViewerFilter,
  uniqueLogViewerSources,
  type LogViewerLevelFilter,
} from "./log-viewer-filter";
import type { LogEntry } from "@/bindings";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface LogViewerProps {
  /** Reactive array of log entries (from a log store). */
  entries: LogEntry[];
  /** Called when the user clicks "Clear". */
  onClear?: () => void;
  /** Show the source chip beside each log row. Default: true. */
  showSource?: boolean;
  /** Placeholder text shown when there are no entries. */
  emptyMessage?: string;
  /** When false, the log list starts with follow-tail paused. Default true. */
  defaultAutoScroll?: boolean;
}

// ── Main component ─────────────────────────────────────────────────────────────

export function LogViewer(props: LogViewerProps): JSX.Element {
  const showSource = () => props.showSource ?? true;

  // UI state
  const [levelFilter, setLevelFilter] = createSignal<LogViewerLevelFilter>("all");
  const [sourceFilter, setSourceFilter] = createSignal<string>("all");
  const [rawSearch, setRawSearch] = createSignal("");
  const [debouncedSearch, setDebouncedSearch] = createSignal("");
  const [showTimestamps, setShowTimestamps] = createSignal(true);
  const [autoScroll, setAutoScroll] = createSignal(props.defaultAutoScroll !== false);
  const [copiedAll, setCopiedAll] = createSignal(false);

  const updateDebouncedSearch = debounce((q: string) => setDebouncedSearch(q), 150);
  let copyTimeout: ReturnType<typeof setTimeout> | undefined;

  // Derive unique sorted sources from the current entries.
  const uniqueSources = createMemo(() => uniqueLogViewerSources(props.entries));

  onCleanup(() => {
    updateDebouncedSearch.cancel();
    clearTimeout(copyTimeout);
  });

  // Filtered view
  const filtered = createMemo(() => {
    const q = debouncedSearch().toLowerCase();
    const lf = levelFilter();
    const sf = sourceFilter();
    return props.entries.filter((e) => matchesLogViewerFilter(e, lf, sf, q));
  });

  async function handleCopyAll(): Promise<void> {
    const text = filtered()
      .map((e) =>
        formatLogViewerEntry(e, { showTimestamp: showTimestamps(), showSource: showSource() })
      )
      .join("\n");
    await copyToClipboard(text);
    setCopiedAll(true);
    clearTimeout(copyTimeout);
    copyTimeout = setTimeout(() => setCopiedAll(false), 1500);
  }

  const totalCount = () => props.entries.length;
  const filteredCount = () => filtered().length;
  const isFiltered = () =>
    levelFilter() !== "all" || sourceFilter() !== "all" || debouncedSearch() !== "";
  const buildLogToolbarCount = createMemo(() =>
    formatBuildLogToolbarCount({
      filterActive: isFiltered(),
      visible: filteredCount(),
      total: totalCount(),
    })
  );

  function handleSearchInput(q: string): void {
    setRawSearch(q);
    updateDebouncedSearch(q);
  }

  function handleClear(): void {
    props.onClear?.();
    updateDebouncedSearch.cancel();
    setSourceFilter("all");
    setRawSearch("");
    setDebouncedSearch("");
  }

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
      <LogViewerToolbar
        levelFilter={levelFilter()}
        onLevelFilterChange={setLevelFilter}
        sourceFilter={sourceFilter()}
        onSourceFilterChange={setSourceFilter}
        sources={uniqueSources()}
        rawSearch={rawSearch()}
        onRawSearchChange={handleSearchInput}
        countText={buildLogToolbarCount().text}
        countTitle={buildLogToolbarCount().title}
        showTimestamps={showTimestamps()}
        onToggleTimestamps={() => setShowTimestamps((v) => !v)}
        autoScroll={autoScroll()}
        onToggleAutoScroll={() => setAutoScroll((v) => !v)}
        copiedAll={copiedAll()}
        onCopyAll={handleCopyAll}
        canClear={props.onClear !== undefined}
        onClear={handleClear}
      />

      {/* ── Log list ── */}
      <div style={{ flex: "1", overflow: "hidden" }}>
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
          <VirtualList
            items={filtered()}
            rowHeight={20}
            autoScroll={autoScroll()}
            onScrolledUp={() => setAutoScroll(false)}
            onScrolledToBottom={() => setAutoScroll(true)}
            renderRow={(entry, index) => (
              <LogViewerRow
                entry={entry}
                index={index}
                showTimestamp={showTimestamps()}
                showSource={showSource()}
              />
            )}
            style={{ height: "100%" }}
          />
        </Show>
      </div>
    </div>
  );
}
