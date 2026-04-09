# Build Panel — Production-Ready Design

**Date:** 2026-04-09  
**Status:** Approved  
**Scope:** Build panel performance, history side panel, logging transparency, bug fixes, code cleanup

---

## Problem Statement

The build panel has three categories of issues that block production readiness:

1. **Performance / UX** — the log output list janks when Gradle lines arrive fast, which causes auto-scroll to get stuck (it detects the jank-induced scroll position change as a user scroll and disables itself).
2. **Missing history UI** — `buildState.history` is populated with build records but there is no UI that displays it.
3. **Logging opacity** — not all build commands and phase transitions are surfaced in the log, making it hard to debug failures.

Additionally, a code review surfaced correctness bugs, dead code, and a duplicate implementation that need to be resolved for the codebase to be maintainable.

---

## Goals

- Smooth log rendering at any build output size (scales to 10K+ lines)
- Reliable auto-scroll that never gets stuck
- Build history visible at all times without leaving the panel
- Every command, tool invocation, and phase transition logged
- Cancelled builds recorded in history
- Eliminate dead code and duplicated logic

---

## Non-Goals

- Per-build log persistence (past build logs not stored to disk; only metadata is kept)
- Task tree view (Android Studio–style collapsible task nodes — deferred to future)
- Log retention settings UI (already exists in settings model, not in scope here)

---

## Architecture

### Layout

The build panel splits into two columns:

```
┌─────────────────────────────────────────────────────────┐
│  Toolbar: [■ cancel] [Log] [Problems]   Building… 0:12  │
│  ─────────────────────────────────────────────────────  │
│  History (140px)  │  Log filter bar                      │
│  ─────────────────│  ─────────────────────────────────  │
│  ⟳ assembleDebug  │  ··· 218 lines above (virtual) ···   │
│    running · 0:12 │  10:42:08 [DBG] > Task :app:compile  │
│  ─────────────────│  10:42:14 [DBG] > Task :app:merge    │
│  ✓ assembleRelease│  10:42:20 [ERR] e: Main.kt:42 — Unre │
│    1m 18s · 5m ago│  10:42:20 [INFO] ▶ Working dir: …    │
│  ─────────────────│  ─────────────────────────────────  │
│  ✗ assembleDebug  │  ↓ streaming… 247 lines              │
│    23s · 12m ago  │                                      │
│    3 errors       │                                      │
│  ─────────────────│                                      │
│  ◼ assembleDebug  │                                      │
│    cancelled      │                                      │
└─────────────────────────────────────────────────────────┘
```

The history panel shows metadata only (task name, status icon, duration, error count, relative timestamp). Clicking a past entry highlights it; the log panel always shows the current build's live output (log lines are not persisted per build).

---

## Components

### New: `VirtualList.tsx` (`src/components/common/`)

Generic fixed-height virtual scroller. Only renders visible rows plus an overscan buffer.

**Props:**
```ts
interface VirtualListProps<T> {
  items: T[];
  rowHeight: number;        // fixed px per row
  overscan?: number;        // extra rows above/below (default: 5)
  autoScroll?: boolean;     // scroll to bottom when items grow
  onScrolledUp?: () => void;
  onScrolledToBottom?: () => void;
  renderRow: (item: T, index: number) => JSX.Element;
}
```

**Implementation:**
- Container: `overflow-y: auto`, full height
- Inner spacer: `height = items.length * rowHeight` (positions scroll thumb correctly)
- Visible slice: `startIndex = floor(scrollTop / rowHeight)`, `endIndex = startIndex + ceil(containerHeight / rowHeight) + overscan`
- Rows rendered with `position: absolute; top: index * rowHeight`
- Auto-scroll: after `items` length increases, uses `requestAnimationFrame` to set `scrollTop = scrollHeight` — ensures DOM is settled before measuring

Row height for build logs: **20px** (Gradle output is single-line; long lines are clipped with `text-overflow: ellipsis` and full text on `title` tooltip).

### New: `BuildHistoryPanel.tsx` (`src/components/build/`)

Left history strip. Reads `buildState.history` reactively.

**Props:**
```ts
interface BuildHistoryPanelProps {
  selectedId: number | null;
  onSelect: (record: BuildRecord | null) => void;
}
```

**Behaviour:**
- Renders `buildState.history` in reverse order (newest first), plus current in-progress build at top
- Each entry shows: status icon (✓ / ✗ / ⟳ / ◼), task name (truncated), duration, relative time, error count if > 0
- Selected entry gets `border-left: 2px solid <status-color>` + background tint
- Currently active build is always auto-selected
- Width: fixed 140px, `overflow-y: auto`

