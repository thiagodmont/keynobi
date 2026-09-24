# Agent Instructions

## Tech Stack

- **Framework**: Tauri 2 (Rust backend + WKWebView frontend), macOS only
- **Frontend**: SolidJS + TypeScript + Vite, CSS Modules
- **Backend**: Rust (tokio async runtime)
- **MCP**: `rmcp` (stdio server, headless via `keynobi --mcp`)
- **Parsing**: `regex` (Gradle output), `roxmltree` (UI Automator XML)
- **IPC Types**: `ts-rs` (Rust → TypeScript auto-generation into `src/bindings/`)
- **State Management**: SolidJS Stores (`createStore`, `produce`)
- **Telemetry**: optional Sentry (`telemetry` Cargo feature, opt-in at runtime)
- **Testing**: Vitest (frontend), Playwright (web-mode e2e, Storybook, visual), Rust `#[test]` / `tokio::test` (backend), Criterion (benchmarks)
- **Design system**: `src/components/ui` + Storybook

## Before You Write Code

1. Read `references/BEST_PRACTICES.md` — architectural principles, security rules, performance targets, and the AI-first design philosophy.
2. Read `references/CODE_PATTERN.md` and `references/DOMAIN_PATTERNS.md` — concrete conventions: file naming, store patterns, IPC patterns, testing patterns.
3. Read `references/USER_MANUAL.md` — understand what the user sees and does, so new features integrate naturally.
4. For MCP work, read `references/MCP_SERVER.md`. For UI work, read `references/DESIGN_SYSTEM.md`.

Each reference doc ends with **Known Gaps**: places where the code does not yet meet the rules. Do not copy a known gap into new code.

## Key Rules

- **Path security**: Every command that accepts a path must validate it against the project root using canonicalization (see `CODE_PATTERN.md` §Path Security). Never use raw `starts_with()` without canonicalization.
- **Keybindings = Actions**: Use `registerKeyAndAction()` in `App.tsx`, never bare `registerKeybinding()`. Shortcuts that bypass the action registry are invisible in the command palette.
- **One implementation per behavior**: Tauri commands and MCP tools call the same service function. Never copy logic between `commands/` and `mcp_server.rs`.
- **Process model**: `keynobi --mcp` runs as a separate process with its own state. Do not assume the GUI sees MCP builds, logcat, or project changes.
- **Input validation**: Validate identifiers with `utils/validation.rs` and paths with `utils/path.rs`. Anything sent through `adb shell` is re-parsed by the device shell.
- **IPC types**: Import from `@/bindings`, never redefine types that originate in Rust.
- **Mutex discipline**: Lock Rust state, clone what you need, drop the lock, then do I/O. Never hold a Mutex across an `await`.
- **Bounded collections**: Every in-memory collection that grows must have an explicit cap (see `BEST_PRACTICES.md`).
- **No `unwrap()` in production Rust**: Use `?` or `.map_err(...)`. `expect()` is allowed only for programmer-error invariants.
- **Errors at the IPC boundary**: New commands return `Result<T, AppError>`.
- **Never touch real user data in tests**: `cargo test` (and `npm run generate:bindings`, which runs it) must not read or write `~/.keynobi`. Confirm `set_data_dir_override` exists in `settings_manager.rs` before running Rust tests.

## Working rules

- Add comments only when they are necessary and provide value. Whenever possible, write code that is self-explanatory.
- Keep comments clear, concise, and direct. Avoid long comments, as they are less likely to be read.
- Avoid referencing external documents or tool name in branch names, comments or code, since tool can change and those documents may be moved, renamed, or deleted.

## Git Workflow

- Branch naming: `<type>/<short-slug>` (e.g., `feature/fix-header`)
- Allowed types: `feature`, `bugfix`, `hotfix`, `chore`
- Slug format: lowercase kebab-case, concise and descriptive


## Testing Instructions

### Frontend
Run these after development is complete. All of them must succeed.
```bash
npm run lint              # ESLint
npm run format:check      # Prettier
npm run typescript:check  # TypeScript
npm run test              # all Vitest tests (includes the IPC contract test)
npm run test:e2e          # Playwright web-mode end-to-end tests
```

**Design system / UI refactor work:** also run `npm run test:ds` (TypeScript, lint, Storybook build, Storybook smoke and axe checks). See `references/CODE_PATTERN.md` § Verification Gate.

### Rust
```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib --tests   # unit + integration tests
cargo bench                # Criterion benchmarks (writes to target/criterion/)
```

### Performance Metrics
```bash
npm run perf:collect       # capture current metrics snapshot
npm run perf:report        # compare latest vs previous snapshot
```

### Regenerate TypeScript Bindings
Run after any Rust model type change:
```bash
npm run generate:bindings
```

## Adding a New Feature

1. **Rust model** (`src-tauri/src/models/`): Define the data type with `#[derive(Debug, Clone, Serialize, Deserialize, TS)]`, `#[serde(rename_all = "camelCase")]`, and `#[ts(export, export_to = "../../src/bindings/")]`.
2. **Rust service** (`src-tauri/src/services/`): Implement business logic. Take `Option<AppHandle>` only if it must emit GUI events, so the headless MCP server can reuse it.
3. **Rust command** (`src-tauri/src/commands/`): Thin validation + delegation to the service, returning `Result<T, AppError>`. Add path security if the command accepts file paths.
4. **Register command** (`src-tauri/src/lib.rs`): Add to `generate_handler![...]` and `.manage()` if a new state is needed.
5. **Regenerate bindings**: `npm run generate:bindings`.
6. **IPC wrapper** (`src/lib/tauri-api.ts`): Add a typed wrapper calling `invoke`.
7. **Mock backend** (`src/test/mock-backend/`): Add a handler. The IPC contract test fails without it.
8. **Store** (`src/stores/{domain}.store.ts`): Add reactive state if the feature needs persistent UI state.
9. **Component** (`src/components/{domain}/`): Build the UI from design-system primitives, reading from the store.
10. **Action** (`src/App.tsx`): Register keyboard shortcut + command palette entry via `registerKeyAndAction`.
11. **MCP tool** (optional, `services/mcp_server.rs`): Wrap the same service function. Follow `references/MCP_SERVER.md` § Adding or Changing a Tool.
12. **Tests**: Add Rust unit tests in the service's `#[cfg(test)]` module; add Vitest tests for the store.

## Session Completion

At the end of every development session:

- Update `references/CODE_PATTERN.md` and `references/DOMAIN_PATTERNS.md` when a new code pattern is established.
- Update `references/BEST_PRACTICES.md` if foundational architecture or principles change.
- Update `references/USER_MANUAL.md` when new user-visible features or keyboard shortcuts are added.
- Update `references/MCP_SERVER.md` when an MCP tool, prompt, resource, or limit changes.
- Update `references/DESIGN_SYSTEM.md` when a primitive, token, or accessibility rule changes.
- Remove a **Known Gaps** entry when your change fixes it; add one when you knowingly leave a rule unmet.

Human contributors: the same expectations are summarized for PRs in [CONTRIBUTING.md](CONTRIBUTING.md) (CI commands, bindings, and doc updates).
