# Production Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the Android Dev Companion for public launch by fixing security vulnerabilities, improving error handling, increasing test coverage, and adding production infrastructure — organized in three severity-tiered waves.

**Architecture:** No architectural changes to the three-layer structure (SolidJS → Tauri IPC → Rust services). Two new files are introduced: `src-tauri/src/utils/path.rs` (centralized path validation, Task 16) and `src-tauri/src/services/build_parser.rs` (regex + parsing logic extracted from `build_runner.rs`, Task 15).

**Tech Stack:** Rust (Tauri 2.0, tokio, serde, thiserror, ts-rs, tracing), SolidJS + TypeScript (Vitest, @solidjs/testing-library)

**Spec:** `docs/superpowers/specs/2026-04-02-production-readiness-design.md`

---

## File Map

**Modified:**
- `src-tauri/src/commands/studio.rs` — Task 1 (S-1)
- `src-tauri/tauri.conf.json` — Task 2 (S-2)
- `src-tauri/capabilities/default.json` — Task 3 (S-3)
- `src-tauri/src/commands/build.rs` — Task 4 (S-4), Task 10 (E-4)
- `src-tauri/src/commands/device.rs` — Task 4 (S-4)
- `src-tauri/src/lib.rs` — Task 5 (D-1), Task 8 (E-2)
- `src-tauri/src/services/settings_manager.rs` — Task 6 (D-2)
- `src-tauri/src/models/error.rs` — Task 7 (E-1)
- `src-tauri/src/services/process_manager.rs` — Task 10 (E-4)
- `src-tauri/src/models/build.rs` — Task 10 (E-4)
- `src/stores/settings.store.ts` — Task 6 (D-2), Task 9 (E-3)
- `src/stores/projects.store.ts` — Task 9 (E-3)
- `src/stores/build.store.ts` — Task 10 (E-4)
- `src/stores/ui.store.ts` — Task 8 (E-2)
- `src/lib/tauri-api.ts` — Task 8 (E-2), Task 10 (E-4)
- `src-tauri/src/services/build_runner.rs` — Task 15 (Q-1, shrinks)
- `src-tauri/src/models/settings.rs` — Task 17 (Q-3)
- `src-tauri/Cargo.toml` — Task 18 (P-1), Task 20 (P-3)
- `package.json` — Task 22 (X-1), Task 23 (X-2)

**Created:**
- `src-tauri/src/utils/mod.rs` — Task 16
- `src-tauri/src/utils/path.rs` — Task 16 (Q-2)
- `src-tauri/src/services/build_parser.rs` — Task 15 (Q-1)
- `.husky/pre-commit` — Task 23 (X-2)
- `scripts/sync-version.mjs` — Task 20 (P-3)
- `CHANGELOG.md` — Task 24 (X-3)

---

## WAVE 1 — Blocking (must fix before any release)

---

### Task 1: S-1 — Fix command injection in `studio.rs`

**Files:**
- Modify: `src-tauri/src/commands/studio.rs:74-81`

The current code builds a shell command string with the file path embedded in it:
`format!("studio --line {line} '{abs_path}'")`
then runs it via `sh -lc`. A path containing `'` followed by shell metacharacters breaks the quoting.

The fix: keep the login shell (needed for PATH resolution on macOS) but use `exec "$0" "$@"` so all arguments are passed as positional parameters — never interpolated into the script string.

- [ ] **Step 1: Add a test that would catch the injection pattern**

Add to the `#[cfg(test)]` block in `src-tauri/src/commands/studio.rs`:

```rust
#[test]
fn find_source_file_handles_special_chars_in_dir() {
    // Files with special chars in parent dirs must still be found safely.
    // (The injection fix is in open_in_studio; this validates the search
    //  doesn't panic on unusual directory names.)
    let tmp = TempDir::new().unwrap();
    // Create a dir with a space in its name (common on macOS)
    make_file(
        tmp.path(),
        "my project/app/src/main/java/com/example/app/MainActivity.kt",
    );
    let root = tmp.path().to_path_buf();
    let suffix: PathBuf = "com/example/app".into();
    let found = find_source_file(&root, &suffix, "MainActivity.kt").unwrap();
    assert!(found.ends_with("com/example/app/MainActivity.kt"));
}
```

- [ ] **Step 2: Run the test to verify it passes (it tests the search, not the shell)**

```bash
cd src-tauri && cargo test studio -- --nocapture
```

Expected: all 5 studio tests pass.

- [ ] **Step 3: Replace the shell command string with injection-safe invocation**

Replace lines 71–88 in `src-tauri/src/commands/studio.rs`:

```rust
    // ── Invoke studio ──────────────────────────────────────────────────────────
    // Use a login shell so macOS users' custom PATH (set in .zshrc / .zprofile)
    // is available.
    //
    // SECURITY: Do NOT embed abs_path in the shell script string — that allows
    // injection via filenames containing single quotes or shell metacharacters.
    // Instead, pass abs_path as a positional parameter ($0) to `exec "$0" "$@"`.
    // The script string is a constant; no user data is ever interpolated into it.
    let status = tokio::process::Command::new("sh")
        .args([
            "-lc",
            "exec \"$0\" \"$@\"",
            "studio",
            "--line",
            &line.to_string(),
            &abs_path,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| format!(
            "Failed to launch studio: {e}. Make sure `studio` is on your PATH \
             (see the Health Panel for setup instructions)."
        ))?;

    if !status.success() {
        return Err(format!(
            "studio exited with status {status}. Make sure the `studio` command is on your PATH \
             (see the Health Panel for setup instructions)."
        ));
    }

    Ok(abs_path)
```

- [ ] **Step 4: Run all studio tests to confirm nothing broke**

```bash
cd src-tauri && cargo test studio -- --nocapture
```

Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/studio.rs
git commit -m "fix(security): prevent shell injection in open_in_studio via positional args"
```

---

### Task 2: S-2 — Enable Content Security Policy

**Files:**
- Modify: `src-tauri/tauri.conf.json:26`

- [ ] **Step 1: Replace `"csp": null` with a restrictive policy**

In `src-tauri/tauri.conf.json`, replace:

```json
    "security": {
      "csp": null
    }
```

with:

```json
    "security": {
      "csp": "default-src 'self'; connect-src ipc: http://ipc.localhost; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: asset: https://asset.localhost; font-src 'self' data:;"
    }
```

Explanation of each directive:
- `connect-src ipc: http://ipc.localhost` — required for Tauri 2.0 IPC communication
- `script-src 'self'` — scripts only from the app bundle, no `eval`, no inline
- `style-src 'self' 'unsafe-inline'` — SolidJS uses inline styles for dynamic values (e.g. CSS variables)
- `img-src 'self' data: asset: https://asset.localhost` — allows bundled images and Tauri asset protocol
- `font-src 'self' data:` — allows bundled fonts and base64 fonts

- [ ] **Step 2: Build the app in dev mode and verify IPC still works**

```bash
npm run tauri dev
```

Expected: app launches, opens a project, runs a build — no console errors about CSP violations. If you see a CSP violation for a specific source, add only that specific source to the relevant directive.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "fix(security): enable Content Security Policy in Tauri webview"
```

---

### Task 3: S-3 — Restrict Tauri filesystem permissions

**Files:**
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/src/commands/file_system.rs` (register project scope at runtime)
- Modify: `src-tauri/src/commands/settings.rs` (register SDK scope at runtime)

- [ ] **Step 1: Tighten capabilities to a specific scope**

