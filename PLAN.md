# Android IDE — AI-First Android Development Environment

## Vision

An Android IDE built from the ground up with AI as a first-class citizen. Lighter and more focused than Android Studio, where every feature — builds, logcat, emulator, code editing, git — is designed to be readable and controllable by AI agents. Inspired by Cursor's AI-first philosophy, but purpose-built for Android and not a VS Code fork.

**Target**: macOS only (v1 beta)
**Language support**: Kotlin + Gradle (initial scope)

---

## Technology Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| **App Framework** | **Tauri 2.0** | 20–40 MB RAM baseline vs Electron's 300–800 MB. Rust backend for heavy lifting. Native WKWebView on macOS (faster startup). First-class sidecar support for bundling kotlin-lsp. Channel API for high-throughput streaming (logcat, build logs). |
| **Frontend** | **SolidJS + TypeScript + Vite** | 2–3x faster rendering than React, fine-grained reactivity (Signals) — critical for IDE workloads: streaming logcat, large file trees, multiple simultaneous editors. JSX syntax familiar to React developers. Smaller bundles. |
| **Code Editor** | **CodeMirror 6** | Works natively with WebKit/WKWebView (Monaco has documented WebKit issues). Fully modular extension system. Lezer parser for Kotlin. LSP integration via extensions. |
| **Backend Parsing** | **Tree-sitter (Rust)** | Incremental parsing, sub-millisecond re-parses. Kotlin grammar available (`tree-sitter-kotlin`). Used in Rust backend for symbol indexing and AI context extraction. |
| **Language Intelligence** | **fwcd/kotlin-language-server** (now) → **JetBrains kotlin-lsp** (future) | fwcd LSP works today: completions, diagnostics, hover, go-to-definition, references. JetBrains official LSP (previewed at KotlinConf 2025) will be the gold standard when stable. Provider-agnostic LSP client allows swapping. |
| **ADB** | **adb_client Rust crate + CLI fallback** | Native Rust for high-frequency operations (logcat streaming). CLI fallback for edge cases. No subprocess overhead for streaming. |
| **Build System** | **./gradlew subprocess** | More reliable than Gradle Tooling API (which would require JNI/JVM dependency in Rust). How every CI system works. Tauri shell plugin handles streaming stdout/stderr. |
| **AI Protocol** | **MCP (Model Context Protocol)** | Industry standard for LLM-tool integration. IDE exposes tools that Claude Code, ChatGPT Desktop, or any MCP client can call. Future-proof. |
| **Embeddings** | **Local via candle Rust crate** (all-MiniLM-L6-v2, ~90 MB) | Works offline. Fast on Apple Silicon. Optional remote API for higher-quality embeddings. |
| **Git** | **git2 crate** (libgit2 Rust bindings) | Full git operations without git CLI dependency. |
| **Full-text Search** | **ripgrep library crates** (grep-regex, grep-searcher, grep-matcher) | Proven speed, gitignore-aware, streaming results. |
| **Terminal** | **xterm.js + portable-pty** | Standard approach for web-based IDEs. Bidirectional PTY over Tauri Channels. |
| **Distribution** | **DMG + Homebrew cask + Tauri auto-updater** | Standard macOS channels. Universal binary (Apple Silicon + Intel). |

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
|  |  ┌─────────────────────┐  |  |   fs_manager      (notify)   | |
|  |  │ Tab Manager         │  |  |   process_manager             | |
|  |  │ File Tree           │  |  |   project_model               | |
|  |  │ CodeMirror 6 Editor │  |  |                               | |
|  |  │ Build Panel         │  |  |  Android Services             | |
|  |  │ Logcat Panel        │  |  |   adb_manager                 | |
|  |  │ Device Panel        │  |  |   logcat (ring buffer)        | |
|  |  │ Search Panel        │  |  |   emulator_ctl  (telnet)      | |
|  |  │ Git Panel           │  |  |   build_runner  (gradlew)     | |
|  |  │ AI Chat Panel       │  |  |   variant_manager             | |
|  |  │ Terminal            │  |  |                               | |
|  |  │ Command Palette     │  |  |  Intelligence Services        | |
|  |  └─────────────────────┘  |  |   lsp_client    (JSON-RPC)   | |
|  +---------------------------+  |   treesitter    (Kotlin AST)  | |
|              │                  |   indexer       (embeddings)   | |
|    Tauri IPC (Commands +        |   search_engine (ripgrep)      | |
|    Channels + Events)           |                               | |
|              │                  |  AI Services                  | |
|  +---------------------------+  |   mcp_server    (MCP proto)   | |
|  |   Sidecar Processes       |  |   context       (@-mentions)  | |
|  |   kotlin-language-server  |  |   embeddings    (candle)      | |
|  |   adb (CLI fallback)      |  +-------------------------------+ |
|  +---------------------------+                                    |
+------------------------------------------------------------------+
         |                                    |
         v                                    v
  LLM APIs (Anthropic,          MCP Clients (Claude Code,
  OpenAI, Ollama local)         ChatGPT Desktop, custom)