### Modified: `LogViewer.tsx` (`src/components/common/`)

- Replace `<For each={filtered()}>` with `<VirtualList items={filtered()} rowHeight={20} renderRow={LogRow} autoScroll={autoScroll()} />`
- Remove the manual `createEffect` + `scrollRef.scrollTop` auto-scroll (delegated to `VirtualList`)
- Remove `userScrolledUp` boolean — `VirtualList` exposes `onScrolledUp` / `onScrolledToBottom` callbacks
- `uniqueSources` memo: unchanged (build logs have a single source so this is O(1) in practice)

### Modified: `BuildPanel.tsx` (`src/components/build/`)

- Add `BuildHistoryPanel` to the left of the content area
- Track `selectedHistoryId` signal (defaults to current build's id, or null)
- Layout: `display: flex; flex-direction: row` for the content area

### Modified: `build.store.ts` (`src/stores/`)

Add line batching:

```ts
// pending buffer — not reactive
let _pendingLines: BuildLine[] = [];
let _flushTimer: ReturnType<typeof setTimeout> | null = null;

function _scheduledFlush(): void {
  _flushTimer = null;
  if (_pendingLines.length === 0) return;
  const batch = _pendingLines.splice(0);
  // single store update for whole batch
  buildLogStore.pushEntries(batch.map(lineToLogEntry));
  // parse errors/warnings from batch
  const errors = batch.filter(l => l.kind === "error");
  const warnings = batch.filter(l => l.kind === "warning");
  if (errors.length > 0 || warnings.length > 0) {
    setBuildState(produce(s => {
      for (const l of errors) s.errors.push(buildLineToError(l));
      for (const l of warnings) s.warnings.push(buildLineToError(l));
    }));
  }
}

export function addBuildLine(line: BuildLine): void {
  _pendingLines.push(line);
  if (_flushTimer === null) {
    _flushTimer = setTimeout(_scheduledFlush, 50);
  }
}

export function flushPendingLines(): void {
  if (_flushTimer !== null) { clearTimeout(_flushTimer); _flushTimer = null; }
  _scheduledFlush();
}
```

`flushPendingLines()` is called from `setBuildResult` and `cancelBuildState` to ensure no lines are lost when the build ends.

Add `pushEntries(entries: LogEntry[])` to `LogStore` (batch variant of `pushEntry`).

---

## Bug Fixes

### 1. Cancelled builds not in history

**File:** `src/services/build.service.ts` — `cancelBuild()`

**Current:** calls `cancelBuildState()` and `cancelBuildApi()` but never calls `finalizeBuild`.

**Fix:**
```ts
export async function cancelBuild(): Promise<void> {
  const resolve = _resolveBuildComplete;
  _resolveBuildComplete = null;
  cancelBuildState();
  flushPendingLines();
  await cancelBuildApi();
  // Record the cancelled build in history
  await finalizeBuild({
    success: false,
    durationMs: Date.now() - (buildState.startedAt ?? Date.now()),
    errors: buildState.errors,
    task: buildState.currentTask ?? "unknown",
    startedAt: new Date(buildState.startedAt ?? Date.now()).toISOString(),
  }).catch(err => console.error("[build] Failed to finalize cancelled build:", err));
  resolve?.({ success: false, durationMs: 0 });
}
```

### 2. `jumpToBuildError` is a stub

**File:** `src/services/build.service.ts` — `jumpToBuildError()`

**Current:** shows a Toast with the error message. Does not open Android Studio.

**Fix:** call `openInStudio(classPath, filename, line)` when `error.file` is present. Parse the file path to extract class path (dots) and filename. Fall back to Toast when file info is unavailable.

### 3. `clear_build_log` race condition

**File:** `src-tauri/src/commands/build.rs` — `run_gradle_task()`

**Current:** `clear_build_log` is called on line 205, *after* `spawn`. The `on_line` callback could theoretically push lines before the clear.

**Fix:** move `clear_build_log(&build_state.build_log)` to *before* the `spawn` call (line 98).

### 4. Dead code: `BuildSettings.buildVariant` / `selectedDevice`

**Files:** `src-tauri/src/models/settings.rs`, `src/bindings/BuildSettings.ts`, `src/stores/settings.store.ts`

**Current:** these two fields exist in `BuildSettings` but are never read. The active variant comes from `variant.store`, the device from `device.store`.

**Fix:** remove both fields from `BuildSettings`, regenerate the TypeScript binding, remove from `settings.store.ts` defaults.

### 5. Duplicate Gradle execution implementations

**Files:** `src-tauri/src/commands/build.rs` (`run_gradle_task`), `src-tauri/src/services/build_runner.rs` (`run_task`)

**Current:** two separate implementations of spawning Gradle — one event-driven (frontend), one polling-based (MCP). They duplicate error parsing, duration extraction, and status management.

**Fix:** `run_task` in `build_runner.rs` delegates to `run_gradle_task`'s core logic by sharing the `on_line` / `on_exit` callbacks via a common helper. The polling loop in `run_task` is replaced with a `tokio::sync::oneshot` channel signaled from `on_exit`. Reduces ~130 lines of duplicated logic.

---

## Logging Improvements

The following log lines are added to `build.service.ts`:

```
▶ Build started: assembleDebug                        ← new
▶ Working directory: ~/projects/myapp
▶ JAVA_HOME: /opt/homebrew/opt/openjdk@17             ← new (if set)
▶ ANDROID_HOME: ~/Library/Android/sdk                 ← new (if set)
▶ ./gradlew assembleDebug --console=plain
  ... (gradle output) ...
▶ Build complete: SUCCESS in 42s                      ← new
▶ Searching for APK (variant: debug)…
▶ APK: app/build/outputs/apk/debug/app-debug.apk
▶ Installing on: Pixel 7 Pro (API 34) [emulator-5554] ← new: device model
▶ adb install app/build/outputs/apk/debug/app-debug.apk
▶ Install: Success (1.2s)                             ← new: duration
▶ Launching: com.example.app/.MainActivity
▶ Launch: OK
```

Environment variables (`JAVA_HOME`, `ANDROID_HOME`) are read from `settingsState` at log time in `build.service.ts`.

Device model/API info comes from `deviceState.devices.find(d => d.serial === serial)`.

---

## Error Handling

- `flushPendingLines()` is called from `build.service.ts` before any state-finalizing call: in the `build:complete` listener (before `setBuildResult`) and in `cancelBuild()` (before `cancelBuildState`) — no lines can be lost when a build ends mid-batch
- `VirtualList` auto-scroll degrades gracefully: if `requestAnimationFrame` callback fires after unmount, it checks `scrollRef` is still attached before setting `scrollTop`
- `jumpToBuildError` wraps `openInStudio` in try/catch and falls back to Toast — Studio may not be available on all machines
- The `finalizeBuild` call in `cancelBuild` is fire-and-forget with a `.catch` logger — a failure to persist a cancelled build is non-fatal

---

## Testing

**New unit tests:**
- `VirtualList.test.tsx` — renders correct slice for given scrollTop; auto-scroll fires after item append; `onScrolledUp` fires when user scrolls up
- `build.store.test.ts` (extend existing) — `addBuildLine` batches correctly; `flushPendingLines` flushes immediately; cancelled build phase clears pending buffer

**Existing tests that need updating:**
- `src/stores/build.store.test.ts` — `clearBuild` and `cancelBuildState` tests must account for `flushPendingLines` being called
- `src-tauri/tests/build_integration.rs` — add test for `run_task` using oneshot channel instead of polling

**Manual verification checklist:**
- [ ] 500+ line build: no jank, auto-scroll stays at bottom
- [ ] User scrolls up mid-build: auto-scroll disables; scrolling back to bottom re-enables
- [ ] Cancel mid-build: cancelled entry appears in history panel
- [ ] Click past failed build in history: entry expands to show errors
- [ ] `jumpToBuildError` on an error with file info: opens Android Studio at correct line
- [ ] Settings without JAVA_HOME/ANDROID_HOME: log lines for those are omitted (not shown as null)

---

## Files Changed Summary

**Frontend:**
- `src/components/common/VirtualList.tsx` — new
- `src/components/common/LogViewer.tsx` — use VirtualList, remove manual scroll
- `src/components/build/BuildHistoryPanel.tsx` — new
- `src/components/build/BuildPanel.tsx` — add history panel, wire layout
- `src/stores/build.store.ts` — add batching, `flushPendingLines`
- `src/stores/log.store.ts` — add `pushEntries` batch method
- `src/services/build.service.ts` — fix `cancelBuild`, richer logging, fix `jumpToBuildError`
- `src/bindings/BuildSettings.ts` — remove `buildVariant`, `selectedDevice`

**Backend:**
- `src-tauri/src/models/settings.rs` — remove `build_variant`, `selected_device` from `BuildSettings`
- `src-tauri/src/commands/build.rs` — move `clear_build_log` before spawn
- `src-tauri/src/services/build_runner.rs` — replace polling loop in `run_task` with oneshot channel

**Tests:**
- `src/components/common/VirtualList.test.tsx` — new
- `src/stores/build.store.test.ts` — extend for batching
- `src-tauri/tests/build_integration.rs` — update for refactored `run_task`