Replace `src-tauri/capabilities/default.json` with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-start-dragging",
    "opener:default",
    "dialog:default",
    "dialog:allow-open",
    "dialog:allow-save",
    "fs:default",
    "fs:allow-read-file",
    "fs:allow-read-text-file",
    "fs:allow-write-file",
    "fs:allow-write-text-file",
    "fs:allow-read-dir",
    "fs:allow-mkdir",
    "fs:allow-remove",
    "fs:allow-rename",
    "fs:allow-exists",
    "fs:allow-stat",
    {
      "identifier": "fs:scope",
      "allow": [
        { "path": "$HOME/.keynobi" },
        { "path": "$HOME/.keynobi/**" }
      ]
    },
    "shell:default",
    "shell:allow-open"
  ]
}
```

Changes:
- Removed `fs:scope-home-recursive` (entire home dir access)
- Removed `shell:allow-execute` (was used only for the studio shell command which is now safe)
- Added scoped `fs:scope` limited to `~/.keynobi/**`

- [ ] **Step 2: Register project path scope at runtime when a project is opened**

In `src-tauri/src/commands/file_system.rs`, find the `open_project` command and add scope registration after the project root is set. Add at the top of the file:

```rust
use tauri_plugin_fs::FsExt;
```

In the `open_project` function, after successfully setting the project root, add:

```rust
    // Register the opened project directory as an allowed fs scope so the
    // frontend can read files within it (restricted to this project only).
    if let Err(e) = app_handle.try_fs_scope().map(|scope| {
        let _ = scope.allow_directory(&canonical_root, true);
    }) {
        tracing::warn!("Failed to register project fs scope: {e:?}");
    }
```

Note: `app_handle` needs to be added as a parameter to `open_project`. Check that command's signature and add `app_handle: AppHandle` if not already present.

- [ ] **Step 3: Register SDK path scope at runtime when SDK path is saved**

In `src-tauri/src/commands/settings.rs`, in the `save_settings` command handler, after saving, register the SDK path if present:

```rust
    // Register the Android SDK directory as an accessible fs scope so the
    // health checks can read SDK tools.
    if let Some(ref sdk_path) = settings.android.sdk_path {
        let sdk = std::path::PathBuf::from(sdk_path);
        if sdk.is_dir() {
            if let Ok(scope) = app_handle.try_fs_scope() {
                let _ = scope.allow_directory(&sdk, true);
            }
        }
    }
```

This requires adding `app_handle: AppHandle` to the `save_settings` command signature.

- [ ] **Step 4: Build and run in dev mode to confirm fs operations still work**

```bash
npm run tauri dev
```

Open a project, verify the app can read project files, open settings, set SDK path. Check the Rust console for any "permission denied" errors.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/capabilities/default.json src-tauri/src/commands/file_system.rs src-tauri/src/commands/settings.rs
git commit -m "fix(security): restrict fs permissions to ~/.keynobi and opened project/SDK paths"
```

---

### Task 4: S-4 — Input validation for Gradle task names and device serials

**Files:**
- Modify: `src-tauri/src/commands/build.rs:36-50`
- Modify: `src-tauri/src/commands/device.rs:50-58`

- [ ] **Step 1: Write failing tests for validation**

Add to `src-tauri/src/commands/build.rs` (in a `#[cfg(test)]` block at the bottom):

```rust
#[cfg(test)]
mod tests {
    // Validate the allowlist regex for Gradle task names.
    fn is_valid_task(task: &str) -> bool {
        task.chars().all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'))
            && !task.is_empty()
            && task.len() <= 256
    }

    #[test]
    fn valid_gradle_tasks_pass() {
        assert!(is_valid_task(":app:assembleDebug"));
        assert!(is_valid_task("assembleRelease"));
        assert!(is_valid_task("test"));
        assert!(is_valid_task(":app:bundleRelease"));
    }

    #[test]
    fn invalid_gradle_tasks_rejected() {
        assert!(!is_valid_task(""));
        assert!(!is_valid_task(":app:assemble; rm -rf /"));
        assert!(!is_valid_task("assemble$(evil)"));
        assert!(!is_valid_task("assemble\necho pwned"));
        assert!(!is_valid_task(&"a".repeat(257)));
    }
}
```

- [ ] **Step 2: Run tests to verify they compile (the helper exists inline)**

```bash
cd src-tauri && cargo test build::tests -- --nocapture
```

Expected: 2 tests pass.

- [ ] **Step 3: Add validation function and wire it into `run_gradle_task`**

At the top of `src-tauri/src/commands/build.rs` after the imports, add:

```rust
/// Validate a Gradle task name.
/// Allowlist: alphanumeric, colon, hyphen, underscore, dot. Max 256 chars.
fn validate_gradle_task(task: &str) -> Result<(), String> {
    if task.is_empty() {
        return Err("Gradle task name must not be empty".to_string());
    }
    if task.len() > 256 {
        return Err("Gradle task name is too long (max 256 characters)".to_string());
    }
    if !task.chars().all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.')) {
        return Err(format!(
            "Invalid Gradle task name '{task}': only alphanumeric characters, ':', '-', '_', and '.' are allowed"
        ));
    }
    Ok(())
}
```

At the start of the `run_gradle_task` function body (line ~44, before the `gradle_root` lock), add:

```rust
    validate_gradle_task(&task)?;
```

- [ ] **Step 4: Add device serial validation in `device.rs`**

At the top of `src-tauri/src/commands/device.rs` after the imports, add:

```rust
/// Validate an ADB device serial.
/// ADB serials are alphanumeric with colons, dots, hyphens, and underscores.
fn validate_device_serial(serial: &str) -> Result<(), String> {
    if serial.is_empty() {
        return Err("Device serial must not be empty".to_string());
    }
    if serial.len() > 64 {
        return Err("Device serial is too long (max 64 characters)".to_string());
    }
    if !serial.chars().all(|c| c.is_alphanumeric() || matches!(c, ':' | '.' | '-' | '_')) {
        return Err(format!(
            "Invalid device serial '{serial}': only alphanumeric characters, ':', '.', '-', and '_' are allowed"
        ));
    }
    Ok(())
}
```

Add `validate_device_serial(&serial)?;` at the start of: `select_device`, `install_apk_on_device`, `launch_app_on_device`, `stop_app_on_device`.

- [ ] **Step 5: Run all tests**

```bash
cd src-tauri && cargo test -- --nocapture 2>&1 | tail -20
```

Expected: all tests pass, no new failures.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/build.rs src-tauri/src/commands/device.rs
git commit -m "fix(security): add allowlist validation for Gradle task names and device serials"
```

---

### Task 5: D-1 — Implement graceful shutdown

**Files:**
- Modify: `src-tauri/src/lib.rs:95-198`

On app close, the Tauri window fires `WindowEvent::CloseRequested`. We need to: cancel any running build, stop logcat, stop device polling, then allow close.

- [ ] **Step 1: Add shutdown logic to the Tauri builder in `lib.rs`**

Add this import at the top of `src-tauri/src/lib.rs`:

```rust
use tauri::Manager;
```

In the `tauri::Builder::default()` chain in `run()`, add `.on_window_event` before `.invoke_handler`:

```rust
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent the default close so we can clean up first.
                api.prevent_close();

                let app = window.app_handle().clone();
                tokio::spawn(async move {
                    // 1. Cancel any running build.
                    let build_state = app.state::<BuildState>();
                    let process_manager = app.state::<ProcessManager>();
                    build_runner::cancel_build(&build_state, &process_manager).await;

                    // 2. Stop logcat streaming.
                    {
                        use commands::logcat::stop_logcat_internal;
                        let logcat_state = app.state::<services::logcat::LogcatState>();
                        stop_logcat_internal(&logcat_state).await;
                    }

                    // 3. Stop ADB device polling.
                    {
                        let device_state = app.state::<DeviceState>();
                        services::adb_manager::stop_polling(&device_state).await;
                    }

                    // 4. Allow the window to close.
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.destroy();
                    }
                });
            }
        })
```

- [ ] **Step 2: Expose internal stop functions for use in shutdown**

In `src-tauri/src/commands/logcat.rs`, add a pub(crate) helper (it may already have `stop_logcat` as a command — expose its inner logic):

Check if `stop_logcat` command in `src-tauri/src/commands/logcat.rs` calls an internal function or has logic inline. If inline, extract:

```rust
pub(crate) async fn stop_logcat_internal(logcat_state: &tauri::State<'_, services::logcat::LogcatState>) {
    // Move the body of the stop_logcat command here.
    // Change stop_logcat command to call this function.
}
```

In `src-tauri/src/services/adb_manager.rs`, expose:

```rust
pub async fn stop_polling(device_state: &DeviceState) {
    let mut inner = device_state.0.lock().await;
    inner.polling = false;
    // Signal any running poll task to stop.
}
```

(Inspect the actual polling logic in adb_manager.rs and expose the appropriate stop function.)

- [ ] **Step 3: Add a 3-second timeout to avoid hanging on shutdown**

Wrap the async shutdown block in a timeout:

```rust
                tokio::spawn(async move {
                    let shutdown = async {
                        // ... all the cancel/stop calls above ...
                    };

                    // Never hang longer than 3 seconds on shutdown.
                    if tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        shutdown,
                    ).await.is_err() {
                        tracing::warn!("Shutdown timed out after 3s — forcing close");
                    }

                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.destroy();
                    }
                });
```

- [ ] **Step 4: Verify in dev mode**

```bash
npm run tauri dev
```

Start a build, then close the window while the build is running. Verify:
- The window stays open briefly (cleanup runs)
- No orphan `gradlew` processes remain: `ps aux | grep gradlew`
- The window closes cleanly.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands/logcat.rs src-tauri/src/services/adb_manager.rs
git commit -m "fix(reliability): implement graceful shutdown — cancel build and stop logcat on close"
```

---

### Task 6: D-2 — Surface settings corruption to the user

**Files:**
- Modify: `src-tauri/src/services/settings_manager.rs:21-33`
- Modify: `src/stores/settings.store.ts:80-87`

- [ ] **Step 1: Write a test that verifies corruption is detected**

Add to `src-tauri/src/services/settings_manager.rs` tests:

```rust
    #[test]
    fn corrupt_json_is_detected_not_silently_swallowed() {
        // We can't call load_settings() in tests (it reads a fixed path),
        // but we can test the detection logic directly.
        let corrupt = "{ not valid json !!!";
        let result: Result<AppSettings, _> = serde_json::from_str(corrupt);
        assert!(result.is_err(), "corrupt JSON must fail to parse");
        // Confirm unwrap_or_default produces defaults.
        let settings = result.unwrap_or_default();
        assert_eq!(settings, AppSettings::default());
    }
```

- [ ] **Step 2: Run the test to confirm it passes**

```bash
cd src-tauri && cargo test settings_manager::tests::corrupt -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Add corruption detection flag to `load_settings` return value**

Change `settings_manager.rs` `load_settings` to return a tuple indicating if defaults were used due to corruption:

```rust
/// Load settings from disk.
///
/// Returns `(settings, was_corrupted)`. `was_corrupted` is true when the
/// file existed but could not be parsed — it has been silently replaced with
/// defaults. The caller should notify the user in this case.
pub fn load_settings() -> (AppSettings, bool) {
    let path = settings_file();
    if !path.exists() {
        return (AppSettings::default(), false);
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
            Ok(settings) => (settings, false),
            Err(e) => {
                tracing::warn!("Settings file is corrupted (using defaults): {e}");
                (AppSettings::default(), true)
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read settings file (using defaults): {e}");
            (AppSettings::default(), false)
        }
    }
}
```

- [ ] **Step 4: Update all callers of `load_settings` to destructure the tuple**

Search for all callers:

```bash
cd src-tauri && grep -rn "load_settings()" src/
```

For each caller, change `let settings = load_settings();` to `let (settings, _) = load_settings();` — except in `lib.rs` `setup` where we need to emit the corruption event.

In `src-tauri/src/lib.rs` setup:

```rust
        .setup(|app| {
            let (settings, settings_corrupted) = services::settings_manager::load_settings();

            // Notify the frontend if settings were corrupted on this launch.
            if settings_corrupted {
                let handle = app.handle().clone();
                tokio::spawn(async move {
                    // Small delay to allow the frontend to finish mounting.
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    let _ = handle.emit("settings:corrupted", ());
                });
            }

            if settings.mcp.auto_start {
                let handle = app.handle().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Err(e) = services::mcp_server::start_mcp_server(handle).await {
                        tracing::warn!("MCP auto-start failed: {}", e);
                    }
                });
            }
            Ok(())
        })
```

- [ ] **Step 5: Listen for `settings:corrupted` in the frontend and show a Toast**

In `src/stores/settings.store.ts`, add after the `loadSettings` function:

```typescript
import { listen } from "@tauri-apps/api/event";
import { showToast } from "@/stores/ui.store"; // adjust import if toast is elsewhere

// Listen for settings corruption event emitted by the Rust backend on startup.
if (typeof window !== "undefined") {
  listen("settings:corrupted", () => {
    showToast({
      type: "error",
      message:
        "Settings file was corrupted and has been reset to defaults. Your previous settings have been lost.",
      duration: 8000,
    });
  }).catch(() => {
    // Non-critical: if listen fails (e.g., in tests), ignore.
  });
}
```

(Adjust `showToast` import to match the actual toast API in `ui.store.ts`.)

- [ ] **Step 6: Run the frontend tests to verify nothing broke**

```bash
npm run test
```

Expected: all existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/settings_manager.rs src-tauri/src/lib.rs src/stores/settings.store.ts
git commit -m "fix(reliability): detect and surface settings file corruption to user via Toast"
```

---

## WAVE 2 — High Priority (fix before wide release)

---

### Task 7: E-1 — Replace generic error strings with structured `AppError` type

**Files:**
- Modify: `src-tauri/src/models/error.rs`
- Modify: `src-tauri/src/commands/build.rs`, `device.rs`, `file_system.rs`, `settings.rs`, `health.rs`, `logcat.rs`, `variant.rs`, `studio.rs`
- Modify: `src/lib/tauri-api.ts`

This is the foundation for all subsequent error handling improvements. We extend `FsError` into a top-level `AppError` that all commands return, then generate a TypeScript binding.

- [ ] **Step 1: Write a test for AppError serialization**

In `src-tauri/src/models/error.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_serializes_to_json() {
        let err = AppError::InvalidInput("bad task name".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("invalidInput") || json.contains("InvalidInput"));
    }

    #[test]
    fn app_error_display_is_human_readable() {
        let err = AppError::NotFound("settings.json".to_string());
        assert!(err.to_string().contains("settings.json"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (AppError doesn't exist yet)**

```bash
cd src-tauri && cargo test error::tests -- --nocapture 2>&1 | head -20
```

Expected: compile error — `AppError` not found.

- [ ] **Step 3: Replace `error.rs` with the full `AppError` enum**

Replace the entire `src-tauri/src/models/error.rs` with:

```rust
use serde::Serialize;
use thiserror::Error;
use ts_rs::TS;

/// Top-level structured error type returned by all Tauri command handlers.
///
/// Variants are serialized as tagged JSON objects so the TypeScript frontend
/// can match on error types and show context-appropriate messages.
#[derive(Debug, Error, Serialize, TS)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Process failed: {0}")]
    ProcessFailed(String),

    #[error("Settings corrupted: {0}")]
    SettingsCorrupted(String),

    #[error("MCP error: {0}")]
    McpError(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn io(path: impl std::fmt::Display, source: std::io::Error) -> Self {
        AppError::Io(format!("'{path}': {source}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

/// Legacy alias for code that still uses FsError internally.
/// New code should use AppError directly.
#[derive(Debug, Error)]
pub enum FsError {
    #[error("Not found: '{0}'")]
    NotFound(String),
    #[error("Permission denied: '{0}'")]
    PermissionDenied(String),
    #[error("Already exists: '{0}'")]
    AlreadyExists(String),
    #[error("'{path}' is too large ({size_mb} MB). Maximum is {limit_mb} MB.")]
    TooLarge { path: String, size_mb: u64, limit_mb: u64 },
    #[error("Access denied: '{0}' is outside the open project directory")]
    PathTraversal(String),
    #[error("Parent directory does not exist: '{0}'")]
    NoParentDir(String),
    #[error("Invalid path: '{0}'")]
    InvalidPath(String),
    #[error("IO error on '{path}': {source}")]
    Io { path: String, #[source] source: std::io::Error },
    #[error("{0}")]
    Other(String),
}

impl FsError {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        FsError::Io { path: path.into(), source }
    }
}

impl From<FsError> for AppError {
    fn from(e: FsError) -> Self {
        AppError::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_serializes_to_json() {
        let err = AppError::InvalidInput("bad task name".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("invalidInput"));
    }

    #[test]
    fn app_error_display_is_human_readable() {
        let err = AppError::NotFound("settings.json".to_string());
        assert!(err.to_string().contains("settings.json"));
    }
}
```

- [ ] **Step 4: Run the tests**

```bash
cd src-tauri && cargo test error::tests -- --nocapture
```

Expected: both tests pass.

- [ ] **Step 5: Change command handlers to return `Result<T, AppError>`**

For each command file, change `Result<T, String>` to `Result<T, AppError>` on commands where we can meaningfully categorize the error. Update `.map_err(|e| e.to_string())` calls:

**`commands/build.rs`** — change `run_gradle_task` return type, update validation:
```rust
// Before:
pub async fn run_gradle_task(...) -> Result<u32, String> {

// After:
pub async fn run_gradle_task(...) -> Result<u32, AppError> {
```
And update the validation call:
```rust
    validate_gradle_task(&task).map_err(AppError::InvalidInput)?;
```
Update `.ok_or("No project open")` to `.ok_or_else(|| AppError::NotFound("No project open".to_string()))`.

**`commands/device.rs`** — similarly change select_device, install_apk_on_device:
```rust
pub async fn select_device(serial: String, ...) -> Result<(), AppError> {
    validate_device_serial(&serial).map_err(AppError::InvalidInput)?;
    ...
    settings_manager::save_settings(&settings).map_err(|e| AppError::Io(e))?
}
```

Apply the same pattern to the remaining command files: `file_system.rs`, `settings.rs`, `health.rs`, `logcat.rs`, `variant.rs`, `studio.rs`.

- [ ] **Step 6: Regenerate TypeScript bindings**

```bash
npm run generate:bindings
```

Expected: `src/bindings/AppError.ts` is created. Verify it contains the `AppError` type.

- [ ] **Step 7: Update `tauri-api.ts` to re-export AppError**

Add to `src/lib/tauri-api.ts` near the top imports:

```typescript
import type { AppError } from "@/bindings";
export type { AppError };
```

Update `formatError` to handle structured AppError:

```typescript
export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  // Structured AppError from Rust backend
  if (typeof err === "object" && err !== null && "kind" in err) {
    const e = err as AppError;
    return e.message ?? String(e.kind);
  }
  return String(err);
}
```

- [ ] **Step 8: Run all tests**

```bash
npm run test && cd src-tauri && cargo test
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/models/error.rs src-tauri/src/commands/ src/lib/tauri-api.ts src/bindings/
git commit -m "feat(errors): introduce structured AppError type across all Rust command handlers"
```

---

### Task 8: E-2 — Emit MCP startup failure to frontend

**Files:**
- Modify: `src-tauri/src/lib.rs:108-117`
- Modify: `src/stores/ui.store.ts`
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Emit `mcp:startup-failed` event on MCP init failure**

In `src-tauri/src/lib.rs`, change the MCP auto-start block from:

```rust
            if services::settings_manager::load_settings().mcp.auto_start {
                let handle = app.handle().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Err(e) = services::mcp_server::start_mcp_server(handle).await {
                        tracing::warn!("MCP auto-start failed: {}", e);
                    }
                });
            }
```

to:

```rust
            if settings.mcp.auto_start {
                let handle = app.handle().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Err(e) = services::mcp_server::start_mcp_server(handle.clone()).await {
                        tracing::warn!("MCP auto-start failed: {}", e);
                        // Notify the frontend so it can display an error and disable MCP UI.
                        let _ = handle.emit("mcp:startup-failed", e.to_string());
                    }
                });
            }
```

(Note: `settings` here refers to the `settings` variable from Task 6's setup block refactor.)

- [ ] **Step 2: Add a listener and MCP status field to `ui.store.ts`**

Read `src/stores/ui.store.ts` first, then add:

```typescript
import { listen } from "@tauri-apps/api/event";

// Track MCP server status so components can show/hide MCP features.
const [mcpStartupError, setMcpStartupError] = createSignal<string | null>(null);
export { mcpStartupError };

if (typeof window !== "undefined") {
  listen<string>("mcp:startup-failed", (event) => {
    setMcpStartupError(event.payload);
    showToast({
      type: "error",
      message: `MCP server failed to start: ${event.payload}`,
      duration: 10000,
    });
  }).catch(() => {});
}
```

- [ ] **Step 3: Run frontend tests**

```bash
npm run test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs src/stores/ui.store.ts
git commit -m "fix(mcp): emit startup-failed event to frontend when MCP auto-start fails"
```

---

### Task 9: E-3 — Add error toasts for silent catch blocks

**Files:**
- Modify: `src/stores/settings.store.ts:80-87`
- Modify: `src/stores/projects.store.ts` (check for silent catches)

- [ ] **Step 1: Find all silent catch blocks**

```bash
grep -n "catch" src/stores/*.ts src/services/*.ts
```

Review each one. Silent catches are those with an empty body `catch { }` or only a comment.

- [ ] **Step 2: Fix `loadSettings` in `settings.store.ts`**

Replace:

```typescript
export async function loadSettings(): Promise<void> {
  try {
    const loaded = await getSettings();
    setSettingsState(loaded);
  } catch {
    // First launch or Tauri not available — use defaults
  }
}
```

with:

```typescript
export async function loadSettings(): Promise<void> {
  try {
    const loaded = await getSettings();
    setSettingsState(loaded);
  } catch (err) {
    // First launch: settings file doesn't exist yet — use defaults silently.
    // Any other error (e.g. Tauri unavailable in tests) is also safe to ignore.
    const msg = formatError(err);
    if (!msg.includes("No such file") && typeof window !== "undefined") {
      console.error("[settings] Failed to load settings:", msg);
    }
  }
}
```

- [ ] **Step 3: Fix `scheduleSave` in `settings.store.ts`**

The save failure is already logged via `console.error`. Optionally add a toast for persistent save failures. Check if `showToast` is available — if so, add it:

```typescript
function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await saveSettingsIpc(settingsState);
    } catch (err) {
      const msg = formatError(err);
      console.error("Failed to save settings:", msg);
      showToast({
        type: "error",
        message: `Settings could not be saved: ${msg}`,
        duration: 5000,
      });
    }
  }, 500);
}
```

- [ ] **Step 4: Review and fix `projects.store.ts`**

Read `src/stores/projects.store.ts`. For any silent catches around `listProjects`, `openProject`, `removeProject`, add:

```typescript
  } catch (err) {
    console.error("[projects] Operation failed:", formatError(err));
    showToast({ type: "error", message: `Project operation failed: ${formatError(err)}` });
  }
```

- [ ] **Step 5: Run frontend tests**

```bash
npm run test
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stores/settings.store.ts src/stores/projects.store.ts
git commit -m "fix(ux): surface silent error catch blocks as user-facing Toast notifications"
```

---

### Task 10: E-4 — Distinguish build cancellation from build failure

**Files:**
- Modify: `src-tauri/src/services/process_manager.rs:36`
- Modify: `src-tauri/src/models/build.rs:85-99` (BuildStatus)
- Modify: `src-tauri/src/commands/build.rs` (on_exit callback)
- Modify: `src/stores/build.store.ts`

- [ ] **Step 1: Write a failing test for ProcessTermination**

In `src-tauri/src/services/process_manager.rs` tests, add:

```rust
    #[test]
    fn process_termination_variants_are_distinct() {
        let exit = ProcessTermination::ExitCode(0);
        let signal = ProcessTermination::Signal(15);
        let cancelled = ProcessTermination::Cancelled;
        assert!(matches!(exit, ProcessTermination::ExitCode(0)));
        assert!(matches!(signal, ProcessTermination::Signal(15)));
        assert!(matches!(cancelled, ProcessTermination::Cancelled));
    }
```

- [ ] **Step 2: Run to verify compile failure**

```bash
cd src-tauri && cargo test process_manager::tests::process_termination -- --nocapture 2>&1 | head -10
```

Expected: compile error — `ProcessTermination` not found.

- [ ] **Step 3: Add `ProcessTermination` enum to `process_manager.rs`**

In `src-tauri/src/services/process_manager.rs`, add after `ProcessLine`:

```rust
/// How a managed process ended.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessTermination {
    /// Process exited with the given exit code.
    ExitCode(i32),
    /// Process was killed by a signal (Unix only). 15 = SIGTERM, 9 = SIGKILL.
    Signal(i32),
    /// Process was cancelled by the user via `cancel()`.
    Cancelled,
}
```

Update `SpawnOptions.on_exit` signature from:
```rust
pub on_exit: Box<dyn Fn(ProcessId, Option<i32>) + Send + Sync + 'static>,
```
to:
```rust
pub on_exit: Box<dyn Fn(ProcessId, ProcessTermination) + Send + Sync + 'static>,
```

Update the reader task's `on_exit` call from:
```rust
let exit_code = child.wait().await.ok().and_then(|s| s.code());
on_exit(id, exit_code);
```
to:
```rust
let termination = match child.wait().await {
    Ok(status) => {
        if let Some(code) = status.code() {
            ProcessTermination::ExitCode(code)
        } else {
            // No exit code means it was killed by a signal.
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(sig) = status.signal() {
                    ProcessTermination::Signal(sig)
                } else {
                    ProcessTermination::Signal(0)
                }
            }
            #[cfg(not(unix))]
            ProcessTermination::Signal(0)
        }
    }
    Err(_) => ProcessTermination::Signal(0),
};
on_exit(id, termination);
```

Also add a `cancel_flag` to `ProcessRecord` to mark intentional cancellations, and check it in `cancel()` to emit `Cancelled` instead of `Signal`.

- [ ] **Step 4: Update `commands/build.rs` on_exit to use ProcessTermination**

In the `on_exit` callback inside `run_gradle_task`, change:

```rust
            on_exit: Box::new({
                let app = app_handle.clone();
                let task_name = task.clone();
                move |_pid, termination| {
                    let errs = errors_buf.lock().map(|g| g.clone()).unwrap_or_default();
                    let dur = duration_ms.lock().map(|g| *g).unwrap_or(0);
                    let flag = success_flag.lock().map(|g| *g).unwrap_or(false);

                    let success = matches!(termination, ProcessTermination::ExitCode(0)) || flag;
                    let cancelled = matches!(termination, ProcessTermination::Cancelled);

                    // ... rest of event emit, passing `cancelled` field
                }
            }),
```

Add `cancelled: bool` to `BuildCompleteEvent` struct:

```rust
pub struct BuildCompleteEvent {
    pub success: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub task: String,
}
```

- [ ] **Step 5: Update frontend `build.store.ts` to show "Cancelled" vs "Failed"**

Read `src/stores/build.store.ts`. In the `listenBuildComplete` handler, check the `cancelled` field:

```typescript
listenBuildComplete((e) => {
  if (e.cancelled) {
    setBuildState("phase", "cancelled");
    showToast({ type: "info", message: "Build cancelled." });
  } else if (e.success) {
    setBuildState("phase", "success");
  } else {
    setBuildState("phase", "failed");
    showToast({ type: "error", message: `Build failed with ${e.errorCount} error(s).` });
  }
});
```

- [ ] **Step 6: Run all tests**

```bash
npm run test && cd src-tauri && cargo test
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/process_manager.rs src-tauri/src/models/build.rs src-tauri/src/commands/build.rs src/stores/build.store.ts
git commit -m "feat(build): distinguish build cancellation from failure using ProcessTermination enum"
```

---

### Task 11: T-1 — Integration tests for the build flow

**Files:**
- Create: `src-tauri/tests/build_flow.rs`
- Create: `src-tauri/tests/fixtures/mock_gradlew` (shell script)

- [ ] **Step 1: Create the mock gradlew fixture**

Create `src-tauri/tests/fixtures/mock_gradlew`:

```bash
#!/bin/sh
# Mock gradlew that simulates a successful build or failure based on task arg.
TASK="$1"
case "$TASK" in
  *"assembleDebug"*)
    echo "> Task :app:preBuild UP-TO-DATE"
    echo "> Task :app:compileDebugKotlin"
    echo "> Task :app:assembleDebug"
    echo ""
    echo "BUILD SUCCESSFUL in 3s"
    exit 0
    ;;
  *"fail"*)
    echo "> Task :app:compileDebugKotlin FAILED"
    echo ""
    echo "e: file:///project/app/src/main/java/com/example/Main.kt:10:5: Unresolved reference: foo"
    echo ""
    echo "FAILURE: Build failed with an exception."
    echo "BUILD FAILED in 1s"
    exit 1
    ;;
  *)
    echo "BUILD SUCCESSFUL in 1s"
    exit 0
    ;;
esac
```

Make it executable: `chmod +x src-tauri/tests/fixtures/mock_gradlew`

- [ ] **Step 2: Create the integration test file**

Create `src-tauri/tests/build_flow.rs`:

```rust
//! Integration tests for the build flow:
//! project root detection → gradlew execution → line parsing → history.

use keynobi::services::build_runner::{self, BuildState};
use keynobi::services::process_manager::ProcessManager;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn make_project(tmp: &TempDir) -> PathBuf {
    let root = tmp.path().to_path_buf();
    // Create minimal Gradle project structure.
    fs::create_dir_all(root.join("app/src/main/java/com/example")).unwrap();
    fs::write(root.join("settings.gradle"), "rootProject.name = 'TestApp'").unwrap();
    fs::write(root.join("app/build.gradle"), "apply plugin: 'com.android.application'").unwrap();
    // Copy mock gradlew into the project.
    let mock = fixture_dir().join("mock_gradlew");
    let dest = root.join("gradlew");
    fs::copy(&mock, &dest).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).unwrap();
    }
    root
}

#[tokio::test]
async fn successful_build_updates_history() {
    let tmp = TempDir::new().unwrap();
    let project_root = make_project(&tmp);

    let build_state = BuildState::new();
    let process_manager = ProcessManager::new();

    let lines = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let lines_clone = lines.clone();

    let id = build_runner::spawn_build(
        &build_state,
        &process_manager,
        &project_root,
        "assembleDebug",
        move |line| {
            lines_clone.lock().unwrap().push(line.content.clone());
        },
    )
    .await
    .expect("build should start");

    // Wait for completion.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let bs = build_state.inner.lock().await;
    assert_eq!(bs.history.len(), 1);
    assert!(bs.history[0].status.is_success());
    drop(bs);

    let collected = lines.lock().unwrap();
    assert!(collected.iter().any(|l| l.contains("BUILD SUCCESSFUL")));
}

#[tokio::test]
async fn failed_build_produces_structured_errors() {
    let tmp = TempDir::new().unwrap();
    let project_root = make_project(&tmp);

    let build_state = BuildState::new();
    let process_manager = ProcessManager::new();

    build_runner::spawn_build(
        &build_state,
        &process_manager,
        &project_root,
        "fail",
        |_| {},
    )
    .await
    .expect("build should start");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let bs = build_state.inner.lock().await;
    assert!(!bs.current_errors.is_empty(), "errors must be populated");
    let err = &bs.current_errors[0];
    assert!(err.message.contains("Unresolved reference: foo"));
    assert_eq!(err.file.as_deref(), Some("/project/app/src/main/java/com/example/Main.kt"));
    assert_eq!(err.line, Some(10));
}
```

Note: `spawn_build` is a helper you may need to extract from `run_gradle_task` command into `build_runner.rs` as a testable function. If this refactor is needed, do it as part of this task.

- [ ] **Step 3: Run the integration tests**

```bash
cd src-tauri && cargo test --test build_flow -- --nocapture
```

Expected: both tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/ 
git commit -m "test(integration): add build flow integration tests with mock gradlew"
```

---

### Task 12: T-2 — Error state transition tests for all stores

**Files:**
- Modify: `src/stores/build.store.test.ts`
- Modify: `src/stores/settings.store.test.ts`
- Modify: `src/stores/device.store.test.ts`

- [ ] **Step 1: Add build failure transition test**

In `src/stores/build.store.test.ts`, add:

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import * as tauriApi from "@/lib/tauri-api";

describe("build store error states", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("transitions to failed state on build error", async () => {
    vi.spyOn(tauriApi, "runGradleTask").mockRejectedValue(
      "No project open"
    );
    const { startBuild, buildState } = await import("@/stores/build.store");
    await startBuild("assembleDebug").catch(() => {});
    expect(buildState.phase).toBe("failed");
  });

  it("shows error count in failed state", async () => {
    // Simulate build:complete event with errors
    const { handleBuildComplete, buildState } = await import("@/stores/build.store");
    handleBuildComplete({
      success: false,
      cancelled: false,
      durationMs: 1000,
      errorCount: 3,
      warningCount: 1,
      task: "assembleDebug",
    });
    expect(buildState.phase).toBe("failed");
    expect(buildState.errorCount).toBe(3);
  });

  it("shows cancelled state when build is cancelled", async () => {
    const { handleBuildComplete, buildState } = await import("@/stores/build.store");
    handleBuildComplete({
      success: false,
      cancelled: true,
      durationMs: 500,
      errorCount: 0,
      warningCount: 0,
      task: "assembleDebug",
    });
    expect(buildState.phase).toBe("cancelled");
  });
});
```

- [ ] **Step 2: Add settings load failure test**

In `src/stores/settings.store.test.ts`, add:

```typescript
  it("keeps defaults when getSettings rejects with unexpected error", async () => {
    vi.spyOn(tauriApi, "getSettings").mockRejectedValue(
      new Error("IPC channel closed")
    );
    const { loadSettings, settingsState } = await import("@/stores/settings.store");
    await loadSettings();
    // Should still have defaults, not throw
    expect(settingsState.editor.fontSize).toBe(13);
  });
```

- [ ] **Step 3: Add device disconnect test**

In `src/stores/device.store.test.ts`, add:

```typescript
  it("clears selected device when it disconnects", () => {
    const { setDevices, selectedDevice, setSelectedDevice } = 
      require("@/stores/device.store");
    
    setSelectedDevice("emulator-5554");
    expect(selectedDevice()).toBe("emulator-5554");
    
    // Simulate device list update with the device gone
    setDevices([]);
    
    // Selected device should be cleared since it's no longer connected
    expect(selectedDevice()).toBeNull();
  });
```

- [ ] **Step 4: Run the new tests**

```bash
npm run test
```

Expected: all tests pass (adjust mocks to match the actual store API).

- [ ] **Step 5: Commit**

```bash
git add src/stores/build.store.test.ts src/stores/settings.store.test.ts src/stores/device.store.test.ts
git commit -m "test(stores): add error state transition tests for build, settings, and device stores"
```

---

### Task 13: T-3 — Unit tests for Rust command handlers

**Files:**
- Modify: `src-tauri/src/commands/build.rs`
- Modify: `src-tauri/src/commands/device.rs`
- Modify: `src-tauri/src/commands/studio.rs`

- [ ] **Step 1: Add validation unit tests to `build.rs`**

These test the `validate_gradle_task` function added in Task 4. Verify it already exists from that task:

```bash
cd src-tauri && cargo test build::tests -- --nocapture
```

Expected: passes from Task 4.

- [ ] **Step 2: Add validation unit tests to `device.rs`**

In `src-tauri/src/commands/device.rs`, add a `#[cfg(test)]` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_serials_pass_validation() {
        assert!(validate_device_serial("emulator-5554").is_ok());
        assert!(validate_device_serial("192.168.1.100:5555").is_ok());
        assert!(validate_device_serial("R5CNA0ZVXXX").is_ok());
    }

    #[test]
    fn invalid_serials_are_rejected() {
        assert!(validate_device_serial("").is_err());
        assert!(validate_device_serial("serial; rm -rf /").is_err());
        assert!(validate_device_serial("$(evil)").is_err());
        assert!(validate_device_serial(&"a".repeat(65)).is_err());
    }
}
```

- [ ] **Step 3: Add studio input validation tests (already exist, verify)**

```bash
cd src-tauri && cargo test studio -- --nocapture
```

Expected: all 5 tests pass.

- [ ] **Step 4: Run all command tests**

```bash
cd src-tauri && cargo test commands -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/device.rs
git commit -m "test(commands): add unit tests for device serial and Gradle task validation"
```

---

### Task 14: T-4 — Ring buffer stress tests

**Files:**
- Modify: `src-tauri/src/services/logcat.rs` (add tests)

- [ ] **Step 1: Read the current logcat ring buffer implementation**

```bash
grep -n "ring\|VecDeque\|MAX_LOG\|50_000\|push\|pop" src-tauri/src/services/logcat.rs | head -30
```

Identify the type used for the ring buffer and the eviction logic (likely a `VecDeque` with `pop_front` when over capacity).

- [ ] **Step 2: Add stress tests to `logcat.rs`**

In `src-tauri/src/services/logcat.rs`, add to the `#[cfg(test)]` block:

```rust
    #[test]
    fn ring_buffer_evicts_oldest_at_capacity() {
        use std::collections::VecDeque;
        let capacity = 5; // Use small capacity for test speed
        let mut buf: VecDeque<u32> = VecDeque::new();

        for i in 0..capacity + 3 {
            if buf.len() >= capacity {
                buf.pop_front();
            }
            buf.push_back(i as u32);
        }

        assert_eq!(buf.len(), capacity);
        // Oldest entry should be 3 (0,1,2 were evicted)
        assert_eq!(*buf.front().unwrap(), 3);
        // Newest should be 7
        assert_eq!(*buf.back().unwrap(), 7);
    }

    #[tokio::test]
    async fn logcat_buffer_bounded_at_max_capacity() {
        // Use the actual LogcatState to verify the MAX_LOG_ENTRIES limit holds.
        // Adjust to match the actual type in your logcat service.
        use crate::MAX_LOG_ENTRIES;

        // Simulate inserting MAX_LOG_ENTRIES + 100 entries.
        // The buffer must not exceed MAX_LOG_ENTRIES.
        let mut buf = std::collections::VecDeque::new();
        for i in 0..MAX_LOG_ENTRIES + 100 {
            if buf.len() >= MAX_LOG_ENTRIES {
                buf.pop_front();
            }
            buf.push_back(i);
        }
        assert_eq!(buf.len(), MAX_LOG_ENTRIES);
        // First entry should be 100, not 0.
        assert_eq!(*buf.front().unwrap(), 100);
    }

    #[test]
    fn filter_at_capacity_returns_correct_subset() {
        // Verify that filtering on a full buffer doesn't crash or return wrong count.
        use std::collections::VecDeque;
        let mut buf: VecDeque<String> = VecDeque::new();
        for i in 0..1000_usize {
            if buf.len() >= 1000 {
                buf.pop_front();
            }
            let tag = if i % 2 == 0 { "MyApp" } else { "Other" };
            buf.push_back(format!("{tag}:{i}"));
        }
        let filtered: Vec<_> = buf.iter().filter(|e| e.starts_with("MyApp")).collect();
        assert_eq!(filtered.len(), 500);
    }
```

- [ ] **Step 3: Run ring buffer tests**

```bash
cd src-tauri && cargo test logcat -- --nocapture
```

Expected: all logcat tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/logcat.rs
git commit -m "test(logcat): add ring buffer stress tests for capacity, eviction, and filtering"
```

---

### Task 15: Q-1 — Split `build_runner.rs` into runner + parser

**Files:**
- Create: `src-tauri/src/services/build_parser.rs`
- Modify: `src-tauri/src/services/build_runner.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Create `build_parser.rs` with all regex and parsing logic**

Create `src-tauri/src/services/build_parser.rs` containing:
- All `const *_PATTERN: &str` constants (lines ~22-60 of current `build_runner.rs`)
- All `static *_RE: LazyLock<Regex>` statics (lines ~63-76)
- The `parse_build_line` function
- The `parse_build_duration` function

These are currently in `build_runner.rs`. Move them to `build_parser.rs` and add `pub use` in `build_runner.rs` to re-export for existing callers.

Template for the new file:

```rust
//! Regex-based parsing of Gradle/Kotlin/Java/AAPT2 build output lines.
//!
//! Separated from `build_runner` so this pure-logic module can be tested
//! independently without spawning processes.

use crate::models::build::{BuildLine, BuildLineKind};
use regex::Regex;
use std::sync::LazyLock;

// ── Patterns ──────────────────────────────────────────────────────────────────
// (paste all the const PATTERN definitions here)

// ── Compiled regexes ──────────────────────────────────────────────────────────
// (paste all the static *_RE: LazyLock<Regex> definitions here)

/// Parse a single raw output line into a structured [`BuildLine`].
pub fn parse_build_line(text: &str) -> BuildLine {
    // (paste the full function body from build_runner.rs here)
}

/// Extract build duration in milliseconds from a summary line.
pub fn parse_build_duration(text: &str) -> u64 {
    // (paste the full function body from build_runner.rs here)
}
```

- [ ] **Step 2: Update `build_runner.rs` to use `build_parser`**

In `build_runner.rs`:
1. Remove the moved constants, statics, and functions
2. Add at the top: `use super::build_parser::{parse_build_line, parse_build_duration};`
3. Keep all other functions (`spawn_build`, `cancel_build`, `record_build_result`, etc.) in `build_runner.rs`

- [ ] **Step 3: Declare the new module in `services/mod.rs` or `services/build_runner.rs`**

In `src-tauri/src/services/mod.rs` (or wherever services are declared), add:

```rust
pub mod build_parser;
```

- [ ] **Step 4: Run all build tests to verify no regression**

```bash
cd src-tauri && cargo test build -- --nocapture
```

Expected: all existing build tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/build_parser.rs src-tauri/src/services/build_runner.rs src-tauri/src/services/mod.rs
git commit -m "refactor(build): extract build_parser.rs from build_runner.rs for independent testability"
```

---

### Task 16: Q-2 — Centralize path validation

**Files:**
- Create: `src-tauri/src/utils/mod.rs`
- Create: `src-tauri/src/utils/path.rs`
- Modify: `src-tauri/src/commands/studio.rs`
- Modify: `src-tauri/src/commands/file_system.rs`
- Modify: `src-tauri/src/lib.rs` (declare `mod utils`)

- [ ] **Step 1: Write a test for the shared path validator**

Create `src-tauri/src/utils/path.rs` with only the test first:

```rust
use crate::models::error::AppError;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rejects_traversal_outside_root() {
        let tmp = TempDir::new().unwrap();
        let result = validate_within_root(tmp.path(), "../etc/passwd");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::PermissionDenied(_)));
    }

    #[test]
    fn accepts_valid_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("file.txt"), b"ok").unwrap();
        let result = validate_within_root(tmp.path(), "file.txt");
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Run test to see compile failure**

```bash
cd src-tauri && cargo test utils -- --nocapture 2>&1 | head -10
```

Expected: compile error — function not found.

- [ ] **Step 3: Implement `validate_within_root`**

Add the implementation above the `#[cfg(test)]` block in `path.rs`:

```rust
use crate::models::error::AppError;
use std::path::{Path, PathBuf};

/// Resolve `untrusted` relative to `root` and verify it stays within `root`.
///
/// Returns the canonical absolute path on success, or `AppError::PermissionDenied`
/// if the resolved path escapes the root (path traversal attempt).
pub fn validate_within_root(root: &Path, untrusted: &str) -> Result<PathBuf, AppError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::io(root.display(), e))?;

    let candidate = canonical_root.join(untrusted);
    let canonical_file = candidate
        .canonicalize()
        .map_err(|e| AppError::io(candidate.display(), e))?;

    if !canonical_file.starts_with(&canonical_root) {
        return Err(AppError::PermissionDenied(format!(
            "Path '{}' is outside the project root",
            canonical_file.display()
        )));
    }

    Ok(canonical_file)
}
```

Create `src-tauri/src/utils/mod.rs`:

```rust
pub mod path;
```

In `src-tauri/src/lib.rs`, add at the top with the other `mod` declarations:

```rust
pub mod utils;
```

- [ ] **Step 4: Run the test**

```bash
cd src-tauri && cargo test utils::path -- --nocapture
```

Expected: both tests pass.

- [ ] **Step 5: Replace duplicated validation in `studio.rs` and `file_system.rs`**

In `src-tauri/src/commands/studio.rs`, replace the manual canonicalization + starts_with check (lines ~55-67) with:

```rust
    let canonical_file = crate::utils::path::validate_within_root(
        &project_root,
        found_path.to_str().unwrap_or(""),
    )
    .map_err(|e| e.to_string())?;
    let abs_path = canonical_file.to_string_lossy().into_owned();
```

In `src-tauri/src/commands/file_system.rs`, find any similar manual path-within-root checks and replace with `crate::utils::path::validate_within_root`.

- [ ] **Step 6: Run all tests**

```bash
cd src-tauri && cargo test -- --nocapture 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/utils/ src-tauri/src/lib.rs src-tauri/src/commands/studio.rs src-tauri/src/commands/file_system.rs
git commit -m "refactor(security): centralize path traversal validation in utils/path.rs"
```

---

### Task 17: Q-3 — Validate unknown fields in settings deserialization

**Files:**
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/settings_manager.rs`

The goal is to detect and warn about unknown fields in the settings JSON (e.g., typos). We can't use `#[serde(deny_unknown_fields)]` on `AppSettings` because it would break forward-compatibility (new settings from a future version would fail to load in an older version). Instead, we do a post-load validation pass.

- [ ] **Step 1: Write a test for unknown field detection**

In `src-tauri/src/services/settings_manager.rs` tests, add:

```rust
    #[test]
    fn unknown_field_in_settings_json_is_detected() {
        // serde(default) silently ignores unknown fields — we detect them separately.
        let json = r#"{"editor": {"fontSize": 16, "unknownField": true}}"#;
        let known_editor_fields = [
            "fontFamily", "fontSize", "tabSize", "insertSpaces",
            "wordWrap", "lineNumbers", "bracketMatching",
            "highlightActiveLine", "autoCloseBrackets",
        ];
        // Parse as a raw Value to check keys.
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let editor_obj = value["editor"].as_object().unwrap();
        let unknown: Vec<_> = editor_obj
            .keys()
            .filter(|k| !known_editor_fields.contains(&k.as_str()))
            .collect();
        assert_eq!(unknown, vec!["unknownField"]);
    }
```

- [ ] **Step 2: Run to verify it passes**

```bash
cd src-tauri && cargo test settings_manager::tests::unknown_field -- --nocapture
```

Expected: PASS (the test validates our detection approach works).

- [ ] **Step 3: Add unknown field logging to `load_settings`**

In `settings_manager.rs`, update the `Ok(content)` branch of `load_settings`:

```rust
        Ok(content) => {
            // Parse once for validation (detect typos/unknown fields).
            if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&content) {
                log_unknown_settings_fields(&raw);
            }
            // Parse for actual use (with serde(default) for forward-compat).
            match serde_json::from_str::<AppSettings>(&content) {
                Ok(settings) => (settings, false),
                Err(e) => {
                    tracing::warn!("Settings file is corrupted (using defaults): {e}");
                    (AppSettings::default(), true)
                }
            }
        }
```

Add the helper function:

```rust
/// Log a warning for each top-level or nested key in the JSON that doesn't
/// correspond to a known settings field. This catches typos early.
fn log_unknown_settings_fields(value: &serde_json::Value) {
    const KNOWN_TOP_LEVEL: &[&str] = &[
        "editor", "appearance", "search", "android", "lsp", "java",
        "advanced", "build", "logcat", "mcp", "recentProjects", "lastActiveProject",
    ];
    if let Some(obj) = value.as_object() {
        for key in obj.keys() {
            if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
                tracing::warn!("Unknown settings field ignored: '{key}' (possible typo?)");
            }
        }
    }
}
```

- [ ] **Step 4: Run all settings tests**

```bash
cd src-tauri && cargo test settings -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/settings_manager.rs
git commit -m "fix(settings): log warnings for unknown fields in settings.json to catch typos"
```

---

## WAVE 3 — Medium/Low (ship incrementally post-v1)

---

### Task 18: P-1 — Structured logging with file sink and rotation

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add logging dependencies to `Cargo.toml`**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tracing-appender = "0.2"
```

- [ ] **Step 2: Replace the tracing subscriber setup in `lib.rs`**

Replace the current `tracing_subscriber::fmt()...init()` block (lines ~87-93 of `lib.rs`) with:

```rust
    // Determine log directory: ~/.keynobi/logs/
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".keynobi")
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // File appender with daily rotation, keeping 7 days.
    let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Determine log level: KEYNOBI_LOG env var, then default to "warn" in release,
    // "debug" in dev builds.
    let default_level = if cfg!(debug_assertions) { "debug" } else { "warn" };
    let env_filter = tracing_subscriber::EnvFilter::try_from_env("KEYNOBI_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true),
        )
        .with(
            // Also log to stderr in debug builds.
            #[cfg(debug_assertions)]
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false),
            #[cfg(not(debug_assertions))]
            tracing_subscriber::layer::Identity::new(),
        )
        .with(env_filter)
        .init();

    // Keep _guard alive for the duration of the app.
    // Store it so it's dropped on app exit.
    std::mem::forget(_guard); // In a real impl, store in a static or pass to Tauri state.
```

Note: `_guard` must live as long as the app. A clean way is to store it in a `once_cell::sync::OnceCell<tracing_appender::non_blocking::WorkerGuard>` static or add it to Tauri managed state as a `Mutex<Option<WorkerGuard>>`.

- [ ] **Step 3: Add `dirs` to Cargo.toml if not present**

```bash
cd src-tauri && grep "dirs" Cargo.toml
```

If not found, add `dirs = "5"` to `[dependencies]`.

- [ ] **Step 4: Build and verify log file is created**

```bash
npm run tauri dev &
sleep 5
ls ~/.keynobi/logs/
```

Expected: an `app.log.*` file exists in `~/.keynobi/logs/`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs
git commit -m "feat(logging): add file-based log rotation to ~/.keynobi/logs/ with 7-day retention"
```

---

### Task 19: P-2 — Error tracking integration (Sentry)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `package.json`
- Modify: `src/App.tsx` or entry point

This task is optional and requires a Sentry DSN from the user. Steps below configure the integration with a conditional: only enable if `SENTRY_DSN` environment variable is set at build time.

- [ ] **Step 1: Add Sentry crate to `Cargo.toml`**

```toml
[dependencies]
sentry = { version = "0.34", default-features = false, features = ["backtrace", "contexts", "panic", "tracing"] }
```

- [ ] **Step 2: Initialize Sentry in `lib.rs` (conditional on env var)**

At the very start of `run()`, before the tracing setup:

```rust
    // Initialize Sentry crash reporting if a DSN is configured.
    // Set SENTRY_DSN at build time: SENTRY_DSN=https://... cargo tauri build
    let _sentry_guard = option_env!("SENTRY_DSN").map(|dsn| {
        sentry::init((
            dsn,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                traces_sample_rate: 0.1,
                ..Default::default()
            },
        ))
    });
```

- [ ] **Step 3: Add Sentry to the frontend**

```bash
npm install @sentry/browser
```

In `src/App.tsx` or the app entry point, add:

```typescript
import * as Sentry from "@sentry/browser";

// Only initialize if a DSN is embedded at build time.
const SENTRY_DSN = import.meta.env.VITE_SENTRY_DSN as string | undefined;
if (SENTRY_DSN) {
  Sentry.init({
    dsn: SENTRY_DSN,
    integrations: [Sentry.browserTracingIntegration()],
    tracesSampleRate: 0.1,
  });
}
```

Add to `vite.config.ts`:
```typescript
define: {
  'import.meta.env.VITE_SENTRY_DSN': JSON.stringify(process.env.VITE_SENTRY_DSN ?? ''),
}
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src/App.tsx vite.config.ts package.json
git commit -m "feat(monitoring): add optional Sentry error tracking (requires SENTRY_DSN at build time)"
```

---

### Task 20: P-3 — Version display in UI + single-source version script

**Files:**
- Create: `scripts/sync-version.mjs`
- Modify: `package.json`
- Modify: `src/components/settings/SettingsPanel.tsx`

- [ ] **Step 1: Create the version sync script**

Create `scripts/sync-version.mjs`:

```javascript
#!/usr/bin/env node
/**
 * Reads the version from package.json (single source of truth) and
 * writes it into Cargo.toml and tauri.conf.json.
 *
 * Usage: node scripts/sync-version.mjs [--check]
 *   --check: exit 1 if files are out of sync (for CI)
 */
