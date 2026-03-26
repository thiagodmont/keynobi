# Architecture Decision Records

> Append-only log of non-obvious architectural decisions. Never delete entries — mark superseded decisions with `[SUPERSEDED by #N]`.
> Format: `### ADR-N: Title` with Date, Status, Context, Decision, Consequences.

---

### ADR-1: Tauri 2.0 over Electron

- **Date**: Phase 1
- **Status**: Active
- **Context**: We needed a native desktop shell for an IDE. The standard choice for web-tech IDEs is Electron (VS Code, Cursor). However, Electron carries a 300–800 MB RAM baseline and bundles a full Chromium renderer.
- **Decision**: Use Tauri 2.0, which uses the platform's native WebView (WKWebView on macOS) and a Rust backend. Baseline RAM is 20–40 MB.
- **Consequences**:
  - (+) Drastically lower memory footprint
  - (+) Rust backend handles all heavy lifting (file I/O, LSP, search, tree-sitter) without blocking the UI
  - (+) First-class sidecar support for bundling external binaries (LSP server)
  - (+) Tauri Channel API enables high-throughput streaming (logcat, build logs)
  - (-) WKWebView has less debugging tooling than Chrome DevTools
  - (-) Some web APIs behave slightly differently in WKWebView

---

### ADR-2: SolidJS over React for the Frontend

- **Date**: Phase 1
- **Status**: Active
- **Context**: The IDE frontend needs to render large, frequently updating datasets: streaming logcat, file trees with 1000+ nodes, multi-tab editors, and real-time diagnostics. React's virtual DOM diffing adds latency at these scales.
- **Decision**: Use SolidJS with fine-grained reactivity (Signals). SolidJS compiles JSX to direct DOM operations rather than a virtual DOM, delivering 2–3x faster rendering than React.
- **Consequences**:
  - (+) IDE workloads (streaming data, many simultaneous reactive sources) benefit directly from fine-grained updates
  - (+) No unnecessary re-renders — only the exact DOM nodes that depend on a changed signal update
  - (+) Smaller bundles than React equivalents
  - (-) Smaller ecosystem; some React-ecosystem libraries must be replaced or built from scratch
  - (-) Developers must understand SolidJS reactivity rules (signals cannot be destructured; stores use `produce` for mutations)

---

### ADR-3: CodeMirror 6 over Monaco Editor

