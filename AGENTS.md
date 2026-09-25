# AGENTS.md

Keynobi is a macOS companion app for Android development: Gradle builds, logcat, devices, UI hierarchy, and an MCP server for AI clients. Tauri 2 (Rust, tokio) backend, SolidJS + TypeScript + Vite frontend, `ts-rs` for IPC types, `rmcp` for MCP.

## Commands

```bash
npm ci                     # install
npm run tauri dev          # run the app (first Rust build takes minutes)
npm run dev:web            # frontend only, against the mock backend
npm run generate:bindings  # after changing Rust models; commit src/bindings/
```

## Where to Look

| Task | Read |
|------|------|
| Any change | `references/BEST_PRACTICES.md`, `references/CODE_PATTERN.md` |
| Build, logcat, devices, layout, settings | `references/DOMAIN_PATTERNS.md` |
| MCP tools, prompts, resources | `references/MCP_SERVER.md` |
| UI components, styling, accessibility | `references/DESIGN_SYSTEM.md` |
| User-visible behavior, shortcuts | `references/USER_MANUAL.md` |

Each reference doc ends with **Known Gaps**: rules the code does not meet yet. Do not copy a known gap into new code.

## Key Rules

- **Layering**: components → stores/services → `src/lib/tauri-api.ts` → Tauri commands → Rust services. Only `tauri-api.ts` calls `invoke`; commands validate and delegate.
- **IPC contract**: a command change updates the Rust handler, `generate_handler!` in `lib.rs`, the `tauri-api.ts` wrapper, and the mock backend together. After changing Rust models, run `npm run generate:bindings`; import IPC types from `@/bindings` and never edit generated files.
- **One implementation per behavior**: Tauri commands and MCP tools call the same service function. Never copy logic into `mcp_server.rs`.
- **Process model**: `keynobi --mcp` attaches to the running app when it can (the app serves the session on its own state) and otherwise runs standalone with its own state, which the GUI does not see. See `references/MCP_SERVER.md` § Modes.
- **Untrusted input**: validate identifiers with `utils/validation.rs` and paths with `utils/path.rs` (canonicalized, never raw `starts_with`). Arguments to `adb shell` are re-parsed by the device shell.
- **Rust**: no `unwrap()` in production code; new commands return `Result<T, AppError>`; never hold a Mutex across `.await`; every growing collection has a named cap.
- **Frontend**: register shortcuts with `registerKeyAndAction()` in `App.tsx`; build UI from `@/components/ui` primitives and theme tokens.
- **User data**: tests must never read or write `~/.keynobi`. Unit tests get a temp data directory automatically (`cfg(test)` in `settings_manager`); integration tests must call `tests/common::isolate_data_dir()` first. CI fails if a test creates `~/.keynobi`.

## Code Style

- Match the surrounding code. Formatting is enforced by Prettier and `rustfmt`.
- Comment only when the code cannot explain itself; keep comments short.
- Do not mention external documents or tool names in code, comments, or branch names.

## Before You Finish

Run the checks for what you changed; all must pass.

```bash
npm run lint && npm run format:check && npm run typescript:check && npm test
npm run test:ds     # design system, shared styles, or tokens
npm run test:e2e    # user flows or IPC
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib --tests
cd src-tauri && cargo clippy --features telemetry -- -D warnings && cargo test --lib --tests --features telemetry  # telemetry code
```

Update the reference doc that owns what you changed (see [Where to Look](#where-to-look)). Remove a Known Gaps entry when you fix it; add one when you knowingly leave a rule unmet.

## Git and PRs

- Branches: `<type>/<short-kebab-slug>`, for example `fix/logcat-reconnect`.
- Commits and PR titles: [Conventional Commits](https://www.conventionalcommits.org/), `type(scope): summary`. Types: `feat`, `fix`, `docs`, `refactor`, `test`, `ci`, `chore`, `style`, `perf`. The changelog is generated from them.
- Fill in `.github/pull_request_template.md`. Keep PRs to one concern.
- Never commit `docs/` (local planning files), secrets, or machine-specific paths.
- Do not bump versions, tag, or run release scripts unless asked.

## Security

Report vulnerabilities privately (see `SECURITY.md`), never in a public issue or PR.
