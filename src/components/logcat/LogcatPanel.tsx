import {
  type JSX,
  createSignal,
  createMemo,
  onMount,
  onCleanup,
  Show,
  For,
  untrack,
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
  listenDeviceListChanged,
  type LogcatEntry,
} from "@/lib/tauri-api";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { save } from "@tauri-apps/plugin-dialog";
import { selectedDevice } from "@/stores/device.store";
import { settingsState } from "@/stores/settings.store";
import { showToast } from "@/components/common/Toast";
import { VirtualList } from "@/components/common/VirtualList";
import { QueryBar } from "@/components/logcat/QueryBar";
import Icon from "@/components/common/Icon";
import {
  parseQuery,
  matchesQuery,
  setAgeInQuery,
} from "@/lib/logcat-query";

// ── Level config ──────────────────────────────────────────────────────────────

const LEVEL_CONFIG = {
  verbose: { label: "V", color: "#9ca3af", bg: "transparent" },
  debug:   { label: "D", color: "#60a5fa", bg: "transparent" },
  info:    { label: "I", color: "#4ade80", bg: "transparent" },
  warn:    { label: "W", color: "#fbbf24", bg: "rgba(251,191,36,0.08)" },
  error:   { label: "E", color: "#f87171", bg: "rgba(248,113,113,0.10)" },
  fatal:   { label: "F", color: "#e879f9", bg: "rgba(232,121,249,0.12)" },
  unknown: { label: "?", color: "#9ca3af", bg: "transparent" },
} as const;

// ── Store ─────────────────────────────────────────────────────────────────────

const MAX_UI_ENTRIES = 20_000;

let evictionPending = false;
let lastEvictionTime = 0;
const EVICTION_INTERVAL_MS = 1_000;

interface LogcatStore {
  entries: LogcatEntry[];
  streaming: boolean;
}

const [logcatStore, setLogcatStore] = createStore<LogcatStore>({
  entries: [],
  streaming: false,
});

// ── Incremental autocomplete data (NOT reactive memos on the full store) ──────
//
// These are updated in O(batch_size) per new batch — not O(total_entries).
// They are exposed as throttled signals so the QueryBar can show suggestions
// without blocking the rendering pipeline.

const _pkgSet = new Set<string>();
const _tagFreqMap = new Map<string, number>();

const [knownPackages, setKnownPackages] = createSignal<string[]>([]);
const [knownTags, setKnownTags] = createSignal<string[]>([]);
let _suggestTimer: ReturnType<typeof setTimeout> | null = null;

/** Ingest new entries into the autocomplete index (O(batch_size)). */
function ingestForSuggestions(entries: LogcatEntry[]) {
  for (const e of entries) {
    if (e.package) _pkgSet.add(e.package);
    if (!e.kind || e.kind === "normal") {
      _tagFreqMap.set(e.tag, (_tagFreqMap.get(e.tag) ?? 0) + 1);
    }
  }
}

/** Flush suggestions to reactive signals — throttled to at most once per 3 s. */
function flushSuggestions(immediate = false) {
  if (_suggestTimer !== null && !immediate) return;
  if (_suggestTimer) clearTimeout(_suggestTimer);
  _suggestTimer = setTimeout(
    () => {
      _suggestTimer = null;
      setKnownPackages(Array.from(_pkgSet).sort());
      setKnownTags(
        Array.from(_tagFreqMap.entries())
          .sort((a, b) => b[1] - a[1])
          .slice(0, 50)
          .map(([tag]) => tag)
      );
    },
    immediate ? 0 : 3_000
  );
}

function clearSuggestions() {
  _pkgSet.clear();
  _tagFreqMap.clear();
  setKnownPackages([]);
  setKnownTags([]);
  if (_suggestTimer !== null) { clearTimeout(_suggestTimer); _suggestTimer = null; }
}

// ── Saved presets (localStorage) ──────────────────────────────────────────────

interface LogcatPreset {
  name: string;
  query: string;
  builtin?: true;
}

