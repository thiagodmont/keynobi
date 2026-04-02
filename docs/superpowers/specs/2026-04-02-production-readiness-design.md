# Production Readiness Design — Android Dev Companion

**Date:** 2026-04-02  
**Status:** Approved  
**Scope:** Full hardening before public launch — security, error handling, testing, code quality, and production infrastructure

---

## Overview

This spec defines all work required to bring the Android Dev Companion (Tauri 2.0 + SolidJS) from a well-architected beta to a production-ready application. Work is organized into three severity-tiered waves with clear release gates.

The project is structurally sound: good separation of concerns, atomic writes, mutex discipline, ring buffer design, and a clean IPC boundary. The gaps are concentrated in security hardening, error handling structure, test coverage, and production infrastructure.

---

## Architecture

No architectural changes. The existing three-layer architecture (SolidJS frontend → Tauri IPC → Rust services) is retained. Improvements operate within existing module boundaries except where explicitly noted (e.g., extracting `build_parser.rs` from `build_runner.rs`).

New additions:
- `src-tauri/src/utils/path.rs` — shared path validation helper
- `src-tauri/src/models/error.rs` — extended `AppError` enum (replaces ad-hoc string errors)
- `src/bindings/AppError.ts` — generated TS binding for `AppError`

---

## Wave 1 — Blocking (must fix before any release)

These are hard blockers: active security vulnerabilities and data corruption risks.

### S-1 — Fix command injection in `studio.rs`

**File:** `src-tauri/src/commands/studio.rs`  
**Problem:** `open_in_studio` builds a shell string: `format!("studio --line {line} '{abs_path}'")` and executes via `sh -lc`. A filename containing a single quote followed by shell metacharacters breaks the escape and allows arbitrary command execution.  
**Fix:** Remove the shell entirely. Invoke `tokio::process::Command::new("studio").args(["--line", &line.to_string(), &abs_path])` directly. No shell, no injection surface.

### S-2 — Enable Content Security Policy

**File:** `src-tauri/tauri.conf.json`  
**Problem:** `"csp": null` leaves the WKWebView unprotected against XSS through any compromised frontend dependency.  
**Fix:** Set `"csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';"`.

### S-3 — Restrict Tauri filesystem permissions

**File:** `src-tauri/capabilities/default.json`  
**Problem:** `fs:scope-home-recursive` grants read/write/delete to the entire home directory. Only needs: `~/.keynobi/`, configured SDK path, and opened project path.  
**Fix:** Replace `fs:scope-home-recursive` with explicit scopes: `~/.keynobi/**` (always), plus register the SDK path and project path as allowed scopes at runtime when the user configures them. Keep `fs:allow-remove` scoped to `~/.keynobi/**` only.

### S-4 — Input validation for Gradle task names and device serials

**Files:** `src-tauri/src/commands/build.rs`, `src-tauri/src/commands/device.rs`  
**Problem:** Gradle task names and ADB device serials flow into shell arguments with no validation.  
**Fix:** Add allowlist validation (alphanumeric + `:-_.` for tasks; alphanumeric + `:-_.` for serials) before use. Return `AppError::InvalidInput` on failure.

### D-1 — Implement graceful shutdown

**File:** `src-tauri/src/lib.rs`  
**Problem:** On app close, in-flight builds and ADB processes are abandoned mid-run. No cleanup hook exists.  
**Fix:** Register a `on_window_event` handler for `WindowEvent::CloseRequested`. On close: send SIGTERM to any running build via `ProcessManager`, stop logcat, allow ADB polling loop to drain, then permit close. Add a 3-second timeout to avoid hanging on shutdown.

### D-2 — Surface settings corruption to user

**File:** `src-tauri/src/services/settings_manager.rs`  
**Problem:** Corrupted `settings.json` triggers silent `unwrap_or_default()`. User loses all configuration without knowing.  
**Fix:** Detect parse failure, log a structured warning, emit a Tauri event to the frontend. Frontend shows a Toast: "Settings file was corrupted — reset to defaults. Your previous settings have been lost."

