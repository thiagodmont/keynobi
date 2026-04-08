# Memory & Log Monitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add app memory and log folder size indicators to the status bar, with size-based log rotation and a new `logMaxSizeMb` setting.

**Architecture:** A Rust background task polls every 5s, reads process RSS via `sysinfo`, sums `~/.keynobi/logs/` file sizes, runs size-based rotation if needed, then emits a `monitor://stats` event. A new SolidJS store listens to the event and exposes reactive signals that two new `StatusBar` components consume.

**Tech Stack:** Rust (`sysinfo`, `tokio`, `tauri::Emitter`), SolidJS (`createSignal`, `listen`), TypeScript

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src-tauri/Cargo.toml` | Add `sysinfo = "0.33"` dependency |
| Modify | `src-tauri/src/models/settings.rs` | Add `log_max_size_mb: u32` to `AdvancedSettings` |
| Create | `src-tauri/src/services/monitor.rs` | Polling task, file scan, rotation logic |
| Modify | `src-tauri/src/services/mod.rs` | Register `pub mod monitor` |
| Modify | `src-tauri/src/lib.rs` | Spawn monitor task in `setup` |
| Create | `src/stores/monitor.store.ts` | SolidJS event bridge, exposes reactive signals |
| Modify | `src/components/layout/StatusBar.tsx` | Add `MemoryIndicator` + `LogSizeIndicator` |
| Modify | `src/components/settings/SettingsPanel.tsx` | Add `logMaxSizeMb` row under Logging section |

---

## Task 1: Add `log_max_size_mb` to `AdvancedSettings`

**Files:**
- Modify: `src-tauri/src/models/settings.rs`

- [ ] **Step 1: Update the `advanced_defaults` test to expect the new field**

In `src-tauri/src/models/settings.rs`, find the `advanced_defaults` test (around line 339) and add one assertion:

```rust
#[test]
fn advanced_defaults() {
    let d = AdvancedSettings::default();
    assert_eq!(d.tree_sitter_cache_size, 50);
    assert_eq!(d.lsp_max_message_size_mb, 64);
    assert_eq!(d.hover_delay_ms, 500);
    assert_eq!(d.recent_files_limit, 20);
    assert_eq!(d.log_retention_days, 7);
    assert_eq!(d.log_max_size_mb, 500);  // ← add this
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cd src-tauri && cargo test advanced_defaults 2>&1 | tail -20
```

Expected: `FAILED` — field `log_max_size_mb` not found on `AdvancedSettings`

- [ ] **Step 3: Add the field to `AdvancedSettings` and its `Default` impl**

In `src-tauri/src/models/settings.rs`, update the struct (around line 117):

```rust
pub struct AdvancedSettings {
    pub tree_sitter_cache_size: u32,
    pub lsp_max_message_size_mb: u32,
    pub watcher_debounce_ms: u32,
    pub lsp_did_change_debounce_ms: u32,
    pub diagnostics_pull_delay_ms: u32,
    pub hover_delay_ms: u32,
    pub navigation_history_depth: u32,
    pub recent_files_limit: u32,
    /// Number of days to retain log files in ~/.keynobi/logs/ (default: 7).
    pub log_retention_days: u32,
    /// Max total size of ~/.keynobi/logs/ in MB before size-based rotation triggers (default: 500).
    pub log_max_size_mb: u32,
}
```

Update the `Default` impl (around line 239):

```rust
impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            tree_sitter_cache_size: 50,
            lsp_max_message_size_mb: 64,
            watcher_debounce_ms: 200,
            lsp_did_change_debounce_ms: 300,
            diagnostics_pull_delay_ms: 1000,
            hover_delay_ms: 500,
            navigation_history_depth: 50,
            recent_files_limit: 20,
            log_retention_days: 7,
            log_max_size_mb: 500,
        }
    }
}
```

- [ ] **Step 4: Run the test to confirm it passes**

```bash
cd src-tauri && cargo test advanced_defaults 2>&1 | tail -10
```

Expected: `test models::settings::tests::advanced_defaults ... ok`

- [ ] **Step 5: Add `sysinfo` to `Cargo.toml`**

In `src-tauri/Cargo.toml`, add after the `walkdir` line:

```toml
# Process memory monitoring (RSS) for the status bar monitor
sysinfo = "0.33"
```

- [ ] **Step 6: Regenerate TypeScript bindings**

```bash
cd src-tauri && cargo test 2>&1 | grep -E "FAILED|error|ok$" | tail -30
```

Expected: all tests pass. This also regenerates `src/bindings/AdvancedSettings.ts` to include `logMaxSizeMb: number`.

- [ ] **Step 7: Verify the binding was updated**

```bash
grep "logMaxSizeMb" src/bindings/AdvancedSettings.ts
```

Expected: `logMaxSizeMb: number,`

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/models/settings.rs src/bindings/AdvancedSettings.ts
git commit -m "feat(monitor): add log_max_size_mb to AdvancedSettings"
```

