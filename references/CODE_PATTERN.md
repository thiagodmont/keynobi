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
- `src/lib/telemetry/` - browser-side crash reporting (consent gate and outgoing-event allowlist).
- `src/utils/` - small browser helpers (clipboard, debounce).
- `src/styles/` - `theme.css` tokens and `global.css`.
- `src/bindings/` - generated TypeScript bindings; do not edit manually.
- `src/test/` - Vitest setup (`setup.ts`), typed factories, the mock backend used by web-mode e2e, and `ipc-fixtures/` (payloads serialized by the Rust backend; generated).

Backend:

- `src-tauri/src/lib.rs` - app setup, managed state, command registration, shutdown.
- `src-tauri/src/main.rs` - entry point; `--mcp` attaches to the running app or starts a standalone MCP server.
- `src-tauri/src/commands/` - thin Tauri command handlers.
- `src-tauri/src/services/` - Rust business logic.
- `src-tauri/src/models/` - Rust IPC models exported with `ts-rs`, plus `AppError`.
- `src-tauri/src/utils/` - shared helpers: `path.rs` (filesystem boundaries), `validation.rs` (identifiers), `line_reader.rs` (bounded process-output lines), `process.rs` (deadlines for one-shot commands), `device_shell.rs` (`adb shell` quoting), and `cli_lookup.rs` (finding user-installed command-line tools).
- `src-tauri/tests/` - integration tests (`build_integration.rs`, `ipc/`, `fixtures/mock_gradlew`), `ipc_fixtures.rs` (writes `src/test/ipc-fixtures/`), and `mcp_headless.rs`, which drives the real `keynobi --mcp` binary through `headless/`, standalone and attached to a test listener.
- `src-tauri/benches/` - Criterion benchmarks.
- `src-tauri/capabilities/` - Tauri permission grants.

Tooling:

- `.storybook/` - Storybook configuration for the design system.
- `e2e/` - Playwright web-mode tests: `smoke/`, `ipc/`, `fixtures/`, plus separately configured `storybook/` and `visual/` suites.
- `vite-plugin-tauri-mock.ts` - replaces Tauri APIs with `src/test/mock-backend` when `VITE_E2E` is set.
- `skills/keynobi/SKILL.md` - the Keynobi agent skill, built into the binary and served as `keynobi://skill` (see `MCP_SERVER.md` § Agent skill).
- `scripts/` - release, versioning, metrics, packaging, the IPC contract and payload tests, and the MCP smoke test (`mcp-smoke.mjs`).

---

## Rust Patterns

### State

Each domain owns its own state struct, wrapped as a newtype around `Arc<tokio::sync::Mutex<...Inner>>` and registered with `.manage()` in `lib.rs`. Avoid unrelated fields in the same Mutex because it creates unnecessary contention.

Implement `Default` by delegating to `new()` when a state type has construction logic.

Services that the standalone MCP server reuses must work without an `AppHandle`: take `Option<AppHandle>` and skip GUI events when it is `None`.

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

The exception is a tokio mutex whose job is to serialize the work itself, such as the per-device UI Automator lock (`ui_automator_lock::acquire`): it is held across the whole operation on purpose, bounded by a deadline, and says so where it is defined.

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
- `utils::path::resolve_project_file(root, relative)` for fixed project files the app reads, such as `app/build.gradle.kts`.
- `utils::path::validate_apk_within_build_outputs(...)` for APK installs: the APK must be under an application module's `build/outputs`, and that directory must itself resolve inside the root.

Each returns the canonical path; use that path afterwards, not the one you checked.

Never use raw `path.starts_with(root)` for security.

### Identifier Validation and Process Arguments