const BUILTIN_PRESETS: LogcatPreset[] = [
  { name: "My App",       query: "package:mine",  builtin: true },
  { name: "Crashes",      query: "is:crash",      builtin: true },
  { name: "Errors+",      query: "level:error",   builtin: true },
  { name: "Last 5 min",   query: "age:5m",        builtin: true },
];

const PRESET_KEY = "logcat_presets_v1";

function loadUserPresets(): LogcatPreset[] {
  try {
    const raw = localStorage.getItem(PRESET_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function saveUserPresets(presets: LogcatPreset[]): void {
  localStorage.setItem(PRESET_KEY, JSON.stringify(presets));
}

// ── LogcatPanel ───────────────────────────────────────────────────────────────

const ROW_HEIGHT = 20;

const AGE_PILLS = [
  { label: "30s", value: "30s" },
  { label: "1m",  value: "1m"  },
  { label: "5m",  value: "5m"  },
  { label: "15m", value: "15m" },
  { label: "1h",  value: "1h"  },
  { label: "All", value: null  },
] as const;

export function LogcatPanel(): JSX.Element {
  // ── Query — separate display value (immediate) from filter value (debounced)
  const [query, setQuery] = createSignal("");
  const [debouncedQuery, setDebouncedQuery] = createSignal("");
  let _queryDebounce: ReturnType<typeof setTimeout> | undefined;

  function updateQuery(q: string) {
    setQuery(q);
    clearTimeout(_queryDebounce);
    _queryDebounce = setTimeout(() => setDebouncedQuery(q), 150);
  }

  const [autoScroll, setAutoScroll] = createSignal(true);
  const [paused, setPaused] = createSignal(false);

  // Crash navigation
  const [jumpTarget, setJumpTarget] = createSignal<number | null>(null);
  const [crashCursor, setCrashCursor] = createSignal(0);

  // Row selection for multi-copy
  const [selectionAnchor, setSelectionAnchor] = createSignal<number | null>(null);
  const [selectionEnd, setSelectionEnd] = createSignal<number | null>(null);

  // Preset UI
  const [presetsOpen, setPresetsOpen] = createSignal(false);
  const [userPresets, setUserPresets] = createSignal<LogcatPreset[]>(loadUserPresets());
  const [savingPreset, setSavingPreset] = createSignal(false);
  const [presetNameDraft, setPresetNameDraft] = createSignal("");

  // Now signal for age filter reactivity (updates every 5s when age token exists)
  const [now, setNow] = createSignal(Date.now());

  // ── filterTick — the ONLY way filteredEntries re-runs on store changes.
  //   Rate-limited to one requestAnimationFrame per batch arrival so the
  //   O(n × matchesQuery) scan never runs more than ~60 times/sec.
  const [filterTick, setFilterTick] = createSignal(0);
  let _filterRafId: number | null = null;

  function scheduleFilterUpdate() {
    if (_filterRafId !== null) return;
    _filterRafId = requestAnimationFrame(() => {
      _filterRafId = null;
      setFilterTick((t) => t + 1);
    });
  }

  let unlistenEntries: (() => void) | undefined;
  let unlistenCleared: (() => void) | undefined;
  let unlistenDevices: (() => void) | undefined;
  let nowTimer: ReturnType<typeof setInterval> | undefined;

  // ── Parsed query (uses debounced value — no full rescan on every keystroke)
  const parsedTokens = createMemo(() => parseQuery(debouncedQuery()));
  const hasAgeFilter = createMemo(() => parsedTokens().some((t) => t.type === "age"));

  // ── filteredEntries — decoupled from store by untrack + filterTick ─────────
  //
  // KEY DESIGN: This memo does NOT subscribe to logcatStore.entries directly.
  // Instead it reads the entries via untrack() and is driven by:
  //   • parsedTokens() — immediately re-runs when the query changes
  //   • filterTick()   — re-runs at rAF rate when new entries arrive
  //   • now()          — re-runs every 5s only when age filter is active
  //
  // This prevents the O(n × matchesQuery) scan from running on every 100ms
  // IPC batch (which caused the freeze).  With rAF throttling the scan runs
  // at most ~60×/sec, but in practice much less.
  //
  // equals:false ensures VirtualList always gets notified (even when the
  // no-filter path returns the same array reference) so it can update its
  // visible slice.
  const filteredEntries = createMemo(
    () => {
      const tokens = parsedTokens();    // track: query changes trigger immediate rescan
      filterTick();                      // track: new entries trigger rAF-limited rescan
      const currentNow = hasAgeFilter() ? now() : Date.now();
      const entries = untrack(() => logcatStore.entries); // read WITHOUT subscribing
      if (tokens.length === 0) return entries;
      return entries.filter((e) => matchesQuery(e, tokens, currentNow));
    },
    undefined,
    { equals: false } // always propagate so VirtualList slice stays current
  );

  const crashIndices = createMemo(() => {
    const indices: number[] = [];
    filteredEntries().forEach((e, i) => { if (e.isCrash) indices.push(i); });
    return indices;
  });

  const activeAge = createMemo(() => {
    const t = parsedTokens().find((t) => t.type === "age") as { type: "age"; seconds: number } | undefined;
    if (!t) return null;
    // Reverse-map seconds to pill label
    for (const p of AGE_PILLS) {
      if (p.value && parseAge(p.value) === t.seconds) return p.value;
    }
    return null;
  });

  function parseAge(v: string): number {
    const m = v.match(/^(\d+)(s|m|h)$/i);
    if (!m) return 0;
    const n = parseInt(m[1]);
    switch (m[2].toLowerCase()) {
      case "s": return n;
      case "m": return n * 60;
      case "h": return n * 3600;
      default: return 0;
    }
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  onMount(async () => {
    try {
      const entries = await getLogcatEntries({ count: 2000 });
      setLogcatStore("entries", entries);
      // Build initial autocomplete index immediately (not throttled)
      ingestForSuggestions(entries);
      flushSuggestions(true);
      scheduleFilterUpdate();
    } catch { /* ignore */ }

    try {
      const streaming = await getLogcatStatus();
      setLogcatStore("streaming", streaming);
    } catch { /* ignore */ }

    unlistenEntries = await listenLogcatEntries((newEntries) => {
      if (paused()) return;
      const n = Date.now();
      const shouldEvict = logcatStore.entries.length + newEntries.length > MAX_UI_ENTRIES;
      if (shouldEvict && n - lastEvictionTime > EVICTION_INTERVAL_MS) {
        evictionPending = true;
        lastEvictionTime = n;
      }
      setLogcatStore(
        produce((s) => {
          for (const e of newEntries) s.entries.push(e);
          if (evictionPending && s.entries.length > MAX_UI_ENTRIES) {
            s.entries.splice(0, s.entries.length - MAX_UI_ENTRIES);
            evictionPending = false;
          }
        })
      );
      // Incremental autocomplete update (O(batch_size), not O(total))
      ingestForSuggestions(newEntries);
      flushSuggestions();
      // Request a filter update at rAF rate — never more than ~60/sec
      scheduleFilterUpdate();
    });

    unlistenCleared = await listenLogcatCleared(() => {
      setLogcatStore("entries", []);
      clearSuggestions();
      setFilterTick(0);
    });

    // Auto-start on device connect
    unlistenDevices = await listenDeviceListChanged((devices) => {
      if (logcatStore.streaming) return;
      const hasAutoStart = (settingsState as any).logcat?.autoStart !== false;
      if (!hasAutoStart) return;
      const online = devices.find((d) => d.connectionState === "online");
      if (online) {
        startLogcat(online.serial)
          .then(() => setLogcatStore("streaming", true))
          .catch(() => {});
      }
    });

    // Age filter timer
    nowTimer = setInterval(() => setNow(Date.now()), 5_000);
  });

  onCleanup(() => {
    unlistenEntries?.();
    unlistenCleared?.();
    unlistenDevices?.();
    clearInterval(nowTimer);
    clearTimeout(_queryDebounce);
    if (_filterRafId !== null) cancelAnimationFrame(_filterRafId);
  });

  // ── Controls ──────────────────────────────────────────────────────────────

  async function handleStart() {
    try {
      const device = selectedDevice();
      await startLogcat(device?.serial ?? undefined);
      setLogcatStore("streaming", true);
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
      setSelectionAnchor(null);
      setSelectionEnd(null);
    } catch (e) {
      showToast(`Failed to clear logcat: ${e}`, "error");
    }
  }

  // ── Crash navigation ──────────────────────────────────────────────────────

  function jumpToCrash(direction: 1 | -1) {
    const indices = crashIndices();
    if (indices.length === 0) return;
    const next = Math.max(0, Math.min(indices.length - 1, crashCursor() + direction));
    setCrashCursor(next);
    setJumpTarget(indices[next]);
    setAutoScroll(false);
  }

  function jumpToLastCrash() {
    const indices = crashIndices();
    if (!indices.length) return;
    const last = indices.length - 1;
    setCrashCursor(last);
    setJumpTarget(indices[last]);
    setAutoScroll(false);
  }

  // Reset crash cursor when entries change
  createMemo(() => {
    filteredEntries(); // subscribe
    setCrashCursor((c) => Math.min(c, Math.max(0, crashIndices().length - 1)));
  });

  // ── Row copy ──────────────────────────────────────────────────────────────

  function formatEntry(e: LogcatEntry): string {
    const pkg = e.package ? `[${e.package}] ` : "";
    return `${e.timestamp}  ${e.level.toUpperCase()}  ${pkg}${e.tag}: ${e.message}`;
  }

  async function copyRow(idx: number) {
    const entry = filteredEntries()[idx];
    if (!entry) return;
    try {
      await navigator.clipboard.writeText(formatEntry(entry));
      showToast("Copied to clipboard", "success");
    } catch { /* ignore */ }
  }

  function handleRowClick(idx: number, e: MouseEvent) {
    if (e.shiftKey && selectionAnchor() !== null) {
      setSelectionEnd(idx);
    } else {
      setSelectionAnchor(idx);
      setSelectionEnd(null);
      copyRow(idx);
    }
  }

  function getSelectionRange(): [number, number] | null {
    const a = selectionAnchor();
    const b = selectionEnd();
    if (a === null) return null;
    if (b === null) return [a, a];
    return [Math.min(a, b), Math.max(a, b)];
  }

  async function copySelectedRows() {
    const range = getSelectionRange();
    if (!range) return;
    const [lo, hi] = range;
    const text = filteredEntries().slice(lo, hi + 1).map(formatEntry).join("\n");
    await navigator.clipboard.writeText(text);
    showToast(`Copied ${hi - lo + 1} rows`, "success");
    setSelectionAnchor(null);
    setSelectionEnd(null);
  }

  // ── Export ────────────────────────────────────────────────────────────────

  async function handleExport() {
    try {
      const path = await save({
        filters: [{ name: "Log", extensions: ["log", "txt"] }],
        defaultPath: "logcat.log",
      });
      if (!path) return;
      const text = filteredEntries().map(formatEntry).join("\n");
      await writeTextFile(path, text);
      showToast(`Exported ${filteredEntries().length} entries`, "success");
    } catch (e) {
      showToast(`Export failed: ${e}`, "error");
    }
  }

  // ── Presets ───────────────────────────────────────────────────────────────

  function applyPreset(q: string) {
    updateQuery(q);
    setPresetsOpen(false);
  }

  function saveCurrentPreset() {
    const name = presetNameDraft().trim();
    if (!name) return;
    const presets = [...userPresets(), { name, query: query() }];
    setUserPresets(presets);
    saveUserPresets(presets);
    setSavingPreset(false);
    setPresetNameDraft("");
    showToast(`Saved preset "${name}"`, "success");
  }

  function deletePreset(name: string) {
    const presets = userPresets().filter((p) => p.name !== name);
    setUserPresets(presets);
    saveUserPresets(presets);
  }

  // ── Age pills ─────────────────────────────────────────────────────────────

  function handleAgePill(value: string | null) {
    updateQuery(setAgeInQuery(query(), value));
  }

  // ── UI helpers ────────────────────────────────────────────────────────────

  const isFiltered = () => query().trim() !== "";
  const count = () => filteredEntries().length;
  const crashes = () => crashIndices().length;
  const selRange = () => getSelectionRange();
  const selCount = () => {
    const r = selRange();
    return r ? r[1] - r[0] + 1 : 0;
  };

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div style={{ display: "flex", "flex-direction": "column", flex: "1", overflow: "hidden", background: "var(--bg-primary)" }}>

      {/* ── Toolbar row 1: controls + query ──────────────────────────────── */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "6px",
          padding: "5px 10px",
          background: "var(--bg-secondary)",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
          "flex-wrap": "wrap",
        }}
      >
        {/* Start/Stop */}
        <Show
          when={logcatStore.streaming}
          fallback={
            <button onClick={handleStart} title="Start Logcat" style={btnStyle("#4ade80")}>
              <Icon name="play" size={13} /> Start
            </button>
          }
        >
          <button onClick={handleStop} title="Stop Logcat" style={btnStyle("#f87171")}>
            <Icon name="stop" size={13} /> Stop
          </button>
        </Show>

        {/* Pause/Resume */}
        <button
          onClick={() => setPaused((v) => !v)}
          title={paused() ? "Resume" : "Pause new entries"}
          style={btnStyle(paused() ? "#fbbf24" : "var(--text-muted)")}
        >
          {paused() ? "▶" : "⏸"}
        </button>

        {/* Clear */}
        <button onClick={handleClear} title="Clear logcat buffer" style={btnStyle("var(--text-muted)")}>
          <Icon name="trash" size={12} />
        </button>

        <div style={{ width: "1px", height: "18px", background: "var(--border)", "flex-shrink": "0" }} />

        {/* Unified query bar */}
        <QueryBar
          value={query()}
          onChange={updateQuery}
          knownTags={knownTags()}
          knownPackages={knownPackages()}
        />

        {/* Crash nav */}
        <Show when={crashes() > 0}>
          <div style={{ display: "flex", "align-items": "center", gap: "2px", "flex-shrink": "0" }}>
            <button
              onClick={jumpToLastCrash}
              title={`${crashes()} crash${crashes() !== 1 ? "es" : ""} — click to jump`}
              style={{
                ...btnStyle("#f87171"),
                gap: "3px",
                animation: "lsp-dot-pulse 3s ease-in-out infinite",
              }}
            >
              ⚡ {crashes()}
            </button>
            <button onClick={() => jumpToCrash(-1)} title="Previous crash" style={btnStyle("var(--text-muted)")}>↑</button>
            <button onClick={() => jumpToCrash(1)} title="Next crash" style={btnStyle("var(--text-muted)")}>↓</button>
          </div>
        </Show>

        {/* Multi-select copy */}
        <Show when={selCount() > 1}>
          <button
            onClick={copySelectedRows}
            title={`Copy ${selCount()} selected rows`}
            style={btnStyle("var(--accent)")}
          >
            ⎘ {selCount()} rows
          </button>
        </Show>

        {/* Presets */}
        <div style={{ position: "relative", "flex-shrink": "0" }}>
          <button
            onClick={() => { setPresetsOpen((v) => !v); setSavingPreset(false); }}
            title="Filter presets"
            style={btnStyle("var(--text-muted)")}
          >
            ☰ Presets
          </button>
          <Show when={presetsOpen()}>
            <div
              style={{
                position: "absolute",
                top: "calc(100% + 3px)",
                right: "0",
                "min-width": "210px",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                "border-radius": "4px",
                "box-shadow": "0 6px 20px rgba(0,0,0,0.45)",
                "z-index": "700",
                "font-size": "11px",
              }}
            >
              <div style={{ padding: "6px 0" }}>
                {/* Built-in presets */}
                <div style={{ padding: "2px 10px 4px", "font-size": "10px", color: "var(--text-muted)", "text-transform": "uppercase", "letter-spacing": "0.05em" }}>
                  Quick Filters
                </div>
                <For each={BUILTIN_PRESETS}>
                  {(p) => (
                    <div
                      onClick={() => applyPreset(p.query)}
                      style={{
                        display: "flex", "align-items": "center",
                        padding: "5px 10px", cursor: "pointer",
                        color: "var(--text-primary)",
                        "font-family": "var(--font-mono)",
                      }}
                      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)"; }}
                      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                    >
                      <span style={{ flex: "1" }}>{p.name}</span>
                      <span style={{ color: "var(--text-muted)", "font-size": "10px" }}>{p.query}</span>
                    </div>
                  )}
                </For>

                {/* User presets */}
                <Show when={userPresets().length > 0}>
                  <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
                  <div style={{ padding: "2px 10px 4px", "font-size": "10px", color: "var(--text-muted)", "text-transform": "uppercase", "letter-spacing": "0.05em" }}>
                    Saved
                  </div>
                  <For each={userPresets()}>
                    {(p) => (
                      <div
                        style={{
                          display: "flex", "align-items": "center",
                          padding: "5px 10px", cursor: "pointer",
                          color: "var(--text-primary)",
                        }}
                        onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)"; }}
                        onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                      >
                        <span onClick={() => applyPreset(p.query)} style={{ flex: "1" }}>{p.name}</span>
                        <button
                          onClick={(e) => { e.stopPropagation(); deletePreset(p.name); }}
                          style={{ background: "none", border: "none", color: "var(--text-muted)", cursor: "pointer", padding: "0 4px" }}
                          title="Delete preset"
                        >✕</button>
                      </div>
                    )}
                  </For>
                </Show>

                {/* Save current */}
                <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
                <Show
                  when={savingPreset()}
                  fallback={
                    <div
                      onClick={() => setSavingPreset(true)}
                      style={{
                        padding: "5px 10px", cursor: "pointer",
                        color: isFiltered() ? "var(--accent)" : "var(--text-muted)",
                      }}
                      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)"; }}
                      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                    >
                      + Save current filter
                    </div>
                  }
                >
                  <div style={{ display: "flex", gap: "4px", padding: "4px 8px" }}>
                    <input
                      type="text"
                      placeholder="Preset name…"
                      value={presetNameDraft()}
                      onInput={(e) => setPresetNameDraft(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") saveCurrentPreset();
                        if (e.key === "Escape") setSavingPreset(false);
                      }}
                      autofocus
                      style={{
                        flex: "1", background: "var(--bg-primary)",
                        border: "1px solid var(--border)",
                        color: "var(--text-primary)",
                        "border-radius": "3px", padding: "3px 6px",
                        "font-size": "11px", outline: "none",
                      }}
                    />
                    <button
                      onClick={saveCurrentPreset}
                      style={btnStyle("var(--accent)")}
                    >Save</button>
                  </div>
                </Show>
              </div>
            </div>
            {/* Backdrop */}
            <div
              style={{ position: "fixed", inset: "0", "z-index": "699" }}
              onClick={() => setPresetsOpen(false)}
            />
          </Show>
        </div>

        {/* Auto-scroll */}
        <button
          onClick={() => setAutoScroll((v) => !v)}
          title={autoScroll() ? "Auto-scroll on" : "Auto-scroll off"}
          style={btnStyle(autoScroll() ? "var(--accent)" : "var(--text-muted)")}
        >
          ↓
        </button>

        {/* Export */}
        <button onClick={handleExport} title="Export filtered log to file" style={btnStyle("var(--text-muted)")}>
          ↓ Export
        </button>

        <div style={{ flex: "1" }} />

        {/* Entry count */}
        <span style={{ "font-size": "11px", color: "var(--text-muted)", "flex-shrink": "0" }}>
          {isFiltered()
            ? `${count().toLocaleString()} / ${logcatStore.entries.length.toLocaleString()}`
            : `${count().toLocaleString()}`}
        </span>

        {/* Streaming dot */}
        <Show when={logcatStore.streaming}>
          <span style={{ width: "6px", height: "6px", "border-radius": "50%", background: "#4ade80", "flex-shrink": "0", animation: "lsp-dot-pulse 2s ease-in-out infinite" }} />
        </Show>
      </div>

      {/* ── Age quick-select pills ────────────────────────────────────────── */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: "3px 10px",
          background: "var(--bg-tertiary)",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        <span style={{ "font-size": "10px", color: "var(--text-muted)", "margin-right": "2px", "flex-shrink": "0" }}>Age:</span>
        <For each={AGE_PILLS}>
          {(pill) => {
            const isActive = () => pill.value === null
              ? !hasAgeFilter()
              : activeAge() === pill.value;
            return (
              <button
                onClick={() => handleAgePill(pill.value ?? null)}
                style={{
                  padding: "1px 7px",
                  "font-size": "10px",
                  background: isActive() ? "var(--accent)" : "var(--bg-primary)",
                  color: isActive() ? "#fff" : "var(--text-muted)",
                  border: `1px solid ${isActive() ? "var(--accent)" : "var(--border)"}`,
                  "border-radius": "10px",
                  cursor: "pointer",
                  "flex-shrink": "0",
                  transition: "all 0.1s",
                }}
              >
                {pill.label}
              </button>
            );
          }}
        </For>

        <Show when={isFiltered()}>
          <button
            onClick={() => updateQuery("")}
            title="Clear all filters"
            style={{
              ...btnStyle("var(--text-muted)"),
              "margin-left": "auto",
              "font-size": "10px",
              padding: "1px 7px",
            }}
          >
            ✕ Clear
          </button>
        </Show>
      </div>

      {/* ── Empty state ───────────────────────────────────────────────────── */}
      <Show when={logcatStore.entries.length === 0}>
        <div
          style={{
            flex: "1", display: "flex", "align-items": "center",
            "justify-content": "center", "flex-direction": "column",
            gap: "8px", color: "var(--text-muted)", "font-size": "13px",
          }}
        >
          <Show
            when={logcatStore.streaming}
            fallback={
              <>
                <span style={{ "font-size": "24px", opacity: "0.3" }}>📋</span>
                <span>No logcat data</span>
                <span style={{ "font-size": "11px", opacity: "0.6" }}>Connect a device — logcat starts automatically</span>
              </>
            }
          >
            <span style={{ "font-size": "24px", opacity: "0.3" }}>⏳</span>
            <span>Waiting for log entries…</span>
          </Show>
        </div>
      </Show>

      {/* ── Virtualised log list ──────────────────────────────────────────── */}
      <Show when={logcatStore.entries.length > 0}>
        <VirtualList
          items={filteredEntries()}
          rowHeight={ROW_HEIGHT}
          autoScroll={autoScroll()}
          jumpTo={jumpTarget()}
          onScrolledToBottom={() => setAutoScroll(true)}
          onScrolledUp={() => setAutoScroll(false)}
          style={{
            flex: "1",
            "font-family": "var(--font-mono)",
            "font-size": "11px",
            "line-height": `${ROW_HEIGHT}px`,
          }}
          renderItem={(entry, idx) => {
            if (entry.kind === "processDied" || entry.kind === "processStarted") {
              return <SeparatorRow entry={entry} />;
            }
            const range = getSelectionRange();
            const selected = range !== null && idx >= range[0] && idx <= range[1];
            return (
              <LogcatRow
                entry={entry}
                selected={selected}
                onClick={(e) => handleRowClick(idx, e)}
              />
            );
          }}
        />
      </Show>
    </div>
  );
}

