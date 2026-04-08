# Best Practices

This is a living document covering architectural principles, design decisions, and industry-standard practices for Keynobi Android Dev Companion. It explains **why** we do things, while `CODE_PATTERN.md` explains **how**.

Update this document when foundational architecture or design decisions change.

---

## Table of Contents

1. [Core Principles](#core-principles)
2. [Security](#security)
3. [Performance](#performance)
4. [Modularity & Maintainability](#modularity--maintainability)
5. [Testing](#testing)
6. [Developer Experience](#developer-experience)
7. [Error Handling](#error-handling)
8. [Observability](#observability)
9. [AI-First Design](#ai-first-design)

---

## Core Principles

### 1. Correctness Before Optimization

Write correct code first. Measure, then optimize. Every optimization in this codebase that is not trivially obvious must have a benchmark proving it is necessary. The performance targets (< 500ms build variant discovery, < 2s device list refresh) exist to define "correct enough," not to encourage premature micro-optimization.

### 2. Explicit Over Implicit

Every non-obvious behavior must be explicit in code. If a function has a side effect, name it that way (`registerKeyAndAction`, not `register`). If a mutex must be dropped before an I/O call, add a comment explaining the invariant. Clever code that works silently is a maintenance hazard.

### 3. Fail Fast, Fail Visibly

Security violations and path traversal attempts must fail immediately with a clear error message. Unknown failures must surface to the user via toast notifications or the status bar.

### 4. Single Source of Truth

For any piece of data, there is exactly one authoritative source:
- Rust models are the source of truth for IPC types (TypeScript bindings are generated from them)
- The Tauri `FsState` is the authoritative backend source for the open project root; `project.store.ts` is its reactive frontend projection populated via IPC
- The `action-registry.ts` is the source of truth for all discoverable app actions

When something needs to be known in two places, derive it from the source — don't duplicate it.

---

## Security

### Path Traversal Prevention

Every path that enters the system from the frontend is untrusted. Before any file operation, validate the path is inside the open project directory using canonicalization:

1. `canonicalize()` the project root (resolves symlinks, removes `..`)
2. `canonicalize()` the target path (same)
3. Check `target.starts_with(root)`

Never skip step 1 and 2 and use raw string prefix matching — symlinks and `..` components bypass it.

### Principle of Least Privilege

Tauri capabilities are scoped to what is actually needed. The frontend receives only what the Rust layer explicitly chooses to send via IPC.

### No Secrets in Frontend

API keys, tokens, and credentials must never be stored in the frontend. They must live in Rust and be passed to external services server-side. The frontend knows only that an operation succeeded or failed.

---

## Performance

### Measure, Then Target

The performance targets for this project:

| Operation | Target |
|-----------|--------|
| Build variant discovery | < 500ms |
| Device list refresh | < 2 seconds |
| ADB install feedback appears | < 1 second after command |
| Logcat 60fps with 1000 lines/sec | smooth rendering |
| Command palette open | < 50ms |

Run `npm run perf:collect` after changes to track regression against these targets.

### Keep the Main Thread Free

The UI runs on the frontend's single JavaScript thread. Any operation that could block for > 16ms must be moved to Rust via IPC, and any CPU-heavy Rust work must be on a `tokio::task::spawn_blocking` thread, not the async executor.

### Bound All Collections

Every in-memory collection that grows over time must have an explicit cap. Examples:
- Logcat ring buffer: 50K entries (drop oldest on overflow)
- Build history: last 10 builds
- Recent projects list: 20 entries
- Build log: bounded `VecDeque`
- MCP activity log: bounded entry list
- Process manager: bounded process map

Unbounded growth is a memory leak on a long-lived app process.

### Hold Locks Briefly

Never hold a Mutex across an `await` or I/O call. Lock, clone what you need, drop, then proceed. See `CODE_PATTERN.md` §Mutex Lock Discipline for the concrete pattern.

### SolidJS Reactivity Efficiency

- Use `createMemo` for derived values used in multiple places — prevents redundant recomputation on each render.
- Do not destructure reactive stores — destructuring breaks fine-grained tracking and causes over-rendering.
- Use `produce` from `solid-js/store` for mutations involving array splices or nested object deletion.

---

## Modularity & Maintainability

### Layered Architecture

```
Frontend UI components
    ↓ reads from
SolidJS Stores (reactive state)
    ↓ populated by
Services (async flows, combine IPC + store updates)
    ↓ calls
lib/tauri-api.ts (typed IPC wrappers)
    ↓ invokes
Rust Commands (thin validation + delegation)
    ↓ delegates to
Rust Services (business logic, no Tauri dependency)
```

No layer skips another. Components do not call `invoke` directly. Commands do not contain business logic. Services do not import Tauri types (they receive plain Rust types).

### Domain-Driven Module Organization

Code is organized by domain (`editor`, `search`, `lsp`, `filetree`, `symbols`), not by type (`controllers`, `views`, `models`). A new feature adds files to its domain folder; it does not scatter changes across multiple type-based folders.

### Dependency Direction

Dependencies flow downward (UI → stores → services → IPC → Rust). If a lower layer needs to communicate upward, it uses events (Tauri emit/listen) or callbacks — never a direct import of a higher-level module.

### Small, Focused Functions

Functions do one thing. If a function has multiple responsibilities, split it. The `registerKeyAndAction` helper is correct — it registers one thing (a keyboard-triggered action) in two systems, not two different things.

### Comments Explain Why, Not What

Code explains what it does. Comments explain why — non-obvious invariants, constraints, or design decisions that the code cannot express.

```rust
// Mutex is held only for the clone — never during the actual I/O operation.
// Holding the lock across an await would serialize unrelated commands.
let root = { let fs = fs_state.0.lock().await; fs.project_root.clone() }?;
```

---

## Testing

### Test Behavior, Not Implementation

Tests assert on observable behavior (what the function returns, what state it sets), not internal implementation details. If a refactor that preserves behavior breaks a test, the test is wrong.

### Test Coverage Priorities

Cover in this order:
1. **Security-critical paths**: path validation, zip extraction, content-length cap — these must have tests proving attacks fail
2. **Business logic in services**: tree-sitter symbol extraction, search engine, LSP client framing
3. **Store actions**: state transitions, edge cases (close active tab, dirty tab handling)
4. **Utilities**: fuzzy matching, navigation history, keybindings

UI component rendering tests have lower priority and higher maintenance cost. Prefer testing store logic and services.

### Test Isolation

Each test must set up its own state and clean up after itself. Stores have reset helpers (e.g. `resetBuildState`, `resetDeviceState`, `resetVariantState`). Rust tests use `tempfile::TempDir` for filesystem fixtures. Tests must not rely on the order they run.

### Regression Benchmarks

Performance benchmarks in `src-tauri/benches/` must run in CI. A PR that causes a > 2x regression in any benchmark must justify it or revert the change. Use `npm run perf:report` to compare against the previous captured baseline.

---

## Developer Experience

### Keybindings = Actions

Every keyboard shortcut must be registered with `registerKeyAndAction()` so it appears in the command palette (`Cmd+Shift+P`). A shortcut that cannot be discovered via the command palette effectively does not exist for most users.

### Clear Error Messages

Error messages must tell the user what happened and, when possible, what to do about it. Prefer:
- "Failed to save 'MainActivity.kt': Disk full" over "Write error"
- "Path is outside the project directory" over "Access denied"
- "Kotlin LSP is not installed. Download it now?" over "LSP unavailable"

### Optimistic UI

For fast operations (tab switch, file tree expand), update the UI immediately and handle errors after. For slow operations (file save, LSP start), show a progress indicator rather than blocking the interface.

### Non-Blocking Intelligence

Code intelligence (LSP) must not block the editor. If the LSP is starting or unavailable, the editor still works with syntax highlighting and Tree-sitter symbols. LSP features activate progressively as they become available.

### Unsaved Work Protection

Never silently discard user work. The close-tab flow, the close-app flow, and the open-new-project flow all check for unsaved files and prompt with Save / Discard / Cancel before proceeding. There is no scenario where a user's edits disappear without a confirmation.

---

## Error Handling

### Error Propagation Strategy

| Layer | Error type | Propagation |
|-------|-----------|-------------|
| Rust services | `FsError` / domain-specific | Return `Result<T, E>` |
| Rust commands | `String` | `.map_err(\|e\| e.to_string())` at the boundary |
| TypeScript services | `unknown` (from `invoke`) | Caught with `try/catch`, shown via `showToast` |
| TypeScript stores | Result/error state fields | Stores hold error state (e.g. `buildState.errors`); they do not throw or propagate |

### Error Visibility

- Errors that require user action: `showToast("...", "error")` (visible, auto-dismisses)
- Errors that indicate degraded state: update relevant store (`setLspStatus("error", message)`) + toast
- Errors the user cannot act on (e.g., file watcher missed event): `tracing::warn!` only

### Never Panic in Production

`unwrap()` is banned in production Rust code. `expect("reason")` is acceptable only when:
1. The condition is statically guaranteed to be safe (e.g., `Regex::new` on a literal pattern inside a `LazyLock` static)
2. Or failure truly means a programmer error (e.g., grammar loading on startup fails — the binary is broken)

---

## Observability

### Structured Logging

All logging uses `tracing` macros. Log levels are:
- `error!`: requires investigation; the feature is broken
- `warn!`: degraded behavior, the system can continue
- `info!`: milestone events (startup, project open, LSP ready)
- `debug!`: developer diagnostics (all LSP JSON-RPC traffic, file events)

Enable debug logging via `RUST_LOG=keynobi_lib=debug cargo tauri dev`.

### Performance Metrics

The `scripts/collect-metrics.mjs` script captures frontend bundle sizes, Rust binary size, and Criterion benchmark results into `perf-metrics/metrics_latest.json`. Commit these snapshots to track trends over time.

### LSP Traffic Debugging

All LSP JSON-RPC messages are logged at `debug` level via `tracing`:
```
RUST_LOG=keynobi_lib::services::lsp_client=debug npm run tauri dev
```

---

## AI-First Design

This companion app is built AI-first. Every feature should be designed with AI consumption in mind:

### Structured Data Over Raw Text

Prefer structured types (LSP `Diagnostic`, Tree-sitter `SymbolInfo`) over raw strings. AI agents can parse structured data; they struggle with free-form text.

### Action Registry as Tool Discovery

The central `action-registry.ts` is not just for the command palette — it is the foundation for the MCP server. AI agents discover available app actions from this registry. Every action registered here becomes a tool an AI can invoke.

### Navigation History for Context

Navigation history enables AI agents to understand where the user has been, providing context about what they are working on. Every file-open and go-to-location call should go through `openFileAtLocation()` in `project.service.ts` to maintain the navigation stack.

### Open Project Root as Sandbox

The path security model (all operations validated against the open project root) also serves as the AI safety boundary. AI-driven code generation and editing is automatically sandboxed to the project the user opened.

### Streaming Over Blocking

Any feature that produces results over time (search, build output, logcat, LSP indexing progress) must stream results rather than return them in a batch. AI agents benefit from partial results to reason incrementally.
