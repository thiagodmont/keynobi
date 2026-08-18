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
  getLogcatContextEntries,
  getLogcatStatus,
  getLogcatStats,
  setLogcatFilter,
  listenLogcatEntries,
  listenLogcatCleared,
  listenLogcatReconnecting,
  listenLogcatStopped,
  listenDeviceListChanged,
  formatError,
  type LogcatEntry,
} from "@/lib/tauri-api";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { save } from "@tauri-apps/plugin-dialog";
import { selectedDevice } from "@/stores/device.store";
import { logcatRowHeightForFontSize, settingsState } from "@/stores/settings.store";
import { EmptyState, Icon, MenuList, MenuListItem, showToast } from "@/components/ui";
import { VirtualList, type VirtualListHandle, isPaletteOpen } from "@/components/ui";
import {
  parseAge,
  parseFilterGroups,
  matchesFilterGroups,
  buildEffectiveQueryWithDisabledPills,
  setAgeInQuery,
  setPackageInQuery,
  getPackageFromQuery,
  appendLogEntryDetailFilterToken,
  type LogEntryDetailFilterMode,
  type FilterGroup,
} from "@/lib/logcat-query";
import { setMinePackage } from "@/lib/logcat-mine-package";
import { resolveQueryVariables } from "@/lib/logcat-query-variables";
import { projectState } from "@/stores/project.store";
import { buildState } from "@/stores/build.store";
import { LogEntryDetailPanel } from "./LogEntryDetailPanel";
import { getLastActiveQuery, setLastActiveQuery } from "@/lib/logcat-filter-storage";
import { uiState } from "@/stores/ui.store";
import {
  clampSelectionIndices,
  nextSelectableIndex,
  shiftSelectionAfterFrontDrop,
} from "./logcat-selection-nav";
import { formatLogcatToolbarCount } from "./logcat-toolbar-count";
import { clampLogcatMaxUiLines, clampLogcatRingMaxEntries } from "@/lib/logcat-ui-lines";
import { effectiveLogcatFollowTail } from "@/lib/logcat-follow-tail";
import { isLifecycleLogcatEntry } from "@/lib/logcat-lifecycle";
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
  setLogcatDroppedLines,
  setLogcatStreaming,
} from "@/stores/logcat.store";
import { createLatestOnlyGuard } from "@/services/logcat.service";
import { JsonDetailPanel } from "./LogcatJsonDetailPanel";
import { LogcatVirtualRow, SeparatorRow } from "./LogcatRows";
import {
  LogcatFilterControls,
  LOGCAT_AGE_PILLS,
  type LogcatAgePillValue,
} from "./LogcatFilterControls";
import { LogcatToolbar } from "./LogcatToolbar";
import { createLogcatSuggestionRuntime } from "./logcat-suggestion-runtime";
import { createLogcatQueryController } from "./logcat-query-controller";
import { formatLogcatEntries } from "./logcat-entry-format";
import { copyToClipboard } from "@/utils/clipboard";
import {
  LOGCAT_CONTEXT_EXPAND_COUNT,
  LOGCAT_MAX_EXPANDED_CONTEXT_ENTRIES,
  isExpandedContextRow,
  mergeExpandedContextEntries,
  mergeLogcatEntriesChronologically,
} from "./logcat-context-expansion";

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

type LogcatContextMenu = {
  entry: LogcatEntry;
  x: number;
  y: number;
};

type LogcatContextDirection = "before" | "after";

