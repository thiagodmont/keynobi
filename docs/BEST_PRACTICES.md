# Best Practices

Foundational engineering rules for Keynobi. This document explains the project-level principles that should rarely change. `CODE_PATTERN.md` covers concrete implementation patterns; `DOMAIN_PATTERNS.md` covers build, device, logcat, layout, and MCP details.

Update this file only when a broad architecture, security, reliability, or product principle changes.

---

## Core Principles

### Correctness Before Optimization

Write correct, observable behavior first. Optimize only after measurement shows a real bottleneck or a project target is at risk.

Current performance targets:

| Operation | Target |
|-----------|--------|
| Build variant discovery | < 500ms |
| Device list refresh | < 2s |
| ADB install feedback appears | < 1s after command |
| Logcat rendering | Smooth at 1000 lines/sec |
| Command palette open | < 50ms |

### Single Source of Truth

- Rust models are the source of truth for IPC types; TypeScript bindings are generated.
- Backend state is authoritative for project roots, devices, build state, and settings.
- Stores are frontend projections of backend or UI state.
- The action registry is the source of truth for command-palette actions.

Do not duplicate authoritative state. Derive it or fetch it from the owner.

### Explicit Over Clever

Make side effects obvious in names and structure. Keep invariants visible when the code cannot express them, especially around locks, process lifecycle, path validation, and event listeners.

### Fail Visibly

Security violations fail immediately with clear errors. User-actionable failures appear in the UI through toast, status, or panel state. Background failures the user cannot act on are logged with `tracing`.

---

## Security

### Path Boundaries

Every path from the frontend, MCP, or an external tool is untrusted. Validate paths against the effective project root before filesystem access.

The effective root is `gradle_root` when available, otherwise `project_root`. Use canonical paths so `..` and symlinks cannot escape the sandbox. Never use raw string prefix checks for security.

### Least Privilege

Tauri capabilities and IPC commands should expose only what the app needs. Prefer narrow commands and typed results over broad filesystem or shell access.

### Secrets

Do not store API keys, tokens, credentials, or private project data in frontend state, logs, telemetry, fixtures, or docs. Sensitive operations belong in Rust or trusted external tools.

---

## Performance

### Keep Work Off the UI Thread

Anything that can block the frontend for more than one frame belongs in Rust, a worker-like async flow, or a virtualized/batched UI path. CPU-heavy Rust work should use `tokio::task::spawn_blocking` instead of blocking the async executor.

### Bound Growing Data

Every long-lived in-memory collection must have an explicit cap. Examples include logcat entries, build logs, build history, recent projects, MCP activity, process maps, autocomplete candidates, and navigation history.

### Stream Incremental Results

Build output, logcat, device state, MCP activity, and long-running inspections should stream or batch incremental updates. Avoid large all-at-once payloads when partial results are useful.

### Hold Locks Briefly

Lock, clone or update the minimal state, then release before I/O, process work, event emission, or `await`. Never hold a Mutex across an `await`.

---

## Architecture

### Layering

Use the normal flow unless there is a documented exception:

```
Frontend components
  -> stores / frontend services
  -> src/lib/tauri-api.ts
  -> Tauri commands
  -> Rust services
```

Components do not call `invoke` directly. Commands validate and delegate. Rust services contain business logic and avoid Tauri dependencies.

### Domain Ownership

Organize by product domain, not generic technical buckets. Keep build, device, logcat, layout, MCP, settings, and project behavior near their domain-specific stores, services, components, commands, models, and tests.

### AI-First Interfaces

Prefer structured data over raw text for anything an AI or automation client may consume. MCP responses should be compact, typed, bounded, and directly actionable.

---

## Testing

Prioritize tests in this order:

1. Security boundaries: path validation, command input validation, shell/tool argument validation.
2. Backend services: build parsing, device parsing, logcat pipeline, UI hierarchy parsing, MCP tool behavior.
3. Frontend stores and services: state transitions, async ordering, cancellation, persistence.
4. Pure utilities: query parsing, formatting, filtering, matching.
5. UI rendering for shared components and high-risk flows.

Tests must isolate state, avoid ordering assumptions, and use typed factories for IPC-shaped data.

Benchmarks live under `src-tauri/benches/` and can be run with `cargo bench`. CI does not currently run Criterion benchmarks; use `npm run perf:collect` / `npm run perf:report` when performance-sensitive code changes.

---

## Error Handling

### Rust

- Services return typed errors where practical.
- Commands convert errors to `String` at the IPC boundary.
- Production Rust must not use `unwrap()`.
- `expect()` is allowed only for programmer-error invariants with a useful message.

### TypeScript

- Frontend services catch IPC failures and update UI state or toast.
- Stores hold error state; they should not throw during normal UI flows.
- Async listeners and timers must be cleaned up.

---

## Observability

Use `tracing` in Rust. Do not use `println!` / `eprintln!` for application logging.

Log level guidance:

- `error!`: feature failed or needs investigation.
- `warn!`: degraded behavior, app can continue.
- `info!`: lifecycle milestones.
- `debug!`: developer diagnostics.

GUI logging reads `KEYNOBI_LOG`:

```bash
KEYNOBI_LOG=keynobi_lib=debug npm run tauri dev
```

Headless MCP logging uses the standard tracing env filter (`RUST_LOG`) and writes to stderr so stdout remains reserved for MCP JSON-RPC.