- Validate Gradle tasks, device serials, and package names with `utils/validation.rs` (`validate_gradle_task`, `validate_device_serial`, `validate_package_name`). Put new identifier validators there.
- Spawn processes with argument vectors (`tokio::process::Command::new(bin).args([...])`). Never build a host shell string.
- Run one-shot external commands (adb queries, `adb install`, aapt2, avdmanager, `sdkmanager --list`, version probes) with `utils::process::output_with_timeout(cmd, DEADLINE)`, never a bare `.output().await`. It sets `kill_on_drop`, so a timed-out child is killed and reaped, and returns an `io::Error` of kind `TimedOut`. Pick a named deadline from `utils/process.rs` (add one there for a new kind of call) and turn errors into messages with `describe_failure(what, &e, hint)`, which adds what to try on timeout (for adb, `ADB_UNRESPONSIVE_HINT`). Long-lived processes that stream output (logcat, Gradle, the emulator, `sdkmanager` downloads) are not one-shot and do not get a total deadline.
- Read streamed process output with `utils::line_reader::CappedLines`, not `AsyncBufReadExt::lines()`. It keeps at most `MAX_LINE_BYTES` per line, discards the rest up to the next newline with a `… [truncated N bytes]` marker, replaces invalid UTF-8 instead of failing, and is safe to use as a `tokio::select!` branch.
- Never wait for a child's output to reach EOF before waiting for its exit: a descendant that inherited the pipes (a Gradle daemon, `adb` server) can hold them open forever. `process_manager` waits for exit alongside the reads and then drains remaining output for at most `POST_EXIT_DRAIN` (2 s).
- Signal a child only through its `tokio::process::Child` handle, from the code that owns it and before it is reaped (`start_kill()`, `kill_on_drop`, or `libc` on `child.id()`, which is `None` once reaped). A stored PID can belong to an unrelated process by the time it is signalled. Stop `process_manager` children with `cancel` or `ProcessManager::shutdown_all`.
- Values sent through `adb shell` are re-parsed by the device shell. Pass every non-literal argument through `utils::device_shell::quote_device_shell_arg` (the `run_adb_shell` and `adb_cmd` helpers already do). Test new call sites with `device_shell::test_support::fake_adb`, which parses arguments the way the device does.

### Persistence

- Write files atomically: write a temporary sibling named with `settings_manager::unique_tmp_path`, then `rename` it over the target. See `settings_manager.rs` and `build_runner.rs`. When replacing a file the user owns (a build file), create the temporary file with `create_new`, give it the original's permissions before the rename, and remove it when any step fails (`project_app_info::write_atomically`).
- Copy a large file into the data directory by streaming it (hash and write in bounded chunks) to a `unique_tmp_path` file without the data lock, then publish it with a rename and record it under the lock in one critical section, so another process never prunes it in between. Tie the temporary file to a guard that removes it when it is not published (`mapping_snapshots::PreparedMapping`).
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

A 64-bit integer crosses IPC as a JSON number, but `ts-rs` types it `bigint`. Mark every `u64`/`i64` field `#[ts(type = "number")]` (`"number | null"` for an `Option`, a tuple of `number` for an array), and only for values that stay below 2^53 (counts, IDs, durations, byte sizes). A `bigint` cannot be sent at all: invoke arguments are serialized as JSON, which throws on one, so the command never reaches Rust. An `Option` field with `skip_serializing_if` is absent rather than `null` on the wire; mark it `#[ts(optional)]` so the binding says so.

#### Payload fixtures

`src-tauri/tests/ipc_fixtures.rs` serializes sample values of every type the frontend invokes or listens for, and every event payload, into `src/test/ipc-fixtures/fixtures.ts`. It fails when that file is stale; regenerate it with:

```bash
npm run generate:ipc-fixtures
```

The fixtures are checked three ways:

- **Compile time:** each sample `satisfies Wire<T>` (the binding with `bigint` read as `number`), so `tsc` fails when a binding and the serializer disagree.
- **`scripts/ipc-payloads.test.mjs`:** each sample has exactly the fields its binding declares; every type used in an `invoke<T>`/`listen<T>` has samples; no field is typed `bigint`; every event the frontend listens for is emitted in Rust, has a payload fixture, and is listened for with that payload type.
- **Mock backend:** the same test calls every mock command whose response is a binding type and drives every mock event, and compares each payload's shape (keys, JSON kinds, `null`s) with the samples.

