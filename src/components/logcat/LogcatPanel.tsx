import {
  type JSX,
  createSignal,
  createMemo,
  createEffect,
  onMount,
  onCleanup,
  Show,
} from "solid-js";
import {
  startLogcat,
  stopLogcat,
  clearLogcat,
  getLogcatEntries,
  getLogcatStatus,
  getLogcatStats,
  setLogcatFilter,
  listenLogcatEntries,
  listenLogcatCleared,
  listenLogcatReconnecting,
  listenDeviceListChanged,
  formatError,
  type LogcatEntry,
} from "@/lib/tauri-api";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { save } from "@tauri-apps/plugin-dialog";
import { selectedDevice } from "@/stores/device.store";
import { settingsState } from "@/stores/settings.store";
import { showToast } from "@/components/ui";
import { VirtualList, type VirtualListHandle, isPaletteOpen } from "@/components/ui";
import {
  parseAge,
  parseFilterGroups,
  matchesFilterGroups,
  setAgeInQuery,
  setPackageInQuery,
  getPackageFromQuery,
  setMinePackage,
  appendLogEntryDetailFilterToken,
  type LogEntryDetailFilterMode,
  type FilterGroup,
} from "@/lib/logcat-query";
import { projectState } from "@/stores/project.store";
import { buildState } from "@/stores/build.store";
import { LogEntryDetailPanel } from "./LogEntryDetailPanel";
import { getLastActiveQuery, setLastActiveQuery } from "@/lib/logcat-filter-storage";
import { uiState } from "@/stores/ui.store";
import { clampSelectionIndices, nextSelectableIndex } from "./logcat-selection-nav";
import { formatLogcatToolbarCount } from "./logcat-toolbar-count";
import { clampLogcatMaxUiLines, clampLogcatRingMaxEntries } from "@/lib/logcat-ui-lines";
import { effectiveLogcatFollowTail } from "@/lib/logcat-follow-tail";
import {
  emptyLogcatFilterSpec,
  groupsToFilterSpec,
  hasAnyFrontendOnlyLogic,
} from "@/lib/logcat-filter-spec";
import {
  appendLogcatEntries,
  clearLogcatEntries,
  logcatState,
  replaceLogcatEntries,
  setLogcatRingBufferTotal,
  setLogcatStreaming,
} from "@/stores/logcat.store";
import { createLatestOnlyGuard } from "@/services/logcat.service";
import { JsonDetailPanel } from "./LogcatJsonDetailPanel";
import { LogcatVirtualRow, ROW_HEIGHT, SeparatorRow } from "./LogcatRows";
import { SavedFilterMenu } from "./SavedFilterMenu";
import {
  LogcatFilterControls,
  LOGCAT_AGE_PILLS,
  type LogcatAgePillValue,
} from "./LogcatFilterControls";
import { LogcatToolbar } from "./LogcatToolbar";
import { createLogcatSuggestionRuntime } from "./logcat-suggestion-runtime";

function maxUiLinesCap(): number {
  return clampLogcatMaxUiLines(
    settingsState.logcat.maxUiLines,
    settingsState.logcat.ringMaxEntries
  );
}

// ── LogcatPanel ───────────────────────────────────────────────────────────────