export function LogcatPanel(): JSX.Element {
  function resetQueryInteraction(): void {
    exitReadMode();
    setSelectionAnchor(null);
    setSelectionEnd(null);
    setSelectedJsonEntry(null);
    setSelectedDetailEntry(null);
  }

  const queryController = createLogcatQueryController({
    onQueryInteractionReset: resetQueryInteraction,
  });
  const {
    query,
    debouncedQuery,
    disabledPillIds,
    queryVariableValues,
    debouncedQueryVariableValues,
    updateQuery,
    updateQueryVariableValue,
    deleteQueryVariable,
    insertQueryVariable,
    applyPresetQuery,
    togglePillDisabled,
    clearQuery,
    restoreQuery,
  } = queryController;

  function isFiltered() {
    return query().trim() !== "";
  }

  const [autoScroll, setAutoScroll] = createSignal(settingsState.logcat.autoScrollToEnd !== false);
  const [scrollCompensate, setScrollCompensate] = createSignal(0);
  const rowHeight = createMemo(() =>
    logcatRowHeightForFontSize(settingsState.logcat.outputFontSize)
  );
  const [frozenEntries, setFrozenEntries] = createSignal<LogcatEntry[] | null>(null, {
    equals: false,
  });
  const [pendingNewEntries, setPendingNewEntries] = createSignal(0);
  let virtualListRef: VirtualListHandle | undefined;
  const [paused, setPaused] = createSignal(false);
  const [restarting, setRestarting] = createSignal(false);
  const [showLifecycle, setShowLifecycle] = createSignal(true);
  const [expandedContextEntries, setExpandedContextEntries] = createSignal<LogcatEntry[]>([], {
    equals: false,
  });
  const [contextMenu, setContextMenu] = createSignal<LogcatContextMenu | null>(null);
  let contextMenuRef: HTMLDivElement | undefined;

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
      selectedDetailEntry: selectedDetailEntry(),
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
  // Set by onCleanup. The listener registrations below are awaited, so an
  // unmount can land before they resolve — without this the assignment happens
  // after cleanup ran and the listener leaks.
  let disposed = false;
  let unlistenStopped: (() => void) | undefined;
  let nowTimer: ReturnType<typeof setInterval> | undefined;

  // ── Parsed query (debounced — avoids re-parsing on every keystroke)
  const effectiveDebouncedQueryTemplate = createMemo(() =>
    buildEffectiveQueryWithDisabledPills(debouncedQuery(), disabledPillIds())
  );
  const effectiveDebouncedQuery = createMemo(() =>
    resolveQueryVariables(effectiveDebouncedQueryTemplate(), debouncedQueryVariableValues())
  );
  const parsedGroups = createMemo(() => parseFilterGroups(effectiveDebouncedQuery()));
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

  const filteredEntryIds = createMemo(() => new Set(filteredEntries().map((entry) => entry.id)));
  const expandedContextIds = createMemo(
    () => new Set(expandedContextEntries().map((entry) => entry.id))
  );
  const mergedEntries = createMemo(() => {
    if (!isFiltered()) return filteredEntries();
    return mergeLogcatEntriesChronologically(filteredEntries(), expandedContextEntries());
  });

  const displayedEntries = createMemo(() => {
    const entries = frozenEntries() ?? mergedEntries();
    return showLifecycle() ? entries : entries.filter((entry) => !isLifecycleLogcatEntry(entry));
  });
  const readMode = createMemo(() => frozenEntries() !== null);

  function enterReadMode(): void {
    if (frozenEntries() === null) {
      setFrozenEntries(mergedEntries().slice());
      setPendingNewEntries(0);
    }
    setAutoScroll(false);
  }

  function clearExpandedContext(): void {
    setExpandedContextEntries([]);
    setContextMenu(null);
  }

  function contextMenuPosition(x: number, y: number): { x: number; y: number } {
    const menuWidth = 176;
    const menuHeight = 80;
    const maxX = Math.max(8, window.innerWidth - menuWidth - 8);
    const maxY = Math.max(8, window.innerHeight - menuHeight - 8);
    return {
      x: Math.min(Math.max(8, x), maxX),
      y: Math.min(Math.max(8, y), maxY),
    };
  }

  function handleRowContextMenu(entry: LogcatEntry, e: MouseEvent): void {
    if (!isFiltered()) return;
    e.preventDefault();
    enterReadMode();
    setContextMenu({ entry, ...contextMenuPosition(e.clientX, e.clientY) });
  }

  function isExpandedRow(entry: LogcatEntry): boolean {
    return isExpandedContextRow(entry.id, expandedContextIds(), filteredEntryIds());
  }

  async function expandLogContext(direction: LogcatContextDirection): Promise<void> {
    const menu = contextMenu();
    if (!menu) return;
    setContextMenu(null);
    try {
      const entries = await getLogcatContextEntries({
        anchorId: menu.entry.id,
        direction,
        count: LOGCAT_CONTEXT_EXPAND_COUNT,
      });
      if (entries.length === 0) {
        showToast(
          direction === "before" ? "No earlier logs in buffer" : "No later logs in buffer",
          "info"
        );
        return;
      }

      setExpandedContextEntries((current) =>
        mergeExpandedContextEntries(current, entries, LOGCAT_MAX_EXPANDED_CONTEXT_ENTRIES)
      );
      const frozen = frozenEntries();
      if (frozen !== null) {
        setFrozenEntries(mergeLogcatEntriesChronologically(frozen, entries));
      }
    } catch (err) {
      showToast(`Failed to expand log context: ${formatError(err)}`, "error");
    }
  }

  function exitReadMode(): void {
    setFrozenEntries(null);
    setPendingNewEntries(0);
  }

  function countVisibleIncoming(entries: LogcatEntry[]): number {
    if (entries.length === 0) return 0;
    const lifecycleFiltered = showLifecycle()
      ? entries
      : entries.filter((entry) => !isLifecycleLogcatEntry(entry));
    if (!needsFrontendFilter()) return lifecycleFiltered.length;
    const groups = parsedGroups();
    const currentNow = hasAgeFilter() ? now() : Date.now();
    return lifecycleFiltered.filter((entry) => matchesFilterGroups(entry, groups, currentNow))
      .length;
  }

  const filteredEntryIndexById = createMemo(() => {
    const indexById = new Map<LogcatEntry["id"], number>();
    displayedEntries().forEach((entry, index) => {
      indexById.set(entry.id, index);
    });
    return indexById;
  });

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
    const frozen = frozenEntries();
    if (frozen !== null) {
      const indices: number[] = [];
      displayedEntries().forEach((entry, index) => {
        if (entry.isCrash) indices.push(index);
      });
      return indices;
    }
    if (!needsFrontendFilter() && showLifecycle()) {
      // Fast path: no frontend filtering, use the incremental index.
      return logcatState.crashIndicesFull;
    }
    // Slow path: frontend tokens or lifecycle display filter active, rescan the visible set.
    const indices: number[] = [];
    displayedEntries().forEach((e, i) => {
      if (e.isCrash) indices.push(i);
    });
    return indices;
  });

  const activeAge = createMemo(() => {
    const t = parsedTokens().find((t) => t.type === "age") as
      { type: "age"; seconds: number } | undefined;
    if (!t) return null;
    for (const p of LOGCAT_AGE_PILLS) {
      if (p.value && parseAge(p.value) === t.seconds) return p.value;
    }
    return null;
  });

  const activePackage = createMemo(() => getPackageFromQuery(effectiveDebouncedQuery()));

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
      setLogcatDroppedLines(Number(s.droppedLines));
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
  let _prevEffectiveDebouncedQuery = "";
  createEffect(() => {
    const q = debouncedQuery();
    const effectiveQuery = effectiveDebouncedQuery();
    if (q !== _prevDebouncedQuery) {
      _prevDebouncedQuery = q;
      setLastActiveQuery(q);
    }
    if (effectiveQuery === _prevEffectiveDebouncedQuery) return;
    _prevEffectiveDebouncedQuery = effectiveQuery;
    clearExpandedContext();
    syncBackendFilter(parseFilterGroups(effectiveQuery));
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
    const q = effectiveDebouncedQuery();
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
    if (buildState.lastLaunchedPackage) {
      setMinePackage(buildState.lastLaunchedPackage);
    }
    const q = query();
    if (q.includes("package:mine") || q.includes("pkg:mine")) {
      void syncBackendFilter(parseFilterGroups(effectiveDebouncedQuery()));
      return;
    }
    const next = setPackageInQuery(q, "mine");
    updateQuery(next.trimEnd() ? next.trimEnd() + " " : "");
  });

  // When Settings changes the in-memory ring size, resync the list from Rust.
  let prevRingCap: number | undefined;
  createEffect(() => {
    const ring = clampLogcatRingMaxEntries(settingsState.logcat.ringMaxEntries);
    if (prevRingCap !== undefined && ring !== prevRingCap) {
      void syncBackendFilter(parseFilterGroups(effectiveDebouncedQuery()));
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
          if (!readMode() && !followTailForList()) {
            setScrollCompensate((c) => c + excess * rowHeight());
          }
        }
      } else if (cap > prevLogcatMaxUi) {
        void syncBackendFilter(parseFilterGroups(effectiveDebouncedQuery()));
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
      restoreQuery(savedQuery);
      _prevDebouncedQuery = savedQuery;
      _prevEffectiveDebouncedQuery = savedQuery;
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
    const _unlistenEntries = await listenLogcatEntries((newEntries) => {
      if (paused()) return;
      const dropped = appendLogcatEntries(newEntries, maxUiLinesCap());
      if (readMode()) {
        const visibleNew = countVisibleIncoming(newEntries);
        if (visibleNew > 0) setPendingNewEntries((count) => count + visibleNew);
      } else if (dropped > 0 && !followTailForList()) {
        const shifted = shiftSelectionAfterFrontDrop(selectionAnchor(), selectionEnd(), dropped);
        setSelectionAnchor(shifted.anchor);
        setSelectionEnd(shifted.end);
        setScrollCompensate((c) => c + dropped * rowHeight());
      }

      suggestions.ingest(newEntries);
      suggestions.flush();
    });
    if (disposed) _unlistenEntries();
    else unlistenEntries = _unlistenEntries;

    const _unlistenCleared = await listenLogcatCleared(() => {
      exitReadMode();
      clearExpandedContext();
      clearLogcatEntries();
      suggestions.clear();
      setAutoScroll(true);
      virtualListRef?.scrollToBottom();
      void refreshLogcatRingStats();
    });
    if (disposed) _unlistenCleared();
    else unlistenCleared = _unlistenCleared;

    // Auto-start on device connect
    const _unlistenDevices = await listenDeviceListChanged((devices) => {
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
    if (disposed) _unlistenDevices();
    else unlistenDevices = _unlistenDevices;

    // Keep streaming status in sync when the backend reconnects after an
    // unexpected ADB server restart (e.g. Android Studio opening Logcat).
    // The backend never sets streaming=false in this case, so the UI stays
    // consistent; this listener is purely for future indicator use.

    const _unlistenReconnecting = await listenLogcatReconnecting(() => {
      setLogcatStreaming(true);
    });
    if (disposed) _unlistenReconnecting();
    else unlistenReconnecting = _unlistenReconnecting;

    // Terminal stop — the backend gave up reconnecting, so the UI must not keep
    // showing a live indicator.
    const _unlistenStopped = await listenLogcatStopped((reason) => {
      setLogcatStreaming(false);
      showToast(`Logcat stopped: ${reason}`, "error");
    });
    if (disposed) _unlistenStopped();
    else unlistenStopped = _unlistenStopped;

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
    disposed = true;
    unlistenEntries?.();
    unlistenCleared?.();
    unlistenDevices?.();
    unlistenReconnecting?.();
    unlistenStopped?.();
    clearInterval(nowTimer);
    filterSyncGuard.invalidate();
    // Clear backend filter on unmount so it doesn't persist
    setLogcatFilter(emptyLogcatFilterSpec()).catch(() => {});
  });

  onMount(() => {
    function closeContextMenu(e: MouseEvent): void {
      const target = e.target as globalThis.Node | null;
      if (target && contextMenuRef?.contains(target)) return;
      setContextMenu(null);
    }

    function handleContextMenuEscape(e: KeyboardEvent): void {
      if (e.key === "Escape") setContextMenu(null);
    }

    document.addEventListener("mousedown", closeContextMenu);
    document.addEventListener("keydown", handleContextMenuEscape);
    onCleanup(() => {
      document.removeEventListener("mousedown", closeContextMenu);
      document.removeEventListener("keydown", handleContextMenuEscape);
    });
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

  async function handleTogglePaused() {
    if (!paused()) {
      setPaused(true);
      return;
    }

    setPaused(false);
    await syncBackendFilter(parseFilterGroups(effectiveDebouncedQuery()));
  }

  async function handleClear() {
    try {
      await clearLogcat();
      clearExpandedContext();
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
    enterReadMode();
  }

  function jumpToLastCrash() {
    const indices = crashIndices();
    if (!indices.length) return;
    const last = indices.length - 1;
    setCrashCursor(last);
    setJumpTarget(indices[last]);
    enterReadMode();
  }

  createEffect(() => {
    displayedEntries();
    setCrashCursor((c) => Math.min(c, Math.max(0, crashIndices().length - 1)));
  });

  // ── Row copy ──────────────────────────────────────────────────────────────────

  function currentEntryIndex(entry: LogcatEntry): number {
    return filteredEntryIndexById().get(entry.id) ?? -1;
  }

  function handleRowClick(entry: LogcatEntry, e: MouseEvent) {
    const idx = currentEntryIndex(entry);
    if (idx < 0) return;
    if (e.shiftKey && selectionAnchor() !== null) {
      setSelectionEnd(idx);
    } else {
      setSelectionAnchor(idx);
      setSelectionEnd(null);
      enterReadMode();
      // Plain click (no shift) — toggle detail panel
      setSelectedDetailEntry((prev) => (prev?.id === entry.id ? null : entry));
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
    const text = formatLogcatEntries(displayedEntries().slice(lo, hi + 1));
    await copyToClipboard(text);
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
      const text = formatLogcatEntries(displayedEntries());
      await writeTextFile(path, text);
      showToast(`Exported ${displayedEntries().length} entries`, "success");
    } catch (e) {
      showToast(`Export failed: ${formatError(e)}`, "error");
    }
  }

  // ── Age pills ─────────────────────────────────────────────────────────────────

  function handleAgePill(value: LogcatAgePillValue) {
    const q = setAgeInQuery(query(), value);
    updateQuery(q.trimEnd() ? q.trimEnd() + " " : "");
  }

  // Clamp row selection when the displayed list shrinks or clears; drop detail if the entry vanished.
  createEffect(() => {
    const entries = displayedEntries();
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

      const entries = displayedEntries();
      if (entries.length === 0) return;

      const direction: 1 | -1 = e.key === "ArrowDown" ? 1 : -1;
      const nextIdx = nextSelectableIndex(entries, selectionAnchor(), direction);
      if (nextIdx === null) return;

      e.preventDefault();
      setSelectionEnd(null);
      setSelectionAnchor(nextIdx);
      setSelectedDetailEntry(entries[nextIdx]);
      enterReadMode();
      virtualListRef?.scrollToIndex(nextIdx);
    }

    document.addEventListener("keydown", handleLogcatGlobalKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleLogcatGlobalKeydown));
  });

  function handleJsonBadgeClick(e: MouseEvent, entry: LogcatEntry) {
    e.stopPropagation();
    setSelectedJsonEntry((prev: LogcatEntry | null) => (prev?.id === entry.id ? null : entry));
    enterReadMode();
  }

  function handleScrollToEnd() {
    exitReadMode();
    setSelectionAnchor(null);
    setSelectionEnd(null);
    setSelectedJsonEntry(null);
    setSelectedDetailEntry(null);
    clearExpandedContext();
    setAutoScroll(true);
    virtualListRef?.scrollToBottom();
  }

  // ── UI helpers ────────────────────────────────────────────────────────────────

  const toolbarCount = createMemo(() =>
    formatLogcatToolbarCount({
      queryActive: isFiltered(),
      visible: displayedEntries().length,
      ringTotal: logcatState.ringBufferTotal,
      droppedLines: logcatState.droppedLines,
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
        newEntriesCount={pendingNewEntries()}
        toolbarCount={toolbarCount()}
        onStart={handleStart}
        onStop={handleStop}
        onTogglePaused={() => {
          void handleTogglePaused();
        }}
        onRestart={handleRestart}
        onClear={handleClear}
        onJumpToLastCrash={jumpToLastCrash}
        onJumpToPreviousCrash={() => jumpToCrash(-1)}
        onJumpToNextCrash={() => jumpToCrash(1)}
        onCopySelectedRows={copySelectedRows}
        onScrollToEnd={handleScrollToEnd}
        onExport={handleExport}
      />

      <LogcatFilterControls
        query={query()}
        knownTags={suggestions.knownTags()}
        knownPackages={suggestions.knownPackages()}
        hasAgeFilter={hasAgeFilter()}
        activeAge={activeAge()}
        activePackage={activePackage()}
        isFiltered={isFiltered()}
        disabledPillIds={disabledPillIds()}
        variableValues={queryVariableValues()}
        showLifecycle={showLifecycle()}
        onQueryChange={updateQuery}
        onTogglePillDisabled={togglePillDisabled}
        onVariableValueChange={updateQueryVariableValue}
        onVariableDelete={deleteQueryVariable}
        onVariableInsert={insertQueryVariable}
        onAgeSelect={handleAgePill}
        onPackageSelect={handlePackageSelect}
        onToggleLifecycle={() => setShowLifecycle((v) => !v)}
        onApplySavedQuery={applyPresetQuery}
        onClear={clearQuery}
      />

      {/* ── Empty state ───────────────────────────────────────────────────── */}
      <Show when={logcatState.entries.length === 0}>
        <div
          style={{
            flex: "1",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
          }}
        >
          <Show
            when={logcatState.streaming}
            fallback={
              <EmptyState
                density="compact"
                icon="terminal"
                title="No logcat data"
                description="Connect a device — logcat starts automatically"
              />
            }
          >
            <EmptyState density="compact" icon="spinner" title="Waiting for log entries…" />
          </Show>
        </div>
      </Show>

      {/* ── Virtualised log list ──────────────────────────────────────────── */}
      <Show when={logcatState.entries.length > 0}>
        <VirtualList
          items={displayedEntries()}
          rowHeight={rowHeight()}
          autoScroll={followTailForList()}
          data-testid="logcat-virtual-list"
          scrollCompensate={scrollCompensate()}
          jumpTo={jumpTarget()}
          onScrolledUp={enterReadMode}
          handle={(api) => {
            virtualListRef = api;
          }}
          style={{
            flex: "1",
            "font-family": "var(--font-mono)",
            "font-size": "var(--font-size-logcat-output)",
            "line-height": "var(--logcat-row-height)",
          }}
          renderRow={(entry) => {
            if (entry.kind === "processDied" || entry.kind === "processStarted") {
              return (
                <SeparatorRow
                  entry={entry}
                  expandedContext={isExpandedRow(entry)}
                  onContextMenu={(e) => handleRowContextMenu(entry, e)}
                />
              );
            }
            return (
              <LogcatVirtualRow
                entry={entry}
                getIndex={() => currentEntryIndex(entry)}
                getSelectionRange={getSelectionRange}
                getAnchor={() => selectionAnchor()}
                getEnd={() => selectionEnd()}
                getDetailEntry={() => selectedDetailEntry()}
                getJsonEntry={() => selectedJsonEntry()}
                expandedContext={isExpandedRow(entry)}
                onRowClick={(e) => handleRowClick(entry, e)}
                onContextMenu={(e) => handleRowContextMenu(entry, e)}
                onJsonClick={(e) => handleJsonBadgeClick(e, entry)}
              />
            );
          }}
        />
      </Show>

      <Show when={contextMenu()}>
        {(menu) => (
          <MenuList
            role="menu"
            surface="floating"
            listRef={(el) => {
              contextMenuRef = el;
            }}
            class="logcat-context-menu"
            style={{
              position: "fixed",
              left: `${menu().x}px`,
              top: `${menu().y}px`,
              width: "176px",
              "z-index": 1000,
            }}
          >
            <MenuListItem role="menuitem" onClick={() => void expandLogContext("before")}>
              <Icon name="arrow-up" size={12} /> Expand 10 up
            </MenuListItem>
            <MenuListItem role="menuitem" onClick={() => void expandLogContext("after")}>
              <Icon name="arrow-down" size={12} /> Expand 10 down
            </MenuListItem>
          </MenuList>
        )}
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