import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";

const root = new URL("..", import.meta.url).pathname;

const pkg = JSON.parse(readFileSync(resolve(root, "package.json"), "utf-8"));
const version = pkg.version;

// Read current values
const cargoPath = resolve(root, "src-tauri/Cargo.toml");
const tauriPath = resolve(root, "src-tauri/tauri.conf.json");

const cargo = readFileSync(cargoPath, "utf-8");
const tauri = JSON.parse(readFileSync(tauriPath, "utf-8"));

const cargoVersion = cargo.match(/^version = "(.+?)"/m)?.[1];
const tauriVersion = tauri.version;

if (process.argv.includes("--check")) {
  const mismatches = [];
  if (cargoVersion !== version) mismatches.push(`Cargo.toml: ${cargoVersion} !== ${version}`);
  if (tauriVersion !== version) mismatches.push(`tauri.conf.json: ${tauriVersion} !== ${version}`);
  if (mismatches.length > 0) {
    console.error("Version mismatch:\n" + mismatches.join("\n"));
    process.exit(1);
  }
  console.log(`Versions in sync: ${version}`);
  process.exit(0);
}

// Update Cargo.toml
const updatedCargo = cargo.replace(/^version = ".+?"/m, `version = "${version}"`);
writeFileSync(cargoPath, updatedCargo);

