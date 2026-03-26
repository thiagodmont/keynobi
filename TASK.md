# Android Dev Companion — Task List

> Organized by phase. Check off tasks as completed.
> Reference: PLAN.md for full architecture details.

---

## Phase 1 — Foundation (COMPLETE)

- [x] Tauri 2.0 + SolidJS + Vite project scaffolding
- [x] App shell layout (title bar, main panel area, status bar)
- [x] Gradle root detection (`find_gradle_root`)
- [x] Project open command (`open_project`)
- [x] Settings persistence (`~/.androidide/settings.json`)
- [x] Dark theme with CSS custom properties
- [x] Action registry + command palette (Cmd+Shift+P)
- [x] Keyboard shortcut system
- [x] Toast notification system
- [x] Error boundary

---

## Phase 2 — Build System + Devices (COMPLETE)

- [x] `process_manager.rs`: spawn processes, stream stdout/stderr, SIGTERM cancellation
- [x] `build_runner.rs`: `./gradlew` execution, output parsing (Kotlin errors/warnings), build history
- [x] `BuildPanel.tsx`: virtualized streaming log, ANSI colors, error list, build toolbar
- [x] `variant_manager.rs`: parse `build.gradle.kts` for `buildTypes` and `productFlavors`
- [x] `VariantSelector.tsx`: searchable dropdown in status bar
- [x] `adb_manager.rs`: device listing (poll every 2s), APK install, app launch
- [x] `DevicePanel.tsx`: connected devices + AVD list, launch/stop emulators
- [x] Run button: variant → build → install → launch
- [x] Status bar: build status, variant, device indicators
- [x] Health check system (Java, SDK, ADB, Gradle wrapper, disk space)
- [x] `HealthPanel.tsx`: detailed health report with fix suggestions

---

## Phase 3 — Logcat (COMPLETE)

- [x] `services/logcat.rs`: parse `threadtime` format, ring buffer (50K entries), batch emit
- [x] `commands/logcat.rs`: `start_logcat`, `stop_logcat`, `clear_logcat`, `get_logcat_entries`
- [x] Logcat state registered in `lib.rs`
- [x] `LogcatPanel.tsx`: real-time display, level colors, tag/text/level filters
- [x] Crash detection (`FATAL EXCEPTION`, `AndroidRuntime`)
- [x] Start/Stop/Pause/Clear controls
- [x] Auto-scroll with scroll-to-disable behavior
- [x] Logcat IPC wrappers in `tauri-api.ts`

---

## Phase 4 — MCP Server (TODO)

**Goal:** Claude Code can interact with the IDE via MCP protocol.

### MCP Server (Rust)

- [ ] Add MCP protocol handler in Rust (`src-tauri/src/commands/mcp.rs` or `src-tauri/src/services/mcp_server.rs`)
  - [ ] Implement `initialize` handshake (MCP 2024-11-05 spec)
  - [ ] Implement `tools/list` — return all exposed tool schemas
  - [ ] Implement `tools/call` — dispatch to service implementations
  - [ ] stdio transport (for `claude mcp add android-companion --command "..."`
  - [ ] HTTP+SSE transport on localhost (for other MCP clients)
  - [ ] Error handling: `INTERNAL_ERROR`, `INVALID_PARAMS` codes

### MCP Tool Implementations

- [ ] **Build tools**
  - [ ] `run_gradle_task(task, variant?)` → delegates to `build_runner`
  - [ ] `get_build_status()` → returns current `BuildStatus`
  - [ ] `get_build_errors()` → returns structured `BuildError[]`
  - [ ] `get_build_log(lines?)` → returns last N lines from build buffer
  - [ ] `cancel_build()` → delegates to `process_manager`
  - [ ] `list_build_variants()` → delegates to `variant_manager`
  - [ ] `set_active_variant(variant)` → delegates to `variant_manager`

- [ ] **Logcat tools**
  - [ ] `get_logcat_entries(count?, filter?)` → reads from `LogcatBuffer`
  - [ ] `get_crash_logs(count?)` → filters `is_crash = true`
  - [ ] `clear_logcat()` → delegates to logcat commands

- [ ] **Device tools**
  - [ ] `list_devices()` → delegates to `adb_manager`
  - [ ] `install_apk(device, path)` → delegates to `adb_manager`
  - [ ] `launch_app(device, package, activity?)` → delegates to `adb_manager`
  - [ ] `stop_app(device, package)` → delegates to `adb_manager`
  - [ ] `list_avds()` → delegates to `adb_manager`
  - [ ] `launch_avd(name)` → delegates to `adb_manager`
  - [ ] `stop_avd(serial)` → delegates to `adb_manager`

### Documentation
- [ ] Add MCP server status indicator to status bar
- [ ] Document setup in README: `claude mcp add android-companion`
- [ ] Add MCP connection info to Health panel

---

## Phase 5 — Polish + UX (TODO)

### Logcat Improvements
- [ ] Logcat session save to file (JSON or plain text)
- [ ] "Jump to Crash" button when crash is detected
- [ ] Package filter (filter by app package name from device)
- [ ] Regex mode for tag/text filter
- [ ] Timestamp format toggle (relative vs absolute)
- [ ] Copy selected entry to clipboard

### Build Improvements
- [ ] Click error in Build panel to open file in external editor (Reveal in Finder / VS Code)
- [ ] Build history dropdown with search
- [ ] Gradle sync detection and suggestion
- [ ] Build time tracking in status bar

### Device Improvements
- [ ] Emulator controls: GPS, network speed, battery level
- [ ] Screenshot capture button
- [ ] Screen recording

### App Polish
- [ ] First-run onboarding wizard (SDK detection)
- [ ] "Open Recent" projects list
- [ ] Keyboard shortcut customization
- [ ] Auto-start logcat when device connects
- [ ] macOS code signing + notarization
- [ ] Universal binary (Apple Silicon + Intel)
- [ ] DMG packaging

---

## Ongoing / Cross-Cutting

- [ ] Write Rust unit tests for logcat parser edge cases
- [ ] Write Vitest tests for LogcatPanel store interactions
- [ ] Set up CI/CD (GitHub Actions):
  - [ ] `cargo test` on every PR
  - [ ] `cargo clippy` lint check
  - [ ] `npm run test` for frontend
  - [ ] Build macOS artifact on main branch
- [ ] Keep PLAN.md updated as architectural decisions evolve
- [ ] Record architectural decisions in DECISIONS.md