// ── SeparatorRow ──────────────────────────────────────────────────────────────

function SeparatorRow(props: { entry: LogcatEntry }): JSX.Element {
  const isDied = () => props.entry.kind === "processDied";
  const pkg = () => props.entry.package ?? props.entry.tag;
  const label = () => isDied()
    ? `⚠  ${pkg()} PROCESS DIED`
    : `▶  ${pkg()} PROCESS RESTARTED`;

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        height: `${ROW_HEIGHT}px`,
        "min-height": `${ROW_HEIGHT}px`,
        padding: "0 8px",
        background: isDied() ? "rgba(248,113,113,0.10)" : "rgba(74,222,128,0.07)",
        "border-top": `1px solid ${isDied() ? "rgba(248,113,113,0.3)" : "rgba(74,222,128,0.2)"}`,
        "border-bottom": `1px solid ${isDied() ? "rgba(248,113,113,0.3)" : "rgba(74,222,128,0.2)"}`,
        overflow: "hidden",
      }}
    >
      <span style={{ flex: "1", "border-top": `1px dashed ${isDied() ? "rgba(248,113,113,0.3)" : "rgba(74,222,128,0.2)"}` }} />
      <span
        style={{
          "font-size": "10px",
          color: isDied() ? "#f87171" : "#4ade80",
          "font-weight": "600",
          "white-space": "nowrap",
          padding: "0 10px",
          "letter-spacing": "0.04em",
        }}
      >
        {label()}
      </span>
      <span style={{ "font-size": "10px", color: "var(--text-muted)", "flex-shrink": "0" }}>
        {props.entry.timestamp}
      </span>
      <span style={{ flex: "1", "border-top": `1px dashed ${isDied() ? "rgba(248,113,113,0.3)" : "rgba(74,222,128,0.2)"}` }} />
    </div>
  );
}

