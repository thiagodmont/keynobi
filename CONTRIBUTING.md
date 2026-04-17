# Contributing to Keynobi

Thank you for helping improve Keynobi. This guide is the entry point; deeper rules live in `docs/` and [AGENTS.md](AGENTS.md).

## Before you write code

1. [AGENTS.md](AGENTS.md) — stack, IPC checklist, testing commands, how to add a feature end-to-end.
2. [docs/BEST_PRACTICES.md](docs/BEST_PRACTICES.md) — security, performance, bounded collections, AI-first design.
3. [docs/CODE_PATTERN.md](docs/CODE_PATTERN.md) — naming, stores, Tauri patterns, path canonicalization, testing gate.
4. [docs/DOMAIN_PATTERN.md](docs/DOMAIN_PATTERN.md) — build, logcat, device, MCP domain conventions.
5. [docs/USER_MANUAL.md](docs/USER_MANUAL.md) — what users see; update this when behavior or shortcuts change.

**Security:** Do not open public issues for vulnerabilities. See [SECURITY.md](SECURITY.md).

**Community:** [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## What CI enforces

Match [.github/workflows/ci.yml](.github/workflows/ci.yml) locally before opening a PR.

### Frontend (Node 22)

```bash
npm ci
npm run lint
npm run typescript:check
npm run test
```

### Rust (`src-tauri/`)

```bash
cd src-tauri
cargo test --lib --tests
cargo clippy -- -D warnings
cargo check --features telemetry
cargo clippy --features telemetry -- -D warnings
cargo test --lib --tests --features telemetry
```

### TypeScript bindings (`ts-rs`)

After any change under `src-tauri/src/models/` (or other types exported to TS), regenerate and commit bindings:

```bash
npm run generate:bindings
git diff src/bindings/   # should be empty after commit; CI fails if stale
```

You can also run `npm run check:bindings` to regenerate and assert a clean diff.

## Pull requests

- Keep PRs **small and focused** (one concern per PR when possible).
- Describe **what** changed and **why** (motivation / tradeoffs).
- **UI changes:** note how to verify (panel, menu path, shortcut). Screenshots help reviewers.
- Link a related **issue** when one exists.
- **Do not commit** API keys, tokens, machine-specific paths, or personal project data.

## Good first contributions

- Documentation, [docs/USER_MANUAL.md](docs/USER_MANUAL.md), or typo fixes in `README.md`.
- **Vitest** tests for `src/stores/*.store.ts` or pure helpers under `src/lib/`.
- **Rust** unit tests in `#[cfg(test)]` modules next to services under `src-tauri/src/services/`.

Large UI surfaces (for example `src/components/logcat/LogcatPanel.tsx`) are harder first issues because they mix layout, IPC, and state; consider starting with smaller components or stores.

## Session completion (maintainers & regular contributors)

When you establish a new pattern or ship user-visible behavior, align with [AGENTS.md](AGENTS.md) § Session Completion: update `docs/CODE_PATTERN.md`, `docs/DOMAIN_PATTERN.md`, or `docs/BEST_PRACTICES.md` when patterns change, and `docs/USER_MANUAL.md` when users need new docs.

## Questions

Open a [GitHub issue](https://github.com/thiagodmont/keynobi/issues) for design questions or unclear behavior. For repository layout, see [docs/CODE_PATTERN.md](docs/CODE_PATTERN.md#project-structure).
