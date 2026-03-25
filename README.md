# Android IDE

An AI-first Android development environment built with Tauri 2.0 + SolidJS. Lighter and more focused than Android Studio, designed so every feature — file browsing, code editing, builds, logcat, emulator — is readable and controllable by AI agents.

**Platform:** macOS only (v0.x beta)
**Language support:** Kotlin + Gradle (initial scope)

---

## Table of Contents

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

## Prerequisites

You need four tools installed before anything else.

### 1. Rust (stable toolchain)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version  # should print rustc 1.94.0 or newer
```

### 2. Node.js 20+

The recommended way is [Volta](https://volta.sh), which pins the version automatically:

```bash
curl https://get.volta.sh | bash
volta install node@22
node --version  # v22.x.x
```

Or install directly from [nodejs.org](https://nodejs.org).

### 3. Xcode Command Line Tools

Required by Tauri to compile and link against macOS system frameworks:

```bash
xcode-select --install
```

If Xcode is already installed, this is a no-op.

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
cd android-ide

# 2. Install JavaScript dependencies
npm install

# 3. Launch the development app
npm run tauri dev
```

`npm run tauri dev` does three things in parallel:
1. Starts the **Vite** dev server on `http://localhost:1420` (frontend hot-reload)
2. Compiles the **Rust backend** with `cargo` (only on first run or when `.rs` files change)
3. Opens a **native macOS window** with the IDE

> **First run note:** The initial Rust compilation downloads and compiles ~400 crates and takes 3–8 minutes depending on your machine. Subsequent starts are typically under 5 seconds.

---

## Development Workflow

### Start the dev server

```bash
npm run tauri dev
```

- **Frontend changes** (`.tsx`, `.ts`, `.css`): hot-reloaded instantly, no restart needed.
- **Rust backend changes** (`.rs` files or `tauri.conf.json`): Tauri automatically recompiles and relaunches the window.

### Type-check the frontend

```bash
npx tsc --noEmit
```

### Lint

```bash
npm run lint
```

### Format

```bash
npm run format       # prettier on all src/**/*.{ts,tsx,css}
cargo fmt            # rustfmt on all Rust source
```

### Check the Rust backend

```bash
cd src-tauri
cargo check          # fast type + borrow check, no binary output
cargo clippy         # extended lint
```

---

## Building a DMG for Distribution

Use the build script for a one-command release build:

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

The script:
- Installs the required Rust cross-compilation target automatically (via `rustup`)
- Builds the frontend with Vite (`npm run build`)
- Compiles the Rust backend in **release mode** (optimised, much faster at runtime)
- Bundles everything into a `.dmg` installer
- Opens Finder pointing at the output file when done

Output location:

```
src-tauri/target/<arch>/release/bundle/dmg/Android IDE_0.1.0_<arch>.dmg
```