- **Date**: Phase 1
- **Status**: Active
- **Context**: VS Code and many web IDEs use Monaco Editor. However, Monaco has documented compatibility issues with WebKit/WKWebView, which is the renderer Tauri uses on macOS.
- **Decision**: Use CodeMirror 6. It is designed for cross-browser compatibility, has a fully modular extension system, and its LSP integration patterns are well-established.
- **Consequences**:
  - (+) Works natively in WKWebView without hacks
  - (+) Fully composable extension system — LSP bridge, linting, completions are all just extensions
  - (+) Lezer parser (CM6's internal parser) is fast enough for large files; viewport rendering handles 10k+ line files
  - (-) Less name recognition than Monaco; fewer out-of-the-box language modes
  - (-) Kotlin mode uses the legacy `clike` bridge initially, not a native Lezer grammar (improvement planned)

---

### ADR-4: Per-Concern Mutex State in Rust

- **Date**: Phase 1
- **Status**: Active
- **Context**: Tauri commands can execute concurrently. A naive single `AppState` Mutex would serialize all IPC calls — a logcat poll would block a file read.
- **Decision**: Each service domain owns its own `Mutex`-wrapped state struct: `FsState`, `LspState`, `TreeSitterState`. Commands take only the state they need, preventing cross-domain contention.
- **Consequences**:
  - (+) File operations never wait for LSP operations to complete
  - (+) Pattern is explicit and discoverable — adding a new service follows the same shape
  - (+) Mutexes are held only for the minimum time needed (lock, clone value, drop lock, do I/O)
  - (-) More boilerplate than a single shared state
  - (-) Developers must be careful not to hold the Mutex lock across an `.await` (would cause deadlocks under contention)

---

### ADR-5: ts-rs for Rust-to-TypeScript Type Generation

- **Date**: Phase 1
- **Status**: Active
- **Context**: The Rust backend and TypeScript frontend share data models (file nodes, diagnostics, LSP types). Maintaining parallel type definitions manually risks drift.
- **Decision**: Use the `ts-rs` crate to derive TypeScript interfaces directly from Rust structs. Running `cargo test` regenerates all bindings into `src/bindings/`. All frontend IPC types are imported from `@/bindings`.
- **Consequences**:
  - (+) Single source of truth for shared types — change Rust, regenerate, TypeScript is updated
  - (+) Eliminates entire class of type drift bugs at the IPC boundary
  - (-) Frontend types must be regenerated after every model change (`npm run generate:bindings`)
  - (-) Generated files must not be manually edited

---

### ADR-6: JetBrains Kotlin/kotlin-lsp over fwcd/kotlin-language-server

- **Date**: Phase 2
- **Status**: Active
- **Context**: The original plan referenced `fwcd/kotlin-language-server`. That project is outdated and unmaintained.
- **Decision**: Use the official [Kotlin/kotlin-lsp](https://github.com/Kotlin/kotlin-lsp) from JetBrains. It is based on the IntelliJ IDEA platform (same analysis engine as Android Studio), supports Kotlin 2.3.0, bundles its own JRE, and ships platform-specific standalone distributions.
- **Consequences**:
  - (+) IntelliJ-grade analysis: K2 compiler diagnostics, full rename/move refactoring, code formatting
  - (+) Actively maintained with releases tracking Kotlin versions
  - (+) Bundles its own JRE — no external Java installation required from users
  - (-) ~400 MB distribution size per platform — cannot be bundled in the app; requires download-on-demand
  - (-) Uses pull-based diagnostics (`textDocument/diagnostic`, LSP 3.17) rather than push-based `publishDiagnostics` — our LSP client must implement the pull model
  - (-) Pre-alpha stability guarantee from JetBrains — breaking changes expected across versions

---

### ADR-7: Download-on-Demand for the LSP Binary

- **Date**: Phase 2
- **Status**: Active
- **Context**: The JetBrains Kotlin LSP standalone distribution is ~400 MB per platform. Bundling it inside the `.app` would make the app download enormous and the DMG impractical.
- **Decision**: Ship the IDE without the LSP binary. On first project open, offer the user a download prompt. The binary is downloaded from JetBrains CDN, extracted to `~/.androidide/kotlin-lsp/{version}/`, and cached across launches.
- **Consequences**:
  - (+) App distribution stays small (~40 MB Rust binary + ~1 MB frontend bundle)
  - (+) Version updates are independent — users can update the LSP without reinstalling the IDE
  - (+) Cache is shared across IDE versions
  - (-) First-time experience requires an internet connection and a wait (~400 MB download)
  - (-) Zip extraction requires Zip Slip protection (implemented: path canonicalization + size caps)

---

### ADR-8: Tree-sitter as Fallback and Instant Symbol Layer

- **Date**: Phase 2
- **Status**: Active
- **Context**: The JetBrains LSP takes several seconds to start (JVM startup + Gradle import). During that window, the IDE should still provide basic code intelligence.
- **Decision**: Use Tree-sitter (`tree-sitter-kotlin-ng`) for instant, always-available symbol extraction, document outline, and basic navigation fallback. Tree-sitter runs synchronously in Rust, producing results in < 50ms. The LSP is preferred when ready; Tree-sitter is used otherwise.
- **Consequences**:
  - (+) Document outline and symbols panel are available immediately on file open
  - (+) Navigation history and jump-to-symbol work before LSP is ready
  - (+) Future Phase 5 AI context extraction can use Tree-sitter AST chunking
  - (-) Tree-sitter symbols are syntactic only (no type resolution, no cross-file references)
  - (-) An LRU cache (50 trees max) is required to bound memory usage

---

### ADR-9: Ripgrep Library Crates for Project-Wide Search

- **Date**: Phase 2
- **Status**: Active
- **Context**: Project-wide text search must be fast, gitignore-aware, and stream results incrementally. Running `rg` as a subprocess adds overhead; a native Rust integration is more controllable.
- **Decision**: Use the `grep-regex`, `grep-searcher`, and `grep-matcher` crates directly (the library components of ripgrep). Combined with the `ignore` crate for gitignore-aware walking.
- **Consequences**:
  - (+) Sub-500ms search across 500-file projects for literal queries
  - (+) Results streamed per-file as found, not batch-delivered at end
  - (+) Gitignore rules and hard-coded exclusions (`build/`, `.git/`, etc.) applied consistently with the file tree
  - (-) More complex than shelling out to `rg`, but avoids subprocess overhead and path quoting bugs

---

### ADR-10: Global Action Registry + Dual Keybinding Registration

- **Date**: Phase 2
- **Status**: Active
- **Context**: The command palette needs a runtime-discoverable list of all IDE actions with their labels, shortcuts, and categories. Registering keybindings and actions separately would create drift.
- **Decision**: Introduce a central `action-registry.ts` module. In `App.tsx`, a `registerKeyAndAction()` helper simultaneously registers both the keyboard shortcut (via `keybindings.ts`) and the action (via the registry). The command palette reads from the registry; keybinding resolution reads from the keybinding system.
- **Consequences**:
  - (+) `Cmd+Shift+P` always shows a complete list of IDE actions including their shortcuts
  - (+) Single point of truth — adding a new feature requires one call to register both
  - (+) The registry will serve as the AI agent tool discovery surface in Phase 5 (MCP)
  - (-) All keybindings must go through `registerKeyAndAction` to appear in the palette; direct `registerKeybinding` calls bypass discovery

---

### ADR-11: Path Security via Canonicalization, Not Prefix Matching

- **Date**: Phase 2 (refined from Phase 1)
- **Status**: Active
- **Context**: File commands initially used `path.starts_with(project_root)` to enforce that operations stay within the open project. This is bypassable via symlinks or `..` components.
- **Decision**: All path validation uses `canonicalize()` on both the target path and the project root before the prefix check. This resolves symlinks and normalizes `..` at the OS level. The pattern is implemented in `ensure_within_project` (file commands) and `ensure_path_in_project` (LSP commands).
- **Consequences**:
  - (+) Symlink traversal and path component attacks are blocked at the OS level
  - (+) Consistent security posture across all command categories (file, LSP, search, tree-sitter)
  - (-) `canonicalize()` requires the path to exist on disk; new files use the parent directory for canonicalization

---

### ADR-12: Navigation History via `openFileAtLocation` Funnel

- **Date**: Final Review
- **Status**: Active
- **Context**: Navigation history (`pushNavigation`) needs to be called every time the user jumps to a new location (search result, symbol click, go-to-definition). Maintaining this call at every individual call site would be error-prone and easy to forget.
- **Decision**: All jump-to-location actions funnel through a single `openFileAtLocation(path, line, col)` function in `project.service.ts`. That function is responsible for pushing the current position to the history stack before navigating.
- **Consequences**:
  - (+) Navigation history is always populated correctly regardless of the source
  - (+) Adding new navigation actions automatically benefits from history
  - (-) Direct `setActiveFile()` calls bypass history — developers must use `openFileAtLocation` for jump-to actions

---

### ADR-13: Zip Extraction Security (Anti-Zip Slip)

- **Date**: Phase 2 (Code Review)
- **Status**: Active
- **Context**: The LSP download-and-install flow extracts a ZIP file from an external CDN. A malicious or corrupted ZIP could contain entries with path traversal (e.g., `../../etc/passwd`), writing files outside the install directory.
- **Decision**: Implement multi-layered Zip Slip protection in `extract_zip`:
  1. Reject entries containing `..` in their path components
  2. After constructing `out_path = dest.join(relative)`, canonicalize the destination and verify `out_path` starts with the canonical destination
  3. Enforce a 512 MB per-entry cap and 2 GB total extraction cap
- **Consequences**:
  - (+) Path traversal attacks from crafted ZIPs are blocked
  - (+) Zip bomb attacks (deeply nested or inflated archives) are bounded
  - (-) Canonicalization requires the destination directory to exist before extraction begins

---

### ADR-14: LSP Content-Length Cap (64 MB)

- **Date**: Phase 2 (Code Review)
- **Status**: Active
- **Context**: The JSON-RPC protocol over stdio uses `Content-Length` headers. If a buggy or hostile LSP server sends an extremely large `Content-Length`, naively allocating that many bytes would cause an out-of-memory crash.
- **Decision**: Enforce a hard 64 MB cap on any single LSP message. Messages exceeding the cap cause the read to fail and log an error.
- **Consequences**:
  - (+) OOM crash from hostile/buggy LSP is prevented
  - (+) Reasonable cap: real LSP messages are typically < 1 MB even for large files
  - (-) If a legitimate future use case requires messages > 64 MB, the constant must be adjusted

---

### ADR-15: Gradle Root Auto-Detection for LSP Workspace Root

- **Date**: Phase 2
- **Status**: Active
- **Context**: When a user opens a Gradle module subfolder (e.g. `the-crazy-project/the-crazy-app`), the Kotlin LSP needs the Gradle project root (where `settings.gradle(.kts)` and `gradlew` live) to function correctly. Without it: (a) the LSP reports "Package directive does not match the file location" because it calculates package paths relative to the wrong root, (b) `gradlew` is not found so Gradle sync fails, and (c) go-to-definition cannot resolve symbols across modules.
- **Decision**: On `open_project`, walk upward from the opened directory (max 10 levels) to find the nearest ancestor containing `settings.gradle` or `settings.gradle.kts`. Store this as `gradle_root` in `FsStateInner` alongside the user-opened `project_root`. Use `gradle_root` for:
  1. LSP `rootUri`, `workspaceFolders`, and process `current_dir`
  2. Path security boundary (`ensure_path_in_project`, `validate_path`) so go-to-definition targets in sibling modules pass validation
  3. Health check `gradlew` probe
- **Consequences**:
  - (+) Package directive errors resolved — LSP sees the full Gradle project structure
  - (+) `gradlew` found — Gradle sync works for dependency resolution and classpath
  - (+) Go-to-definition works across Gradle modules — security checks use the broader Gradle root scope
  - (+) No UX change when user opens the actual Gradle root — `gradle_root` equals `project_root`
  - (-) Security boundary is broader than the user-opened folder — paths anywhere in the Gradle tree are accessible for read/write. This is standard IDE behaviour (Android Studio works the same way).