```

### Key Data Flows

**1. Code editing → LSP diagnostics**
```
CodeMirror keystroke
  → SolidJS state update (local buffer)
  → Tauri command: save_file(path, content)
  → Rust: write disk + LSP textDocument/didChange notification
  → LSP responds: diagnostics (errors/warnings)
  → Rust: forward via Tauri Channel to frontend
  → CodeMirror: renders inline squiggles + gutter markers
```

**2. Logcat streaming**
```
Rust: adb_client logcat stream (continuous)
  → Parse threadtime format → structured LogEntry
  → Server-side filter (tag/package/level)
  → Batch at 60fps → Channel to frontend
  → SolidJS VirtualList renders visible rows only
  → MCP tool get_recent_logs() available to AI at all times
```

**3. AI chat interaction**
```
User message + @-context references
  → Context Assembler gathers:
      @file → read from disk
      @codebase → vector search over embeddings
      @logcat → last N entries from ring buffer
      @build → last build errors/output
      @selection → current editor selection
  → LLM API (streaming)
  → Tokens streamed via Channel to AI Chat Panel
  → File edits: apply via editor API (inline diff preview)
  → Tool calls: execute via MCP tools
```

**4. Build execution**
```
User clicks Run (or AI calls run_gradle_task)
  → Rust spawns ./gradlew assembleDebug (or active variant task)
  → stdout/stderr streamed line-by-line via Channel
  → Rust parses: errors (e: file:///path:line:col: msg), warnings, task progress
  → Build Panel: raw ANSI log + structured error list with clickable links
  → On success: adb install → adb shell am start
```

---

## Project Structure

```
android-ide/
├── src-tauri/                          # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json                # Tauri permissions (shell, fs, etc.)
│   ├── binaries/                       # Sidecar binaries
│   │   ├── kotlin-lsp-aarch64-apple-darwin
│   │   └── kotlin-lsp-x86_64-apple-darwin
│   └── src/
│       ├── main.rs                     # Entry point
│       ├── lib.rs                      # Tauri setup, plugin registration, state
│       ├── commands/                   # Tauri IPC command handlers
│       │   ├── mod.rs
│       │   ├── file_system.rs          # File CRUD, tree, watching
│       │   ├── editor.rs               # Open, save, file metadata
│       │   ├── build.rs                # Gradle task execution
│       │   ├── device.rs               # ADB device management
│       │   ├── logcat.rs               # Logcat streaming and filtering
│       │   ├── emulator.rs             # Emulator control
│       │   ├── search.rs               # Full-text and structural search
│       │   ├── git.rs                  # Git operations
│       │   └── ai.rs                   # AI chat, context assembly
│       ├── services/                   # Core business logic
│       │   ├── mod.rs
│       │   ├── fs_manager.rs           # File watching (notify), tree building (walkdir+ignore)
│       │   ├── process_manager.rs      # Child process lifecycle management
│       │   ├── project_model.rs        # Gradle project structure, module graph
│       │   ├── adb_manager.rs          # ADB protocol client (adb_client crate)
│       │   ├── logcat.rs               # Parser, filter engine, ring buffer (50K entries)
│       │   ├── emulator_ctl.rs         # Telnet console, snapshot management
│       │   ├── build_runner.rs         # ./gradlew execution, output parsing
│       │   ├── variant_manager.rs      # Build variant discovery and switching
│       │   ├── lsp_client.rs           # LSP JSON-RPC client over stdio
│       │   ├── treesitter.rs           # Kotlin AST parsing, symbol extraction
│       │   ├── indexer.rs              # Codebase indexing (Tree-sitter chunks + embeddings)
│       │   ├── search_engine.rs        # ripgrep-based project search
│       │   └── git_service.rs          # git2 crate: status, diff, log, commit
│       ├── ai/
│       │   ├── mod.rs
│       │   ├── mcp_server.rs           # MCP protocol handler (all tools)
│       │   ├── context.rs              # @-context assembly, token budget management
│       │   ├── embeddings.rs           # Vector embedding (candle), HNSW index
│       │   └── tools.rs                # MCP tool definitions
│       └── models/                     # Shared data types
│           ├── mod.rs
│           ├── file.rs
│           ├── log_entry.rs            # { timestamp, pid, tid, level, tag, message }
│           ├── build.rs
│           ├── device.rs
│           └── project.rs
│
├── src/                                # Frontend (SolidJS + TypeScript)
│   ├── index.html
│   ├── main.tsx                        # App entry point
│   ├── App.tsx                         # Root layout
│   ├── stores/                         # SolidJS reactive stores (createStore/createSignal)
│   │   ├── editor.store.ts             # Open files, active tab, dirty state
│   │   ├── project.store.ts            # Project model, active variant
│   │   ├── build.store.ts              # Build state, logs, errors
│   │   ├── logcat.store.ts             # Logcat entries, filters, sessions
│   │   ├── device.store.ts             # Connected devices, emulator state
│   │   ├── search.store.ts             # Search results
│   │   ├── git.store.ts                # Git status, diff
│   │   ├── ai.store.ts                 # Chat history, AI context
│   │   └── ui.store.ts                 # Panel visibility, layout state
│   ├── components/
│   │   ├── layout/
│   │   │   ├── Sidebar.tsx
│   │   │   ├── PanelContainer.tsx      # Resizable bottom/side panels
│   │   │   ├── StatusBar.tsx           # Build status, device, variant, LSP status
│   │   │   └── TitleBar.tsx            # Custom macOS title bar
│   │   ├── editor/
│   │   │   ├── EditorTabs.tsx          # Tab bar with dirty indicators
│   │   │   ├── CodeEditor.tsx          # CodeMirror 6 wrapper component
│   │   │   └── EditorToolbar.tsx
│   │   ├── filetree/
│   │   │   ├── FileTree.tsx            # Virtualized tree with lazy loading
│   │   │   └── FileTreeNode.tsx
│   │   ├── build/
│   │   │   ├── BuildPanel.tsx          # ANSI log + structured error list
│   │   │   ├── BuildLogViewer.tsx      # Virtualized log with ANSI colors
│   │   │   └── VariantSelector.tsx     # Build variant dropdown
│   │   ├── logcat/
│   │   │   ├── LogcatPanel.tsx         # Main logcat view
│   │   │   ├── LogcatFilter.tsx        # tag: level: package: filter bar
│   │   │   └── LogcatEntry.tsx         # Single log row (color-coded)
│   │   ├── device/
│   │   │   ├── DevicePanel.tsx         # Connected devices + AVD list
│   │   │   └── EmulatorControls.tsx    # GPS, network, battery, rotation
│   │   ├── search/
│   │   │   ├── SearchPanel.tsx
│   │   │   └── SearchResult.tsx
│   │   ├── git/
│   │   │   ├── GitPanel.tsx            # Changed files, commit UI
│   │   │   └── DiffViewer.tsx
│   │   ├── ai/
│   │   │   ├── AIChatPanel.tsx         # Chat with markdown + streaming
│   │   │   ├── ChatMessage.tsx
│   │   │   ├── ContextSelector.tsx     # @-mention autocomplete
│   │   │   └── InlineAssist.tsx        # Cmd+K inline edit UI
│   │   ├── terminal/
│   │   │   └── TerminalPanel.tsx       # xterm.js terminal
│   │   └── common/
│   │       ├── CommandPalette.tsx      # Cmd+P / Cmd+Shift+P
│   │       ├── VirtualList.tsx         # High-perf virtualized list (logcat, build)
│   │       ├── Resizable.tsx           # Resizable panel splitter
│   │       └── Icon.tsx
│   ├── lib/
│   │   ├── tauri-api.ts                # Typed wrappers for Tauri commands
│   │   └── codemirror/
│   │       ├── setup.ts                # Base CM6 configuration
│   │       ├── kotlin.ts               # Kotlin language mode (Lezer grammar)
│   │       ├── gradle.ts               # Gradle/Kotlin script mode
│   │       ├── lsp-extension.ts        # LSP diagnostics/completions bridge
│   │       ├── ai-extension.ts         # Ghost text, inline diff
│   │       └── theme.ts                # Editor theme (dark/light)
│   └── styles/
│       ├── global.css
│       └── theme.css
│
├── package.json
├── tsconfig.json
├── vite.config.ts
├── PLAN.md                             # This file
└── TASK.md                             # Detailed task checklist
```

---

## MCP Tools Exposed by the IDE

The IDE runs an MCP server so any compatible AI agent (Claude Code, ChatGPT Desktop, custom agents) can interact with Android development:

### File Operations
- `read_file(path)` — Read file content
- `write_file(path, content)` — Write/create file
- `list_directory(path)` — List directory contents
- `search_files(query, options)` — Full-text search across project
- `search_symbols(query)` — Symbol search via Tree-sitter index

### Build Operations
- `run_gradle_task(task, variant?)` — Execute any Gradle task
- `get_build_status()` — Current build state
- `get_build_errors()` — Errors and warnings from last build
- `list_build_variants()` — All available build variants
- `set_active_variant(variant)` — Switch active build variant

### Logcat Operations
- `get_recent_logs(count?, filter?)` — Read recent logcat entries
- `search_logs(query, options)` — Search through logcat history
- `get_crash_logs()` — Recent crash stack traces

### Device/Emulator Operations
- `list_devices()` — Connected devices and emulators
- `install_apk(device, path)` — Install an APK
- `launch_app(device, package, activity?)` — Launch an app
- `take_screenshot(device)` — Capture device screen
- `set_emulator_location(lat, lon)` — Set GPS coordinates
- `set_emulator_network(type)` — Simulate network conditions

### Code Intelligence
- `get_diagnostics(file?)` — Current LSP diagnostics
- `get_definition(file, line, col)` — Go to definition
- `get_references(file, line, col)` — Find usages
- `get_file_symbols(file)` — List all symbols in a file
- `get_project_structure()` — Module/dependency graph

### Git Operations
- `git_status()` — Working tree status
- `git_diff(file?)` — Show diffs
- `git_log(count?)` — Recent commit history
- `git_commit(message, files?)` — Create a commit

---

## Development Phases

### Phase 1 — Foundation (Weeks 1–4)
**Goal:** A working Tauri app that opens an Android project, shows the file tree, and edits Kotlin files with syntax highlighting.

- Initialize Tauri 2.0 + SolidJS + Vite + TypeScript project
- App shell: title bar, sidebar, editor area, bottom panel, status bar (CSS Grid + resizable splitters)
- `fs_manager.rs`: directory walking (walkdir + ignore crates for .gitignore), file watching (notify crate, FSEvents backend)
- `FileTree.tsx`: virtualized tree, expand/collapse, file icons, right-click context menu
- `CodeEditor.tsx`: CodeMirror 6 with Kotlin syntax highlighting (legacy clike mode initially), bracket matching, line numbers, auto-close brackets
- Tab system: open/close/switch, dirty indicators (unsaved dot)
- File save (Cmd+S), file open (double-click), create/delete/rename files
- Dark theme inspired by modern IDEs

**Deliverable:** Open Android project directory → browse files → edit Kotlin with syntax highlighting.

---

### Phase 2 — Build System + Devices (Weeks 5–8)
**Goal:** Complete build-deploy-run cycle from within the IDE.

- `build_runner.rs`: spawn `./gradlew <task>`, stream stdout/stderr line-by-line via Tauri Channel, parse Gradle errors (regex: `e: file:///path:line:col: message`), support build cancellation (SIGTERM)
- `BuildPanel.tsx`: virtualized streaming log with ANSI color support + structured error list with clickable file:line links, build history
- `variant_manager.rs`: parse `build.gradle.kts` with Tree-sitter for `buildTypes {}` + `productFlavors {}`, compute Cartesian product for all variants
- `VariantSelector.tsx`: searchable dropdown in status bar
- `adb_manager.rs`: device listing (poll every 2s), APK install (`adb install -r`), app launch (`adb shell am start`), device property queries
- `DevicePanel.tsx`: connected physical devices + AVD list, launch/stop emulators
- "Run" button: variant → build → install on selected device → launch
- Status bar: build status, connected devices count, active variant

**Deliverable:** Build → install → launch cycle working end-to-end.

---

### Phase 3 — Code Intelligence (Weeks 9–12)
**Goal:** Go-to-definition, completions, diagnostics, and find-usages via Kotlin LSP.

- `lsp_client.rs`: full LSP JSON-RPC client over stdio — initialize, capabilities negotiation, textDocument/didOpen, didChange, didSave, completion, definition, references, hover, diagnostics, rename
- Bundle `kotlin-language-server` as Tauri sidecar (pre-built for aarch64-apple-darwin and x86_64-apple-darwin)
- `lsp-extension.ts` (CodeMirror 6): diagnostics (inline squiggles + gutter markers), completion popup, hover tooltips, signature help
- Go-to-definition (Cmd+Click / F12)
- Find references / find usages (Shift+F12)
- Go-to-implementation
- Document symbols sidebar (Cmd+Shift+O)
- `treesitter.rs`: Kotlin grammar for fallback navigation when LSP is unavailable/slow
- `search_engine.rs`: project-wide text search with results panel, regex support, streaming results
- `CommandPalette.tsx`: Cmd+P file search, Cmd+Shift+P commands, Cmd+T symbol search

**Deliverable:** Code-intelligent Kotlin editor with LSP-powered navigation and completions.

---

### Phase 4 — Logcat + Emulator (Weeks 13–16)
**Goal:** Production-quality logcat and emulator control, both AI-accessible.

- `logcat.rs`: stream via adb_client logcat API, parse `threadtime` format into `LogEntry { timestamp, pid, tid, level, tag, message }`, ring buffer (50K entries, configurable), server-side filtering
- `LogcatPanel.tsx`: virtualized list (render visible rows only), color-coded by level (V=gray, D=blue, I=green, W=yellow, E=red, F=purple)
- Logcat filter bar supporting Android Studio syntax: `tag:MyTag level:ERROR package:com.example -tag:Volley tag~:My.*App age:5m is:crash`
- Logcat session persistence (save/load named sessions)
- `emulator_ctl.rs`: list AVDs (scan `~/.android/avd/`), launch via `emulator` binary, control via telnet console (port 5554+): network simulation, GPS, battery, phone/SMS, rotation, snapshots
- `EmulatorControls.tsx`: control palette for running emulators
- Screenshot capture via `adb shell screencap`
- MCP tools: `get_recent_logs`, `get_crash_logs`, `list_devices`, `take_screenshot`, emulator controls

**Deliverable:** Best-in-class logcat viewer and emulator control, fully AI-accessible.

---

### Phase 5 — AI Integration (Weeks 17–22)
**Goal:** AI chat, inline assist, MCP server, and semantic codebase search.

- `mcp_server.rs`: MCP protocol handler exposing all tools from the MCP tools section above. Supports stdio and HTTP+SSE transports.
- `AIChatPanel.tsx`: chat interface with markdown rendering, syntax-highlighted code blocks, streaming token display
- `context.rs`: @-context system assembly — @file, @selection, @diagnostics, @logcat, @build, @codebase, @terminal. Token budget management (prioritize most relevant, truncate intelligently via Tree-sitter AST)
- `ContextSelector.tsx`: @-mention autocomplete popup in chat input
- Ghost text completions in CodeMirror 6 (AI inline suggestions as gray text, accept with Tab)
- Inline edit (Cmd+K): select code → type instruction → AI generates diff → inline diff preview with accept/reject per hunk
- `indexer.rs`: Tree-sitter chunk extraction (functions, classes, top-level declarations), incremental re-indexing on file change
- `embeddings.rs`: candle crate running all-MiniLM-L6-v2 locally, HNSW vector index (`instant-distance` crate), @codebase semantic search
- AI-powered contextual actions:
  - "Explain crash" button in logcat crash entries
  - "Fix this error" button on build errors
  - "Generate commit message" in git panel
  - "Review changes" before commit
- LLM provider abstraction: Anthropic API (Claude), OpenAI API, Ollama (local). Config UI for API keys, model selection.

**Deliverable:** AI-first IDE experience with codebase-aware chat, inline editing, and Android-specific AI features.

---

### Phase 6 — Git + Terminal + Polish (Weeks 23–28)
**Goal:** Complete feature set, polished UX, beta-ready packaging.

- `git_service.rs` (git2 crate): status, diff, log, branch list, checkout, stage, commit, stash, pull/push
- `GitPanel.tsx`: changed files grouped by status (staged/modified/untracked), inline diffs, commit message input + commit button, branch selector
- Editor gutter git decorations (green=added, yellow=modified, red=deleted lines)
- `TerminalPanel.tsx`: xterm.js + portable-pty backend, multiple terminal tabs
- Settings/preferences UI: editor font + size, tab size, theme, keybindings, AI provider config, Kotlin LSP config
- Keyboard shortcut system with full customization and conflict detection
- Performance optimization pass: profile logcat with chatty app, file tree with 1000+ file project, editor with 3000+ line files
- SDK detection on first launch: locate `ANDROID_HOME`, prompt SDK installation if missing
- First-run onboarding: SDK setup, project import, API key configuration
- macOS code signing + notarization (Apple Developer account, Developer ID Application cert, notarytool)
- Universal binary: `--target universal-apple-darwin` (Apple Silicon + Intel)
- DMG packaging + auto-update via Tauri updater plugin
- Error reporting and crash analytics

**Deliverable:** Beta-ready application shippable to early users.

---

## Features Recommended for Beta (Not in Original Scope)

### Should Have (High Priority)
1. **SDK Manager integration** — Detect `ANDROID_HOME`, verify required SDK components, prompt to install missing ones. Without this, first-run experience breaks for users without Android Studio.
2. **XML syntax highlighting** — Android projects have XML layouts, drawables, manifests. At minimum: syntax highlighting and basic completion.
3. **Gradle Sync** — Explicit "Sync Project" action that resolves dependencies and updates the classpath for the LSP. Users expect this flow from Android Studio.
4. **Multi-module project support** — Most Android projects have multiple Gradle modules (`:app`, `:core`, `:feature-x`). The file tree, build system, and LSP must understand module boundaries.
5. **Code formatting** — Integrate `ktfmt` or `ktlint` for Kotlin formatting on save (configurable).

### Nice to Have (Medium Priority)
6. **Debugging (DAP)** — Debug Adapter Protocol with a Kotlin/JVM adapter. Users will ask for it. Can be post-beta.
7. **Resource navigation** — `R.string.app_name` click → jumps to `res/values/strings.xml`. High value, moderate complexity.
8. **Run configurations** — Save/recall different run configurations (module, variant, device, launch activity, args).
9. **Rename refactoring** — Rename symbol across files via LSP `workspace/rename`.

### Post-Beta
10. Compose Preview
11. Layout Inspector
12. APK Analyzer
13. Network Inspector
14. CPU/Memory Profiling
15. Plugin/extension system

---

## Key Rust Crates

| Crate | Purpose |
|-------|---------|
| `tauri` + `tauri-build` | App framework |
| `notify` | File watching (FSEvents on macOS) |
| `walkdir` | Recursive directory traversal |
| `ignore` | .gitignore-aware walking |
| `tree-sitter` + `tree-sitter-kotlin` | Kotlin AST parsing |
| `lsp-types` | LSP protocol type definitions |
| `serde` + `serde_json` | JSON serialization |
| `tokio` | Async runtime |
| `adb_client` | ADB protocol client |
| `git2` | Git operations (libgit2 bindings) |
| `grep-regex` + `grep-searcher` + `grep-matcher` | ripgrep-based search |
| `candle-core` + `candle-nn` + `candle-transformers` | Local ML inference (embeddings) |
| `instant-distance` | HNSW approximate nearest-neighbor search |
| `portable-pty` | PTY for terminal emulation |
| `regex` | Regex matching (logcat filters, build output parsing) |
| `tempfile` | Temporary files for tests |

---

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| No IDE-scale Tauri app exists yet | High | Build vertical slice early (Phase 1), stress test with large real projects. Escape plan: swap to Electron — frontend is portable standard web tech; Rust backend becomes NAPI-RS native module. Keep all Rust services independent of Tauri types. |
| Kotlin LSP quality (fwcd) | High | Tree-sitter fallback for navigation. Set user expectations clearly in beta docs. Monitor JetBrains' official kotlin-lsp progress. Consider contributing Android-specific fixes to fwcd LSP. |
| WebKit + CodeMirror 6 performance on large files | Medium | CM6 uses viewport-based rendering (handles large docs well). Test with real 3000-line Kotlin files early. If needed: move syntax highlighting to Rust (Tree-sitter), send highlight ranges to frontend. |
| Logcat volume overwhelming UI | Medium | Server-side filtering in Rust (only send matching entries). Virtualized list (render visible rows only). Rate-limit to 60fps batches. Configurable ring buffer size. Test with chatty app. |
| Gradle build complexity / non-standard configs | Medium | Start with standard AGP structure. Fallback to `./gradlew tasks --all` for variant discovery instead of parsing. Allow manual task configuration. |
| SolidJS ecosystem gaps vs React | Low-Medium | SolidJS primitives cover all needs. Custom VirtualList, resizable panels, DnD — all buildable without React ecosystem. Direct DOM manipulation available as escape hatch. |
| AI API costs + latency | Low | AI features are additive, not blocking (IDE fully usable without AI). Support multiple providers including local (Ollama). Cache embeddings aggressively. Use smaller/faster models for ghost text completions. |
| macOS security restrictions | Low-Medium | Follow Tauri's macOS signing guide precisely. Test on clean macOS install. Sidecar binaries need codesigning with the app. Entitlements must include JIT execution permission for WKWebView. |

---

## Testing Strategy

### Rust Backend
- **Unit tests** (`#[test]`, `tokio::test`): Each service module. Test logcat parser, Gradle output parser, variant discovery, LSP JSON-RPC serialization, filter matching, ring buffer.
- **Integration tests**: LSP client + real kotlin-language-server with a small Kotlin project. ADB manager + emulator. Build runner + test Gradle project (`tempfile` for temp directories).
- **Crates**: `tokio::test`, `tempfile`, `mockall`

### Frontend
- **Component tests**: `@solidjs/testing-library` + `vitest`. Each panel component, VirtualList behavior, CodeMirror wrapper, tab system.
- **E2E tests**: `tauri-driver` (Tauri's WebDriver tool) or Playwright.

### AI Integration
- Mock LLM responses to test context assembly and response rendering.
- Test MCP tools with a mock MCP client.
- Test embedding pipeline with a small synthetic corpus.

---

## Distribution

```bash
# Development
npm run tauri dev

# Production - Universal binary (Apple Silicon + Intel)
npm run tauri build -- --target universal-apple-darwin
```

Outputs: `.app` bundle, `.dmg` installer.

**Distribution channels:**
1. Direct `.dmg` download from website
2. `brew install --cask android-ide` (Homebrew cask)
3. Auto-update via `@tauri-apps/plugin-updater` (checks JSON endpoint, downloads differential updates)

**Signing flow:** Apple Developer account → Developer ID Application certificate → `tauri.conf.json` signing identity → `notarytool` notarization → staple ticket to `.app` and `.dmg`.

---

## Verification Checklist

After the prototype is built, verify:

1. Open Google's Sunflower sample project — file tree loads correctly with .gitignore respected
2. Edit a Kotlin file, verify syntax highlighting and save (Cmd+S)
3. Trigger `assembleDebug` build, verify streaming logs with clickable error links
4. Switch build variant, verify correct Gradle task runs
5. Connect Android emulator, install APK, launch app
6. Open logcat, verify streaming with tag/level/package filters
7. Use go-to-definition (Cmd+Click) on a Kotlin symbol
8. Use find-references on a function
9. Open AI chat, use @logcat to ask about a log entry
10. Use Cmd+K inline edit on a function, accept the diff
11. Connect Claude Code via MCP, verify it can read logcat, trigger builds, and edit files
12. Git panel shows changes after editing files
13. Full-text search works across the project (Cmd+Shift+F)
14. Command palette (Cmd+P) shows and opens files