> **Unsigned builds:** The DMG is built without a Developer ID certificate (`APPLE_SIGNING_IDENTITY=-`). This is fine for internal testing. On first launch, testers must right-click the app → **Open** to bypass Gatekeeper. See [Apple's documentation](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac) for details.

### Release build without DMG (just the `.app`)

```bash
npm run tauri build -- --target aarch64-apple-darwin --bundles app
```

---

## Project Structure

```
android-ide/
├── src/                          # Frontend — SolidJS + TypeScript
│   ├── index.tsx                 # App entry point
│   ├── App.tsx                   # Root layout (CSS Grid shell)
│   ├── stores/
│   │   ├── ui.store.ts           # Panel visibility, sidebar state
│   │   ├── project.store.ts      # Open project root, file tree
│   │   └── editor.store.ts       # Open files, dirty state, cursor position
│   ├── components/
│   │   ├── layout/
│   │   │   ├── TitleBar.tsx      # Custom macOS title bar (drag region)
│   │   │   ├── Sidebar.tsx       # Icon bar + collapsible panel
│   │   │   ├── PanelContainer.tsx# Bottom panel (Build/Logcat/Terminal tabs)
│   │   │   └── StatusBar.tsx     # Ln/Col, language, project name
│   │   ├── editor/
│   │   │   ├── CodeEditor.tsx    # CodeMirror 6 wrapper (single EditorView)
│   │   │   └── EditorTabs.tsx    # Tab bar with dirty indicators
│   │   ├── filetree/
│   │   │   ├── FileTree.tsx      # Project file tree with live updates
│   │   │   └── FileTreeNode.tsx  # Individual tree node + context menu
│   │   └── common/
│   │       ├── Resizable.tsx     # Drag-to-resize splitter
│   │       ├── Toast.tsx         # Auto-dismiss notifications
│   │       └── Icon.tsx          # Inline SVG icon library
│   ├── lib/
│   │   ├── tauri-api.ts          # Typed wrappers for all Tauri IPC calls
│   │   ├── keybindings.ts        # Global keyboard shortcut registry
│   │   └── codemirror/
│   │       ├── setup.ts          # Base CodeMirror 6 extensions
│   │       ├── kotlin.ts         # Kotlin syntax highlighting
│   │       ├── gradle.ts         # Gradle/Kotlin DSL highlighting
│   │       └── theme.ts          # VS Code Dark+ token colours
│   └── styles/
│       ├── global.css            # Reset + drag region helpers
│       └── theme.css             # CSS custom properties (colour palette)
│
├── src-tauri/                    # Rust backend — Tauri 2.0
│   ├── Cargo.toml                # Rust dependencies
│   ├── tauri.conf.json           # App config (window, bundle, permissions)
│   ├── capabilities/
│   │   └── default.json          # Tauri 2.0 security permissions
│   └── src/
│       ├── main.rs               # Binary entry point
│       ├── lib.rs                # Tauri builder, plugin registration, AppState
│       ├── models/
│       │   └── file.rs           # FileNode, FileEvent (serde types)
│       ├── services/
│       │   └── fs_manager.rs     # File tree, watching, atomic CRUD
│       └── commands/
│           └── file_system.rs    # #[tauri::command] IPC handlers
│
├── scripts/
│   └── build-dmg.sh              # One-command DMG builder
├── package.json
├── vite.config.ts
└── tsconfig.json
```

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│                   macOS Window (Tauri 2.0)                   │
│                                                              │
│  ┌─────────────────────┐   ┌──────────────────────────────┐ │
│  │  Frontend (WKWebView)│   │  Rust Backend (src-tauri)    │ │
│  │                     │   │                              │ │
│  │  SolidJS + TypeScript│   │  AppState (tokio::Mutex)     │ │
│  │                     │   │  ├─ project_root             │ │
│  │  ┌─────────────────┐│   │  └─ file watcher             │ │
│  │  │ TitleBar        ││   │                              │ │
│  │  │ Sidebar         ││   │  fs_manager.rs               │ │
│  │  │  └─ FileTree    ││   │  ├─ build_file_tree()        │ │
│  │  │ CodeEditor (CM6)││   │  │   (ignore::WalkBuilder)   │ │
│  │  │ EditorTabs      ││   │  ├─ start_watching()         │ │
│  │  │ StatusBar       ││   │  │   (notify + FSEvents)     │ │
│  │  └─────────────────┘│   │  └─ read/write/create/delete │ │
│  │                     │   │                              │ │
│  └──────────┬──────────┘   └──────────────────────────────┘ │
│             │                           ▲                    │
│             │   Tauri IPC (invoke)       │                    │
│             └───────────────────────────┘                    │
│                  file:changed events (emit)                   │
└──────────────────────────────────────────────────────────────┘
```

**Key design decisions:**
- **Single `EditorView` instance** — CodeMirror state is swapped per-file, not recreated. Tab switching is instant (< 50 ms) with full cursor/scroll/undo history preserved per file.
- **`ignore::WalkBuilder`** for file tree — reads `.gitignore` at every directory level. Android projects have massive `build/` directories; this keeps the tree fast and clean.
- **Atomic file writes** — all saves go through write-to-temp → `rename()`. A crash mid-save cannot corrupt the original file.
- **200 ms debounced file watcher** — editors fire multiple FS events per save; debouncing prevents flooding the frontend with redundant refreshes.

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+O` | Open project folder |
| `Cmd+S` | Save current file |
| `Cmd+Option+S` | Save all dirty files |
| `Cmd+W` | Close current tab |
| `Cmd+Shift+[` | Switch to previous tab |
| `Cmd+Shift+]` | Switch to next tab |
| `Cmd+B` | Toggle sidebar |
| `Cmd+J` | Toggle bottom panel |
| `Cmd+F` | Find in current file (CodeMirror) |
| `Cmd+Z` | Undo |
| `Cmd+Shift+Z` | Redo |

---

## Troubleshooting

### `cargo: command not found` when running `npm run tauri dev`

Rust is installed but not on your shell's `PATH`. Add this to your `~/.zshrc`:

```bash
source "$HOME/.cargo/env"
```

Then restart your terminal and retry.

### Port 1420 is already in use

Another instance of the dev server is running. Kill it:

```bash
lsof -ti:1420 | xargs kill -9
```

Then run `npm run tauri dev` again.

### First `cargo check` / `tauri dev` is very slow

Expected — Rust compiles all dependencies from source on first run. This is a one-time cost. Subsequent builds only recompile changed crates (usually just `android-ide` itself, taking 2–5 seconds).

### Opening a file shows no syntax highlighting

The Kotlin and Gradle modes are loaded from `@codemirror/legacy-modes`. Make sure `npm install` completed without errors. Delete `node_modules/` and run `npm install` again if needed.

### `App can't be opened because it is from an unidentified developer` (on shared DMG)

The build is unsigned. Tell the recipient to:
1. Right-click the app in Applications → **Open**
2. Click **Open** in the dialog

This only needs to be done once. After that, the app opens normally.

### White flash on launch

Ensure `index.html` has `background: #1e1e1e` on `body` (already set). If you see a flash in a future build, check that `global.css` is imported before any component renders.

---

## Roadmap

The project is built in phases. Phase 1 (this release) covers the foundational IDE shell.

| Phase | Status | Scope |
|---|---|---|
| **1 — Foundation** | ✅ Complete | File tree, code editor, tabs, dark theme |
| 2 — Build System | Planned | Gradle builds, ADB, deploy to device |
| 3 — Code Intelligence | Planned | Kotlin LSP, completions, go-to-definition |
| 4 — Logcat + Emulator | Planned | Streaming logcat, emulator controls |
| 5 — AI Integration | Planned | AI chat, inline edits, MCP server |
| 6 — Git + Terminal + Polish | Planned | Git panel, terminal, DMG signing, auto-update |

See [`PLAN.md`](PLAN.md) for the full architecture and [`TASK.md`](TASK.md) for the detailed task checklist.
