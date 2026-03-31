# Code Patterns

This document captures the conventions we follow in this repo. These are foundational rules to keep the codebase modular, maintainable, and consistent. See `BEST_PRACTICES.md` for the *why*; this document explains the *how*.

---

## Table of Contents

1. [Project Structure](#project-structure)
2. [Rust Patterns](#rust-patterns)
3. [Frontend Patterns (SolidJS)](#frontend-patterns-solidjs)
4. [IPC / Tauri Bridge Patterns](#ipc--tauri-bridge-patterns)
5. [Testing Patterns](#testing-patterns)
6. [File Naming Conventions](#file-naming-conventions)

---

## Project Structure

```
android-ide/
├── src/                        # Frontend (SolidJS + TypeScript)
│   ├── bindings/               # Auto-generated Rust-to-TS types (DO NOT EDIT)
│   ├── components/             # UI components (organized by domain)
│   │   ├── common/             # Shared UI primitives (Icon, Toast, Dialog, ...)
│   │   ├── editor/             # CodeMirror wrapper and tab bar
│   │   ├── filetree/           # File explorer
│   │   ├── layout/             # Shell: Sidebar, StatusBar, PanelContainer, ...
│   │   ├── search/             # Search panel and results
│   │   └── symbols/            # Document outline panel
│   ├── lib/                    # Pure logic, no UI
│   │   ├── codemirror/         # CodeMirror 6 extensions and configuration
│   │   ├── action-registry.ts  # Central IDE action registry
│   │   ├── fuzzy-match.ts      # Fuzzy search utility
│   │   ├── keybindings.ts      # Global keybinding system
│   │   ├── navigation-history.ts
│   │   ├── file-utils.ts
│   │   └── tauri-api.ts        # Typed Tauri IPC wrappers
│   ├── services/               # Async flows combining stores + IPC
│   │   └── project.service.ts
│   ├── stores/                 # SolidJS reactive stores
│   └── styles/                 # Global CSS and theme variables
│
├── src-tauri/                  # Rust backend
│   ├── src/
│   │   ├── commands/           # Tauri IPC command handlers (thin layer)
│   │   ├── models/             # Shared data types (Serialize + ts-rs exports)
│   │   └── services/           # Business logic (no Tauri dependency)
│   └── benches/                # Criterion benchmarks
│
└── docs/                       # Architecture documentation
```

---

## Rust Patterns

### 1. Per-Concern State Structs

Each service domain owns its own `Mutex`-wrapped state. Never put unrelated fields in the same state struct.

```rust
// CORRECT — separate Mutexes, no cross-domain blocking
pub struct FsState(pub Mutex<FsStateInner>);
pub struct LspState(pub Mutex<LspStateInner>);
pub struct TreeSitterState(pub Mutex<TreeSitterService>);

// WRONG — everything in one state creates contention
pub struct AppState(pub Mutex<AppStateInner>); // DO NOT DO THIS
```

Always implement `Default` by delegating to `new()`:

```rust
impl Default for FsState {
    fn default() -> Self { Self::new() }
}
```

### 2. Mutex Lock Discipline

Hold the Mutex for the minimum time. Clone what you need and drop the lock before any I/O or `await`.

```rust
// CORRECT
let root = {
    let fs = fs_state.0.lock().await;
    fs.project_root.clone().ok_or("No project open")?
}; // lock dropped here
let tree = build_file_tree(&root); // I/O outside the lock

// WRONG — holding the lock across I/O
let fs = fs_state.0.lock().await;
let tree = build_file_tree(fs.project_root.as_ref()?); // lock held during I/O
```

### 3. Commands Are Thin Wrappers

Tauri command handlers in `commands/` are thin. They validate inputs, delegate to `services/`, and convert errors to strings. No business logic lives in commands.

```rust
// CORRECT — command validates and delegates
#[tauri::command]
pub async fn read_file(path: String, fs_state: State<'_, FsState>) -> Result<String, String> {
    validate_path(&path, &fs_state).await?;           // validate
    fs_manager::read_file(Path::new(&path))           // delegate to service
        .map_err(|e| e.to_string())                   // convert error
}

// WRONG — business logic in the command handler
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let content = tokio::fs::read_to_string(&path)...; // logic in command
    // ...
}
```

### 4. Path Security — Always Canonicalize

Any command that accepts a path from the frontend must validate it against the **effective project root** using canonicalization before passing it to a service.

The effective root is `gradle_root` (detected Gradle project root) when available, otherwise `project_root` (ADR-15). This allows go-to-definition navigation to reach files in sibling Gradle modules.

```rust
async fn ensure_path_in_project(
    path: &str,
    fs_state: &tauri::State<'_, FsState>,
) -> Result<(), String> {
    let fs = fs_state.0.lock().await;
    let root = fs.gradle_root.as_ref()
        .or(fs.project_root.as_ref())
        .ok_or("No project open")?;
    let canonical_root = root
        .canonicalize().map_err(|e| format!("Failed to resolve root: {e}"))?;
    drop(fs);

    let canonical_target = Path::new(path)
        .canonicalize().map_err(|e| format!("Failed to resolve path: {e}"))?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err("Path is outside the project directory".into());
    }
    Ok(())
}
```

Never use `path.starts_with(root)` without canonicalization first.

### 5. Error Handling

- Services return `Result<T, FsError>` (or `Result<T, String>` for simpler domains)
- Commands convert errors to `String` at the IPC boundary: `.map_err(|e| e.to_string())`
- Never use `unwrap()` in production code; use `expect()` only when failure truly means programmer error (e.g., grammar loading at startup)
- Use `#[derive(thiserror::Error)]` for structured error types in services

### 6. Model Types — Derive ts-rs

All types that cross the IPC boundary must derive `Serialize`, `Deserialize`, `Clone`, and `TS` for TypeScript binding generation.

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub children: Option<Vec<FileNode>>,
    pub extension: Option<String>,
}
```

Always use `#[serde(rename_all = "camelCase")]` for JSON serialization to match TypeScript conventions.

After changing any model type, regenerate bindings:
```bash
npm run generate:bindings
```

### 7. Structured Logging

Use `tracing` macros, never `println!`. Match the log level to the audience:

```rust
tracing::error!("LSP process crashed: {}", err);   // must be investigated
tracing::warn!("File watcher missed event");         // degraded behavior
tracing::info!("LSP initialized: {:?}", server);    // lifecycle milestones
tracing::debug!("LSP request [{}] {}", id, method); // developer debugging
```

Configure at runtime via `RUST_LOG=android_ide_lib=debug`.

---

## Frontend Patterns (SolidJS)

### 1. Store Structure

Every logical domain has one store file: `{domain}.store.ts`. The store exports the reactive state object and named action functions. Components never mutate the store directly.

```typescript
// stores/editor.store.ts

const [editorState, setEditorState] = createStore<EditorStoreState>({ ... });

export { editorState }; // read-only reactive state
export function setActiveFile(path: string | null) { ... } // named action
export function markDirty(path: string) { ... }            // named action
```

Use `produce` from `solid-js/store` for mutations that involve array splicing or nested object deletion — regular `setEditorState("key", value)` for scalar updates.

### 2. Component Architecture

- **`components/common/`** — No domain logic. Pure UI primitives that can be used anywhere.
- **`components/layout/`** — Shell structure. No business logic; connects stores to UI.
- **`components/{domain}/`** — Domain-specific panels. Read from stores, delegate mutations to service functions.

Components do not call Tauri `invoke` directly. IPC calls belong in `lib/tauri-api.ts` or `services/`.

### 3. Registering Keybindings and Actions Together

Every keyboard shortcut that should appear in the command palette must be registered via `registerKeyAndAction()` in `App.tsx`, not with bare `registerKeybinding()`.

```typescript
// CORRECT — registers in both the keybinding system and the action registry
registerKeyAndAction({
  id: "file.save",
  key: "s",
  metaKey: true,
  label: "Save Active File",
  category: "File",
  action: async () => { /* ... */ },
});

// WRONG — only registers the keyboard shortcut, never appears in Cmd+Shift+P
registerKeybinding({ key: "s", metaKey: true, action: () => { /* ... */ } });
```

### 4. Reactive Computations — Use `createMemo`

Values derived from reactive state that are used in multiple places (or in render paths) must be wrapped in `createMemo` to avoid redundant recomputation.

```typescript
// CORRECT
const buildLabel = createMemo(() => getBuildStatusLabel());
const summary = createMemo(() => healthSummary());

// WRONG — called multiple times per render, recomputes on every access
const label = getBuildStatusLabel(); // inside JSX expression
```

### 5. SolidJS Reactivity Rules

- **Never destructure stores**: `const { status } = buildState` breaks reactivity. Always access as `buildState.status`.
- **Signals in JSX must be called**: `{buildLabel()}` not `{buildLabel}`.
- **`onCleanup` for timers**: Any `setTimeout` set in a component must be cleared in `onCleanup`.

```typescript
// CORRECT
let timer: ReturnType<typeof setTimeout> | undefined;
onCleanup(() => { if (timer) clearTimeout(timer); });
timer = setTimeout(() => { /* ... */ }, 300);
```

### 6. TypeScript Types — Import from `@/bindings`

Types that originate from Rust models must be imported from `@/bindings`, not redefined locally.

```typescript
// CORRECT
import type { BuildError, Device, SystemHealthReport } from "@/bindings";

// WRONG — creates a parallel definition that can drift from the Rust model
interface BuildError { message: string; file: string; /* ... */ }
```

For store types that need additional computed fields beyond the binding shape, extend the binding type:

```typescript
import type { Diagnostic } from "@/bindings";
interface DiagnosticWithCount extends Diagnostic { count: number; }
```

---

## IPC / Tauri Bridge Patterns

### 1. Typed IPC Wrappers

All `invoke` calls live in `src/lib/tauri-api.ts`. Components and services import from there, never call `invoke` directly.

```typescript
// lib/tauri-api.ts
export async function readFile(path: string): Promise<string> {
    return invoke<string>("read_file", { path });
}

// component.tsx — CORRECT
import { readFile } from "@/lib/tauri-api";
const content = await readFile(path);

// component.tsx — WRONG
import { invoke } from "@tauri-apps/api/core";
const content = await invoke<string>("read_file", { path }); // bypasses typed wrapper
```

### 2. Tauri Events vs Commands

- **Commands** (`invoke`): Request-response. Use for operations where the frontend needs the result before proceeding.
- **Events** (`emit`/`listen`): Fire-and-forget notifications from Rust to frontend. Use for status updates, file change notifications, LSP progress events.

```rust
// Rust — emit for status changes the frontend should react to
let _ = app.emit("lsp:status", LspStatus { state: LspStatusState::Ready, message: None });

// Frontend — listen for the event in the relevant store or component
const unlisten = await listen("lsp:status", (event) => {
    setLspStatus(event.payload.state, event.payload.message);
});
```

Always call `unlisten()` in `onCleanup` to prevent listener leaks.

---

## Testing Patterns

### Co-location

Test files live next to the code they test: `editor.store.test.ts` beside `editor.store.ts`, `treesitter.rs` tests inside the `#[cfg(test)]` module at the bottom of the file.

### Frontend Tests (Vitest)

- Mock all Tauri APIs in `src/test/setup.ts` — tests run in jsdom without a real Tauri runtime.
- Use a `resetState()` helper in `beforeEach` to restore stores to defaults.
- Test behavior, not implementation: assert on exported state and the effects of actions.

```typescript
beforeEach(() => { resetUIState(); });

it("closes the active tab and activates the next one", () => {
    addOpenFile({ path: "/a.kt", ... });
    addOpenFile({ path: "/b.kt", ... });
    setActiveFile("/a.kt");
    removeOpenFile("/a.kt");
    expect(editorState.activeFilePath).toBe("/b.kt");
});
```

### Rust Tests

- Unit tests go inside `#[cfg(test)]` modules at the bottom of each service file.
- Use `tempfile::TempDir` for tests that need real files on disk.
- Tests for `#[tauri::command]` functions focus on path validation logic, not Tauri integration.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn excludes_build_directory() {
        let dir = TempDir::new().unwrap();
        // ... setup ...
        assert!(!tree.contains("build/"));
    }
}
```

### Benchmarks

Performance-critical services have Criterion benchmarks in `src-tauri/benches/`. Run with `cargo bench`. Results are captured by `npm run perf:collect` into `perf-metrics/`.

---

## File Naming Conventions

| Scope | Pattern | Example |
|-------|---------|---------|
| Rust modules | `snake_case.rs` | `fs_manager.rs`, `lsp_client.rs` |
| TypeScript modules | `kebab-case.ts` | `file-utils.ts`, `fuzzy-match.ts` |
| Store files | `{domain}.store.ts` | `editor.store.ts`, `lsp.store.ts` |
| Store tests | `{domain}.store.test.ts` | `editor.store.test.ts` |
| Components | `PascalCase.tsx` | `FileTree.tsx`, `SearchPanel.tsx` |
| Component tests | `{name}.test.ts(x)` | `filetree.test.ts` |
| Services | `{domain}.service.ts` | `project.service.ts` |
| Bindings | Auto-generated — do not create manually | `FileNode.ts`, `Diagnostic.ts` |

---

## Build / Device Patterns (Phase 3)

### Channel Streaming (Build Output)

For high-throughput line-by-line streaming from Rust to frontend, use Tauri Channels (not events). The channel reference is passed as a command parameter:

```rust
// Rust command — accepts a Channel<BuildLine>
#[tauri::command]
pub async fn run_gradle_task(
    task: String,
    on_line: Channel<BuildLine>,   // <-- channel parameter
    ...
) -> Result<u32, String> {
    // ...
    let _ = on_line.send(line);   // fires on each output line
}
```

```typescript
// TypeScript — create a Channel and pass it to invoke
const channel = new Channel<BuildLine>();
channel.onmessage = (line) => addBuildLine(line);
await invoke<number>("run_gradle_task", { task, onLine: channel });
```

Use Tauri **events** (`emit`/`listen`) for fire-and-forget lifecycle notifications (build complete, device list changed). Use **channels** for high-frequency streaming data (build output).

### Build State Pattern

Build state follows the per-concern Mutex pattern (ADR-4). `BuildState` holds only the in-flight process ID, current status, errors, and bounded history. No Tauri types in `build_runner.rs`.

```rust
pub struct BuildState(pub Mutex<BuildStateInner>);
pub struct BuildStateInner {
    pub current_build: Option<ProcessId>,
    pub status: BuildStatus,
    pub history: VecDeque<BuildRecord>,   // bounded: MAX_HISTORY = 10
    pub current_errors: Vec<BuildError>,
    next_id: u32,
}
```

### Device State Pattern

Same per-concern pattern for ADB device state:

```rust
pub struct DeviceState(pub Mutex<DeviceStateInner>);
pub struct DeviceStateInner {
    pub devices: Vec<Device>,
    pub selected_serial: Option<String>,
    pub polling: bool,            // guards against double-starting the poll task
}
```

Device polling runs as a detached `tokio::spawn` task. When the device list changes, it emits `device:list_changed` via `AppHandle::emit`. The frontend listens and calls `setDevices()` in the store.

### AVD Management Pattern

AVD lifecycle (create/delete/wipe) goes through `avdmanager` CLI, resolved from `$ANDROID_HOME/cmdline-tools/`:

```rust
// Service layer (adb_manager.rs)
pub fn get_avdmanager_path(settings: &AppSettings) -> PathBuf { /* tries latest/, versioned paths */ }
pub fn list_system_images(settings: &AppSettings) -> Vec<SystemImageInfo> { /* scans system-images/ dir */ }
pub async fn list_device_definitions(avdmanager: &Path) -> Vec<DeviceDefinition> { /* avdmanager list device -c */ }
pub async fn create_avd(avdmanager, name, sdk_id, device_id) -> Result<(), String> { /* avdmanager create avd */ }
pub async fn delete_avd(avdmanager, name) -> Result<(), String> { /* avdmanager delete avd */ }
```

Commands return the refreshed AVD list (`Vec<AvdInfo>`) so the frontend can update in one round-trip.

### DevicePanel Mode Pattern

`DevicePanel` accepts a `mode` prop for its two usage contexts:
- `mode="panel"` (default): full-width tab content with toolbar, two sections, AVD lifecycle actions
- `mode="popover"`: compact 280px dropdown for the StatusBar pill, with "Manage Devices" footer link

```tsx
// Tab (App.tsx)
<DevicePanel />

// StatusBar popover
<DevicePanel mode="popover" onClose={() => setOpen(false)} />
```

`CreateDeviceDialog` is opened from the panel, loads system images and device definitions lazily (with store cache), and returns the refreshed AVD list via `onCreated` callback.

### Build Service Pattern

A `build.service.ts` coordinates multiple IPC calls into a user-visible action:

```
runBuild() → runGradleTask (IPC, returns immediately after spawn)
           → await build:complete event (one-shot Promise resolved by listener)
           → finalizeBuild (with correct success/duration from event)

runAndDeploy() → resolveDevice (pick dialog if no online device selected)
              → setDeployPhase("building") → runBuild
              → setDeployPhase("installing") → findApkPath → installApkOnDevice
              → setDeployPhase("launching") → launchAppOnDevice (using applicationId)
              → setDeployPhase(null)
```

**Key invariants:**
- `runBuild()` only resolves after `build:complete` fires — `buildState.phase` is always correct when it returns.
- `_resolveBuildComplete` is a module-level one-shot resolver. It is set before `runGradleTask` and cleared by the `listenBuildComplete` handler.
- If `cancelBuild()` is called mid-build, `_resolveBuildComplete` is nulled so the awaiting `buildComplete` promise is abandoned without hanging.
- Build errors accumulate in `accumulatedErrors` (including those without file locations) and are passed to `finalizeBuild` for backend persistence.

**Device resolution:** `resolveDevice()` checks if `deviceState.selectedSerial` is online; if not, it dynamically imports and shows `DevicePickerDialog`. The dialog is lazy-imported to avoid circular dependency between build service and device UI.

**DeployPhase:** `buildState.deployPhase` drives the status bar during the install/launch steps after a successful build. Always cleared in the `finally` block.

---

## Logcat Pipeline Pattern (Phase 3+)

### Architecture

The logcat data flow is a four-stage pipeline:

```
adb logcat
  → Ingester (parse raw lines → RawLogLine, zero mutex)
  → Pipeline+Batcher task (every 100ms: run processors, lock state once, emit filtered)
  → StreamManager (filter entries before IPC emit)
  → Frontend (render pre-filtered entries; frontend-only filtering for age/regex/negate)
```

### Processor Chain (`services/log_pipeline.rs`)

Processors implement the `LogProcessor` trait and run sequentially per entry:

```rust
pub trait LogProcessor: Send + Sync {
    fn process(&self, entry: &mut ProcessedEntry, ctx: &mut PipelineContext);
}
```

Built-in processors (in order):
1. **PackageResolver** — attaches `package` from PID→package map
2. **CrashAnalyzer** — detects FATAL/ANR/native, assigns `crash_group_id`
3. **JsonExtractor** — detects valid JSON in message, stores raw string in `json_body`
4. **CategoryClassifier** — O(1) tag→category lookup

`PipelineContext` is owned by the pipeline task (no mutex) and carries cross-entry state (pid_to_package, crash group tracking). It is synced into `LogcatStateInner.known_packages` once per 100ms tick.

### LogStore (`services/log_store.rs`)

Replaces the old `LogcatBuffer`. Adds secondary indexes for O(1) crash/JSON lookups:

```rust
pub struct LogStore {
    entries: VecDeque<ProcessedEntry>,  // 50K ring buffer
    crash_ids: VecDeque<u64>,           // IDs of crash entries
    json_ids: VecDeque<u64>,            // IDs of JSON entries
    pub stats: LogStats,                // running counters (O(1) per entry)
}
```

### Backend Filtering (`services/log_stream.rs`)

`StreamState` holds the active `LogcatFilter`. The batcher calls `filter_batch()` before emitting — only matching entries cross the IPC bridge. Updated via `set_logcat_filter` command:

```typescript
// Frontend: on query change, send simple tokens to backend
await setLogcatFilter({ minLevel: "error", tag: null, text: null, package: null, onlyCrashes: false });
// Then fetch backfill with same filter
const entries = await getLogcatEntries({ minLevel: "error" });
```

### Frontend Filter Split

The frontend `filteredEntries` memo only applies tokens the backend cannot handle:
- `age:N` — time-based, needs `Date.now()`
- `-tag:X` / `-message:X` — negation
- `tag~:X` / `message~:X` — regex

Everything else (level, simple tag/text/package, only_crashes) is handled in Rust. This reduces the JS O(n) scan to a much smaller pre-filtered set.

### ProcessedEntry (IPC type)

```rust
pub struct ProcessedEntry {
    // Core fields (same as old LogcatEntry)
    pub id: u64, pub timestamp: String, pub pid: i32, pub tid: i32,
    pub level: LogcatLevel, pub tag: String, pub message: String,
    pub package: Option<String>, pub kind: LogcatKind, pub is_crash: bool,

    // Pipeline-enriched fields
    pub flags: u32,                  // EntryFlags bitfield (CRASH, ANR, JSON_BODY, …)
    pub category: EntryCategory,     // Network / Lifecycle / Performance / GC / …
    pub crash_group_id: Option<u64>, // groups consecutive crash stack lines
    pub json_body: Option<String>,   // raw JSON (parsed on-demand in frontend)
}
```

`json_body` is `Option<String>` (not `Value`) to avoid double-serialisation. Frontend parses when user expands the JSON viewer panel.