---

## Task 2: Create `monitor.rs` — file scanning and rotation logic (TDD)

**Files:**
- Create: `src-tauri/src/services/monitor.rs`

- [ ] **Step 1: Create the file with the `collect_log_files` test**

Create `src-tauri/src/services/monitor.rs` with this content:

```rust
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStats {
    pub app_memory_bytes: u64,
    pub log_folder_bytes: u64,
    pub rotation_triggered: bool,
}

/// Returns (total_bytes, list of (path, modified, size)) for app.log* files in log_dir.
pub fn collect_log_files(log_dir: &Path) -> (u64, Vec<(PathBuf, SystemTime, u64)>) {
    todo!()
}

/// Deletes oldest app.log* files until total_bytes <= limit. Returns true if any files deleted.
pub fn rotate_logs(files: Vec<(PathBuf, SystemTime, u64)>, limit: u64) -> bool {
    todo!()
}

pub async fn run_monitor(_app_handle: AppHandle, _log_dir: PathBuf, _log_max_size_bytes: u64) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_file(dir: &Path, name: &str, size: usize) {
        fs::write(dir.join(name), vec![0u8; size]).unwrap();
    }

    #[test]
    fn collect_returns_zero_for_empty_dir() {
        let dir = tempdir().unwrap();
        let (total, files) = collect_log_files(dir.path());
        assert_eq!(total, 0);
        assert!(files.is_empty());
    }

    #[test]
    fn collect_sums_only_app_log_files() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "app.log", 1000);
        write_file(dir.path(), "app.log.2026-04-01", 2000);
        write_file(dir.path(), "other.log", 500); // should be ignored
        let (total, files) = collect_log_files(dir.path());
        assert_eq!(total, 3000);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_returns_zero_for_missing_dir() {
        let (total, files) = collect_log_files(Path::new("/tmp/keynobi-nonexistent-dir-xyz"));
        assert_eq!(total, 0);
        assert!(files.is_empty());
    }
}
```

- [ ] **Step 2: Register the module so `cargo test` can find it**

In `src-tauri/src/services/mod.rs`, add at the end:

```rust
pub mod monitor;
```

- [ ] **Step 3: Run the collect tests to confirm they fail**

```bash
cd src-tauri && cargo test monitor::tests::collect 2>&1 | tail -20
```

Expected: `FAILED` with "not yet implemented" (todo! panics)

- [ ] **Step 4: Implement `collect_log_files`**

In `src-tauri/src/services/monitor.rs`, replace the `todo!()` in `collect_log_files`:

```rust
pub fn collect_log_files(log_dir: &Path) -> (u64, Vec<(PathBuf, SystemTime, u64)>) {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return (0, vec![]);
    };
    let mut total = 0u64;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("app.log") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            let size = meta.len();
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            total += size;
            files.push((path, modified, size));
        }
    }
    (total, files)
}
```

- [ ] **Step 5: Run the collect tests to confirm they pass**