---

## Wave 2 — High Priority (fix before wide release)

Reliability, debuggability, and correctness issues.

### E-1 — Replace generic error strings with structured `AppError` type

**Files:** `src-tauri/src/models/error.rs`, all `src-tauri/src/commands/*.rs`  
**Problem:** All command handlers use `.map_err(|e| e.to_string())`. The frontend cannot distinguish error types and cannot trigger recovery flows.  
**Fix:** Extend `models/error.rs` into a full `AppError` enum with variants: `Io`, `InvalidInput`, `ProcessFailed`, `NotFound`, `PermissionDenied`, `SettingsCorrupted`, `McpError`. Derive `TS` and `Serialize`. Update all command handlers to return `Result<T, AppError>`. Generate `src/bindings/AppError.ts`. Frontend matches on variants.

### E-2 — Emit MCP startup failure to frontend

**File:** `src-tauri/src/lib.rs`, `src/stores/ui.store.ts`  
**Problem:** MCP server fires-and-forgets in a `tokio::spawn`. Startup failure is `tracing::warn` only.  
**Fix:** On MCP init failure, emit `mcp://startup-failed` Tauri event with error message. Frontend `ui.store.ts` listens and shows a persistent Toast with the error. MCP-related UI elements disable themselves.

### E-3 — Add error toasts for silent catch blocks

**Files:** `src/stores/settings.store.ts`, `src/stores/projects.store.ts`, related stores  
**Problem:** Several `catch` blocks swallow errors silently. Users have no feedback when background operations fail.  
**Fix:** Audit all catch blocks. For user-affecting errors, call `showToast({ type: 'error', message: ... })`. For non-user-affecting errors (e.g., optional telemetry), add structured `console.error` with context.

### E-4 — Distinguish build cancellation from build failure

**Files:** `src-tauri/src/services/process_manager.rs`, `src-tauri/src/models/build.rs`  
**Problem:** `Option<i32>` exit code cannot distinguish signal-kill from never-started.  
**Fix:** Replace with `ProcessTermination` enum: `ExitCode(i32)`, `Signal(i32)`, `NeverStarted`. Propagate through build model and IPC. Frontend shows "Build cancelled" vs "Build failed (exit 1)" accordingly.

### T-1 — Integration tests for the build flow

**Scope:** `src-tauri/src/` (Rust integration tests)  
**Problem:** The full `project open → variant selection → gradlew invocation → error parsing → store update` path has zero integration test coverage.  
**Fix:** Add Rust integration tests with a fixture minimal Gradle project. Cover: happy path build, compilation failure with structured diagnostics, build cancellation. Use a mock `gradlew` script that returns known outputs.

### T-2 — Error state transition tests for all stores

**Files:** `src/stores/*.test.ts`  
**Problem:** All existing store tests are happy-path only. Error transitions are completely untested.  
**Fix:** For each store add tests: build fails mid-run, device disconnects during logcat, settings fail to load, project path becomes invalid after opening. Focus on state consistency after failure.

### T-3 — Unit tests for all Rust command handlers

**Files:** `src-tauri/src/commands/*.rs`  
**Problem:** Command handlers are completely untested — no coverage of serialization, error propagation, or permission gating.  
**Fix:** Add unit tests that mock the service layer (via trait objects or test doubles) and verify: correct `AppError` variant returned, correct serialization shape, input validation rejects bad input.

### T-4 — Ring buffer stress tests

**File:** `src-tauri/src/services/logcat.rs`  
**Problem:** 50K entry eviction policy is untested. No coverage of memory behavior at capacity.  
**Fix:** Add tests: insert 50,001 entries, verify oldest is evicted, verify count stays at 50K. Add test for filter correctness at capacity. Add test that memory does not grow unboundedly.

### Q-1 — Split `build_runner.rs` by concern

