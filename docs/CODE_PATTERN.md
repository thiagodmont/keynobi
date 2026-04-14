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
7. [Build / Device / Logcat / MCP Patterns](#build--device--logcat--mcp-patterns) (see also `DOMAIN_PATTERNS.md`)

---

## Project Structure

```
keynobi/
├── src/                        # Frontend (SolidJS + TypeScript)
│   ├── bindings/               # Auto-generated Rust-to-TS types (DO NOT EDIT)
│   ├── components/             # UI components (organized by domain)
│   │   ├── build/              # Build output panel
│   │   ├── common/             # Shared UI primitives (Icon, Toast, Dialog, ...)
│   │   ├── device/             # Device management and AVD panels
│   │   ├── health/             # System health checks
│   │   ├── layout/             # Shell: Sidebar, StatusBar, PanelContainer, ...
│   │   ├── logcat/             # Logcat viewer
│   │   ├── ui-hierarchy/      # Layout / UI Automator tree viewer
│   │   ├── mcp/               # MCP server panel
│   │   ├── projects/           # Project switcher and recent projects
│   │   └── settings/           # Settings panel
│   ├── lib/                    # Pure logic, no UI
│   │   ├── action-registry.ts  # Central app action registry
│   │   ├── fuzzy-match.ts      # Fuzzy search utility
│   │   ├── keybindings.ts      # Global keybinding system
│   │   ├── file-utils.ts
│   │   ├── logcat-query.ts     # Logcat filter query parser
│   │   └── tauri-api.ts        # Typed Tauri IPC wrappers
│   ├── services/               # Async flows combining stores + IPC
│   │   ├── build.service.ts    # Build + deploy orchestration
│   │   └── project.service.ts  # Project open/switch/navigation
│   ├── stores/                 # SolidJS reactive stores ({domain}.store.ts)
│   ├── test/                   # Vitest setup and Tauri API mocks
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

The effective root is `gradle_root` (detected Gradle project root) when available, otherwise `project_root`. This allows go-to-definition navigation to reach files in sibling Gradle modules.

The validation pattern (applied inline in each command that needs it):

1. Lock `FsState`, read `gradle_root` or `project_root`, drop lock
2. `canonicalize()` both root and target path
3. Check `canonical_target.starts_with(&canonical_root)`

```rust
// Example: inline path validation in a command
let root = {
    let fs = fs_state.0.lock().await;
    fs.gradle_root.as_ref().or(fs.project_root.as_ref())
        .ok_or("No project open")?.clone()
}; // lock dropped
let canonical_root = root.canonicalize().map_err(|e| format!("Failed to resolve root: {e}"))?;
let canonical_target = Path::new(&path).canonicalize().map_err(|e| format!("Failed to resolve path: {e}"))?;
if !canonical_target.starts_with(&canonical_root) {
    return Err("Path is outside the project directory".into());
}
```

Never use `path.starts_with(root)` without canonicalization first. For MCP tools, use the domain-specific validators (e.g. `validate_apk_path` for APK paths).

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

Use `tracing` macros, never `println!` or `eprintln!`. Match the log level to the audience:

```rust
tracing::error!("LSP process crashed: {}", err);   // must be investigated
tracing::warn!("File watcher missed event");         // degraded behavior
tracing::info!("LSP initialized: {:?}", server);    // lifecycle milestones
tracing::debug!("LSP request [{}] {}", id, method); // developer debugging
```

Configure at runtime via `RUST_LOG=keynobi_lib=debug`.

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

### 7. Design system (`src/components/ui/`)

Reusable atoms and layout pieces live under `src/components/ui/` with **CSS Modules** and **`var(--*)` tokens** from `src/styles/theme.css` (see `docs/superpowers/specs/2026-04-12-design-system-design.md`). Import from `@/components/ui` via the barrel file.

- **`Button` vs `IconButton`**: Use `Button` for labeled actions (primary/secondary/ghost/danger). Use **`IconButton`** for compact toolbar controls (fixed square hit target, `title` for tooltip, optional `active` / `size="sm" | "md"`).
- **`Dropdown` / `MenuItem`**: Set `destructive: true` on items that remove data so they get error-colored styling.
- **Colors**: Prefer semantic tokens (`var(--success)`, `var(--error)`, `var(--info)`, `var(--warning)`, `var(--text-muted)`) instead of hardcoded hex in app components. For translucent tints, use `color-mix(in srgb, var(--token) N%, transparent)` where needed.
- **Roadmap**: Prefer existing **`Panel`**, **`Tabs`**, and **`Toolbar`** when replacing duplicated headers or tab rows. For icon-only controls, **`Tooltip`** is preferred over bare `title` for accessibility (migrate incrementally).

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

### 3. Tauri Plugin APIs in Components

Tauri plugin APIs (`@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-fs`, `@tauri-apps/api/window`) may be used directly in components when the operation is UI-specific and does not need a typed wrapper. Examples: file save dialog in `LogcatPanel`, window controls in `TitleBar`. These are distinct from `invoke` calls which must always go through `tauri-api.ts`.

---

## Testing Patterns

### Co-location

Test files live next to the code they test: `editor.store.test.ts` beside `editor.store.ts`, `treesitter.rs` tests inside the `#[cfg(test)]` module at the bottom of the file.

### Frontend Tests (Vitest)

- Mock all Tauri APIs in `src/test/setup.ts` — tests run in jsdom without a real Tauri runtime.
- Use a `resetState()` helper in `beforeEach` to restore stores to defaults.
- Test behavior, not implementation: assert on exported state and the effects of actions.

### Design system and UI refactors (verification gate)

Before merging PRs that touch `src/components/ui/`, shared panel styling, or broad token/color refactors, run all of:

```bash
npm test && npm run typescript:check && npm run lint
```

This is the project’s standard gate for catching broken imports, type errors, and regressions without relying on a separate coverage threshold.

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
| Store files | `{domain}.store.ts` | `build.store.ts`, `device.store.ts` |
| Store tests | `{domain}.store.test.ts` | `build.store.test.ts` |
| Components | `PascalCase.tsx` | `FileTree.tsx`, `SearchPanel.tsx` |
| Component tests | `{name}.test.ts(x)` | `filetree.test.ts` |
| Services | `{domain}.service.ts` | `project.service.ts` |
| Bindings | Auto-generated — do not create manually | `FileNode.ts`, `Diagnostic.ts` |

---

## Build / Device / Logcat / MCP Patterns

Domain-specific implementation details are documented in `DOMAIN_PATTERNS.md`. This section covers only the reusable patterns that apply across all domains.

### Channel Streaming

For high-throughput line-by-line streaming from Rust to frontend, use Tauri Channels (not events). The channel reference is passed as a command parameter:

```rust
#[tauri::command]
pub async fn run_gradle_task(
    task: String,
    on_line: Channel<BuildLine>,
    ...
) -> Result<u32, String> {
    let _ = on_line.send(line);
}
```

```typescript
const channel = new Channel<BuildLine>();
channel.onmessage = (line) => addBuildLine(line);
await invoke<number>("run_gradle_task", { task, onLine: channel });
```

Use **events** (`emit`/`listen`) for fire-and-forget lifecycle notifications. Use **channels** for high-frequency streaming data.

### Service Coordination

Frontend services (`build.service.ts`, `project.service.ts`) coordinate multiple IPC calls into user-visible actions. They own the orchestration logic: spawn a backend task, listen for its completion event, then update stores. Services may dynamically import UI components (e.g. device picker dialog) to avoid circular dependencies.

### MCP Tools

MCP tools use the `rmcp` crate with `#[tool_router]` / `#[tool]` macros. Key rules:
- Validate all string inputs before acting (task names, package names, device serials, paths)
- Lock state, clone, drop before I/O — same as Tauri commands
- Use `CallToolResult::structured(json!({...}))` for data the AI will reason about
- Use `CallToolResult::error(...)` for execution failures (not protocol errors)
- `AndroidMcpServer` works in both GUI mode (shared Tauri state) and headless mode (`--mcp` CLI flag)

See `DOMAIN_PATTERNS.md` for detailed MCP tool definitions, resource/prompt patterns, and lifecycle events.