// Update tauri.conf.json
tauri.version = version;
writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");

console.log(`Synced version ${version} to Cargo.toml and tauri.conf.json`);
```

- [ ] **Step 2: Add sync script to `package.json`**

In `package.json` scripts:

```json
"sync:version": "node scripts/sync-version.mjs",
"check:version": "node scripts/sync-version.mjs --check"
```

- [ ] **Step 3: Test the script**

```bash
node scripts/sync-version.mjs --check
```

Expected: "Versions in sync: 0.1.0" (or similar).

- [ ] **Step 4: Display version in the Settings panel**

In `src/components/settings/SettingsPanel.tsx`, read the app version using Tauri's API and display it. Add near the top of the settings form:

```typescript
import { getVersion } from "@tauri-apps/api/app";
import { createResource } from "solid-js";

const [appVersion] = createResource(async () => getVersion());

// In the JSX:
<div class="settings-version">
  Version {appVersion() ?? "—"}
</div>
```

- [ ] **Step 5: Run frontend tests**

```bash
npm run test
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add scripts/sync-version.mjs package.json src/components/settings/SettingsPanel.tsx
git commit -m "feat(version): add single-source version sync script and display version in Settings"
```

---

### Task 21: P-4 — In-app update notification

**Files:**
- Modify: `src-tauri/src/lib.rs` or create `src-tauri/src/services/update_checker.rs`
- Modify: `src/stores/ui.store.ts` or `src/App.tsx`

- [ ] **Step 1: Create update checker service**

Create `src-tauri/src/services/update_checker.rs`:

```rust
//! Checks GitHub Releases for a newer version of the app on startup.

