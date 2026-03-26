# Android Dev Companion — Project Plan

## Vision

A focused **Android development companion** providing best-in-class build logs, logcat viewing, and device management. Designed to work alongside Android Studio (for coding) and Claude Code (for AI assistance via MCP).

**Target**: macOS only  
**Language support**: Kotlin + Gradle projects (project-open detection)

---

## Technology Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| **App Framework** | **Tauri 2.0** | 20–40 MB RAM baseline. Rust backend for heavy lifting. Native WKWebView. Channel API for high-throughput streaming. |
| **Frontend** | **SolidJS + TypeScript + Vite** | Fine-grained reactivity — critical for streaming logcat (50K+ entries). |
| **Backend** | **Rust (tokio async runtime)** | ADB process management, build streaming, logcat parsing. |
| **IPC Types** | **ts-rs** (Rust → TypeScript auto-generation) | Type-safe IPC. |
| **State Management** | **SolidJS Stores** | Reactive, fine-grained updates. |
| **Testing** | **Vitest** (frontend), Rust `#[test]` (backend) | |

---

## Architecture

### High-Level Overview

```
+------------------------------------------------------------------+
|                    macOS Application (Tauri 2.0)                  |
|                                                                   |
|  +---------------------------+  +-------------------------------+ |
|  |   Frontend (WKWebView)    |  |    Rust Backend (src-tauri)   | |
|  |                           |  |                               | |
|  |  SolidJS + TypeScript     |  |  Core Services                | |
|  |  ┌─────────────────────┐  |  |   fs_manager      (gradle)   | |
|  |  │ Build Panel          │  |  |   process_manager             | |
|  |  │ Logcat Panel         │  |  |                               | |
|  |  │ Device Panel         │  |  |  Android Services             | |
|  |  │ Health Panel         │  |  |   adb_manager                 | |
|  |  │ Settings Panel       │  |  |   logcat (ring buffer)        | |
|  |  │ Command Palette      │  |  |   build_runner  (gradlew)     | |
|  |  └─────────────────────┘  |  |   variant_manager             | |
|  +---------------------------+  +-------------------------------+ |
+------------------------------------------------------------------+
          |                                    |
          v                                    v
  Android Device / Emulator          Claude Code (MCP server)
  adb logcat + build output          get_recent_logs, run_build,
                                     list_devices, etc.
```

### Key Data Flows

**1. Build streaming**
```
User clicks Run (or Claude calls run_gradle_task)
  → Rust spawns ./gradlew assembleDebug (or active variant task)
  → stdout/stderr streamed line-by-line via Tauri Channel
  → Rust parses: errors, warnings, task progress
  → Build Panel: ANSI log + structured error list
  → On success: adb install → adb shell am start
```

**2. Logcat streaming**
```
Rust: adb logcat -v threadtime (continuous)
  → Parse threadtime format → structured LogcatEntry
  → Ring buffer (50K entries, evict oldest on overflow)
  → Batch at 100ms intervals → Channel to frontend
  → SolidJS renders visible rows only
  → MCP tool get_logcat_entries() available to Claude Code at all times
```

**3. Claude Code integration (MCP)**
```
Claude Code connects via: claude mcp add android-companion
  → MCP server exposes tools: run_gradle_task, get_build_errors,
    get_logcat_entries, list_devices, install_apk, etc.
  → Claude can trigger builds, read logs, manage devices
  → IDE shows live results in panels as Claude works
```

---

## Project Structure

```
android-ide/
├── src-tauri/                          # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs                      # Tauri setup, state, command registration
│       ├── commands/
│       │   ├── build.rs               # Gradle task execution
│       │   ├── device.rs              # ADB device management
│       │   ├── file_system.rs         # Project open, Gradle root detection
│       │   ├── health.rs              # System health checks
│       │   ├── logcat.rs              # Logcat streaming commands
│       │   ├── settings.rs            # App settings
│       │   └── variant.rs             # Build variant discovery
│       ├── services/
│       │   ├── adb_manager.rs         # ADB device polling
│       │   ├── build_runner.rs        # ./gradlew execution, output parsing
│       │   ├── fs_manager.rs          # Gradle root detection
│       │   ├── logcat.rs              # Parser, filter engine, ring buffer (50K entries)
│       │   ├── process_manager.rs     # Child process lifecycle
│       │   ├── settings_manager.rs    # Settings persistence
│       │   └── variant_manager.rs     # Build variant parsing
│       └── models/
│           ├── build.rs
│           ├── device.rs
│           ├── error.rs
│           ├── health.rs
│           ├── log_entry.rs
│           ├── settings.rs
│           └── variant.rs
│
└── src/                                # Frontend (SolidJS + TypeScript)
    ├── App.tsx                         # Root layout — tab bar + panels
    ├── stores/
    │   ├── build.store.ts             # Build state, logs, errors
    │   ├── device.store.ts            # Connected devices, emulator state
    │   ├── health.store.ts            # IDE health checks
    │   ├── project.store.ts           # Project root, name, Gradle root
    │   ├── settings.store.ts          # App settings
    │   ├── ui.store.ts                # Active tab state
    │   ├── variant.store.ts           # Build variants
    │   └── log.store.ts               # Generic log store factory
    ├── services/
    │   ├── build.service.ts           # Build orchestration
    │   └── project.service.ts         # Project open flow
    ├── components/
    │   ├── build/
    │   │   ├── BuildPanel.tsx         # ANSI log + structured error list
    │   │   └── VariantSelector.tsx    # Build variant dropdown
    │   ├── device/
    │   │   └── DevicePanel.tsx        # Connected devices + AVD management
    │   ├── logcat/
    │   │   └── LogcatPanel.tsx        # Streaming logcat with filters
    │   ├── health/
    │   │   └── HealthPanel.tsx        # SDK, ADB, Java, Gradle checks
    │   ├── settings/
    │   │   ├── SettingsPanel.tsx      # Settings UI
    │   │   ├── SettingRow.tsx         # Setting row components
    │   │   └── ToolStatus.tsx         # SDK/Java path pickers
    │   ├── layout/
    │   │   ├── TitleBar.tsx           # Custom macOS title bar
    │   │   └── StatusBar.tsx          # Health, build status, variant, device
    │   └── common/
    │       ├── CommandPalette.tsx     # Cmd+Shift+P command palette
    │       ├── LogViewer.tsx          # Shared ANSI log viewer
    │       ├── Toast.tsx              # Toast notifications
    │       ├── Dialog.tsx             # Modal dialogs
    │       ├── Resizable.tsx          # Resizable panel splitter
    │       ├── ErrorBoundary.tsx      # Error boundary
    │       └── Icon.tsx               # Icon component
    └── lib/
        ├── tauri-api.ts              # Typed Tauri IPC wrappers
        ├── keybindings.ts            # Keyboard shortcut registry
        ├── action-registry.ts        # Command palette actions
        ├── fuzzy-match.ts            # Fuzzy matching for command palette
        ├── ansi-strip.ts             # ANSI escape code stripping
        └── file-utils.ts             # File type utilities
```