```bash
cd src-tauri && cargo test monitor::tests::collect 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 6: Add the `rotate_logs` tests**

Append to the `tests` module in `monitor.rs`:

```rust
    #[test]
    fn rotate_does_nothing_when_under_limit() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "app.log.2026-04-01", 100);
        let (_, files) = collect_log_files(dir.path());
        let rotated = rotate_logs(files, 1000);
        assert!(!rotated);
        assert!(dir.path().join("app.log.2026-04-01").exists());
    }

    #[test]
    fn rotate_deletes_oldest_file_first() {
        let dir = tempdir().unwrap();
        // Write two files; ensure older one has an earlier modified time
        write_file(dir.path(), "app.log.2026-04-01", 300);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-02", 300);

        let (_, files) = collect_log_files(dir.path());
        // Limit of 400 → must delete oldest (300 bytes) to get to 300 ≤ 400
        let rotated = rotate_logs(files, 400);
        assert!(rotated);
        // Newer file must survive
        assert!(dir.path().join("app.log.2026-04-02").exists());
        // Older file must be gone
        assert!(!dir.path().join("app.log.2026-04-01").exists());
    }

    #[test]
    fn rotate_deletes_multiple_files_until_under_limit() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "app.log.2026-04-01", 200);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-02", 200);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_file(dir.path(), "app.log.2026-04-03", 200);

        let (_, files) = collect_log_files(dir.path());
        // Limit of 250 → must delete first two files (400 bytes) to reach 200 ≤ 250
        let rotated = rotate_logs(files, 250);
        assert!(rotated);
        assert!(dir.path().join("app.log.2026-04-03").exists());
        assert!(!dir.path().join("app.log.2026-04-01").exists());
        assert!(!dir.path().join("app.log.2026-04-02").exists());
    }
```

- [ ] **Step 7: Run rotate tests to confirm they fail**

```bash
cd src-tauri && cargo test monitor::tests::rotate 2>&1 | tail -20
```

Expected: `FAILED` (todo! panics)

- [ ] **Step 8: Implement `rotate_logs`**

Replace the `todo!()` in `rotate_logs`:

```rust
pub fn rotate_logs(mut files: Vec<(PathBuf, SystemTime, u64)>, limit: u64) -> bool {
    // Sort oldest-first
    files.sort_by_key(|(_, modified, _)| *modified);
    let mut total: u64 = files.iter().map(|(_, _, size)| size).sum();
    let mut rotated = false;
    for (path, _, size) in &files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            tracing::info!("Size-based log rotation: removed {}", path.display());
            total = total.saturating_sub(*size);
            rotated = true;
        }
    }
    rotated
}
```

- [ ] **Step 9: Run all monitor tests**

```bash
cd src-tauri && cargo test monitor::tests 2>&1 | tail -15
```

Expected: 6 tests pass (`collect_*` × 3, `rotate_*` × 3).

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/services/monitor.rs src-tauri/src/services/mod.rs
git commit -m "feat(monitor): implement collect_log_files and rotate_logs with tests"
```

---

## Task 3: Complete `monitor.rs` — `run_monitor` task, wire into `lib.rs`

**Files:**
- Modify: `src-tauri/src/services/monitor.rs` (implement `run_monitor`)
- Modify: `src-tauri/src/lib.rs` (spawn task in setup)

- [ ] **Step 1: Implement `run_monitor` in `monitor.rs`**

Replace the `todo!()` in `run_monitor`:

```rust
pub async fn run_monitor(app_handle: AppHandle, log_dir: PathBuf, log_max_size_bytes: u64) {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let pid = Pid::from(std::process::id() as usize);
    let mut sys = System::new();

    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        // 1. Read app process RSS memory
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
        let app_memory_bytes = sys.process(pid).map(|p| p.memory()).unwrap_or(0);

        // 2. Scan log folder
        let (log_folder_bytes, files) = collect_log_files(&log_dir);

        // 3. Rotate if needed
        let rotation_triggered = if log_folder_bytes > log_max_size_bytes {
            rotate_logs(files, log_max_size_bytes)
        } else {
            false
        };

        // 4. Emit stats to frontend
        let stats = MonitorStats {
            app_memory_bytes,
            log_folder_bytes,
            rotation_triggered,
        };
        let _ = app_handle.emit("monitor://stats", stats);
    }
}
```