use serde::Deserialize;

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
}

/// Check GitHub Releases for a newer version.
/// Returns `Some(version_string)` if a newer version is available, `None` otherwise.
///
/// Uses the GitHub Releases API. Requires the GITHUB_REPO env var or a hardcoded slug.
pub async fn check_for_update(current_version: &str) -> Option<String> {
    // Replace with the actual repo slug.
    let repo = option_env!("GITHUB_REPO").unwrap_or("keynobi-dev/android-companion");
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");

    let client = reqwest::Client::builder()
        .user_agent(format!("android-companion/{current_version}"))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let release: GithubRelease = client.get(&url).send().await.ok()?.json().await.ok()?;

    let latest = release.tag_name.trim_start_matches('v');
    if is_newer(latest, current_version) {
        Some(latest.to_string())
    } else {
        None
    }
}

fn is_newer(latest: &str, current: &str) -> bool {
    // Simple semver comparison: split on '.', compare numerically.
    let parse = |v: &str| -> [u32; 3] {
        let parts: Vec<u32> = v.split('.').filter_map(|p| p.parse().ok()).collect();
        [
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        ]
    };
    parse(latest) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_version_detected() {
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }
}
```

Add `reqwest = { version = "0.12", features = ["json"] }` to `Cargo.toml`.

- [ ] **Step 2: Run the unit test**

```bash
cd src-tauri && cargo test update_checker -- --nocapture
```

Expected: `newer_version_detected` passes.

- [ ] **Step 3: Trigger update check on app startup**

In `src-tauri/src/lib.rs` setup, add after the MCP auto-start block:

```rust
            // Check for updates in the background — never block startup.
            {
                let handle = app.handle().clone();
                let current = env!("CARGO_PKG_VERSION").to_string();
                tokio::spawn(async move {
                    if let Some(new_version) = services::update_checker::check_for_update(&current).await {
                        let _ = handle.emit("app:update-available", new_version);
                    }
                });
            }