---

## MCP Tools Exposed by the IDE

The IDE runs an MCP server so Claude Code can interact with Android development:

### Build Operations
- `run_gradle_task(task, variant?)` — Execute any Gradle task
- `get_build_status()` — Current build state
- `get_build_errors()` — Errors and warnings from last build
- `get_build_log(lines?)` — Recent build output
- `cancel_build()` — Cancel running build
- `list_build_variants()` / `set_active_variant(variant)`

### Logcat Operations
- `get_logcat_entries(count?, filter?)` — Read recent logcat entries
- `get_crash_logs()` — Recent crash stack traces
- `clear_logcat()` — Clear buffer

### Device Operations
- `list_devices()` — Connected devices and emulators
- `install_apk(device, path)` — Install an APK
- `launch_app(device, package)` — Launch app
- `stop_app(device, package)` — Stop app
- `list_avds()` / `launch_avd(name)` / `stop_avd(name)`

---

## Development Phases

### Phase 1 — Foundation (DONE)
- Tauri + SolidJS project scaffolding
- App shell layout (title bar, panels, status bar)
- File system backend (Gradle root detection)
- Settings persistence
- Health check system (Java, SDK, ADB, Gradle, disk space)

### Phase 2 — Build System + Devices (DONE)
- Gradle build runner (`./gradlew` subprocess, streaming output)
- Build panel with ANSI log + structured error list
- Build variant discovery and selector
- ADB device management and polling
- Device panel with AVD launching
- Run → Build → Install → Launch cycle

### Phase 3 — Logcat (DONE)
- `adb logcat -v threadtime` streaming via process
- Ring buffer (50K entries) in Rust
- LogcatPanel with real-time display
- Level-based filtering (V/D/I/W/E/F)
- Tag and text filtering
- Crash detection and highlighting
- 100ms batch-emit to frontend for performance

### Phase 4 — MCP Server (Next)
- MCP protocol handler in Rust (stdio + HTTP transports)
- Expose build, logcat, and device tools
- Claude Code integration: `claude mcp add android-companion`

### Phase 5 — Polish + UX
- Better Build error highlighting and navigation
- Logcat session save/export
- Device emulator controls (GPS, network, battery)
- Screenshots from devices
- First-run onboarding (SDK setup wizard)
- Performance profiling

---

## Key Rust Crates

| Crate | Purpose |
|-------|---------|
| `tauri` + `tauri-build` | App framework |
| `serde` + `serde_json` | JSON serialization |
| `tokio` | Async runtime |
| `thiserror` | Typed error handling |
| `tracing` + `tracing-subscriber` | Structured logging |
| `ts-rs` | TypeScript bindings from Rust types |
| `dirs` | Home directory resolution |
| `chrono` | Timestamps for log entries |
| `futures-util` | Async utilities |
| `libc` | SIGTERM for process cancellation |
| `regex` | Build output parsing |

---

## Verification Checklist

After a development session, verify:

1. Open an Android project folder — project name shown in title bar
2. Trigger `assembleDebug` build — streaming logs appear in Build panel
3. Switch build variant — correct Gradle task runs
4. Connect Android emulator — appears in Devices panel and status bar
5. Start logcat — entries stream with correct colors (V=gray, D=blue, I=green, W=yellow, E=red, F=purple)
6. Apply logcat filters (tag, level, text) — entries filter correctly
7. Install and launch APK via Run button
8. Health checks panel shows correct status for SDK, ADB, Java
9. Command palette (Cmd+Shift+P) lists all actions
10. Settings persist across app restarts
