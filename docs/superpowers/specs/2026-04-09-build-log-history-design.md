# Build Log History Design

**Date:** 2026-04-09
**Status:** Approved

## Goal

Persist the full structured log for each completed build so the user can click any history entry and see exactly what Gradle printed — errors, warnings, task progress, and all output — just like a live build.

## Approach

Option A — re-parse the existing raw log buffer at finalization. The in-memory `build_log: VecDeque<String>` already holds all raw lines from the current build. At `record_build_result` time, re-run `parse_build_line` on each string to recover structured `BuildLine` objects, then write them as JSON Lines to disk. No new buffers, no changes to the `on_line` callback.

---

## Section 1: Storage & Format

**Location:** `~/.keynobi/build-logs/build-{id}.jsonl`

One file per completed build. Format is JSON Lines — each line is a serialized `BuildLine` (the existing Rust struct, already `Serialize`/`Deserialize`). The directory is created on first write.

**Write trigger:** Inside `record_build_result` in `build_runner.rs`. A new helper:

```rust
pub fn save_build_log(id: u32, raw_lines: &VecDeque<String>)
```

iterates `raw_lines`, calls `parse_build_line` on each, serializes the result with `serde_json::to_string`, and writes to `~/.keynobi/build-logs/build-{id}.jsonl` using an atomic temp+rename. Max 5,000 lines (matching `MAX_BUILD_LOG`). Write failures are silently ignored (best-effort, same pattern as `save_build_history`).

**Read:** New Tauri command `get_build_log_entries(id: u32) -> Result<Vec<BuildLine>, String>` reads the file, parses each line, and returns up to 10,000 entries. Returns an empty vec if the file does not exist (build predates the feature or was rotated away).

---

## Section 2: Rotation

A single function:

```rust
pub fn rotate_build_logs(
    build_log_dir: &Path,
    retention_days: u32,
    max_folder_mb: u32,
    history: &VecDeque<BuildRecord>,
)
```

Called in two places:
1. App startup — in `lib.rs` after `BuildState` is created and settings are loaded, before the window is shown
2. After each build finalizes — at the end of `record_build_result` (receives settings via parameter)

**Three passes, in order:**

1. **Age** — delete any `.jsonl` file whose mtime is older than `retention_days`
2. **Orphans** — delete any `build-{id}.jsonl` whose ID is not in the current history ring-buffer (the build was evicted by the 10-entry cap)
3. **Size cap** — if total folder size exceeds `max_folder_mb`, delete oldest files by mtime until under the cap

All file operations are best-effort; individual failures are silently ignored.

---

## Section 3: Settings

Two new fields on `BuildSettings` in `src-tauri/src/models/settings.rs`:

```rust
pub struct BuildSettings {
    pub auto_install_on_build: bool,
    /// Days to keep build log files in ~/.keynobi/build-logs/ (default: 7).
    pub build_log_retention_days: u32,
    /// Max total size of ~/.keynobi/build-logs/ in MB before size-based rotation (default: 100).
    pub build_log_max_folder_mb: u32,
}
```

Defaults:
```rust
impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            auto_install_on_build: true,
            build_log_retention_days: 7,
            build_log_max_folder_mb: 100,
        }
    }
}
```

Both fields use `#[serde(default)]` (inherited from the struct attribute) — existing settings files without these fields silently get the defaults. No migration needed.

**Settings UI** (`src/components/settings/SettingsPanel.tsx`): two new `SettingNumberInput` rows added to the existing Build section:
- "Build log retention" — min 1, max 365, unit label "days"
- "Build log folder limit" — min 10, max 2048, unit label "MB"

---

## Section 4: Frontend — Log Switching

### New Tauri binding

`src/lib/tauri-api.ts`:
```typescript
export async function getBuildLogEntries(id: number): Promise<BuildLine[]> {
  return invoke<BuildLine[]>("get_build_log_entries", { id });
}
```

### Store helper export

`src/stores/build.store.ts`: rename `_lineToLogEntry` → `lineToLogEntry` and export it. `BuildPanel` uses it to convert `BuildLine[]` → `LogEntry[]` for the `LogViewer`.

### `BuildPanel.tsx` changes

1. **Historical log signal:** `const [historicalLog, setHistoricalLog] = createSignal<LogEntry[]>([]);`

2. **Effect on selection change** (SolidJS `createEffect` must be synchronous — async work is fire-and-forget):
```typescript
createEffect(() => {
  const id = selectedHistoryId();
  if (id === null) {
    setHistoricalLog([]);
    return;
  }
  getBuildLogEntries(id)
    .then((lines) => setHistoricalLog(lines.map(lineToLogEntry)))
    .catch(() => setHistoricalLog([]));
});
```

3. **LogViewer data source:**
```typescript
const logEntries = () =>
  selectedHistoryId() !== null ? historicalLog() : buildLogStore.entries;
```
Pass `logEntries()` to `<LogViewer entries={logEntries()} ... />`.

4. **Historical banner:** A small `<Show when={selectedHistoryId() !== null}>` block above the LogViewer displaying `"Viewing build from {relativeTime(record.startedAt)}"`. The `record` is looked up from `buildState.history`. Hidden during live view.

---

## Files Affected

| File | Change |
|------|--------|
| `src-tauri/src/models/settings.rs` | Add `build_log_retention_days`, `build_log_max_folder_mb` to `BuildSettings` |
| `src-tauri/src/services/build_runner.rs` | Add `save_build_log`, `rotate_build_logs`; call `save_build_log` + `rotate_build_logs` from `record_build_result` |
| `src-tauri/src/lib.rs` (startup) | Call `rotate_build_logs` once at app startup after settings are loaded |
| `src-tauri/src/commands/build.rs` | Add `get_build_log_entries` command |
| `src-tauri/src/lib.rs` | Import + register `get_build_log_entries` |
| `src/lib/tauri-api.ts` | Add `getBuildLogEntries` binding |
| `src/stores/build.store.ts` | Export `lineToLogEntry` (rename from `_lineToLogEntry`) |
| `src/components/build/BuildPanel.tsx` | Log switching signal + effect + LogViewer data source + historical banner |
| `src/components/settings/SettingsPanel.tsx` | Two new `SettingNumberInput` rows in Build section |

## Non-Goals

- No UI for viewing logs from builds before the feature was installed (file won't exist — returns empty, LogViewer shows empty state)
- No search within historical logs (LogViewer already has its own filtering if needed)
- No export/copy of historical logs
- No per-record delete (clear-history already removes all)