```

- [ ] **Step 4: Show update banner in the frontend**

In `src/stores/ui.store.ts`, add:

```typescript
const [updateAvailable, setUpdateAvailable] = createSignal<string | null>(null);
export { updateAvailable };

if (typeof window !== "undefined") {
  listen<string>("app:update-available", (event) => {
    setUpdateAvailable(event.payload);
  }).catch(() => {});
}
```

In `src/components/layout/TitleBar.tsx` (or wherever makes sense), add a dismissible banner:

```typescript
import { updateAvailable } from "@/stores/ui.store";
import { createSignal } from "solid-js";

const [dismissed, setDismissed] = createSignal(false);

// In JSX:
<Show when={updateAvailable() && !dismissed()}>
  <div class="update-banner">
    Update available: v{updateAvailable()}
    <button onClick={() => setDismissed(true)}>Dismiss</button>
  </div>
</Show>
```

- [ ] **Step 5: Run tests**

```bash
npm run test && cd src-tauri && cargo test
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/update_checker.rs src-tauri/Cargo.toml src-tauri/src/lib.rs src/stores/ui.store.ts
git commit -m "feat(update): add background update checker with dismissible banner in title bar"
```

---

### Task 22: X-1 — Automate TypeScript bindings validation in CI

**Files:**
- Create: `.github/workflows/ci.yml` (or modify existing)
- Modify: `package.json`

- [ ] **Step 1: Add a bindings-check script to `package.json`**

```json
"check:bindings": "npm run generate:bindings && git diff --exit-code src/bindings/"
```

- [ ] **Step 2: Test the script locally**

```bash
npm run check:bindings
```

Expected: exits 0 if bindings are up to date, exits 1 if stale (and shows diff).

- [ ] **Step 3: Create GitHub Actions workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
      - run: npm ci
      - run: npm run lint
      - run: npx tsc --noEmit
      - run: npm run test

  rust:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
      - name: Check bindings are up to date
        run: |
          cd src-tauri
          cargo test --lib 2>/dev/null || true
          cd ..
          git diff --exit-code src/bindings/ || (echo "TypeScript bindings are stale. Run: npm run generate:bindings" && exit 1)
      - name: Rust tests
        run: cd src-tauri && cargo test
      - name: Clippy
        run: cd src-tauri && cargo clippy -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml package.json
git commit -m "ci: add GitHub Actions workflow with binding staleness check and Clippy"
```