// ── LogcatRow ─────────────────────────────────────────────────────────────────

function LogcatRow(props: {
  entry: LogcatEntry;
  selected: boolean;
  onClick: (e: MouseEvent) => void;
}): JSX.Element {
  const cfg = () => LEVEL_CONFIG[props.entry.level] ?? LEVEL_CONFIG.unknown;

  return (
    <div
      onClick={props.onClick}
      title="Click to copy · Shift+click to select range"
      style={{
        display: "flex",
        "align-items": "center",
        gap: "6px",
        padding: "0 10px",
        height: `${ROW_HEIGHT}px`,
        "min-height": `${ROW_HEIGHT}px`,
        background: props.selected
          ? "rgba(var(--accent-rgb, 59,130,246),0.25)"
          : props.entry.isCrash
          ? "rgba(248,113,113,0.12)"
          : cfg().bg,
        "border-left": props.entry.isCrash ? "2px solid #f87171" : "2px solid transparent",
        overflow: "hidden",
        cursor: "pointer",
      }}
      onMouseEnter={(e) => {
        if (!props.selected) (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.04)";
      }}
      onMouseLeave={(e) => {
        if (!props.selected) (e.currentTarget as HTMLElement).style.background = props.entry.isCrash ? "rgba(248,113,113,0.12)" : cfg().bg;
      }}
    >
      {/* Timestamp */}
      <span style={{ color: "var(--text-disabled, #4b5563)", "white-space": "nowrap", "flex-shrink": "0", "font-size": "10px", opacity: "0.7" }}>
        {props.entry.timestamp}
      </span>

      {/* Level badge */}
      <span style={{ color: cfg().color, "font-weight": "700", "min-width": "12px", "text-align": "center", "flex-shrink": "0", "font-size": "11px" }}>
        {cfg().label}
      </span>

      {/* Package chip */}
      <Show when={props.entry.package}>
        <span
          style={{ "font-size": "9px", color: "var(--accent)", "flex-shrink": "0", "max-width": "90px", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap", opacity: "0.8" }}
          title={props.entry.package ?? ""}
        >
          {props.entry.package}
        </span>
      </Show>

      {/* Tag */}
      <span
        style={{ color: "var(--text-secondary)", "min-width": "80px", "max-width": "120px", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap", "flex-shrink": "0", "font-size": "10px" }}
        title={props.entry.tag}
      >
        {props.entry.tag}
      </span>

      {/* Message */}
      <span
        style={{
          flex: "1",
          color: props.entry.isCrash ? "#f87171" : cfg().color === "#4ade80" ? "var(--text-primary)" : cfg().color,
          "white-space": "nowrap",
          overflow: "hidden",
          "text-overflow": "ellipsis",
        }}
        title={props.entry.message}
      >
        {props.entry.message}
      </span>
    </div>
  );
}

// ── Style helpers ─────────────────────────────────────────────────────────────

function btnStyle(color: string): Record<string, string> {
  return {
    display: "flex", "align-items": "center", gap: "4px",
    padding: "3px 8px", background: "none",
    border: "1px solid var(--border)", color,
    "border-radius": "4px", cursor: "pointer",
    "font-size": "11px", "white-space": "nowrap",
  };
}