Also add the missing imports at the top of `monitor.rs` (replace the existing `use` lines):

```rust
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
```

- [ ] **Step 2: Spawn the monitor task in `lib.rs` setup block**

In `src-tauri/src/lib.rs`, inside the `.setup(move |app| { ... })` closure, after the `cleanup_old_logs` call (around line 206), add:

```rust
            // Spawn monitor: polls memory + log folder size every 5s.
            {
                let handle = app.handle().clone();
                let log_dir_monitor = log_dir.clone();
                let log_max_bytes = u64::from(settings.advanced.log_max_size_mb) * 1024 * 1024;
                tauri::async_runtime::spawn(async move {
                    services::monitor::run_monitor(handle, log_dir_monitor, log_max_bytes).await;
                });
            }
```

- [ ] **Step 3: Verify it compiles**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

- [ ] **Step 4: Run full test suite**

```bash
cd src-tauri && cargo test 2>&1 | grep -E "FAILED|error\[" | head -20
```

Expected: no failures.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/monitor.rs src-tauri/src/lib.rs
git commit -m "feat(monitor): wire run_monitor background task into app startup"
```

---

## Task 4: Create `monitor.store.ts`

**Files:**
- Create: `src/stores/monitor.store.ts`

- [ ] **Step 1: Create the store**

```typescript
import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";

interface MonitorStats {
  appMemoryBytes: number;
  logFolderBytes: number;
  rotationTriggered: boolean;
}

const [appMemoryBytes, setAppMemoryBytes] = createSignal(0);
const [logFolderBytes, setLogFolderBytes] = createSignal(0);
const [rotationTriggered, setRotationTriggered] = createSignal(false);

export { appMemoryBytes, logFolderBytes, rotationTriggered };

if (typeof window !== "undefined") {
  listen<MonitorStats>("monitor://stats", (event) => {
    setAppMemoryBytes(event.payload.appMemoryBytes);
    setLogFolderBytes(event.payload.logFolderBytes);
    setRotationTriggered(event.payload.rotationTriggered);
  }).catch(() => {});
}
```

- [ ] **Step 2: Verify it type-checks**

```bash
npm run build 2>&1 | grep -E "error TS|ERROR" | head -20
```

Expected: no TypeScript errors.

- [ ] **Step 3: Commit**

```bash
git add src/stores/monitor.store.ts
git commit -m "feat(monitor): add monitor.store.ts — listens to monitor://stats events"
```

---

## Task 5: Add `MemoryIndicator` and `LogSizeIndicator` to `StatusBar.tsx`

**Files:**
- Modify: `src/components/layout/StatusBar.tsx`

- [ ] **Step 1: Add the import for monitor store at the top of `StatusBar.tsx`**

After the last import line (after `import Icon from "@/components/common/Icon";`), add:

```typescript
import { appMemoryBytes, logFolderBytes, rotationTriggered } from "@/stores/monitor.store";
import { settingsState } from "@/stores/settings.store";
```

- [ ] **Step 2: Add the `formatBytes` helper after the existing `formatElapsed` function (around line 99)**

```typescript
function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 MB";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}
```

- [ ] **Step 3: Add the `MemoryIndicator` component after `formatBytes`**

```typescript
// ── Memory indicator ─────────────────────────────────────────────────────────

function MemoryIndicator(): JSX.Element {
  const bytes = () => appMemoryBytes();

  const color = () => {
    const mb = bytes() / (1024 * 1024);
    if (mb >= 500) return "#f87171";
    if (mb >= 300) return "#fbbf24";
    return "rgba(255,255,255,0.6)";
  };

  return (
    <span
      title={`App memory: ${formatBytes(bytes())}`}
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        color: color(),
        "font-size": "11px",
        "white-space": "nowrap",
        "flex-shrink": "0",
      }}
    >
      {formatBytes(bytes())}
    </span>
  );
}
```

- [ ] **Step 4: Add the `LogSizeIndicator` component after `MemoryIndicator`**

```typescript
// ── Log size indicator ────────────────────────────────────────────────────────