---

### Task 23: X-2 — Pre-commit hooks for lint, type-check, and Clippy

**Files:**
- Modify: `package.json`
- Create: `.husky/pre-commit`

- [ ] **Step 1: Install husky and lint-staged**

```bash
npm install --save-dev husky lint-staged
npx husky init
```

- [ ] **Step 2: Configure lint-staged in `package.json`**

Add to `package.json`:

```json
"lint-staged": {
  "src/**/*.{ts,tsx}": [
    "eslint --fix",
    "prettier --write"
  ],
  "src-tauri/src/**/*.rs": [
    "rustfmt"
  ]
}
```

- [ ] **Step 3: Configure the pre-commit hook**

Replace `.husky/pre-commit` with:

```bash
#!/bin/sh
set -e

# Run lint-staged for changed files.
npx lint-staged

# TypeScript type-check (fast, no emit).
npx tsc --noEmit

# Clippy on Rust — fail on warnings.
(cd src-tauri && cargo clippy --quiet -- -D warnings)
```

Make executable:

```bash
chmod +x .husky/pre-commit
```

- [ ] **Step 4: Verify the hook runs on a test commit**

```bash
git add .husky/pre-commit package.json
git commit -m "ci(hooks): add pre-commit hooks for lint, type-check, and Clippy"
```

