import {
  type JSX,
  createSignal,
  createMemo,
  createEffect,
  onMount,
  onCleanup,
  Show,
  For,
} from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  startLogcat,
  stopLogcat,
  clearLogcat,
  getLogcatEntries,
  getLogcatStatus,
  setLogcatFilter,
  listenLogcatEntries,
  listenLogcatCleared,
  listenDeviceListChanged,
  type LogcatEntry,
  type LogcatFilterSpec,
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
  parseFilterGroups,
  matchesFilterGroups,
  getFrontendOnlyTokens,
  setAgeInQuery,
  setPackageInQuery,
  getPackageFromQuery,
  getMinePackage,
  parseStackFrame,
  isProjectFrame,
  type QueryToken,
  type FilterGroup,
} from "@/lib/logcat-query";
import { PackageDropdown } from "@/components/logcat/PackageDropdown";
import {
  loadFilterStorage,
  addSavedFilter,
  deleteSavedFilter,
  renameSavedFilter,
  getLastActiveQuery,
  setLastActiveQuery,
  MAX_SAVED_FILTERS,
  type SavedFilter,
} from "@/lib/logcat-filter-storage";
import { openInStudio } from "@/lib/tauri-api";
import { healthState } from "@/stores/health.store";

// ── EntryFlags (mirrors Rust EntryFlags consts) ────────────────────────────────
const ENTRY_FLAGS = {
  CRASH: 1 << 0,
  ANR: 1 << 1,
  JSON_BODY: 1 << 2,
  NATIVE_CRASH: 1 << 3,
} as const;

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

// ── Incremental autocomplete data ─────────────────────────────────────────────

const _pkgSet = new Set<string>();
const _tagFreqMap = new Map<string, number>();

const [knownPackages, setKnownPackages] = createSignal<string[]>([]);
const [knownTags, setKnownTags] = createSignal<string[]>([]);
let _suggestTimer: ReturnType<typeof setTimeout> | null = null;

function ingestForSuggestions(entries: LogcatEntry[]) {
  for (const e of entries) {
    if (e.package) _pkgSet.add(e.package);
    if (!e.kind || e.kind === "normal") {
      _tagFreqMap.set(e.tag, (_tagFreqMap.get(e.tag) ?? 0) + 1);
    }
  }
}

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

// ── Query → backend FilterSpec conversion ─────────────────────────────────────

/**
 * Extract the subset of query tokens that can be applied by the Rust backend.
 * Only simple (non-regex, non-negated) tokens are converted.
 * Complex tokens (regex, negate, age) remain for frontend-side filtering.
 */
function tokensToFilterSpec(tokens: QueryToken[]): LogcatFilterSpec {
  const spec: LogcatFilterSpec = { minLevel: null, tag: null, text: null, package: null, onlyCrashes: false };

  for (const token of tokens) {
    // Skip negated tokens — backend has no negation support
    if ("negate" in token && token.negate) continue;

    switch (token.type) {
      case "level":
        if (!spec.minLevel) spec.minLevel = token.value;
        break;
      case "tag":
        // Skip regex tokens — backend does substring only
        if (!token.regex && !spec.tag) spec.tag = token.value;
        break;
      case "message":
        if (!token.regex && !spec.text) spec.text = token.value;
        break;
      case "package": {
        if (!spec.package) {
          // Resolve "mine" to the actual package name
          const pkg = token.value === "mine" ? (getMinePackage() ?? null) : token.value;
          if (pkg) spec.package = pkg;
        }
        break;
      }
      case "is":
        if (token.value === "crash") spec.onlyCrashes = true;
        break;
      case "freetext":
        // Only use the first freetext token for backend (rest handled in JS)
        if (!spec.text) spec.text = token.value;
        break;
    }
  }

  return spec;
}

/**
 * Compute the most-permissive union `LogcatFilterSpec` across all OR groups.
 *
 * For multi-group queries, the backend cannot know which group will match a
 * given entry, so we send the least-restrictive spec that still pre-filters
 * obvious non-matches (e.g. if every group requires `level:error` or higher,
 * we can still push that to the backend). All precise OR logic is done on the
 * frontend via `matchesFilterGroups`.
 */
