# Changelog

All notable changes to Android Dev Companion are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)  
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)  
Commits: [Conventional Commits](https://www.conventionalcommits.org/)

---

## [Unreleased]

### Security
- Fix shell injection in `open_in_studio` — path now passed as positional arg, never interpolated into shell script string
- Enable Content Security Policy in Tauri webview (was `null`)
- Restrict filesystem permissions to `~/.keynobi/` and runtime-registered project/SDK paths (was entire home directory)
- Add allowlist validation for Gradle task names and ADB device serials

### Fixed
- Graceful shutdown: cancel running build and stop logcat/ADB polling on window close (3s timeout)
- Surface settings file corruption as a Toast notification instead of silently resetting
- Emit `mcp:startup-failed` event to frontend when MCP auto-start fails
- Resolve build promise immediately on cancel (was hanging 5 minutes until timeout)
- Return `AppError::NotFound` when APK file is missing in `install_apk_on_device`
- Device polling loop and logcat pipeline now observe their stop flags on shutdown
- Canonicalize paths before registering in Tauri fs scope

### Added
- Structured `AppError` type across all Rust command handlers with TypeScript bindings
- `ProcessTermination` enum: frontend now shows "Build cancelled" vs "Build failed" correctly
- File-based log rotation to `~/.keynobi/logs/` with configurable retention (default: 7 days)
- Version display in Settings panel; `npm run version:sync` keeps package.json/Cargo.toml/tauri.conf.json in sync
- `utils/path.rs`: centralized path traversal validation helper
- Post-load warning for unknown/misspelled keys in `settings.json`
- Build output parser extracted to `build_parser.rs` for independent testability
- GitHub Actions CI workflow with TypeScript bindings staleness check and Clippy

### Tests
- 22 build-flow integration tests with mock gradlew fixture
- 15 store error-transition tests (build cancelled/failed/start-failure, settings IPC rejection, device disconnect)
- Ring buffer stress tests: capacity, eviction, filter correctness at capacity
- Command handler boundary tests: serial/task name validation edge cases
- Path validation unit tests in `utils/path.rs`

---

## [0.1.0] — 2026-04-02

Initial beta release.

### Features
- Open Android/Gradle project, multi-project registry
- Streaming Gradle builds with ANSI log + structured error list
- Build variant selector (buildTypes × productFlavors)
- One-click Run → Build → Install → Launch
- Real-time logcat streaming (50K ring buffer, 100ms batching)
- Logcat filters: level, tag, text; crash detection
- Connected device + AVD management; Create/Delete/Wipe AVDs
- System health checks (Java, SDK, ADB, Gradle, disk)
- Command palette (Cmd+Shift+P) with action registry
- Project app info editor (versionName / versionCode)
