# Build History Improvements Design

**Date:** 2026-04-09
**Status:** Approved

## Problem

Two bugs in the build history side panel:

1. **Multi-select on same name:** After an app restart, `BuildStateInner::new()` resets `next_id` to `1` while `load_build_history()` loads persisted records that already carry IDs from the previous session. New builds are assigned IDs that collide with existing history entries. When the user clicks one record, all records sharing that ID highlight simultaneously.

2. **No way to clear history:** Users cannot remove old build records from the panel.

## Approach

Option A — minimal and targeted:

- Fix the ID bug by initializing `next_id` from persisted history rather than hardcoding `1`.
- Add a clear button to the history panel header that wipes the in-memory and on-disk history.

No schema changes, no migrations, no new files.

## Changes

### 1. ID Collision Fix (Rust)

**File:** `src-tauri/src/services/build_runner.rs`

In `BuildStateInner::new()`, replace `next_id: 1` with a value derived from the loaded history:

```rust
let history = load_build_history();
let next_id = history.iter().map(|r| r.id).max().unwrap_or(0) + 1;
Self {
    current_build: None,
    status: BuildStatus::Idle,
    history,
    current_errors: vec![],
    next_id,
}
```

This is forward-looking: existing persisted records keep their IDs and the counter simply continues from where the last session left off.

### 2. Clear History Command (Rust)

**File:** `src-tauri/src/commands/build.rs`

New Tauri command `clear_build_history`:
- Locks `BuildStateInner`.
- Clears the `history` deque.
- Deletes `build-history.json` from disk (or writes an empty array).
- Returns `Ok(())`.

Register the command in `src-tauri/src/lib.rs`.

### 3. Frontend Store Action

**File:** `src/stores/build.store.ts`

New exported function `clearBuildHistory()`:
- Calls `invoke("clear_build_history")`.
- Sets `buildState.history` to `[]`.

### 4. Tauri API Binding

**File:** `src/lib/tauri-api.ts`

Add `clearBuildHistory()` wrapper that calls `invoke<void>("clear_build_history")`.

### 5. UI — Clear Button

**File:** `src/components/build/BuildHistoryPanel.tsx`

- Add optional `onClear?: () => void` prop to `BuildHistoryPanelProps`.
- In the "Builds" header row, add a small trash icon button (`×` or similar), visible only when `history().length > 0`.
- Clicking it calls `props.onClear?.()`.

**File:** `src/components/build/BuildPanel.tsx`

- Wire `onClear` prop: calls `clearBuildHistory()` then `setSelectedHistoryId(null)`.

## Data Flow

```
User clicks trash
  → BuildPanel.handleClearHistory()
    → clearBuildHistory() [store]
      → invoke("clear_build_history") [Tauri]
        → clears deque + disk file [Rust]
    → setSelectedHistoryId(null)
      → history panel renders empty state
```

## Non-Goals

- No confirmation dialog — the action is low-stakes (history auto-rebuilds with new runs).
- No undo.
- No per-record delete.

## Files Affected

| File | Change |
|------|--------|
| `src-tauri/src/services/build_runner.rs` | Fix `next_id` initialization |
| `src-tauri/src/commands/build.rs` | Add `clear_build_history` command |
| `src-tauri/src/lib.rs` | Register new command |
| `src/lib/tauri-api.ts` | Add `clearBuildHistory` binding |
| `src/stores/build.store.ts` | Add `clearBuildHistory` action |
| `src/components/build/BuildHistoryPanel.tsx` | Add `onClear` prop + trash button |
| `src/components/build/BuildPanel.tsx` | Wire `onClear` handler |