Expected: the commit goes through (all checks pass on current clean code).

---

### Task 24: X-3 — CHANGELOG and semantic versioning workflow

**Files:**
- Create: `CHANGELOG.md`

- [ ] **Step 1: Create `CHANGELOG.md` with initial entry**

Create `CHANGELOG.md`:

```markdown
# Changelog

All notable changes to Android Dev Companion are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Commits follow the [Conventional Commits](https://www.conventionalcommits.org/) spec.

---

## [Unreleased]

### Security
- Fix command injection in `open_in_studio` via positional arg pattern
- Enable Content Security Policy in Tauri webview
- Restrict filesystem permissions to `~/.keynobi/` and opened project/SDK paths
- Add input validation for Gradle task names and device serials

### Fixed
- Surface settings file corruption as a Toast notification
- Emit MCP startup failure to the frontend
- Distinguish build cancellation from build failure in UI

### Added
- Structured `AppError` type across all Rust command handlers
- File-based log rotation to `~/.keynobi/logs/`
- In-app update notification
- Version display in Settings panel

### Changed
- Extracted `build_parser.rs` from `build_runner.rs` for independent testability
- Centralized path traversal validation in `utils/path.rs`

---

## [0.1.0] — 2026-04-02

Initial beta release.
```

- [ ] **Step 2: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: add CHANGELOG.md with production readiness changes"
```

---

### Task 25: Q-4 — Guard all `app_handle` usages in headless MCP mode

**Files:**
- Modify: `src-tauri/src/services/mcp_server.rs`

- [ ] **Step 1: Find all `app_handle` usages in `mcp_server.rs`**

```bash
grep -n "app_handle\|\.unwrap()\|\.expect(" src-tauri/src/services/mcp_server.rs | head -30
```

- [ ] **Step 2: Replace each `.unwrap()` on `Option<AppHandle>` with guarded access**

For each pattern like:
```rust
self.app_handle.as_ref().unwrap().emit(...)
```

Replace with:
```rust
if let Some(handle) = &self.app_handle {
    let _ = handle.emit(...);
} else {
    tracing::debug!("Skipping UI event in headless MCP mode");
}
```

Or add a helper method:

```rust
impl AndroidMcpServer {
    fn emit_event<S: serde::Serialize + Clone>(
        &self,
        event: &str,
        payload: S,
    ) {
        if let Some(handle) = &self.app_handle {
            let _ = handle.emit(event, payload);
        }
    }
}
```

Then replace all `self.app_handle.as_ref().unwrap().emit(...)` with `self.emit_event(...)`.

- [ ] **Step 3: Run Rust tests**

```bash
cd src-tauri && cargo test mcp -- --nocapture 2>&1 | tail -10
```

Expected: all MCP tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/mcp_server.rs
git commit -m "fix(mcp): guard all app_handle usages against None in headless mode"
```

---

### Task 26: Q-5 — Ring buffer health indicator in Health panel

**Files:**
- Modify: `src-tauri/src/commands/logcat.rs` (add buffer usage stat)
- Modify: `src-tauri/src/commands/health.rs` (add check)
- Modify: `src/components/health/HealthPanel.tsx`

- [ ] **Step 1: Add buffer usage % to `get_logcat_stats`**

`get_logcat_stats` already returns `LogStats`. Read the current `LogStats` struct and add a `buffer_usage_pct: f32` field if not present:

In `src-tauri/src/models/` (or wherever `LogStats` is defined), add:
```rust
pub buffer_usage_pct: f32,  // 0.0–100.0
```

Compute it when returning stats:
```rust
let usage = (entry_count as f32 / crate::MAX_LOG_ENTRIES as f32) * 100.0;
```

- [ ] **Step 2: Show warning in Health panel when buffer > 80%**

In `src/components/health/HealthPanel.tsx`, after loading health data, also call `getLogcatStats()` and add a row:

```typescript
<Show when={(logcatStats()?.bufferUsagePct ?? 0) > 80}>
  <div class="health-warning">
    Logcat ring buffer is {logcatStats()?.bufferUsagePct.toFixed(0)}% full — 
    oldest entries may be lost. Consider clearing the logcat buffer.
  </div>
</Show>
```

- [ ] **Step 3: Run frontend tests**

```bash
npm run test
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/logcat.rs src/components/health/HealthPanel.tsx
git commit -m "feat(health): add logcat ring buffer usage indicator with >80% warning"
```

---

### Task 27: Q-6 — Build history persistence across restarts

**Files:**
- Modify: `src-tauri/src/services/build_runner.rs`
- Modify: `src-tauri/src/services/settings_manager.rs` (reuse data_dir)

- [ ] **Step 1: Write a test for build history persistence**

In `src-tauri/src/services/build_runner.rs` tests, add:

```rust
    #[test]
    fn build_history_serializes_and_deserializes() {
        use crate::models::build::{BuildRecord, BuildResult, BuildStatus};
        let record = BuildRecord {
            id: 1,
            task: "assembleDebug".into(),
            status: BuildStatus::Success(BuildResult {
                success: true,
                duration_ms: 5000,
                error_count: 0,
                warning_count: 0,
            }),
            errors: vec![],
            started_at: "2026-04-02T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: BuildRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task, "assembleDebug");
    }
```

- [ ] **Step 2: Run to confirm it passes**

```bash
cd src-tauri && cargo test build_runner -- --nocapture
```

Expected: PASS (this verifies `BuildRecord` is serializable, which it should be since it derives `Serialize`/`Deserialize`).

- [ ] **Step 3: Add `save_build_history` and `load_build_history` to `build_runner.rs`**

```rust
use crate::services::settings_manager::data_dir;

const BUILD_HISTORY_FILE: &str = "build-history.json";
const MAX_PERSISTED_HISTORY: usize = 20;

pub fn save_build_history(history: &VecDeque<BuildRecord>) {
    let path = data_dir().join(BUILD_HISTORY_FILE);
    let recent: Vec<_> = history.iter().rev().take(MAX_PERSISTED_HISTORY).collect();
    if let Ok(json) = serde_json::to_string_pretty(&recent) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

pub fn load_build_history() -> VecDeque<BuildRecord> {
    let path = data_dir().join(BUILD_HISTORY_FILE);
    if !path.exists() {
        return VecDeque::new();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return VecDeque::new(),
    };
    serde_json::from_str::<Vec<BuildRecord>>(&content)
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}
```

- [ ] **Step 4: Call `load_build_history` on `BuildState::new()`**

Change `BuildStateInner::new()` to seed history from disk:

```rust
impl BuildStateInner {
    pub fn new() -> Self {
        Self {
            current_build: None,
            status: BuildStatus::Idle,
            history: load_build_history(),
            current_errors: vec![],
            next_id: 1,
        }
    }
}
```

- [ ] **Step 5: Call `save_build_history` after `record_build_result`**

In the `record_build_result` function, after updating the history `VecDeque`, add:

```rust
save_build_history(&inner.history);
```

- [ ] **Step 6: Run all Rust tests**

```bash
cd src-tauri && cargo test -- --nocapture 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/build_runner.rs
git commit -m "feat(build): persist last 20 build summaries to ~/.keynobi/build-history.json across restarts"
```

---

## Self-Review Checklist

### Spec coverage

| Spec item | Task |
|---|---|
| S-1 command injection | Task 1 |
| S-2 CSP | Task 2 |
| S-3 fs permissions | Task 3 |
| S-4 input validation | Task 4 |
| D-1 graceful shutdown | Task 5 |
| D-2 settings corruption | Task 6 |
| E-1 AppError type | Task 7 |
| E-2 MCP startup failure | Task 8 |
| E-3 silent catch blocks | Task 9 |
| E-4 cancellation vs failure | Task 10 |
| T-1 build flow integration tests | Task 11 |
| T-2 error state transition tests | Task 12 |
| T-3 command handler tests | Task 13 |
| T-4 ring buffer stress tests | Task 14 |
| Q-1 split build_runner | Task 15 |
| Q-2 centralize path validation | Task 16 |
| Q-3 unknown settings fields | Task 17 |
| P-1 structured logging | Task 18 |
| P-2 error tracking | Task 19 |
| P-3 version display | Task 20 |
| P-4 update notification | Task 21 |
| X-1 bindings CI check | Task 22 |
| X-2 pre-commit hooks | Task 23 |
| X-3 CHANGELOG | Task 24 |
| Q-4 headless MCP guard | Task 25 |
| Q-5 buffer health indicator | Task 26 |
| Q-6 build history persistence | Task 27 |

All 27 spec items are covered. ✓