function isLogcatTypingTarget(target: unknown): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export function LogcatPanel(): JSX.Element {
  const [query, setQuery] = createSignal("");
  const [debouncedQuery, setDebouncedQuery] = createSignal("");
  let _queryDebounce: ReturnType<typeof setTimeout> | undefined;

  function updateQuery(q: string) {
    setQuery(q);
    clearTimeout(_queryDebounce);
    _queryDebounce = setTimeout(() => setDebouncedQuery(q), 150);
  }

  const [autoScroll, setAutoScroll] = createSignal(settingsState.logcat.autoScrollToEnd !== false);
  const [scrollCompensate, setScrollCompensate] = createSignal(0);
  let virtualListRef: VirtualListHandle | undefined;
  const [paused, setPaused] = createSignal(false);
  const [restarting, setRestarting] = createSignal(false);

  // Crash navigation
  const [jumpTarget, setJumpTarget] = createSignal<number | null>(null);
  const [crashCursor, setCrashCursor] = createSignal(0);

  // Row selection
  const [selectionAnchor, setSelectionAnchor] = createSignal<number | null>(null);
  const [selectionEnd, setSelectionEnd] = createSignal<number | null>(null);

  const [selectedJsonEntry, setSelectedJsonEntry] = createSignal<LogcatEntry | null>(null);
  const [selectedDetailEntry, setSelectedDetailEntry] = createSignal<LogcatEntry | null>(null);

  const followTailForList = createMemo(() =>
    effectiveLogcatFollowTail({
      autoScroll: autoScroll(),
      selectionAnchor: selectionAnchor(),
      selectedJsonEntry: selectedJsonEntry(),
    })
  );

  // Now signal for age filter reactivity (updates every 5s when age token exists)
  const [now, setNow] = createSignal(Date.now());

  const filterSyncGuard = createLatestOnlyGuard();
  const suggestions = createLogcatSuggestionRuntime();

  let unlistenEntries: (() => void) | undefined;
  let unlistenCleared: (() => void) | undefined;
  let unlistenDevices: (() => void) | undefined;
  let unlistenReconnecting: (() => void) | undefined;
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
  // The backend has already filtered `logcatState.entries` to match the simple
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
      const entries = logcatState.entries;
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
  // When frontend tokens are active filteredEntries may differ from the store,
  // so we fall back to scanning filteredEntries() (the set is already small).
  const crashIndices = createMemo(() => {
    if (!needsFrontendFilter()) {
      // Fast path: no frontend filtering, use the incremental index.
      return logcatState.crashIndicesFull;
    }
    // Slow path: frontend tokens active, rescan the (small) filtered set.
    const indices: number[] = [];
    filteredEntries().forEach((e, i) => {
      if (e.isCrash) indices.push(i);
    });
    return indices;
  });

  const activeAge = createMemo(() => {
    const t = parsedTokens().find((t) => t.type === "age") as
      | { type: "age"; seconds: number }
      | undefined;
    if (!t) return null;
    for (const p of LOGCAT_AGE_PILLS) {
      if (p.value && parseAge(p.value) === t.seconds) return p.value;
    }
    return null;
  });

  const activePackage = createMemo(() => getPackageFromQuery(debouncedQuery()));

  function handlePackageSelect(pkg: string | null) {
    const q = setPackageInQuery(query(), pkg);
    updateQuery(q.trimEnd() ? q.trimEnd() + " " : "");
  }

  function handleDetailFilter(filter: { token: string; mode: LogEntryDetailFilterMode }) {
    updateQuery(appendLogEntryDetailFilterToken(query(), filter.token, filter.mode));
  }

  async function refreshLogcatRingStats(): Promise<void> {
    try {
      const s = await getLogcatStats();
      setLogcatRingBufferTotal(Number(s.bufferEntryCount));
    } catch {
      setLogcatRingBufferTotal(null);
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
    const syncToken = filterSyncGuard.begin();
    const spec = groupsToFilterSpec(groups);

    try {
      await setLogcatFilter(spec);

      // Fetch backfill from stored buffer with the same filter.
      const entries = await getLogcatEntries({
        count: maxUiLinesCap(),
        minLevel: spec.minLevel ?? undefined,
        tag: spec.tag ?? undefined,
        text: spec.text ?? undefined,
        package: spec.package ?? undefined,
        onlyCrashes: spec.onlyCrashes,
      });
      if (!filterSyncGuard.isLatest(syncToken)) return;
      replaceLogcatEntries(entries);
      suggestions.ingest(entries);
      suggestions.flush(true);
    } catch (err) {
      if (filterSyncGuard.isLatest(syncToken)) {
        showToast(`Failed to sync logcat filter: ${formatError(err)}`, "error");
      }
    }
    await refreshLogcatRingStats();
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

  // Re-sync the backend filter when the project's applicationId becomes available
  // (or changes on project switch). This fixes a startup race where the LogcatPanel
  // mounts and restores a `package:mine` query before doOpenProject() has finished
  // resolving getApplicationId() — the initial sync runs with _minePackage = null
  // and the guard on the effect above (_prevDebouncedQuery) prevents it from re-running.
  let _prevAppId: string | null | undefined = undefined;
  createEffect(() => {
    const appId = projectState.applicationId; // reactive — tracks project changes
    if (appId === _prevAppId) return;
    _prevAppId = appId;
    setMinePackage(appId);
    // Re-evaluate the backend filter only if the current query references "mine".
    const q = debouncedQuery();
    if (q.includes("package:mine") || q.includes("pkg:mine")) {
      syncBackendFilter(parseFilterGroups(q));
    }
  });

  // ── Auto-apply package:mine after a successful deploy ─────────────────────────
  // When the build service launches an app it sets buildState.lastLaunchedAt to
  // Date.now(). Subscribing here lets us merge package:mine into the active query
  // automatically so the user immediately sees logs for their app.
  let _prevLaunchedAt: number | null | undefined = undefined;
  createEffect(() => {
    const launchedAt = buildState.lastLaunchedAt;
    if (launchedAt === _prevLaunchedAt) return;
    _prevLaunchedAt = launchedAt;
    if (launchedAt === null) return; // initial mount — skip
    const q = query();
    if (q.includes("package:mine") || q.includes("pkg:mine")) return;
    const next = setPackageInQuery(q, "mine");
    updateQuery(next.trimEnd() ? next.trimEnd() + " " : "");
  });

  // When Settings changes the in-memory ring size, resync the list from Rust.
  let prevRingCap: number | undefined;
  createEffect(() => {
    const ring = clampLogcatRingMaxEntries(settingsState.logcat.ringMaxEntries);
    if (prevRingCap !== undefined && ring !== prevRingCap) {
      void syncBackendFilter(parseFilterGroups(debouncedQuery()));
      void refreshLogcatRingStats();
    }
    prevRingCap = ring;
  });

  // When Settings changes the Logcat UI line cap: trim immediately if lowered, or
  // backfill from the ring buffer if raised.
  let prevLogcatMaxUi: number | undefined;
  createEffect(() => {
    const cap = maxUiLinesCap();
    if (prevLogcatMaxUi !== undefined) {
      if (cap < prevLogcatMaxUi) {
        const len = logcatState.entries.length;
        const excess = len - cap;
        if (excess > 0) {
          replaceLogcatEntries(cap === 0 ? [] : logcatState.entries.slice(-cap));
          if (!followTailForList()) {
            setScrollCompensate((c) => c + excess * ROW_HEIGHT);
          }
        }
      } else if (cap > prevLogcatMaxUi) {
        void syncBackendFilter(parseFilterGroups(debouncedQuery()));
      }
    }
    prevLogcatMaxUi = cap;
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
      await setLogcatFilter(restoredSpec).catch((err) => {
        showToast(`Failed to restore logcat filter: ${formatError(err)}`, "error");
      });
    }

    try {
      const entries = await getLogcatEntries({
        count: maxUiLinesCap(),
        // Apply the restored spec so the backfill is already filtered
        minLevel: restoredSpec?.minLevel ?? undefined,
        tag: restoredSpec?.tag ?? undefined,
        text: restoredSpec?.text ?? undefined,
        package: restoredSpec?.package ?? undefined,
        onlyCrashes: restoredSpec?.onlyCrashes ?? false,
      });
      replaceLogcatEntries(entries);
      suggestions.ingest(entries);
      suggestions.flush(true);
    } catch (err) {
      showToast(`Failed to load logcat entries: ${formatError(err)}`, "error");
    }

    try {
      const streaming = await getLogcatStatus();
      setLogcatStreaming(streaming);
    } catch (err) {
      showToast(`Failed to read logcat status: ${formatError(err)}`, "error");
    }

    // eslint-disable-next-line solid/reactivity
    unlistenEntries = await listenLogcatEntries((newEntries) => {
      if (paused()) return;
      const dropped = appendLogcatEntries(newEntries, maxUiLinesCap());
      if (dropped > 0 && !followTailForList()) {
        setScrollCompensate((c) => c + dropped * ROW_HEIGHT);
      }

      suggestions.ingest(newEntries);
      suggestions.flush();
    });

    unlistenCleared = await listenLogcatCleared(() => {
      clearLogcatEntries();
      suggestions.clear();
      setAutoScroll(true);
      virtualListRef?.scrollToBottom();
      void refreshLogcatRingStats();
    });

    // Auto-start on device connect
    unlistenDevices = await listenDeviceListChanged((devices) => {
      if (logcatState.streaming) return;
      const hasAutoStart = settingsState.logcat?.autoStart !== false;
      if (!hasAutoStart) return;
      const online = devices.find((d) => d.connectionState === "online");
      if (online) {
        startLogcat(online.serial)
          .then(() => setLogcatStreaming(true))
          .catch((err) => showToast(`Failed to auto-start logcat: ${formatError(err)}`, "error"));
      }
    });

    // Keep streaming status in sync when the backend reconnects after an
    // unexpected ADB server restart (e.g. Android Studio opening Logcat).
    // The backend never sets streaming=false in this case, so the UI stays
    // consistent; this listener is purely for future indicator use.

    unlistenReconnecting = await listenLogcatReconnecting(() => {
      setLogcatStreaming(true);
    });

    nowTimer = setInterval(() => setNow(Date.now()), 5_000);
    void refreshLogcatRingStats();
  });

  // Refresh ring count when the user opens the Logcat tab (denominator is Rust-only).
  createEffect(() => {
    if (uiState.activeTab === "logcat") void refreshLogcatRingStats();
  });

  // Keep ring-buffer denominator fresh while streaming (cheap IPC; throttled).
  createEffect(() => {
    if (!logcatState.streaming || uiState.activeTab !== "logcat") return;
    void refreshLogcatRingStats();
    const id = window.setInterval(() => {
      void refreshLogcatRingStats();
    }, 2_000);
    onCleanup(() => clearInterval(id));
  });

  onCleanup(() => {
    unlistenEntries?.();
    unlistenCleared?.();
    unlistenDevices?.();
    unlistenReconnecting?.();
    clearInterval(nowTimer);
    clearTimeout(_queryDebounce);
    filterSyncGuard.invalidate();
    // Clear backend filter on unmount so it doesn't persist
    setLogcatFilter(emptyLogcatFilterSpec()).catch(() => {});
  });

  // ── Controls ──────────────────────────────────────────────────────────────────

  async function handleStart() {
    try {
      const device = selectedDevice();
      await startLogcat(device?.serial ?? undefined);
      setLogcatStreaming(true);
      void refreshLogcatRingStats();
    } catch (e) {
      showToast(`Failed to start logcat: ${formatError(e)}`, "error");
    }
  }

  async function handleStop() {
    try {
      await stopLogcat();
      setLogcatStreaming(false);
      void refreshLogcatRingStats();
    } catch (e) {
      showToast(`Failed to stop logcat: ${formatError(e)}`, "error");
    }
  }

  async function handleClear() {
    try {
      await clearLogcat();
      setSelectionAnchor(null);
      setSelectionEnd(null);
    } catch (e) {
      showToast(`Failed to clear logcat: ${formatError(e)}`, "error");
    }
  }

  async function handleRestart() {
    if (restarting()) return;
    setRestarting(true);
    try {
      await stopLogcat();
      await clearLogcat(); // emits logcat:cleared → entries cleared + scroll to bottom
      const device = selectedDevice();
      await startLogcat(device?.serial ?? undefined);
      setLogcatStreaming(true);
    } catch (e) {
      showToast(`Failed to restart logcat: ${formatError(e)}`, "error");
    } finally {
      setRestarting(false);
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

  createEffect(() => {
    filteredEntries();
    setCrashCursor((c) => Math.min(c, Math.max(0, crashIndices().length - 1)));
  });

  // ── Row copy ──────────────────────────────────────────────────────────────────

  function formatEntry(e: LogcatEntry): string {
    const pkg = e.package ? `[${e.package}] ` : "";
    return `${e.timestamp}  ${e.level.toUpperCase()}  ${pkg}${e.tag}: ${e.message}`;
  }

  function handleRowClick(idx: number, e: MouseEvent) {
    if (e.shiftKey && selectionAnchor() !== null) {
      setSelectionEnd(idx);
    } else {
      setSelectionAnchor(idx);
      setSelectionEnd(null);
      // Plain click (no shift) — toggle detail panel
      const entry = filteredEntries()[idx];
      if (entry) {
        setSelectedDetailEntry((prev) => (prev?.id === entry.id ? null : entry));
      }
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
    const text = filteredEntries()
      .slice(lo, hi + 1)
      .map(formatEntry)
      .join("\n");
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
      showToast(`Export failed: ${formatError(e)}`, "error");
    }
  }

  // ── Age pills ─────────────────────────────────────────────────────────────────

  function handleAgePill(value: LogcatAgePillValue) {
    const q = setAgeInQuery(query(), value);
    updateQuery(q.trimEnd() ? q.trimEnd() + " " : "");
  }

  // Clamp row selection when the filtered list shrinks or clears; drop detail if the entry vanished.
  createEffect(() => {
    const entries = filteredEntries();
    const n = entries.length;
    const anchor = selectionAnchor();
    const end = selectionEnd();

    if (n === 0) {
      if (anchor !== null) setSelectionAnchor(null);
      if (end !== null) setSelectionEnd(null);
      if (selectedDetailEntry() !== null) setSelectedDetailEntry(null);
      return;
    }

    const { anchor: na, end: nb } = clampSelectionIndices(anchor, end, n);
    if (na !== anchor) setSelectionAnchor(na);
    if (nb !== end) setSelectionEnd(nb);

    const detail = selectedDetailEntry();
    if (detail !== null && !entries.some((e) => e.id === detail.id)) {
      setSelectedDetailEntry(null);
    }
  });

  // Arrow keys: move selection, show bottom detail, scroll into view (Logcat tab only).
  onMount(() => {
    function handleLogcatGlobalKeydown(e: KeyboardEvent): void {
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      if (uiState.activeTab !== "logcat") return;
      if (isPaletteOpen()) return;
      if (isLogcatTypingTarget(e.target)) return;

      const entries = filteredEntries();
      if (entries.length === 0) return;

      const direction: 1 | -1 = e.key === "ArrowDown" ? 1 : -1;
      const nextIdx = nextSelectableIndex(entries, selectionAnchor(), direction);
      if (nextIdx === null) return;

      e.preventDefault();
      setSelectionEnd(null);
      setSelectionAnchor(nextIdx);
      setSelectedDetailEntry(entries[nextIdx]);
      setAutoScroll(false);
      virtualListRef?.scrollToIndex(nextIdx);
    }

    document.addEventListener("keydown", handleLogcatGlobalKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleLogcatGlobalKeydown));
  });

  function handleJsonBadgeClick(e: MouseEvent, entry: LogcatEntry) {
    e.stopPropagation();
    setSelectedJsonEntry((prev: LogcatEntry | null) => (prev?.id === entry.id ? null : entry));
    setAutoScroll(false);
  }

  function handleScrollToEnd() {
    setSelectionAnchor(null);
    setSelectionEnd(null);
    setSelectedJsonEntry(null);
    setSelectedDetailEntry(null);
    setAutoScroll(true);
    virtualListRef?.scrollToBottom();
  }

  // ── UI helpers ────────────────────────────────────────────────────────────────

  const isFiltered = () => query().trim() !== "";
  const toolbarCount = createMemo(() =>
    formatLogcatToolbarCount({
      queryActive: isFiltered(),
      visible: filteredEntries().length,
      ringTotal: logcatState.ringBufferTotal,
    })
  );
  const crashes = () => crashIndices().length;
  const selRange = () => getSelectionRange();
  const selCount = () => {
    const r = selRange();
    return r ? r[1] - r[0] + 1 : 0;
  };

  // ── Render ────────────────────────────────────────────────────────────────────

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
      <LogcatToolbar
        streaming={logcatState.streaming}
        paused={paused()}
        restarting={restarting()}
        crashes={crashes()}
        selectedCount={selCount()}
        autoScroll={autoScroll()}
        toolbarCount={toolbarCount()}
        onStart={handleStart}
        onStop={handleStop}
        onTogglePaused={() => setPaused((v) => !v)}
        onRestart={handleRestart}
        onClear={handleClear}
        onJumpToLastCrash={jumpToLastCrash}
        onJumpToPreviousCrash={() => jumpToCrash(-1)}
        onJumpToNextCrash={() => jumpToCrash(1)}
        onCopySelectedRows={copySelectedRows}
        onScrollToEnd={handleScrollToEnd}
        onExport={handleExport}
        renderSavedFilterMenu={() => (
          <SavedFilterMenu query={query()} isFiltered={isFiltered()} onApplyQuery={updateQuery} />
        )}
      />

      <LogcatFilterControls
        query={query()}
        knownTags={suggestions.knownTags()}
        knownPackages={suggestions.knownPackages()}
        hasAgeFilter={hasAgeFilter()}
        activeAge={activeAge()}
        activePackage={activePackage()}
        isFiltered={isFiltered()}
        onQueryChange={updateQuery}
        onAgeSelect={handleAgePill}
        onPackageSelect={handlePackageSelect}
        onClear={() => updateQuery("")}
      />

      {/* ── Empty state ───────────────────────────────────────────────────── */}
      <Show when={logcatState.entries.length === 0}>
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
            when={logcatState.streaming}
            fallback={
              <>
                <span style={{ "font-size": "24px", opacity: "0.3" }}>📋</span>
                <span>No logcat data</span>
                <span style={{ "font-size": "11px", opacity: "0.6" }}>
                  Connect a device — logcat starts automatically
                </span>
              </>
            }
          >
            <span style={{ "font-size": "24px", opacity: "0.3" }}>⏳</span>
            <span>Waiting for log entries…</span>
          </Show>
        </div>
      </Show>

      {/* ── Virtualised log list ──────────────────────────────────────────── */}
      <Show when={logcatState.entries.length > 0}>
        <VirtualList
          items={filteredEntries()}
          rowHeight={ROW_HEIGHT}
          autoScroll={followTailForList()}
          scrollCompensate={scrollCompensate()}
          jumpTo={jumpTarget()}
          onScrolledToBottom={() => setAutoScroll(true)}
          onScrolledUp={() => setAutoScroll(false)}
          handle={(api) => {
            virtualListRef = api;
          }}
          style={{
            flex: "1",
            "font-family": "var(--font-mono)",
            "font-size": "11px",
            "line-height": `${ROW_HEIGHT}px`,
          }}
          renderRow={(entry, idx) => {
            if (entry.kind === "processDied" || entry.kind === "processStarted") {
              return <SeparatorRow entry={entry} />;
            }
            return (
              <LogcatVirtualRow
                entry={entry}
                index={idx}
                getSelectionRange={getSelectionRange}
                getAnchor={() => selectionAnchor()}
                getEnd={() => selectionEnd()}
                getDetailEntry={() => selectedDetailEntry()}
                getJsonEntry={() => selectedJsonEntry()}
                onRowClick={(e) => handleRowClick(idx, e)}
                onJsonClick={(e) => handleJsonBadgeClick(e, entry)}
              />
            );
          }}
        />
      </Show>

      {/* ── JSON Detail Panel ─────────────────────────────────────────────── */}
      <Show when={selectedJsonEntry() !== null}>
        <JsonDetailPanel entry={selectedJsonEntry()!} onClose={() => setSelectedJsonEntry(null)} />
      </Show>

      {/* ── Entry Detail Panel ────────────────────────────────────────────── */}
      <Show when={selectedDetailEntry()}>
        {(entry) => (
          <LogEntryDetailPanel
            entry={entry()}
            onClose={() => setSelectedDetailEntry(null)}
            onAddFilter={handleDetailFilter}
          />
        )}
      </Show>
    </div>
  );
}