When you add a command or event that returns a new type, add samples for it, with every `Option` both present and absent.

### Errors and Logging

- Use `thiserror` for structured service errors (`AppError`, `FsError` in `models/error.rs`).
- Do not use `unwrap()` in production Rust. `expect("why this cannot fail")` is acceptable for programmer-error invariants such as static regexes.
- Use `tracing` macros for logs.
- GUI log filtering uses `KEYNOBI_LOG`; `keynobi --mcp` uses `RUST_LOG`.

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
- **Channels** carry a stream owned by one request: SDK image download progress. Build output is an event stream instead, because the app shows builds it did not request (an agent's).
- **Events** carry app-wide notifications and batched streams not tied to one request.

| Event | Payload / purpose |
|-------|-------------------|
| `build:started` | A build started, from the app or an agent: run ID, task, `origin`. |
| `build:lines` | Batched output of one run (every 50 ms, up to 500 lines). |
| `build:complete` | Build finished, failed, or was cancelled, with its history `recordId`, `origin`, and `cancelledBy`. |
| `build:launch_timing` | Display times of a Run App launch arrived after the launch returned and were recorded on the build (`LaunchTimingEvent`). |
| `device:list_changed` | Connected devices changed; payload is `DeviceListChangedEvent`. |
| `logcat:entries` | Batched processed log entries (every 100 ms, up to 500). |
| `logcat:cleared`, `logcat:reconnecting`, `logcat:stopped` | Logcat stream lifecycle. |
| `mcp:sessions_changed` | MCP clients attached to the app changed; payload is the full `McpAttachedSession[]`. |
| `settings:corrupted` | Settings file was unreadable and was reset. |
| `monitor://stats` | App memory and log-folder size (`MonitorStats`), every 5 s. |

Name new events `{domain}:{event_name}`. Export the payload type to `@/bindings` and add the event to `src-tauri/tests/ipc_fixtures.rs` (see [Payload fixtures](#payload-fixtures)). Always call the event `unlisten()` in cleanup.

---

## Actions and Keybindings

Every keyboard shortcut that should appear in the command palette must be registered with `registerKeyAndAction()` in `App.tsx`. It wraps `registerAction()` (`lib/action-registry.ts`) and `registerKeybinding()` (`lib/keybindings.ts`).

Use `registerAction()` for command-palette actions without shortcuts.

Do not use bare `registerKeybinding()` for app commands unless the shortcut is intentionally hidden from the command palette. Update the shortcut table in `USER_MANUAL.md` in the same change.

Registered shortcuts do not run while a modal dialog (`aria-modal="true"`) or a menu (`role="menu"`) is open; the key is still `preventDefault`ed so the webview does not act on it. A new overlay must carry those attributes, or shortcuts will run behind it (see `DESIGN_SYSTEM.md` § Keyboard Contract).

---

## Testing Patterns

### Vitest

- Tests live next to the code they cover.
- `src/test/setup.ts` mocks Tauri APIs with `vi.mock`. Override per test with `vi.mocked(...)`.
- Stub every IPC call a test makes (`vi.mocked(invoke).mockResolvedValueOnce(...)` or `mockImplementation`). An unstubbed `invoke` rejects and fails the test, even when the code under test catches the rejection.
- `setup.ts` also fails a test whose `invoke` arguments contain a `bigint`, which the real IPC layer cannot send; the mock backend (`handleInvoke`) serializes arguments the same way in e2e.
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
- IPC payload samples live in `tests/ipc_fixtures.rs`; see [Payload fixtures](#payload-fixtures).
- End-to-end MCP behavior goes in `tests/mcp_headless.rs`. `headless::Sandbox` gives each server process its own `HOME` (so its data dir is a temp dir, created under `/tmp` to keep the socket path short), a fake SDK whose `adb` records its arguments, and a project whose `gradlew` runs the script you give it; `Sandbox::start()` launches `keynobi --mcp` and completes the MCP handshake. `headless::TestApp::listen` serves attach requests on the sandbox's socket from the test process, standing in for the app.
- Security validators need negative tests: traversal, symlinks, option-shaped values, and shell metacharacters.
- Tests must pass on a loaded machine (parallel builds and test runs). Wait for a condition (a channel, a file the fake writes, a polled state) instead of sleeping for a fixed time, and give a wait that only catches a hang a generous bound (30 s). macOS checks a newly written executable on its first run, which can take several seconds under load: a test that runs a fake script against a short deadline runs it once first with `utils::process::test_support::run_once` (`headless::run_once` in integration tests).

### Packaged binary

`scripts/mcp-smoke.mjs <path/to/keynobi>` runs `keynobi --mcp` against a throwaway project and `HOME`, completes `initialize`, `tools/list`, and `get_project_info` over stdio, and requires a standalone session that opened the project and a clean exit. CI runs it on the debug binary after the Rust tests; the release runs it on the binary inside each DMG before publishing. `scripts/mcp-smoke.test.mjs` covers its failure modes with fake servers, and also runs it on `target/debug/keynobi` when that exists.

### Verification Gate

Run the checks that match your change before handoff:

| Change | Command |
|--------|---------|
| Any frontend change | `npm run lint && npm run format:check && npm run typescript:check && npm test` |
| Design system, shared styling, tokens, broad UI refactor | Frontend checks + `npm run test:ds` |
| User flows or IPC | `npm run test:e2e` |
| Rust | `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib --tests` |
| Rust models | `npm run check:bindings && npm run generate:ipc-fixtures`, then commit both |

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

- **One-shot commands without the timeout helper.** `logcat::seed_pid_map_from_ps` has no deadline. The login-shell probes in `settings_manager.rs` wrap `.output()` in `tokio::time::timeout` without `kill_on_drop`, so a timed-out child keeps running. `commands/variant.rs` has its own equivalent of the helper.
- **`String` errors.** Most commands still return `Result<_, String>`; only about 16 return `AppError`.
- **Effective-root resolution is repeated.** The `gradle_root`-or-`project_root` lookup is copied inline in `commands/variant.rs`, `build.rs`, `device.rs`, and `health.rs` instead of one shared helper.
- **Data directory rebuilt by hand.** `lib.rs` joins `~/.keynobi/logs` itself instead of calling `settings_manager::data_dir()`.
- **Legacy settings fields.** `AdvancedSettings` (`tree_sitter_cache_size`, `lsp_*`, `navigation_history_depth`, and related fields), `LspSettings`, and `SystemHealthReport.lsp_system_dir_ok` belong to removed editor features and are still exported to the frontend.
- **Stale generated files.** `src-tauri/bindings/` holds old `LogEntry.ts`/`LogLevel.ts` exports that nothing uses.
- **Uncapped output reader.** `adb_manager::download_system_image` still reads `sdkmanager` output with `AsyncBufReadExt::lines()` instead of `CappedLines`.
- **Store naming.** `layoutViewer.store.ts` uses camelCase instead of kebab-case.
- **Typed factories are rarely used.** Only one test imports `src/test/factories/`; most tests build IPC data inline.
- **Event payload types are declared by hand.** `tests/ipc_fixtures.rs` names each event's payload type; nothing ties it to the value the emit site passes.
- **Some mock checks are vacuous.** Mock commands that return empty lists (`get_build_errors`, `get_mcp_activity`, `get_logcat_context_entries` without an anchor, and the AVD and system-image lists) have no elements to compare, and the mock never emits `logcat:reconnecting`, `logcat:stopped`, `monitor://stats`, or `settings:corrupted`.
