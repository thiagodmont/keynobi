# Memory & Log Monitor — Design Spec

**Date:** 2026-04-06  
**Status:** Approved

---

## Overview

Add two persistent indicators to the bottom status bar:

1. **App memory** — RSS memory consumed by the Keynobi process itself
2. **Log folder size** — total size of `~/.keynobi/logs/`

Both are always visible, color-coded by severity. The log indicator also drives size-based log rotation: when the folder exceeds the configured maximum, the oldest files are deleted until back under the limit. A new `log_max_size_mb` setting is added to the **Advanced** section of Settings.

---

## User-Facing Behaviour

### Status Bar Indicators

Two new indicators appear on the right side of the status bar, to the left of the settings gear:

```
⬡ My Project  release · arm64  ● Health OK  ◎ MCP idle       🧠 124 MB  📁 38 MB  ⚙
```

Both update every **5 seconds**.

**Color thresholds:**

| Indicator | Green | Yellow | Red |
|-----------|-------|--------|-----|
| 🧠 App memory | < 300 MB | 300–500 MB | > 500 MB |
| 📁 Log folder | < 70% of max | 70–90% of max | > 90% of max, or rotation just triggered |

When rotation fires, the log indicator briefly shows a `↻` suffix and turns red for one poll cycle.

Memory thresholds are fixed. Log folder thresholds are percentage-based relative to `log_max_size_mb`.

### Log Rotation

Two constraints apply — whichever triggers first wins:

- **Date-based** (existing): files older than `log_retention_days` are deleted
- **Size-based** (new): when total folder size exceeds `log_max_size_mb`, the oldest files are deleted one-by-one until back under the limit

Rotation runs inside the Rust polling loop (backend-driven), not from the frontend.

### Settings

A new field under **Advanced**:

| Label | Key | Default | Description |
|-------|-----|---------|-------------|
| Max log folder size (MB) | `log_max_size_mb` | 500 | Total size limit for `~/.keynobi/logs/` before size-based rotation triggers |

Displayed as a number input next to the existing **Log retention (days)** row.

---

## Architecture

### Approach

Rust-polled, event-driven (Tauri `emit`). The backend owns all timing, rotation logic, and stats collection. The frontend is a passive consumer — it listens for events and updates reactive signals.

This is consistent with how `mcp.store`, `build.store`, and `health.store` already work.

### Data Flow

```
[Rust monitor task, every 5s]
  → sysinfo: read process RSS
  → fs::read_dir: walk ~/.keynobi/logs/, sum file sizes
  → if total > log_max_size_mb: delete oldest files until under limit
  → emit("monitor://stats", MonitorStats { app_memory_bytes, log_folder_bytes, rotation_triggered })

[Frontend monitor.store.ts]
  → listen("monitor://stats", ...)
  → update appMemoryBytes(), logFolderBytes(), rotationTriggered()

[StatusBar.tsx]
  → reads monitor.store signals
  → renders MemoryIndicator + LogSizeIndicator
```

### Files to Create

| File | Purpose |
|------|---------|
| `src-tauri/src/services/monitor.rs` | Background polling task: memory sampling, folder size walk, rotation, event emit |
| `src/stores/monitor.store.ts` | SolidJS store: listens to `monitor://stats`, exposes reactive signals |
| `src/bindings/MonitorStats.ts` | Auto-generated TS binding for `MonitorStats` Rust struct (via `ts-rs`) |

### Files to Modify

| File | Change |
|------|--------|
| `src-tauri/src/models/settings.rs` | Add `log_max_size_mb: u32` to `AdvancedSettings` (default: 500) |
| `src-tauri/src/services/mod.rs` | Register `monitor` module |
| `src-tauri/src/services/settings_manager.rs` | Add `log_max_size_mb` to `KNOWN_SETTINGS_FIELDS` |
| `src-tauri/src/lib.rs` | Spawn the monitor background task at startup |
| `src/components/layout/StatusBar.tsx` | Add `MemoryIndicator` and `LogSizeIndicator` internal components |
| `src/components/settings/SettingsPanel.tsx` | Add `SettingNumberInput` row for `log_max_size_mb` under Advanced |
| `src/bindings/index.ts` | Re-export `MonitorStats` |

---

## Backend Detail — `monitor.rs`

```rust
pub struct MonitorStats {
    pub app_memory_bytes: u64,
    pub log_folder_bytes: u64,
    pub rotation_triggered: bool,
}
```

- Use `sysinfo::System::new_with_specifics` with `RefreshKind::new().with_processes(...)` to read the current process RSS. `sysinfo` is already a transitive Tauri dependency.
- Walk `~/.keynobi/logs/` with `std::fs::read_dir`, accumulate file sizes, collect `(path, modified)` pairs for rotation.
- Rotation: sort by `modified` ascending, delete from the front until `total <= limit`.
- `rotation_triggered` is `true` only in the single poll cycle where deletion occurred.
- Emit via `app_handle.emit("monitor://stats", stats)`.
- Task is spawned with `tauri::async_runtime::spawn` in `lib.rs` after the app is built.

---

## Frontend Detail — `monitor.store.ts`

```typescript
export interface MonitorStoreState {
  appMemoryBytes: number;
  logFolderBytes: number;
  rotationTriggered: boolean;
}

export function createMonitorStore(): MonitorStore { ... }
export const monitorStore = createMonitorStore();
```

- Calls `listen<MonitorStats>("monitor://stats", ...)` at store creation time (module load), storing the returned unlisten function.
- The unlisten is called via a cleanup registered with `onCleanup` inside the store factory.
- Exposes plain reactive getters: `appMemoryBytes()`, `logFolderBytes()`, `rotationTriggered()`.

---

## Frontend Detail — `StatusBar.tsx`

Two new internal components following the existing pattern:

**`MemoryIndicator`**
- Reads `monitorStore.appMemoryBytes()`
- Formats as `X MB` or `X.X GB`
- Color: green / yellow / red per thresholds above

**`LogSizeIndicator`**
- Reads `monitorStore.logFolderBytes()` and `monitorStore.rotationTriggered()`
- Formats as `X MB` or `X.X GB`
- Shows `↻` suffix for one cycle when `rotationTriggered` is true
- Color: green / yellow / red — percentage of `settingsState.advanced.logMaxSizeMb`

Both use `e.stopPropagation()` on `onMouseDown` (consistent with other status bar items).

---

## Settings Detail

**`AdvancedSettings` (Rust)**
```rust
/// Max total size of ~/.keynobi/logs/ in MB before size-based rotation. Default: 500.
pub log_max_size_mb: u32,
```

Default: `500`. Added to `KNOWN_SETTINGS_FIELDS` list in `settings_manager.rs`.

**`SettingsPanel.tsx`**
```tsx
<SettingNumberInput
  label="Max log folder size (MB)"
  description="Size limit for ~/.keynobi/logs/ before oldest files are deleted"
  value={settingsState.advanced.logMaxSizeMb}
  onChange={(v) => updateSetting("advanced", "logMaxSizeMb", v)}
  min={50}
/>
```

Placed adjacent to the existing `log_retention_days` row in the **Advanced** category.

---

## Out of Scope

- System-wide RAM monitoring
- Compressing log files (only deletion)
- Configurable memory warning thresholds
- Clicking the indicators to open a detail panel
