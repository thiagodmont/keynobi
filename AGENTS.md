# Agent Instructions

## Tech Stack

- **Framework**: Tauri 2.0 (Rust backend + WKWebView frontend)
- **Frontend**: SolidJS + TypeScript + Vite
- **Code Editor**: CodeMirror 6
- **Backend**: Rust (tokio async runtime)
- **Parsing**: Tree-sitter (`tree-sitter-kotlin-ng`)
- **Search**: ripgrep library crates (`grep-regex`, `grep-searcher`, `grep-matcher`)
- **Language Intelligence**: JetBrains Kotlin/kotlin-lsp (download-on-demand)
- **IPC Types**: `ts-rs` (Rust → TypeScript auto-generation)
- **State Management**: SolidJS Stores (`createStore`, `produce`)
- **Testing**: Vitest (frontend), Rust `#[test]` / `tokio::test` (backend), Criterion (benchmarks)

## Before You Write Code

1. Read `docs/BEST_PRACTICES.md` — architectural principles, security rules, performance targets, and the AI-first design philosophy.
2. Read `docs/CODE_PATTERN.md` — concrete conventions: file naming, store patterns, IPC patterns, testing patterns.
3. Read `docs/DECISIONS.md` — understand why key architectural decisions were made before challenging them.
4. Read `docs/USER_MANUAL.md` — understand what the user sees and does, so new features integrate naturally.

## Key Rules

- **Path security**: Every file/LSP command that accepts a path must call `ensure_path_in_project` or `ensure_within_project`. Never use raw `starts_with()` without canonicalization.
- **Keybindings = Actions**: Use `registerKeyAndAction()` in `App.tsx`, never bare `registerKeybinding()`. Shortcuts that bypass the action registry are invisible in the command palette.
- **Navigation**: All jump-to-location actions must call `openFileAtLocation()` from `project.service.ts`, not `setActiveFile()` directly. This populates the navigation history stack.
- **IPC types**: Import from `@/bindings`, never redefine types that originate in Rust.
- **Mutex discipline**: Lock Rust state, clone what you need, drop the lock, then do I/O. Never hold a Mutex across an `await`.
- **Bounded collections**: Every in-memory collection that grows must have an explicit cap (see `BEST_PRACTICES.md`).
- **No `unwrap()` in production Rust**: Use `?` or `.map_err(...)`. `expect()` is allowed only for programmer-error invariants.

## Testing Instructions

### Frontend
```bash
npm run test              # run all frontend tests once
npm run test:watch        # watch mode during development
npm run test:ui           # Vitest browser UI
```

### Rust
```bash
cd src-tauri
cargo test                 # run all Rust unit tests
cargo bench                # run Criterion benchmarks (writes to target/criterion/)
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

1. **Rust model** (`src-tauri/src/models/`): Define the data type with `#[derive(Serialize, Deserialize, Clone, TS)]` and `#[serde(rename_all = "camelCase")]`.
2. **Rust service** (`src-tauri/src/services/`): Implement business logic with no Tauri dependency.
3. **Rust command** (`src-tauri/src/commands/`): Thin validation + delegation to service. Add path security if the command accepts file paths.
4. **Register command** (`src-tauri/src/lib.rs`): Add to `invoke_handler!` and `.manage()` if a new state is needed.
5. **Regenerate bindings**: `npm run generate:bindings`.
6. **IPC wrapper** (`src/lib/tauri-api.ts`): Add a typed wrapper calling `invoke`.
7. **Store** (`src/stores/{domain}.store.ts`): Add reactive state if the feature needs persistent UI state.
8. **Component** (`src/components/{domain}/`): Build the UI reading from the store.
9. **Action** (`src/App.tsx`): Register keyboard shortcut + command palette entry via `registerKeyAndAction`.
10. **Tests**: Add Rust unit tests in the service's `#[cfg(test)]` module; add Vitest tests for the store.

## Session Completion

At the end of every development session:

- Record new architectural decisions in `docs/DECISIONS.md` (append-only, never delete).
- Update `docs/CODE_PATTERN.md` when a new code pattern is established.
- Update `docs/BEST_PRACTICES.md` if foundational architecture or principles change.
- Update `docs/USER_MANUAL.md` when new user-visible features or keyboard shortcuts are added.