function groupsToFilterSpec(groups: FilterGroup[]): LogcatFilterSpec {
  if (groups.length === 0) return { minLevel: null, tag: null, text: null, package: null, onlyCrashes: false };
  if (groups.length === 1) return tokensToFilterSpec(groups[0]);

  const LEVEL_PRIORITY_MAP: Record<string, number> = {
    verbose: 0, debug: 1, info: 2, warn: 3, error: 4, fatal: 5,
  };

  // Collect per-group specs
  const specs = groups.map((g) => tokensToFilterSpec(g));

  // minLevel: use the minimum across groups (most permissive)
  const levels = specs.map((s) => (s.minLevel ? (LEVEL_PRIORITY_MAP[s.minLevel] ?? 0) : 0));
  const minLevelPriority = Math.min(...levels);
  const minLevel = minLevelPriority === 0 ? null :
    Object.entries(LEVEL_PRIORITY_MAP).find(([, v]) => v === minLevelPriority)?.[0] ?? null;

  // tag: only push to backend if every group filters the same tag; otherwise omit
  const tags = specs.map((s) => s.tag);
  const tag = tags.every((t) => t !== null && t === tags[0]) ? tags[0] : null;

  // text: same rule as tag
  const texts = specs.map((s) => s.text);
  const text = texts.every((t) => t !== null && t === texts[0]) ? texts[0] : null;

  // package: same rule
  const pkgs = specs.map((s) => s.package);
  const packageFilter = pkgs.every((p) => p !== null && p === pkgs[0]) ? pkgs[0] : null;

  // onlyCrashes: only if ALL groups require it
  const onlyCrashes = specs.every((s) => s.onlyCrashes);

  return { minLevel, tag, text, package: packageFilter, onlyCrashes };
}

/**
 * When OR groups are present, the frontend must apply full OR logic.
 * Returns true if any group contains frontend-only tokens (overflow, age,
 * negation, regex, is:stacktrace), or if there are multiple OR groups.
 */
function hasAnyFrontendOnlyLogic(groups: FilterGroup[]): boolean {
  if (groups.length > 1) return true;
  return getFrontendOnlyTokens(groups[0] ?? []).length > 0;
}