function LogSizeIndicator(): JSX.Element {
  const bytes = () => logFolderBytes();
  const rotating = () => rotationTriggered();

  const color = () => {
    const maxBytes = (settingsState.advanced.logMaxSizeMb ?? 500) * 1024 * 1024;
    const pct = maxBytes > 0 ? bytes() / maxBytes : 0;
    if (rotating() || pct >= 0.9) return "#f87171";
    if (pct >= 0.7) return "#fbbf24";
    return "rgba(255,255,255,0.6)";
  };

  const label = () => `${formatBytes(bytes())}${rotating() ? " ↻" : ""}`;

  const tooltip = () => {
    const maxMb = settingsState.advanced.logMaxSizeMb ?? 500;
    const usedMb = Math.round(bytes() / (1024 * 1024));
    return `Log folder: ${usedMb} MB / ${maxMb} MB${rotating() ? " (rotation triggered)" : ""}`;
  };

  return (
    <span
      title={tooltip()}
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        color: color(),
        "font-size": "11px",
        "white-space": "nowrap",
        "flex-shrink": "0",
      }}
    >
      {label()}
    </span>
  );
}
```

- [ ] **Step 5: Replace the right-side div in `StatusBar` to include both indicators**

Find the right-side `<div>` (around line 335) that currently contains `<span>Android Dev Companion</span>`:

```tsx
      {/* Right side */}
      <div
        style={{
          display: "flex",
          gap: "10px",
          "align-items": "center",
          "pointer-events": "none",
          "flex-shrink": "0",
          opacity: "0.8",
        }}
      >
        <span>Keynobi</span>
      </div>
```

Replace it with:

```tsx
      {/* Right side */}
      <div
        style={{
          display: "flex",
          gap: "10px",
          "align-items": "center",
          "flex-shrink": "0",
          opacity: "0.85",
        }}
      >
        <MemoryIndicator />
        <LogSizeIndicator />
      </div>
```

- [ ] **Step 6: Verify it builds**

```bash
npm run build 2>&1 | grep -E "error TS|ERROR" | head -20
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/layout/StatusBar.tsx src/stores/monitor.store.ts
git commit -m "feat(monitor): add MemoryIndicator and LogSizeIndicator to StatusBar"
```

---

## Task 6: Add `logMaxSizeMb` setting to `SettingsPanel`

**Files:**
- Modify: `src/components/settings/SettingsPanel.tsx`

- [ ] **Step 1: Add the new setting row in the Logging section**

Find the Logging section in `AdvancedSettings` function (around line 560). After the closing `</Show>` of the existing log retention row, add:

```tsx
      <Show when={props.matchesSearch("Max log folder size", "Size limit for log files before rotation")}>
        <SettingRow
          label="Max log folder size (MB)"
          description="Size limit for ~/.keynobi/logs/ before oldest files are deleted"
        >
          <SettingNumberInput
            value={settingsState.advanced.logMaxSizeMb}
            min={50}
            max={10000}
            step={50}
            onChange={(v) => updateSetting("advanced", "logMaxSizeMb", v)}
          />
        </SettingRow>
      </Show>
```

- [ ] **Step 2: Verify it builds**

```bash
npm run build 2>&1 | grep -E "error TS|ERROR" | head -20
```

Expected: no errors.

- [ ] **Step 3: Run the full Rust test suite one final time**

```bash
cd src-tauri && cargo test 2>&1 | grep -E "FAILED|error\[" | head -20
```

Expected: no failures.

- [ ] **Step 4: Commit**

```bash
git add src/components/settings/SettingsPanel.tsx
git commit -m "feat(monitor): add logMaxSizeMb setting under Advanced > Logging"
```

---

## Done

All tasks complete. The feature delivers:
- 🧠 App memory (RSS) always visible in status bar, green/yellow/red by threshold
- 📁 Log folder size always visible, percentage-based color relative to `logMaxSizeMb`
- Size-based rotation: oldest `app.log*` files deleted when limit exceeded (runs every 5s)
- New Advanced setting: `Max log folder size (MB)` — default 500 MB
- Existing date-based rotation unchanged
