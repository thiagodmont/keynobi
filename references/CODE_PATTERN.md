# Code Patterns

Concrete implementation rules for Keynobi. `BEST_PRACTICES.md` explains why these rules exist; `DOMAIN_PATTERNS.md` adds per-domain invariants; `DESIGN_SYSTEM.md` owns UI primitives, tokens, and accessibility.

Update this file when a cross-cutting implementation pattern changes. When the code does not yet meet a rule, keep the rule and record the gap under [Known Gaps](#known-gaps).

---

## Repository Layout

Frontend:

- `src/` - SolidJS frontend.
- `src/components/ui/` - shared design-system primitives, one folder per primitive, exported from `@/components/ui`.
- `src/components/{domain}/` - domain UI panels and presentational pieces.
- `src/components/layout/` - app shell (title bar, status bar, sidebars).
- `src/stores/` - Solid stores and state actions.
- `src/services/` - frontend orchestration across stores, IPC, and UI flows.
- `src/lib/tauri-api.ts` - typed wrappers for every Tauri `invoke` call.
- `src/lib/` - pure frontend utilities (query parsing, matching, formatting).
- `src/lib/telemetry/` - browser-side crash reporting and scrubbing.
- `src/utils/` - small browser helpers (clipboard, debounce).
- `src/styles/` - `theme.css` tokens and `global.css`.
- `src/bindings/` - generated TypeScript bindings; do not edit manually.
- `src/test/` - Vitest setup (`setup.ts`), typed factories, and the mock backend used by web-mode e2e.

Backend:

- `src-tauri/src/lib.rs` - app setup, managed state, command registration, shutdown.
- `src-tauri/src/main.rs` - entry point; `--mcp` starts the headless MCP server.
- `src-tauri/src/commands/` - thin Tauri command handlers.
- `src-tauri/src/services/` - Rust business logic.
- `src-tauri/src/models/` - Rust IPC models exported with `ts-rs`, plus `AppError`.
- `src-tauri/src/utils/` - shared helpers: `path.rs` (filesystem boundaries), `validation.rs` (identifiers), and `line_reader.rs` (bounded process-output lines).
- `src-tauri/tests/` - integration tests (`build_integration.rs`, `ipc/`, `fixtures/mock_gradlew`).
- `src-tauri/benches/` - Criterion benchmarks.
- `src-tauri/capabilities/` - Tauri permission grants.

Tooling:

- `.storybook/` - Storybook configuration for the design system.
- `e2e/` - Playwright web-mode tests: `smoke/`, `ipc/`, `fixtures/`, plus separately configured `storybook/` and `visual/` suites.
- `vite-plugin-tauri-mock.ts` - replaces Tauri APIs with `src/test/mock-backend` when `VITE_E2E` is set.
- `scripts/` - release, versioning, metrics, packaging, and the IPC contract test.

---

## Rust Patterns

### State

Each domain owns its own state struct, wrapped as a newtype around `Arc<tokio::sync::Mutex<...Inner>>` and registered with `.manage()` in `lib.rs`. Avoid unrelated fields in the same Mutex because it creates unnecessary contention.

Implement `Default` by delegating to `new()` when a state type has construction logic.

Services that the headless MCP server reuses must work without an `AppHandle`: take `Option<AppHandle>` and skip GUI events when it is `None`.

### Mutex Discipline

Hold locks for the shortest possible time. Clone what you need inside a block so the guard drops before any I/O:

```rust
async fn resolve_gradle_root(fs_state: &State<'_, FsState>) -> Result<PathBuf, String> {
    let fs = fs_state.0.lock().await;
    fs.gradle_root
        .as_ref()
        .or(fs.project_root.as_ref())
        .cloned()
        .ok_or_else(|| "No project open".to_string())
} // guard dropped here

let root = resolve_gradle_root(&fs_state).await?;
let result = do_io(&root).await?;
```

Do not hold a Mutex across I/O, process execution, event emission, or `await`. Use `std::sync::Mutex` only for short, synchronous critical sections inside callbacks.

### Commands

Tauri commands:

- Accept IPC-shaped inputs.
- Validate paths and other untrusted inputs with the shared validators.
- Clone required state and drop locks.
- Delegate business logic to `services/`.
- Return `Result<T, AppError>` for new commands. Use `AppError` variants (`InvalidInput`, `NotFound`, `ProcessFailed`, ...) so the frontend can branch on `kind`.

No business logic belongs in command handlers. If an MCP tool needs the same behavior, both call the same service function.

Registering a command touches four places, and the IPC contract test (`scripts/ipc-contract.test.mjs`) fails if one is missing:

1. The handler in `src-tauri/src/commands/`.
2. `tauri::generate_handler![...]` in `lib.rs`.
3. The typed wrapper in `src/lib/tauri-api.ts`.
4. A handler in `src/test/mock-backend/`.

### Path Security

Any command or tool that accepts a path must validate it against the effective project root.

The effective root is `gradle_root` when available, otherwise `project_root`. Canonicalize both root and target, then check the canonical target is within the canonical root. Use the shared validators:

- `utils::path::validate_within_root(root, untrusted)` for project-relative paths. It rejects absolute paths and `..` before canonicalizing.
- `utils::path::validate_apk_within_build_outputs(...)` for APK installs.

Never use raw `path.starts_with(root)` for security.

### Identifier Validation and Process Arguments

- Validate Gradle tasks, device serials, and package names with `utils/validation.rs` (`validate_gradle_task`, `validate_device_serial`, `validate_package_name`). Put new identifier validators there.
- Spawn processes with argument vectors (`tokio::process::Command::new(bin).args([...])`). Never build a host shell string.
- Read streamed process output with `utils::line_reader::CappedLines`, not `AsyncBufReadExt::lines()`. It keeps at most `MAX_LINE_BYTES` per line, discards the rest up to the next newline with a `… [truncated N bytes]` marker, replaces invalid UTF-8 instead of failing, and is safe to use as a `tokio::select!` branch.
- Never wait for a child's output to reach EOF before waiting for its exit: a descendant that inherited the pipes (a Gradle daemon, `adb` server) can hold them open forever. `process_manager` waits for exit alongside the reads and then drains remaining output for at most `POST_EXIT_DRAIN` (2 s).
- Values sent through `adb shell` are re-parsed by the device shell. Pass every non-literal argument through `utils::device_shell::quote_device_shell_arg` (the `run_adb_shell` and `adb_cmd` helpers already do). Test new call sites with `device_shell::test_support::fake_adb`, which parses arguments the way the device does.

### Persistence

- Write files atomically: write a temporary sibling, then `rename` it over the target. See `settings_manager.rs` and `build_runner.rs`.
- Settings structs use `#[serde(default)]` so older files load after fields are added. Clamp numeric settings to safe ranges on load.
- Resolve storage paths through `settings_manager::data_dir()`. Tests must not touch the real `~/.keynobi`.

### Models and Bindings

Every Rust type crossing IPC derives:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
```

After changing exported models, regenerate and commit `src/bindings/`:

```bash
npm run generate:bindings   # runs `cargo test`, which exports the bindings
npm run check:bindings      # regenerate and fail on any diff
```

`generate:bindings` runs the full Rust test suite. Check the data-dir warning in `BEST_PRACTICES.md` § Known Gaps first.

### Errors and Logging

- Use `thiserror` for structured service errors (`AppError`, `FsError` in `models/error.rs`).
- Do not use `unwrap()` in production Rust. `expect("why this cannot fail")` is acceptable for programmer-error invariants such as static regexes.
- Use `tracing` macros for logs.
- GUI log filtering uses `KEYNOBI_LOG`; headless MCP uses `RUST_LOG`.

### Formatting and Lints

- `rustfmt.toml` (`max_width = 100`); CI runs `cargo fmt --check`.
- CI runs `cargo clippy --all-targets -- -D warnings`, with and without `--features telemetry`.
- The toolchain is pinned in `rust-toolchain.toml`.

---

## Frontend Patterns

### Stores

Each logical domain has a `{domain}.store.ts` file that exports:

- The reactive state object.
- Named action functions.
- A reset helper for tests when state is mutable across tests (for example `resetBuildState`).

Components read stores but do not mutate them directly. Use `produce` for array splices and nested updates; use direct path setters for scalar updates.

Do not destructure Solid stores. Access properties through the store object so fine-grained reactivity remains intact.

### Services

Frontend services coordinate user-visible flows across IPC calls, events, stores, and dialogs. Examples: build/run/deploy, project open, logcat start/stop, update check.

Services may dynamically import UI pieces to avoid cycles, but reusable UI state still belongs in stores.

### Components

- `components/ui/` contains reusable primitives. See `DESIGN_SYSTEM.md`.
- `components/layout/` contains the shell layout.
- `components/{domain}/` contains domain-specific UI.
- `components/common/` holds legacy shared pieces (`ErrorBoundary`, `LogViewer`). Put new primitives in `components/ui/`.

Components delegate side effects to services or store actions. They may call typed wrappers from `tauri-api.ts` for narrow, component-local actions (for example launching an AVD from its row), but never `invoke` directly. They may use Tauri plugin APIs directly only for UI-specific operations such as dialogs, window controls, or writing a file the user just chose in a save dialog.

Static styling belongs in CSS Modules with theme tokens. Reserve inline `style` for runtime-derived values.

### SolidJS Reactivity

- Use `createMemo` for derived values used in render paths or multiple places.
- Do not create `createMemo` or `createEffect` at module scope in singleton stores. Export plain accessor functions for store-derived values, or create effects inside a component/root so Solid can own and dispose the computation.
- Call signals/memos in JSX: `{label()}`, not `{label}`.
- Register `onCleanup` for timers, subscriptions, and listeners.
- Guard async flows where stale responses can arrive out of order (request IDs or epoch counters).

### Design System

Use shared primitives from `@/components/ui` before writing local markup or styles. `DESIGN_SYSTEM.md` has the ownership map, token rules, accessibility and keyboard contract, Storybook standard, and adoption order.

---

## IPC and Events

### Typed IPC

All `invoke` calls belong in `src/lib/tauri-api.ts`. Components, stores, and services import typed wrappers from there. Import IPC types from `@/bindings`; never redefine a type that originates in Rust.

```typescript
export async function getBuildStatus(): Promise<BuildStatus> {
  return invoke<BuildStatus>("get_build_status");
}
```

Render errors with `formatError(err)`, which understands `AppError` (`{ kind, message }`), strings, and `Error`.

### Commands, Events, and Channels

- **Commands** are request/response.
- **Channels** carry a stream owned by one request: build output (`Channel<BuildLine>`), SDK image download progress.
- **Events** carry app-wide notifications and batched streams not tied to one request.

| Event | Payload / purpose |
|-------|-------------------|
| `build:complete` | Build finished, failed, or was cancelled. |
| `device:list_changed` | Connected device serials changed. |
| `logcat:entries` | Batched processed log entries (every 100 ms, up to 500). |
| `logcat:cleared`, `logcat:reconnecting`, `logcat:stopped` | Logcat stream lifecycle. |
| `mcp:started`, `mcp:client_connected`, `mcp:stopped`, `mcp:startup-failed` | In-process MCP server lifecycle. |
| `settings:corrupted` | Settings file was unreadable and was reset. |
| `monitor://stats` | App memory and log-folder size, every 5 s. |

Name new events `{domain}:{event_name}`. Always call the event `unlisten()` in cleanup.

---

## Actions and Keybindings

Every keyboard shortcut that should appear in the command palette must be registered with `registerKeyAndAction()` in `App.tsx`. It wraps `registerAction()` (`lib/action-registry.ts`) and `registerKeybinding()` (`lib/keybindings.ts`).

Use `registerAction()` for command-palette actions without shortcuts.

Do not use bare `registerKeybinding()` for app commands unless the shortcut is intentionally hidden from the command palette. Update the shortcut table in `USER_MANUAL.md` in the same change.

---

## Testing Patterns

### Vitest

- Tests live next to the code they cover.
- `src/test/setup.ts` mocks Tauri APIs with `vi.mock`. Override per test with `vi.mocked(...)`.
- Use factories from `src/test/factories/` for IPC-shaped data (build, devices, logcat, settings).
- Reset mutable stores in `beforeEach` with their reset helpers.
- Test behavior and state transitions, not internal implementation details.

### Playwright

- E2E tests live under `e2e/` and run against the Vite dev server in web mode.
- `vite-plugin-tauri-mock.ts` swaps Tauri APIs for `src/test/mock-backend/` when `VITE_E2E` is set.
- Tests may call `window.__e2e__.invoke(...)` and `window.__e2e__.triggerEvent(...)` for IPC contract checks.
- Per-test startup settings go in `window.__keynobi_e2e_settings_overrides` before `page.goto("/")`.

Visual regression tests live under `e2e/visual/` and run through `playwright.visual.config.ts`, separate from the normal e2e suite. Prefer locator-level screenshots of critical surfaces over full-page screenshots. Generate local baselines with `npm run test:e2e:visual:update`, then verify with `npm run test:e2e:visual`. This lane is local-only; CI does not run it yet.

### Rust

- Unit tests live in `#[cfg(test)]` modules near the service code.
- Use `tempfile::TempDir` for filesystem fixtures. Never read or write the real `~/.keynobi`: unit tests are isolated automatically, and integration tests in `tests/` must call `common::isolate_data_dir()` before touching persisted state.
- Command tests should focus on validation and boundary behavior.
- Security validators need negative tests: traversal, symlinks, option-shaped values, and shell metacharacters.

### Verification Gate

Run the checks that match your change before handoff:

| Change | Command |
|--------|---------|
| Any frontend change | `npm run lint && npm run format:check && npm run typescript:check && npm test` |
| Design system, shared styling, tokens, broad UI refactor | Frontend checks + `npm run test:ds` |
| User flows or IPC | `npm run test:e2e` |
| Rust | `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib --tests` |
| Rust models | `npm run check:bindings` |

`CONTRIBUTING.md` mirrors the full CI matrix.

---

## Naming

| Scope | Pattern | Example |
|-------|---------|---------|
| Rust modules | `snake_case.rs` | `fs_manager.rs` |
| TypeScript modules | `kebab-case.ts` | `file-utils.ts` |
| Stores | `{domain}.store.ts` | `build.store.ts` |
| Store tests | `{domain}.store.test.ts`, or `{domain}.store.{topic}.test.ts` for focused suites | `logcat.store.dropped.test.ts` |
| Services | `{domain}.service.ts` | `project.service.ts` |
| Components | `PascalCase.tsx` | `DeviceSidebar.tsx` |
| Component tests | `{Name}.test.tsx` | `Button.test.tsx` |
| UI primitives | `ui/{Name}/{Name}.tsx` + `.module.css`, `.stories.tsx`, `.test.tsx`, `index.ts` | `ui/Button/` |
| Domain helpers | `{domain}-{topic}.ts` beside the components | `logcat-row-selection.ts` |
| Events | `{domain}:{event_name}` | `build:complete` |
| Generated bindings | Do not create manually | `BuildStatus.ts` |

---

## Known Gaps

Places where the code does not yet meet the rules above. Remove an entry when it is fixed.

- **`String` errors.** Most commands still return `Result<_, String>`; only about 16 return `AppError`.
- **Effective-root resolution is repeated.** The `gradle_root`-or-`project_root` lookup is copied inline in `commands/variant.rs`, `build.rs`, `device.rs`, and `health.rs` instead of one shared helper.
- **Data directory rebuilt by hand.** `lib.rs` joins `~/.keynobi/logs` itself instead of calling `settings_manager::data_dir()`.
- **Legacy settings fields.** `AdvancedSettings` (`tree_sitter_cache_size`, `lsp_*`, `navigation_history_depth`, and related fields), `LspSettings`, and `SystemHealthReport.lsp_system_dir_ok` belong to removed editor features and are still exported to the frontend.
- **Stale generated files.** `src-tauri/bindings/` holds old `LogEntry.ts`/`LogLevel.ts` exports that nothing uses.
- **Uncapped output reader.** `adb_manager::download_system_image` still reads `sdkmanager` output with `AsyncBufReadExt::lines()` instead of `CappedLines`.
- **Store naming.** `layoutViewer.store.ts` uses camelCase instead of kebab-case.
- **Typed factories are rarely used.** Only one test imports `src/test/factories/`; most tests build IPC data inline.