// ── Saved filters (via logcat-filter-storage) ────────────────────────────────

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
  { name: "My App OR Crashes", query: "package:mine | is:crash", builtin: true },
];

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

  // Row selection
  const [selectionAnchor, setSelectionAnchor] = createSignal<number | null>(null);
  const [selectionEnd, setSelectionEnd] = createSignal<number | null>(null);

  // Preset / saved filter UI
  const [presetsOpen, setPresetsOpen] = createSignal(false);
  const [savedFilters, setSavedFilters] = createSignal<SavedFilter[]>(loadFilterStorage().filters);
  const [savingPreset, setSavingPreset] = createSignal(false);
  const [presetNameDraft, setPresetNameDraft] = createSignal("");
  // Per-filter inline rename state: maps filter id → draft name string
  const [renamingId, setRenamingId] = createSignal<string | null>(null);
  const [renameDraft, setRenameDraft] = createSignal("");

  // Now signal for age filter reactivity (updates every 5s when age token exists)
  const [now, setNow] = createSignal(Date.now());

  // Currently active backend filter spec (for sync between filter changes and new entries)
  const [_activeBackendSpec, setActiveBackendSpec] = createSignal<LogcatFilterSpec>({ minLevel: null, tag: null, text: null, package: null, onlyCrashes: false });

  let unlistenEntries: (() => void) | undefined;
  let unlistenCleared: (() => void) | undefined;
  let unlistenDevices: (() => void) | undefined;
  let nowTimer: ReturnType<typeof setInterval> | undefined;

  // ── Parsed query (debounced — avoids re-parsing on every keystroke)
  const parsedGroups = createMemo(() => parseFilterGroups(debouncedQuery()));
  // Flat token list for single-group utilities (age detection, etc.)
  const parsedTokens = createMemo(() => parsedGroups().flat());
  const hasAgeFilter = createMemo(() => parsedTokens().some((t) => t.type === "age"));
  // Whether the frontend must apply any filtering logic (OR groups or complex tokens)
  const needsFrontendFilter = createMemo(() => hasAnyFrontendOnlyLogic(parsedGroups()));

  // ── filteredEntries ───────────────────────────────────────────────────────────
  //
  // The backend has already filtered `logcatStore.entries` to match the simple
  // parts of the query (level, tag, text, package, only_crashes).
  //
  // This memo handles:
  //   • OR groups  — entries must satisfy at least one group
  //   • age:N      — time-based, needs live `now()`
  //   • -tag:X     — negation
  //   • tag~:X     — regex
  //   • -message:X / message~:X
  //
  // For single-group queries without complex tokens, `needsFrontendFilter()`
  // is false and we short-circuit immediately — same performance as before.
  const filteredEntries = createMemo(
    () => {
      const groups = parsedGroups();
      const entries = logcatStore.entries;
      if (!needsFrontendFilter()) return entries;
      const currentNow = hasAgeFilter() ? now() : Date.now();
      return entries.filter((e) => matchesFilterGroups(e, groups, currentNow));
    },
    undefined,
    { equals: false }
  );

  // ── Incremental crash indices ─────────────────────────────────────────────────
  //
  // Instead of rescanning all of `filteredEntries()` on every batch arrival,
  // we maintain the crash index list incrementally:
  //   • Reset when the store is replaced (filter change or clear).
  //   • Append from new arrivals only (O(batch_size), not O(total)).
  //
  // `filteredEntries` may further shrink the set (age/regex tokens), so we
  // rebuild fully only when frontend-only tokens are active.
  const [crashIndicesFull, setCrashIndicesFull] = createSignal<number[]>([]);

  // When frontend tokens are active filteredEntries may differ from the store,
  // so we fall back to scanning filteredEntries() (the set is already small).
  const crashIndices = createMemo(() => {
    if (!needsFrontendFilter()) {
      // Fast path: no frontend filtering, use the incremental index.
      return crashIndicesFull();
    }
    // Slow path: frontend tokens active, rescan the (small) filtered set.
    const indices: number[] = [];
    filteredEntries().forEach((e, i) => { if (e.isCrash) indices.push(i); });
    return indices;
  });

  const activeAge = createMemo(() => {
    const t = parsedTokens().find((t) => t.type === "age") as { type: "age"; seconds: number } | undefined;
    if (!t) return null;
    for (const p of AGE_PILLS) {
      if (p.value && parseAge(p.value) === t.seconds) return p.value;
    }
    return null;
  });

  const activePackage = createMemo(() => getPackageFromQuery(debouncedQuery()));

  function handlePackageSelect(pkg: string | null) {
    updateQuery(setPackageInQuery(query(), pkg));
  }

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

  // ── Backend filter sync ───────────────────────────────────────────────────────
  //
  // When the debounced query changes, we:
  //   1. Compute the union backend FilterSpec from all OR groups.
  //   2. Send it to Rust via `set_logcat_filter`.
  //   3. Fetch a fresh backfill from the buffer using the same spec.
  //   4. Replace the store entries with the backfill.
  //
  // For single-group queries (no `|`) this behaves identically to before.
  // For multi-group queries the union spec is sent so the backend pre-filters
  // broadly; precise OR matching is done client-side in `filteredEntries`.
  async function syncBackendFilter(groups: FilterGroup[]) {
    const spec = groupsToFilterSpec(groups);
    setActiveBackendSpec(spec);

    try {
      await setLogcatFilter(spec);

      // Fetch backfill from stored buffer with the same filter.
      const entries = await getLogcatEntries({
        count: 2000,
        minLevel: spec.minLevel ?? undefined,
        tag: spec.tag ?? undefined,
        text: spec.text ?? undefined,
        package: spec.package ?? undefined,
        onlyCrashes: spec.onlyCrashes,
      });
      setLogcatStore("entries", entries);
      // Rebuild incremental crash index from the new backfill.
      setCrashIndicesFull(entries.reduce<number[]>((acc, e, i) => {
        if (e.isCrash) acc.push(i);
        return acc;
      }, []));
      ingestForSuggestions(entries);
      flushSuggestions(true);
    } catch { /* ignore — don't break the UI if IPC fails */ }
  }

  // Trigger backend filter sync whenever the debounced query changes.
  // createEffect is correct here (not createMemo): effects are for side-effects,
  // memos must be pure. The comparison guard prevents running on the initial
  // mount since onMount already fetches the unfiltered backfill.
  let _prevDebouncedQuery = "";
  createEffect(() => {
    const q = debouncedQuery();
    if (q === _prevDebouncedQuery) return;
    _prevDebouncedQuery = q;
    setLastActiveQuery(q);
    syncBackendFilter(parseFilterGroups(q));
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────────────

  onMount(async () => {
    // Restore the last active query before fetching entries so the initial
    // backfill respects any persisted filter.
    const savedQuery = getLastActiveQuery();
    if (savedQuery) {
      setQuery(savedQuery);
      setDebouncedQuery(savedQuery);
      _prevDebouncedQuery = savedQuery;
    }

    // Build the filter spec for the initial backfill.
    // When a saved query is restored we must also sync the backend filter —
    // the createEffect guard (_prevDebouncedQuery check) prevents it from
    // running and the streaming listener would otherwise deliver unfiltered entries.
    const restoredGroups = savedQuery ? parseFilterGroups(savedQuery) : null;
    const restoredSpec = restoredGroups ? groupsToFilterSpec(restoredGroups) : null;

    if (restoredSpec && savedQuery) {
      setActiveBackendSpec(restoredSpec);
      await setLogcatFilter(restoredSpec).catch(() => {});
    }

    try {
      const entries = await getLogcatEntries({
        count: 2000,
        // Apply the restored spec so the backfill is already filtered
        minLevel: restoredSpec?.minLevel ?? undefined,
        tag: restoredSpec?.tag ?? undefined,
        text: restoredSpec?.text ?? undefined,
        package: restoredSpec?.package ?? undefined,
        onlyCrashes: restoredSpec?.onlyCrashes ?? false,
      });
      setLogcatStore("entries", entries);
      setCrashIndicesFull(entries.reduce<number[]>((acc, e, i) => {
        if (e.isCrash) acc.push(i);
        return acc;
      }, []));
      ingestForSuggestions(entries);
      flushSuggestions(true);
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

      // Update incremental crash index before touching the store so we know
      // the offset at which new entries will be appended.
      const baseLen = logcatStore.entries.length;
      const newCrashOffsets: number[] = [];
      newEntries.forEach((e, i) => { if (e.isCrash) newCrashOffsets.push(baseLen + i); });

      let didEvict = false;
      setLogcatStore(
        produce((s) => {
          for (const e of newEntries) s.entries.push(e);
          if (evictionPending && s.entries.length > MAX_UI_ENTRIES) {
            const dropped = s.entries.length - MAX_UI_ENTRIES;
            s.entries.splice(0, dropped);
            evictionPending = false;
            didEvict = true;
          }
        })
      );

      if (didEvict) {
        // After eviction all pre-computed offsets are stale — rebuild fully.
        setCrashIndicesFull(
          logcatStore.entries.reduce<number[]>((acc, e, i) => {
            if (e.isCrash) acc.push(i);
            return acc;
          }, [])
        );
      } else if (newCrashOffsets.length > 0) {
        // No eviction — safe to append the incremental offsets.
        setCrashIndicesFull((prev) => [...prev, ...newCrashOffsets]);
      }

      ingestForSuggestions(newEntries);
      flushSuggestions();
    });

    unlistenCleared = await listenLogcatCleared(() => {
      setLogcatStore("entries", []);
      setCrashIndicesFull([]);
      clearSuggestions();
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

    nowTimer = setInterval(() => setNow(Date.now()), 5_000);
  });

  onCleanup(() => {
    unlistenEntries?.();
    unlistenCleared?.();
    unlistenDevices?.();
    clearInterval(nowTimer);
    clearTimeout(_queryDebounce);
    // Clear backend filter on unmount so it doesn't persist
    setLogcatFilter({ minLevel: null, tag: null, text: null, package: null, onlyCrashes: false }).catch(() => {});
  });

  // ── Controls ──────────────────────────────────────────────────────────────────

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

  // ── Crash navigation ──────────────────────────────────────────────────────────

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

  createMemo(() => {
    filteredEntries();
    setCrashCursor((c) => Math.min(c, Math.max(0, crashIndices().length - 1)));
  });

  // ── Row copy ──────────────────────────────────────────────────────────────────

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

  // ── Export ────────────────────────────────────────────────────────────────────

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

  // ── Presets / saved filters ───────────────────────────────────────────────────

  function applyPreset(q: string) {
    updateQuery(q);
    setPresetsOpen(false);
  }

  function saveCurrentFilter() {
    const name = presetNameDraft().trim();
    if (!name) return;
    try {
      const saved = addSavedFilter(name, query());
      setSavedFilters(loadFilterStorage().filters);
      setSavingPreset(false);
      setPresetNameDraft("");
      showToast(`Saved filter "${saved.name}"`, "success");
    } catch (e) {
      showToast(String(e), "error");
    }
  }

  function deleteSavedFilterItem(id: string) {
    deleteSavedFilter(id);
    setSavedFilters(loadFilterStorage().filters);
  }

  function startRename(filter: SavedFilter) {
    setRenamingId(filter.id);
    setRenameDraft(filter.name);
  }

  function commitRename() {
    const id = renamingId();
    if (id) {
      renameSavedFilter(id, renameDraft());
      setSavedFilters(loadFilterStorage().filters);
    }
    setRenamingId(null);
    setRenameDraft("");
  }

  function cancelRename() {
    setRenamingId(null);
    setRenameDraft("");
  }

  // ── Age pills ─────────────────────────────────────────────────────────────────

  function handleAgePill(value: string | null) {
    updateQuery(setAgeInQuery(query(), value));
  }

  // ── JSON viewer state ─────────────────────────────────────────────────────────

  const [selectedJsonEntry, setSelectedJsonEntry] = createSignal<LogcatEntry | null>(null);

  function handleJsonBadgeClick(e: MouseEvent, entry: LogcatEntry) {
    e.stopPropagation();
    setSelectedJsonEntry((prev: LogcatEntry | null) => (prev?.id === entry.id ? null : entry));
    setAutoScroll(false);
  }

  // ── UI helpers ────────────────────────────────────────────────────────────────

  const isFiltered = () => query().trim() !== "";
  const count = () => filteredEntries().length;
  const crashes = () => crashIndices().length;
  const selRange = () => getSelectionRange();
  const selCount = () => {
    const r = selRange();
    return r ? r[1] - r[0] + 1 : 0;
  };

  // ── Render ────────────────────────────────────────────────────────────────────

  return (
    <div style={{ display: "flex", "flex-direction": "column", flex: "1", overflow: "hidden", background: "var(--bg-primary)" }}>

      {/* ── Toolbar row 1: controls + query ──────────────────────────────── */}
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
            onClick={() => { setPresetsOpen((v) => !v); setSavingPreset(false); setRenamingId(null); }}
            title="Filter presets"
            style={btnStyle("var(--text-muted)")}
          >
            ☰ Filters
          </button>
          <Show when={presetsOpen()}>
            <div
              style={{
                position: "absolute",
                top: "calc(100% + 3px)",
                right: "0",
                "min-width": "260px",
                "max-width": "340px",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                "border-radius": "4px",
                "box-shadow": "0 6px 20px rgba(0,0,0,0.45)",
                "z-index": "700",
                "font-size": "11px",
              }}
            >
              <div style={{ padding: "6px 0" }}>
                {/* Quick Filters (built-in) */}
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
                      <span style={{ color: "var(--text-muted)", "font-size": "10px", "max-width": "130px", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>{p.query}</span>
                    </div>
                  )}
                </For>

                {/* Saved Filters */}
                <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
                <div style={{ display: "flex", "align-items": "center", padding: "2px 10px 4px" }}>
                  <span style={{ "font-size": "10px", color: "var(--text-muted)", "text-transform": "uppercase", "letter-spacing": "0.05em", flex: "1" }}>
                    Saved
                  </span>
                  <span style={{ "font-size": "10px", color: "var(--text-muted)" }}>
                    {savedFilters().length} / {MAX_SAVED_FILTERS}
                  </span>
                </div>

                <Show when={savedFilters().length === 0}>
                  <div style={{ padding: "4px 10px 6px", "font-size": "10px", color: "var(--text-muted)", "font-style": "italic" }}>
                    No saved filters yet
                  </div>
                </Show>

                <For each={savedFilters()}>
                  {(f) => (
                    <div
                      style={{
                        display: "flex", "align-items": "center",
                        padding: "4px 10px", cursor: "pointer",
                        color: "var(--text-primary)",
                        gap: "4px",
                      }}
                      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)"; }}
                      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                    >
                      <Show
                        when={renamingId() === f.id}
                        fallback={
                          <>
                            <span
                              onClick={() => applyPreset(f.query)}
                              style={{ flex: "1", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}
                              title={f.query}
                            >
                              {f.name}
                            </span>
                            {/* Rename button */}
                            <button
                              onClick={(e) => { e.stopPropagation(); startRename(f); }}
                              style={{ background: "none", border: "none", color: "var(--text-muted)", cursor: "pointer", padding: "0 3px", "font-size": "10px" }}
                              title="Rename"
                            >✎</button>
                            {/* Delete button */}
                            <button
                              onClick={(e) => { e.stopPropagation(); deleteSavedFilterItem(f.id); }}
                              style={{ background: "none", border: "none", color: "var(--text-muted)", cursor: "pointer", padding: "0 3px", "font-size": "10px" }}
                              title="Delete"
                            >✕</button>
                          </>
                        }
                      >
                        {/* Inline rename row */}
                        <input
                          type="text"
                          value={renameDraft()}
                          onInput={(e) => setRenameDraft(e.currentTarget.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") { e.stopPropagation(); commitRename(); }
                            if (e.key === "Escape") { e.stopPropagation(); cancelRename(); }
                          }}
                          autofocus
                          style={{
                            flex: "1", background: "var(--bg-primary)",
                            border: "1px solid var(--accent)",
                            color: "var(--text-primary)",
                            "border-radius": "3px", padding: "2px 5px",
                            "font-size": "11px", outline: "none",
                          }}
                        />
                        <button onClick={(e) => { e.stopPropagation(); commitRename(); }} style={btnStyle("var(--accent)")}>✓</button>
                        <button onClick={(e) => { e.stopPropagation(); cancelRename(); }} style={btnStyle("var(--text-muted)")}>✕</button>
                      </Show>
                    </div>
                  )}
                </For>

                {/* Save current filter */}
                <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
                <Show
                  when={savingPreset()}
                  fallback={
                    <div
                      onClick={() => { setSavingPreset(true); setPresetNameDraft(""); }}
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
                      placeholder="Filter name…"
                      value={presetNameDraft()}
                      onInput={(e) => setPresetNameDraft(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") saveCurrentFilter();
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
                    <button onClick={saveCurrentFilter} style={btnStyle("var(--accent)")}>Save</button>
                  </div>
                </Show>
              </div>
            </div>
            <div
              style={{ position: "fixed", inset: "0", "z-index": "699" }}
              onClick={() => { setPresetsOpen(false); cancelRename(); }}
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

        <div style={{ width: "1px", height: "14px", background: "var(--border)", "flex-shrink": "0", "margin-left": "2px" }} />

        {/* Package filter dropdown */}
        <PackageDropdown
          packages={knownPackages()}
          selected={activePackage()}
          onSelect={handlePackageSelect}
        />

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
            const isJsonSelected = () => selectedJsonEntry()?.id === entry.id;
            return (
              <LogcatRow
                entry={entry}
                selected={selected}
                jsonSelected={isJsonSelected()}
                onClick={(e) => handleRowClick(idx, e)}
                onJsonClick={(e) => handleJsonBadgeClick(e, entry)}
              />
            );
          }}
        />
      </Show>

      {/* ── JSON Detail Panel ─────────────────────────────────────────────── */}
      <Show when={selectedJsonEntry() !== null}>
        <JsonDetailPanel
          entry={selectedJsonEntry()!}
          onClose={() => setSelectedJsonEntry(null)}
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

// ── StudioJumpButton ──────────────────────────────────────────────────────────

/**
 * A small "↗ Studio" button rendered on hover for crash-group stack frame lines.
 * Parses the message to extract the file and line, then calls `open_in_studio`.
 */
function StudioJumpButton(props: { message: string }): JSX.Element {
  const frame = () => {
    const f = parseStackFrame(props.message);
    // Only show the button for frames that belong to the project —
    // hide it for Android / Kotlin / Java framework packages.
    if (f && !isProjectFrame(f.classPath)) return null;
    return f;
  };
  const studioReady = () => healthState.systemReport?.studioCommandFound === true;
  const [hovered, setHovered] = createSignal(false);
  const [opening, setOpening] = createSignal(false);

  const handleOpen = async (e: MouseEvent) => {
    e.stopPropagation();
    const f = frame();
    if (!f) return;
    if (!studioReady()) {
      showToast("Install the studio command — see Health Panel for setup instructions", "warning");
      return;
    }
    setOpening(true);
    try {
      await openInStudio(f.packagePath, f.filename, f.line);
    } catch (err: unknown) {
      showToast(String(err), "error");
    } finally {
      setOpening(false);
    }
  };

  return (
    <Show when={frame() !== null}>
      <button
        onClick={handleOpen}
        title={
          studioReady()
            ? `Open ${frame()!.filename}:${frame()!.line} in Android Studio`
            : "Install the studio command to enable jump-to-line (see Health Panel)"
        }
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        style={{
          "flex-shrink": "0",
          display: "inline-flex",
          "align-items": "center",
          gap: "3px",
          padding: "1px 6px",
          "border-radius": "3px",
          border: `1px solid ${studioReady() ? "rgba(96,165,250,0.4)" : "rgba(156,163,175,0.3)"}`,
          background: hovered()
            ? studioReady()
              ? "rgba(96,165,250,0.15)"
              : "rgba(156,163,175,0.1)"
            : "transparent",
          color: studioReady() ? "rgba(96,165,250,0.9)" : "var(--text-disabled)",
          cursor: opening() ? "wait" : studioReady() ? "pointer" : "not-allowed",
          "font-size": "9px",
          "font-family": "var(--font-ui)",
          "font-weight": "500",
          opacity: opening() ? "0.6" : "1",
          transition: "background 0.1s, opacity 0.1s",
          "white-space": "nowrap",
        }}
      >
        {opening() ? "⟳" : "↗"} Studio
      </button>
    </Show>
  );
}

// ── LogcatRow ─────────────────────────────────────────────────────────────────

function LogcatRow(props: {
  entry: LogcatEntry;
  selected: boolean;
  jsonSelected: boolean;
  onClick: (e: MouseEvent) => void;
  onJsonClick: (e: MouseEvent) => void;
}): JSX.Element {
  const cfg = () => LEVEL_CONFIG[props.entry.level as keyof typeof LEVEL_CONFIG] ?? LEVEL_CONFIG.unknown;
  const hasJson = () => (props.entry.flags & ENTRY_FLAGS.JSON_BODY) !== 0;
  const hasAnr = () => (props.entry.flags & ENTRY_FLAGS.ANR) !== 0;
  // Entries in a crash group but not the header get a subtle left border variation
  const inCrashGroup = () => props.entry.crashGroupId !== null && !props.entry.isCrash;

  // Left border: crash group lines get a softer indicator than crash headers
  const borderColor = () => {
    if (props.selected) return "var(--accent)";
    if (props.entry.isCrash) return "#f87171";
    if (hasAnr()) return "#fbbf24";
    if (inCrashGroup()) return "rgba(248,113,113,0.4)";
    return "transparent";
  };

  return (
    <div
      onClick={(e) => props.onClick(e)}
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
          : props.jsonSelected
          ? "rgba(96,165,250,0.12)"
          : props.entry.isCrash
          ? "rgba(248,113,113,0.12)"
          : hasAnr()
          ? "rgba(251,191,36,0.08)"
          : cfg().bg,
        "border-left": `2px solid ${borderColor()}`,
        overflow: "hidden",
        cursor: "pointer",
      }}
      onMouseEnter={(e) => {
        if (!props.selected && !props.jsonSelected) (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.04)";
      }}
      onMouseLeave={(e) => {
        if (!props.selected && !props.jsonSelected) {
          (e.currentTarget as HTMLElement).style.background =
            props.entry.isCrash ? "rgba(248,113,113,0.12)" :
            hasAnr() ? "rgba(251,191,36,0.08)" :
            cfg().bg;
        }
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

      {/* JSON badge — clickable to open detail panel */}
      <Show when={hasJson()}>
        <span
          onClick={(e) => props.onJsonClick(e)}
          title="Click to view formatted JSON"
          style={{
            "font-size": "9px",
            color: props.jsonSelected ? "#fff" : "#60a5fa",
            "flex-shrink": "0",
            opacity: "1",
            "font-weight": "600",
            background: props.jsonSelected ? "rgba(96,165,250,0.5)" : "rgba(96,165,250,0.1)",
            padding: "0 4px",
            "border-radius": "2px",
            cursor: "pointer",
            border: "1px solid rgba(96,165,250,0.3)",
          }}
        >
          {"{}"}</span>
      </Show>

      {/* ANR badge */}
      <Show when={hasAnr()}>
        <span style={{ "font-size": "9px", color: "#fbbf24", "flex-shrink": "0", "font-weight": "600", background: "rgba(251,191,36,0.1)", padding: "0 3px", "border-radius": "2px" }}>
          ANR
        </span>
      </Show>

      {/* Message */}
      <span
        style={{
          flex: "1",
          color: props.entry.isCrash ? "#f87171" :
                 hasAnr() ? "#fbbf24" :
                 cfg().color === "#4ade80" ? "var(--text-primary)" : cfg().color,
          "white-space": "nowrap",
          overflow: "hidden",
          "text-overflow": "ellipsis",
        }}
        title={props.entry.message}
      >
        {props.entry.message}
      </span>

      {/* Open in Studio button — shown for crash-group stack frame lines */}
      <Show when={inCrashGroup() || props.entry.isCrash}>
        <StudioJumpButton message={props.entry.message} />
      </Show>
    </div>
  );
}

// ── JsonDetailPanel ───────────────────────────────────────────────────────────

function JsonDetailPanel(props: {
  entry: LogcatEntry;
  onClose: () => void;
}): JSX.Element {
  const [copied, setCopied] = createSignal(false);

  const formattedJson = () => {
    try {
      const raw = props.entry.jsonBody;
      if (!raw) return null;
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return props.entry.jsonBody;
    }
  };

  async function copyJson() {
    const json = formattedJson();
    if (!json) return;
    try {
      await navigator.clipboard.writeText(json);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* ignore */ }
  }

  return (
    <div
      style={{
        "flex-shrink": "0",
        "max-height": "220px",
        display: "flex",
        "flex-direction": "column",
        background: "var(--bg-secondary)",
        "border-top": "1px solid var(--border)",
      }}
    >
      {/* Panel header */}
      <div style={{
        display: "flex",
        "align-items": "center",
        gap: "6px",
        padding: "3px 10px",
        background: "var(--bg-tertiary)",
        "border-bottom": "1px solid var(--border)",
        "flex-shrink": "0",
      }}>
        <span style={{ "font-size": "10px", color: "#60a5fa", "font-weight": "600" }}>JSON</span>
        <span style={{ "font-size": "10px", color: "var(--text-muted)", flex: "1", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
          {props.entry.tag}: {props.entry.timestamp}
        </span>
        <button
          onClick={copyJson}
          title="Copy JSON"
          style={{
            background: "none", border: "1px solid var(--border)",
            color: copied() ? "#4ade80" : "var(--text-muted)",
            "border-radius": "3px", cursor: "pointer",
            "font-size": "10px", padding: "1px 6px",
          }}
        >
          {copied() ? "Copied!" : "Copy"}
        </button>
        <button
          onClick={() => props.onClose()}
          title="Close JSON viewer"
          style={{
            background: "none", border: "none",
            color: "var(--text-muted)", cursor: "pointer",
            "font-size": "12px", padding: "0 4px",
          }}
        >✕</button>
      </div>

      {/* JSON content */}
      <pre
        style={{
          flex: "1",
          overflow: "auto",
          margin: "0",
          padding: "8px 12px",
          "font-family": "var(--font-mono)",
          "font-size": "11px",
          "line-height": "1.5",
          color: "var(--text-primary)",
          "white-space": "pre",
          background: "transparent",
        }}
      >
        {formattedJson() ?? "(invalid JSON)"}
      </pre>
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