**File:** `src-tauri/src/services/build_runner.rs`  
**Problem:** Mixes regex patterns, output parsing, error classification, process lifecycle, and history management.  
**Fix:** Extract `build_parser.rs` (regex definitions + line parsing + error classification). `build_runner.rs` retains process lifecycle only. Each module is independently testable.

### Q-2 — Centralize path validation

**Files:** `src-tauri/src/commands/studio.rs`, `src-tauri/src/commands/file_system.rs`  
**Problem:** Path traversal validation duplicated in multiple command handlers.  
**Fix:** Create `src-tauri/src/utils/path.rs` with `fn validate_project_path(root: &Path, untrusted: &str) -> Result<PathBuf, AppError>`. All commands delegate to it.

### Q-3 — Validate unknown fields in settings deserialization

**File:** `src-tauri/src/models/settings.rs`  
**Problem:** `#[serde(default)]` on all fields silently ignores typos in `settings.json`.  
**Fix:** Add a post-deserialization validation pass that logs unknown keys as warnings. Consider adding version field to settings struct for future migration support.

---

## Wave 3 — Medium/Low (ship incrementally post-v1)

Polish, developer experience, and long-term maintainability.

### P-1 — Structured logging with log rotation

Write `tracing` output to `~/.keynobi/logs/app.log` with daily rotation (keep 7 days). Add a `logLevel` setting (error/warn/info/debug). Frontend uses info level in production builds.

### P-2 — Error tracking integration

Integrate Sentry (or equivalent) for unhandled panics in Rust (`sentry` crate) and uncaught frontend errors (`@sentry/browser`). This is the single most impactful lever for diagnosing field issues.

### P-3 — Version display in UI + single-source version

Display app version in Settings panel or title bar. Create a script that reads `package.json` version as the single source of truth and syncs `Cargo.toml` and `tauri.conf.json`. Run as part of the build script.

### P-4 — In-app update notification

On startup, check a GitHub Releases manifest (or a hosted JSON file) for a newer version. Show a dismissible banner in the title bar if an update is available. No forced update.

### X-1 — Automate TypeScript bindings validation in CI

Run `generate:bindings` in CI and fail the build if any generated file changed (i.e., committed bindings are out of sync with Rust models).

### X-2 — Pre-commit hooks for lint + type-check + clippy

Add `husky` + `lint-staged`: run `eslint`, `tsc --noEmit` on frontend changes. Run `cargo clippy -- -D warnings` on Rust changes. Prevent regressions from reaching CI.

### X-3 — CHANGELOG + semantic versioning workflow

Add `CHANGELOG.md`, adopt Conventional Commits convention, automate version bumping with `cargo-release` or `standard-version`.

### Q-4 — Guard all `app_handle` usages in headless MCP mode

Audit `AndroidMcpServer` for any `.unwrap()` or `.expect()` on `Option<AppHandle>`. Replace with early-return guards that log + return `McpError::HeadlessMode`.

### Q-5 — Ring buffer health indicator in Health panel

Add a soft warning to the Health panel when the logcat ring buffer is >80% full, so users know they may be losing old entries.

### Q-6 — Build history persistence across restarts

Persist the last 20 build summaries (error list, variant, duration — not full log) to `~/.keynobi/build-history.json`. Display in the Build panel as collapsible previous runs. Evict oldest beyond 20 on write.

---

## Release Gates

| Wave | Condition to proceed |
|---|---|
| **Wave 1 complete** | All S-* and D-* tasks pass review and tests. App is safe to share with trusted testers. |
| **Wave 2 complete** | All E-*, T-*, and Q-* (1–3) tasks pass review. Test coverage acceptable for wide beta. |
| **Wave 3 complete** | All P-*, X-*, and Q-* (4–6) tasks done. App is v1.0 quality. |

---

## Out of Scope

- New features (emulator GPS/network/battery controls, logcat export, onboarding — these are Phase 5/6 per PLAN.md)
- MCP server implementation (Phase 4 per PLAN.md, separate spec)
- Windows/Linux platform support
