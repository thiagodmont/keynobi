# Android Dev Companion

A focused **Android development companion** built with Tauri 2.0 + SolidJS. Not a full IDE — designed to run *alongside* Android Studio and Claude Code, providing best-in-class build logs, logcat streaming, device management, and an MCP server so AI agents can trigger builds, read crash logs, and control devices directly.

**Platform:** macOS only (v0.x beta)  
**Language support:** Kotlin + Gradle projects

---

## Table of Contents

- [What It Does](#what-it-does)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Development Workflow](#development-workflow)
- [Building a DMG for Distribution](#building-a-dmg-for-distribution)
- [Project Structure](#project-structure)
- [Architecture Overview](#architecture-overview)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Troubleshooting](#troubleshooting)
- [Roadmap](#roadmap)

---

## What It Does

| Feature | Status |
|---|---|
| Open Android/Gradle project, multi-project registry | ✅ |
| Streaming Gradle builds with ANSI log + structured error list | ✅ |
| Build variant selector (buildTypes × productFlavors) | ✅ |
| One-click Run → Build → Install → Launch | ✅ |
| Real-time logcat streaming (50K ring buffer, 100 ms batching) | ✅ |
| Logcat filters: level, tag, text; crash detection | ✅ |
| Connected device + AVD management; Create/Delete/Wipe AVDs | ✅ |
| System health checks (Java, SDK, ADB, Gradle, disk) | ✅ |
| Command palette (`Cmd+Shift+P`) with action registry | ✅ |
| Project app info editor (versionName / versionCode) | ✅ |
| MCP server — Claude Code integration | 🔜 Phase 4 |

---

## Prerequisites

### 1. Rust (stable toolchain)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version  # rustc 1.78.0 or newer
```

### 2. Node.js 20+

[Volta](https://volta.sh) pins the version automatically:

```bash
curl https://get.volta.sh | bash
volta install node@22
node --version  # v22.x.x
```

Or install directly from [nodejs.org](https://nodejs.org).

### 3. Xcode Command Line Tools

```bash
xcode-select --install
```

### 4. Tauri CLI (cargo plugin)

```bash
cargo install tauri-cli
cargo tauri --version  # tauri-cli 2.x.x
```

---

## Quick Start

```bash
# 1. Clone the repo
git clone <repo-url>
cd keynobi

# 2. Install JavaScript dependencies
npm install

# 3. Launch the development app
npm run tauri dev
```

`npm run tauri dev` does three things in parallel:
1. Starts the **Vite** dev server on `http://localhost:1420` (frontend hot-reload)
2. Compiles the **Rust backend** with `cargo` (only on first run or when `.rs` files change)
3. Opens a **native macOS window** with the app

> **First run note:** The initial Rust compilation downloads and compiles ~400 crates and takes 3–8 minutes. Subsequent starts are typically under 5 seconds.

---

## Development Workflow

### Start the dev server

```bash
npm run tauri dev
```

- **Frontend changes** (`.tsx`, `.ts`, `.css`): hot-reloaded instantly, no restart needed.
- **Rust backend changes** (`.rs` files): Tauri automatically recompiles and relaunches.

### Type-check the frontend

```bash
npx tsc --noEmit
```

### Lint and format

```bash
npm run lint
npm run format       # prettier on src/**/*.{ts,tsx,css}
cargo fmt            # rustfmt on all Rust source
```

### Check the Rust backend

```bash
cd src-tauri
cargo check          # fast type + borrow check
cargo clippy         # extended lint
```

### Run tests

```bash
npm run test         # Vitest frontend tests
cd src-tauri && cargo test   # Rust unit tests
```

### Regenerate TypeScript bindings

Run after any Rust model type change:

```bash
npm run generate:bindings
```

---

## Building a DMG for Distribution

```bash
# Apple Silicon Mac (M1/M2/M3) — default
./scripts/build-dmg.sh

# Intel Mac
./scripts/build-dmg.sh --intel

# Universal binary (runs on both architectures)
./scripts/build-dmg.sh --universal
```

Or use the npm aliases:

```bash
npm run build:dmg           # Apple Silicon
npm run build:dmg:intel     # Intel x86_64
npm run build:dmg:universal # Universal
```

Output location:

```
src-tauri/target/<arch>/release/bundle/dmg/Android Dev Companion_0.1.0_<arch>.dmg
```

> **Unsigned builds:** The DMG is built without a Developer ID certificate. On first launch, testers must right-click the app → **Open** to bypass Gatekeeper. See [Apple's documentation](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac) for details.

---

## Project Structure

```
keynobi/
├── src/                                # Frontend — SolidJS + TypeScript
│   ├── App.tsx                         # Root layout + action/keybinding registry
│   ├── stores/
│   │   ├── build.store.ts             # Build state, streaming logs, error list
│   │   ├── device.store.ts            # Connected devices, AVD state
│   │   ├── health.store.ts            # App health checks
│   │   ├── project.store.ts           # Project root, name, Gradle root
│   │   ├── projects.store.ts          # Multi-project registry
│   │   ├── settings.store.ts          # App settings
│   │   ├── ui.store.ts                # Active tab, panel visibility
│   │   ├── variant.store.ts           # Build variants
│   │   └── log.store.ts               # Generic log store factory
│   ├── services/
│   │   ├── build.service.ts           # Build orchestration
│   │   └── project.service.ts         # Project open + navigation history
│   ├── components/
│   │   ├── build/
│   │   │   ├── BuildPanel.tsx         # Streaming ANSI log + error list
│   │   │   └── VariantSelector.tsx    # Build variant dropdown in status bar
│   │   ├── device/
│   │   │   └── DevicePanel.tsx        # Devices + AVD management (Create/Delete/Wipe)
│   │   ├── logcat/
│   │   │   └── LogcatPanel.tsx        # Real-time logcat, filters, crash detection
│   │   ├── health/
│   │   │   └── HealthPanel.tsx        # Java, SDK, ADB, Gradle, disk checks
│   │   ├── settings/
│   │   │   ├── SettingsPanel.tsx
│   │   │   ├── SettingRow.tsx
│   │   │   └── ToolStatus.tsx         # SDK/Java path pickers
│   │   ├── layout/
│   │   │   ├── TitleBar.tsx           # Custom macOS title bar + project switcher
│   │   │   └── StatusBar.tsx          # Health, build status, variant, device
│   │   └── common/
│   │       ├── CommandPalette.tsx     # Cmd+Shift+P action palette
│   │       ├── LogViewer.tsx          # Shared ANSI log renderer
│   │       ├── VirtualList.tsx        # Virtualized list for large buffers
│   │       ├── Toast.tsx              # Auto-dismiss notifications
│   │       ├── Dialog.tsx             # Modal dialogs
│   │       ├── Resizable.tsx          # Drag-to-resize splitter
│   │       └── Icon.tsx               # Inline SVG icon library
│   ├── lib/
│   │   ├── tauri-api.ts              # Typed wrappers for all Tauri IPC calls
│   │   ├── keybindings.ts            # Global keyboard shortcut registry
│   │   ├── action-registry.ts        # Command palette action registry
│   │   ├── fuzzy-match.ts            # Fuzzy matching utility
│   │   ├── ansi-strip.ts             # ANSI escape code stripping
│   │   └── file-utils.ts             # File type utilities
│   └── bindings/                     # Auto-generated TypeScript types (ts-rs)
│
├── src-tauri/                         # Rust backend — Tauri 2.0
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json              # Tauri 2.0 security permissions
│   └── src/
│       ├── lib.rs                    # Tauri builder, state, command registration
│       ├── commands/
│       │   ├── build.rs              # Gradle task commands
│       │   ├── device.rs             # ADB / AVD commands
│       │   ├── file_system.rs        # Project open, Gradle root detection
│       │   ├── health.rs             # Health check commands
│       │   ├── logcat.rs             # Logcat streaming commands
│       │   ├── settings.rs           # Settings persistence commands
│       │   └── variant.rs            # Build variant commands
│       ├── services/
│       │   ├── adb_manager.rs        # ADB device polling
│       │   ├── build_runner.rs       # ./gradlew execution, output parsing
│       │   ├── fs_manager.rs         # Gradle root detection
│       │   ├── logcat.rs             # Parser, ring buffer (50K entries)
│       │   ├── process_manager.rs    # Child process lifecycle + SIGTERM
│       │   ├── settings_manager.rs   # ~/.keynobi/settings.json
│       │   └── variant_manager.rs    # buildTypes × productFlavors parsing
│       └── models/
│           ├── build.rs
│           ├── device.rs
│           ├── error.rs
│           ├── health.rs
│           ├── log_entry.rs
│           ├── settings.rs
│           └── variant.rs
│
├── scripts/
│   └── build-dmg.sh                  # One-command DMG builder
├── package.json
├── vite.config.ts
└── tsconfig.json
```

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                  macOS Application (Tauri 2.0)                   │
│                                                                  │
│  ┌──────────────────────────┐  ┌──────────────────────────────┐  │
│  │   Frontend (WKWebView)   │  │   Rust Backend (src-tauri)   │  │
│  │  SolidJS + TypeScript    │  │                              │  │
│  │                          │  │  Core Services               │  │
│  │  ┌────────────────────┐  │  │   fs_manager (gradle root)   │  │
│  │  │ BuildPanel         │  │  │   process_manager            │  │
│  │  │  ANSI log          │  │  │   settings_manager           │  │
│  │  │  Error list        │  │  │                              │  │
│  │  ├────────────────────┤  │  │  Android Services            │  │
│  │  │ LogcatPanel        │  │  │   build_runner (gradlew)     │  │
│  │  │  50K ring buffer   │  │  │   variant_manager            │  │
│  │  │  Level/tag filters │  │  │   adb_manager (poll 2s)      │  │
│  │  ├────────────────────┤  │  │   logcat (ring buffer 50K)   │  │
│  │  │ DevicePanel        │  │  │                              │  │
│  │  │  Devices + AVDs    │  │  │  [Phase 4]                   │  │
│  │  ├────────────────────┤  │  │   mcp_server (stdio + HTTP)  │  │
│  │  │ HealthPanel        │  │  │                              │  │
│  │  │ SettingsPanel      │  │  └──────────────────────────────┘  │
│  │  │ CommandPalette     │  │                ▲                   │
│  │  └────────────────────┘  │                │ Tauri IPC         │
│  └────────────┬─────────────┘                │ (invoke/emit)     │
│               └──────────────────────────────┘                   │
└──────────────────────────────────────────────────────────────────┘
         │                                    │
         ▼                                    ▼
Android Device / Emulator          Claude Code (Phase 4)
adb logcat + build output          run_gradle_task, get_build_errors,
                                   get_logcat_entries, list_devices…
```

**Key design decisions:**

- **Ring buffer, 50K entries** — Logcat entries are stored in Rust, never serialized to disk. The frontend only receives what's visible. This keeps memory flat regardless of session length.
- **100 ms batch emit** — `adb logcat` fires events at very high frequency. Batching at the Rust layer prevents flooding the SolidJS store with thousands of individual signals.
- **Atomic file writes** — project settings go through write-to-temp → `rename()`. A crash mid-save cannot corrupt state.
- **Mutex discipline** — Rust state is locked, cloned, then the lock is dropped before any `await`. Nothing is ever held across an async boundary.
- **`ts-rs` bindings** — every Rust model type derives `TS`. After changing a model, run `npm run generate:bindings` to keep the TypeScript side in sync.

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+Shift+P` | Open command palette |
| `Cmd+O` | Open project folder |
| `Cmd+R` | Run (build → install → launch) |
| `Cmd+Shift+R` | Run with variant picker |
| `Cmd+.` | Cancel current build |
| `Cmd+B` | Toggle Build panel |
| `Cmd+L` | Toggle Logcat panel |
| `Cmd+D` | Toggle Device panel |
| `Cmd+H` | Toggle Health panel |
| `Cmd+,` | Open Settings |

---

## Troubleshooting

### `cargo: command not found` when running `npm run tauri dev`

Rust is installed but not on your shell's `PATH`. Add this to `~/.zshrc`:

```bash
source "$HOME/.cargo/env"
```

Restart your terminal and retry.

### Port 1420 is already in use

Another instance of the dev server is running. Kill it:

```bash
lsof -ti:1420 | xargs kill -9
```

### First `tauri dev` is very slow

Expected — Rust compiles all dependencies from source on first run (~3–8 minutes). Subsequent builds only recompile changed crates (typically 2–5 seconds).

### ADB not found / devices not appearing

Open **Settings** (`Cmd+,`) and set the Android SDK path. The Health panel (`Cmd+H`) will show which tools are missing and suggest fixes.

### `App can't be opened because it is from an unidentified developer`

The build is unsigned. Right-click the app in Applications → **Open**, then click **Open** in the dialog. This only needs to be done once.

### White flash on launch

Ensure `index.html` has `background: #1e1e1e` on `body`. If you see a flash after a fresh build, verify `global.css` is imported before any component renders.

---

## Roadmap

| Phase | Status | Scope |
|---|---|---|
| **1 — Foundation** | ✅ Complete | Project open, settings, health checks, command palette, toast system |
| **2 — Build + Devices** | ✅ Complete | Gradle builds, build variants, ADB device management, Run cycle |
| **3 — Logcat** | ✅ Complete | Real-time logcat streaming, filters, crash detection, ring buffer |
| **4 — MCP Server** | 🔜 Next | `claude mcp add android-companion` — build, logcat, and device tools for Claude |
| **5 — Polish + UX** | Planned | Logcat export, emulator controls (GPS/network/battery), screenshots, onboarding |
| **6 — Multi-Project** | Partially done | Project registry, title-bar switcher, ProjectInfoEditor; per-project variant/device persistence pending |

See [`PLAN.md`](PLAN.md) for the full architecture and [`TASK.md`](TASK.md) for the detailed task checklist.
