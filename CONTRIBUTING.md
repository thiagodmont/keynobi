# Contributing to Keynobi

Thank you for helping improve Keynobi. This guide is the entry point; deeper rules live in `references/` and [AGENTS.md](AGENTS.md).

## Before you write code

1. [AGENTS.md](AGENTS.md) — key rules, commands, checks to run, and Git conventions. For adding a command end to end, see [references/CODE_PATTERN.md](references/CODE_PATTERN.md#commands).
2. [references/BEST_PRACTICES.md](references/BEST_PRACTICES.md) — security, performance, bounded collections, AI-first design.
3. [references/CODE_PATTERN.md](references/CODE_PATTERN.md) — naming, stores, Tauri patterns, path canonicalization, testing gate.
4. [references/DOMAIN_PATTERNS.md](references/DOMAIN_PATTERNS.md) — build, logcat, device, MCP domain conventions.
5. [references/USER_MANUAL.md](references/USER_MANUAL.md) — what users see; update this when behavior or shortcuts change.
6. [references/MCP_SERVER.md](references/MCP_SERVER.md) — MCP tools, error model, limits, and security model.
7. [references/DESIGN_SYSTEM.md](references/DESIGN_SYSTEM.md) — UI primitives, tokens, accessibility, and Storybook.

**Security:** Do not open public issues for vulnerabilities. See [SECURITY.md](SECURITY.md).

**Community:** [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## What CI enforces

Match [.github/workflows/ci.yml](.github/workflows/ci.yml) locally before opening a PR.

### Frontend (Node 22)

```bash
npm ci
npm run lint
npm run format:check
npm run typescript:check
npm run test
```

### Design System

```bash
npx playwright install chromium
npm run test:ds    # TypeScript, lint, Storybook build, Storybook smoke + axe checks
```

### E2E

```bash
npm run test:e2e   # Playwright against the web-mode mock backend
```

### Rust (`src-tauri/`, toolchain pinned in `rust-toolchain.toml`)

```bash
cd src-tauri
cargo test --lib --tests
cargo check --features telemetry
cargo clippy --features telemetry -- -D warnings
cargo test --lib --tests --features telemetry
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Rust tests must never touch your real `~/.keynobi`. Unit tests are redirected to a temp directory automatically. Integration tests under `tests/` must call `common::isolate_data_dir()` (or use `common::isolated_build_state()`) before loading or saving any persisted state. CI fails if a test creates `~/.keynobi`.

The Husky pre-commit hook runs `lint-staged` (ESLint + Prettier), `tsc --noEmit`, and `cargo clippy -- -D warnings`.

### TypeScript bindings (`ts-rs`)

After any change under `src-tauri/src/models/` (or other types exported to TS), regenerate and commit bindings. This runs only the `ts-rs` export tests (`cargo test --lib export_bindings`):

```bash
npm run generate:bindings
git diff src/bindings/   # should be empty after commit; CI fails if stale
```

You can also run `npm run check:bindings` to regenerate and assert a clean diff.

## Pull requests

- Keep PRs **small and focused** (one concern per PR when possible).
- Describe **what** changed and **why** (motivation / tradeoffs).
- **UI changes:** note how to verify (panel, menu path, shortcut). Screenshots help reviewers.
- **New Tauri command:** add the Rust handler, the `tauri-api.ts` wrapper, and a mock-backend handler; `scripts/ipc-contract.test.mjs` fails otherwise.
- Link a related **issue** when one exists.
- **Do not commit** API keys, tokens, machine-specific paths, or personal project data.

## Good first contributions

- Documentation, [references/USER_MANUAL.md](references/USER_MANUAL.md), or typo fixes in `README.md`.
- **Vitest** tests for `src/stores/*.store.ts` or pure helpers under `src/lib/`.
- **Rust** unit tests in `#[cfg(test)]` modules next to services under `src-tauri/src/services/`.

Large UI surfaces (for example `src/components/logcat/LogcatPanel.tsx`) are harder first issues because they mix layout, IPC, and state; consider starting with smaller components or stores.

## Session completion (maintainers & regular contributors)

When you establish a new pattern or ship user-visible behavior, align with [AGENTS.md](AGENTS.md) § Before You Finish: update `references/CODE_PATTERN.md`, `references/DOMAIN_PATTERNS.md`, or `references/BEST_PRACTICES.md` when patterns change, `references/MCP_SERVER.md` or `references/DESIGN_SYSTEM.md` when those surfaces change, and `references/USER_MANUAL.md` when users need new docs. Remove a **Known Gaps** entry when your change fixes it.

## Questions

Open a [GitHub issue](https://github.com/thiagodmont/keynobi/issues) for design questions or unclear behavior. For repository layout, see [references/CODE_PATTERN.md](references/CODE_PATTERN.md#repository-layout).
